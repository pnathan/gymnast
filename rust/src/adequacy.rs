//! Adequacy campaign: mutation, concurrency, and fault injection
//! (`docs/rust-port-plan-phase9.md`, sections A-D). Ports
//! `src/adequacy.lisp`'s behavioral intent against the Rust IR contract
//! (`docs/ir-contract-deltas.md`).
//!
//! Passing happy-path tests is insufficient evidence that the verifier
//! can detect realistic synthesis defects. This module seeds known
//! defects into the IR, re-runs verification, and reports which
//! obligations detected each defect. A campaign fails when critical
//! mutants survive undetected.
//!
//! THE ONE DELIBERATE SEMANTIC DELTA — baseline-aware detection: the
//! reference counts a mutant "killed" when ANY obligation is `failed`
//! after mutation. Against `todo.gym` that is vacuous — the baseline
//! already has two `failed` obligations, so every mutant (the identity
//! mutation included) would count as killed. Here verification runs
//! over the BASELINE IR once, then over each mutated IR: a mutant is
//! **killed** iff some obligation is `failed` in the mutated results
//! AND was not `failed` in the baseline (a NEW failure, including an
//! obligation id that only exists post-mutation). Obligations whose
//! status moved to `indeterminate` from anything else are reported as
//! **degraded** — visibility loss, not detection: an undecidable
//! verdict detects nothing.
//!
//! Every function is TOTAL over arbitrary input: a mutation naming a
//! missing target returns the IR unchanged (the mutant then trivially
//! survives), never a panic. `replace_limit` recurses only on the
//! (already parse-depth-bounded) predicate tree.

use crate::fingerprint;
use crate::ir::{Ir, IrNode};
use crate::sexpr::Sexpr;
use crate::transition::{clause_head_is, extract_transitions};
use crate::verify::{lower_all_obligations, verify_obligation};

pub const ADEQUACY_SCHEMA: &str = "gymnast.adequacy/0.1";

// -----------------------------------------------------------------------
// Types (plan section A).
// -----------------------------------------------------------------------

/// The closed set of mutation operators (no closures — the reference's
/// `mutator` lambda field is replaced by this data enum so mutants stay
/// serializable and the operator set stays auditable).
#[derive(Debug, Clone, PartialEq)]
pub enum Mutation {
    /// Drop all `requires` clauses from the named behavior.
    WeakenPrecondition { behavior_name: String },
    /// Remove the named invariant node from every partition.
    RemoveInvariant { invariant_name: String },
    /// Rewrite the `<=`/`<` limit inside the named invariant's
    /// `:always` predicate (see `replace_limit`).
    WeakenLimit {
        invariant_name: String,
        new_limit: i64,
    },
    /// Drop all `fails` clauses from the named behavior.
    RemoveFailureMode { behavior_name: String },
    /// Empty the named behavior's `:writes` field.
    SkipStateWrite { behavior_name: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mutant {
    pub id: String,
    pub class: String, // "weaken-precondition" | "remove-invariant" | ...
    pub description: String,
    pub mutation: Mutation,
    pub critical: bool,
}

impl Mutant {
    /// Constructor sets `critical: true`, matching the reference's
    /// `gymnast-mutant`.
    pub fn new(id: &str, class: &str, description: &str, mutation: Mutation) -> Mutant {
        Mutant {
            id: id.to_string(),
            class: class.to_string(),
            description: description.to_string(),
            mutation,
            critical: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MutantResult {
    pub mutant_id: String,
    pub class: String,
    pub critical: bool,
    /// `true` iff the mutation actually CHANGED the IR (phase-9 gate,
    /// finding 1): a mutant whose target is absent from this spec is
    /// an identity mutation — re-verifying the unchanged IR yields no
    /// evidence about the verifier, so an inapplicable mutant is never
    /// a kill, never a survivor, and never a blind spot.
    pub applied: bool,
    pub killed: bool,
    /// NEW failures only: obligation ids `failed` after mutation that
    /// were not `failed` in the baseline.
    pub detecting_obligations: Vec<String>,
    /// Obligation ids that moved to `indeterminate` from anything else
    /// (the baseline-aware DELTA field; the reference has no notion of
    /// degradation).
    pub degraded_obligations: Vec<String>,
    pub description: String,
}

// -----------------------------------------------------------------------
// IR surgery helpers.
//
// The mutated IR is a TRANSIENT VERIFICATION INPUT, never serialized:
// none of these helpers re-fingerprint the IR (or re-sort partitions —
// node ids never change, so sort order is preserved by construction).
// The mutated value's `fingerprint` field still carries the ORIGINAL
// fingerprint, and that is deliberate (plan section B).
// -----------------------------------------------------------------------

/// The first node of `kind` whose `name` matches — the reference's
/// `(car (filter ...))`, first match only, kind-scoped.
fn find_target<'a>(ir: &'a Ir, kind: &str, name: &str) -> Option<&'a IrNode> {
    ir.nodes_of_kind(kind).into_iter().find(|n| n.name == name)
}

/// Replaces the node with `target_id` in every partition it appears in.
fn replace_ir_node(ir: &Ir, target_id: &str, new_node: &IrNode) -> Ir {
    let mut out = ir.clone();
    for partition in [
        &mut out.design,
        &mut out.transitions,
        &mut out.obligations,
        &mut out.synthesis,
    ] {
        for node in partition.iter_mut() {
            if node.id == target_id {
                *node = new_node.clone();
            }
        }
    }
    out
}

/// Drops the node with `target_id` from every partition it appears in.
fn remove_ir_node(ir: &Ir, target_id: &str) -> Ir {
    let mut out = ir.clone();
    for partition in [
        &mut out.design,
        &mut out.transitions,
        &mut out.obligations,
        &mut out.synthesis,
    ] {
        partition.retain(|node| node.id != target_id);
    }
    out
}

/// A copy of `node` with `key`'s value replaced in place (field order
/// preserved), or — reference `gymnast-put-assoc` parity — the pair
/// appended into its canonically sorted position when the key is
/// absent.
fn with_field(node: &IrNode, key: &str, value: Sexpr) -> IrNode {
    let mut out = node.clone();
    if let Some(slot) = out.fields.iter_mut().find(|(k, _)| k == key) {
        slot.1 = value;
    } else {
        out.fields.push((key.to_string(), value));
        out.fields.sort_by(|a, b| a.0.cmp(&b.0));
    }
    out
}

/// A copy of `node` without any clause headed by `head` (relative
/// order of the surviving clauses preserved — clause order is
/// semantic in the IR contract).
fn without_clauses(node: &IrNode, head: &str) -> IrNode {
    let mut out = node.clone();
    out.clauses.retain(|c| !clause_head_is(c, head));
    out
}

// -----------------------------------------------------------------------
// Mutation application (plan section B).
// -----------------------------------------------------------------------

/// Applies one mutation as pure IR surgery. Total: a missing target
/// (wrong name, or a name that exists only under another kind) returns
/// the IR unchanged. The result is NEVER re-fingerprinted — see the
/// module-level surgery note.
pub fn apply_mutation(ir: &Ir, mutation: &Mutation) -> Ir {
    match mutation {
        Mutation::WeakenPrecondition { behavior_name } => {
            match find_target(ir, "behavior", behavior_name) {
                None => ir.clone(),
                Some(target) => {
                    replace_ir_node(ir, &target.id.clone(), &without_clauses(target, "requires"))
                }
            }
        }
        Mutation::RemoveInvariant { invariant_name } => {
            match find_target(ir, "invariant", invariant_name) {
                None => ir.clone(),
                Some(target) => remove_ir_node(ir, &target.id.clone()),
            }
        }
        Mutation::WeakenLimit {
            invariant_name,
            new_limit,
        } => match find_target(ir, "invariant", invariant_name) {
            None => ir.clone(),
            Some(target) => {
                // Reference parity (`gymnast-assoc-value` then
                // `gymnast-put-assoc`): an absent `:always` reads as
                // nil, and nil passes through `replace_limit`
                // unchanged.
                let old_always = target
                    .field(":always")
                    .cloned()
                    .unwrap_or_else(|| Sexpr::List(vec![]));
                let new_always = replace_limit(&old_always, *new_limit);
                replace_ir_node(
                    ir,
                    &target.id.clone(),
                    &with_field(target, ":always", new_always),
                )
            }
        },
        Mutation::RemoveFailureMode { behavior_name } => {
            match find_target(ir, "behavior", behavior_name) {
                None => ir.clone(),
                Some(target) => {
                    replace_ir_node(ir, &target.id.clone(), &without_clauses(target, "fails"))
                }
            }
        }
        Mutation::SkipStateWrite { behavior_name } => {
            match find_target(ir, "behavior", behavior_name) {
                None => ir.clone(),
                Some(target) => replace_ir_node(
                    ir,
                    &target.id.clone(),
                    &with_field(target, ":writes", Sexpr::List(vec![])),
                ),
            }
        }
    }
}

/// The reference's `gymnast-replace-limit` recursion, ported exactly:
/// `(<= a N)` / `(< a N)` with an Int in third position gets the new
/// limit (rebuilt as exactly three elements, like the reference's
/// `(list '<= (cadr predicate) new-limit)`); `(forall binders body)`
/// recurses into the body; anything else — nil, atoms, other heads, a
/// comparison whose third position is not an Int — is unchanged.
/// Total, bounded by the (already parse-depth-bounded) predicate tree.
pub fn replace_limit(predicate: &Sexpr, new_limit: i64) -> Sexpr {
    let items = match predicate.as_list() {
        // Atom (symbol / string / int): unchanged.
        None => return predicate.clone(),
        Some(items) => items,
    };
    // nil (the empty list): unchanged.
    if items.is_empty() {
        return predicate.clone();
    }
    let empty = Sexpr::List(vec![]);
    match items.first().and_then(|h| h.as_sym()) {
        Some(op @ ("<=" | "<")) if matches!(items.get(2), Some(Sexpr::Int(_))) => {
            Sexpr::list(vec![
                Sexpr::sym(op),
                items.get(1).cloned().unwrap_or_else(|| empty.clone()),
                Sexpr::Int(new_limit),
            ])
        }
        Some("forall") => Sexpr::list(vec![
            Sexpr::sym("forall"),
            items.get(1).cloned().unwrap_or_else(|| empty.clone()),
            replace_limit(items.get(2).unwrap_or(&empty), new_limit),
        ]),
        _ => predicate.clone(),
    }
}

// -----------------------------------------------------------------------
// Concurrency and fault scaffolding (plan section C — reference
// parity, DATA only: scenario descriptions, never executed; the
// campaign runs mutants only).
// -----------------------------------------------------------------------

/// An adversarial-interleaving scenario over the FIRST transition with
/// a non-empty write set (transitions in `extract_transitions` order,
/// i.e. behavior-node id order). `None` when no transition writes.
/// Steps count DOWN from `boundary_count` to 1 — the reference's
/// recursion order (cons N, then recurse on N-1) — so
/// `boundary_count <= 0` yields a scenario with EMPTY steps (the
/// recursion base), the boundary echoed verbatim.
pub fn boundary_interleaving(ir: &Ir, boundary_count: i64) -> Option<Sexpr> {
    let transitions = extract_transitions(ir);
    let tr = transitions.iter().find(|t| !t.writes.is_empty())?;
    let steps: Vec<Sexpr> = (1..=boundary_count.max(0))
        .rev()
        .map(|n| {
            Sexpr::list(vec![
                Sexpr::sym(&tr.operation),
                Sexpr::Str(format!("actor-{}", n)),
                Sexpr::Str(format!("input-{}", n)),
            ])
        })
        .collect();
    Some(Sexpr::list(vec![
        Sexpr::sym("interleaving-scenario"),
        Sexpr::pair("operation", Sexpr::sym(&tr.operation)),
        Sexpr::pair("boundary", Sexpr::Int(boundary_count)),
        Sexpr::pair("steps", Sexpr::List(steps)),
        Sexpr::pair("expected-violations", Sexpr::Int(0)),
    ]))
}

fn fault_scenario(name: &str, fault_type: &str, after: &str) -> Sexpr {
    Sexpr::list(vec![
        Sexpr::sym("fault-scenario"),
        Sexpr::pair("name", Sexpr::sym(name)),
        Sexpr::pair("type", Sexpr::sym(fault_type)),
        Sexpr::pair("after", Sexpr::sym(after)),
        // The constructor fixes every expectation to `detected`,
        // matching the reference's `gymnast-make-fault-scenario`.
        Sexpr::pair("expected", Sexpr::sym("detected")),
    ])
}

/// The four standard fault-injection scenario descriptions, in the
/// reference's order.
pub fn standard_fault_scenarios() -> Vec<Sexpr> {
    vec![
        fault_scenario("restart-after-write", "restart", "acknowledged-write"),
        fault_scenario("timeout-mid-operation", "timeout", "operation-start"),
        fault_scenario(
            "duplicate-delivery",
            "duplicate-delivery",
            "acknowledged-write",
        ),
        fault_scenario("stale-version", "stale-version", "read"),
    ]
}

// -----------------------------------------------------------------------
// Campaign execution (plan section D).
// -----------------------------------------------------------------------

/// The five standard mutants for the Todo specification, ids m1..m5,
/// every one critical (constructor).
pub fn standard_todo_mutants() -> Vec<Mutant> {
    vec![
        Mutant::new(
            "m1",
            "weaken-precondition",
            "create_task accepts requests with all authorization preconditions dropped",
            Mutation::WeakenPrecondition {
                behavior_name: "create_task".to_string(),
            },
        ),
        Mutant::new(
            "m2",
            "remove-invariant",
            "the sharing_limit invariant is removed entirely",
            Mutation::RemoveInvariant {
                invariant_name: "sharing_limit".to_string(),
            },
        ),
        Mutant::new(
            "m3",
            "weaken-limit",
            "sharing_limit's cap is weakened from 256 to 512",
            Mutation::WeakenLimit {
                invariant_name: "sharing_limit".to_string(),
                new_limit: 512,
            },
        ),
        Mutant::new(
            "m4",
            "remove-failure-mode",
            "invite_user's declared failure modes are dropped",
            Mutation::RemoveFailureMode {
                behavior_name: "invite_user".to_string(),
            },
        ),
        Mutant::new(
            "m5",
            "skip-state-write",
            "create_task acknowledges without writing to tasks",
            Mutation::SkipStateWrite {
                behavior_name: "create_task".to_string(),
            },
        ),
    ]
}

fn result_obligation_id(result: &Sexpr) -> Option<&str> {
    result.assoc("obligation-id").and_then(|v| v.as_str())
}

fn result_status(result: &Sexpr) -> Option<&str> {
    result.assoc("status").and_then(|s| s.as_sym())
}

/// The baseline status of one obligation id, or `None` for an id the
/// baseline never produced (first match wins, the crate's uniform
/// assoc discipline).
fn baseline_status<'a>(baseline: &'a [Sexpr], id: &str) -> Option<&'a str> {
    baseline
        .iter()
        .find(|r| result_obligation_id(r) == Some(id))
        .and_then(result_status)
}

/// Runs one mutant with BASELINE-AWARE detection (the module-level
/// delta): `baseline` is the slice of verification-result forms from
/// the UNMUTATED IR. Killed iff some obligation is `failed` post-
/// mutation and was not `failed` in the baseline — including an
/// obligation id that only exists post-mutation. An obligation whose
/// status moved to `indeterminate` from anything else (an absent
/// baseline entry included) is degraded, never a kill. Detection order
/// follows the mutated run's lowering order (deterministic).
pub fn run_mutant(ir: &Ir, baseline: &[Sexpr], mutant: &Mutant) -> MutantResult {
    let mutated = apply_mutation(ir, &mutant.mutation);
    // Identity mutation (missing target): no defect was seeded, so
    // there is nothing to detect — report inapplicability honestly
    // instead of fabricating a "survived" verdict (phase-9 gate,
    // finding 1: the todo mutant set over a foreign spec produced five
    // fabricated blind spots about mutations that never happened).
    if mutated == *ir {
        return MutantResult {
            mutant_id: mutant.id.clone(),
            class: mutant.class.clone(),
            critical: mutant.critical,
            applied: false,
            killed: false,
            detecting_obligations: vec![],
            degraded_obligations: vec![],
            description: mutant.description.clone(),
        };
    }
    let obligations = lower_all_obligations(&mutated);
    let results: Vec<Sexpr> = obligations
        .iter()
        .map(|o| verify_obligation(&mutated, o))
        .collect();

    let mut detecting = Vec::new();
    let mut degraded = Vec::new();
    for result in &results {
        let id = match result_obligation_id(result) {
            Some(id) => id,
            None => continue,
        };
        match result_status(result) {
            Some("failed") if baseline_status(baseline, id) != Some("failed") => {
                detecting.push(id.to_string());
            }
            Some("indeterminate") if baseline_status(baseline, id) != Some("indeterminate") => {
                degraded.push(id.to_string());
            }
            _ => {}
        }
    }

    MutantResult {
        mutant_id: mutant.id.clone(),
        class: mutant.class.clone(),
        critical: mutant.critical,
        applied: true,
        killed: !detecting.is_empty(),
        detecting_obligations: detecting,
        degraded_obligations: degraded,
        description: mutant.description.clone(),
    }
}

/// t / nil, the crate's boolean convention (`Sexpr` has no Bool).
fn bool_sexpr(b: bool) -> Sexpr {
    if b {
        Sexpr::sym("t")
    } else {
        Sexpr::List(vec![])
    }
}

/// The FLAT mutant-result projection (reference record projection; the
/// phase-6 flat-vs-nested convention split documented in
/// `docs/ir-contract-deltas.md`).
fn mutant_result_to_sexpr(result: &MutantResult) -> Sexpr {
    Sexpr::list(vec![
        Sexpr::sym("mutant-result"),
        Sexpr::pair("mutant-id", Sexpr::Str(result.mutant_id.clone())),
        Sexpr::pair("class", Sexpr::sym(&result.class)),
        Sexpr::pair("critical", bool_sexpr(result.critical)),
        Sexpr::pair("applied", bool_sexpr(result.applied)),
        Sexpr::pair("killed", bool_sexpr(result.killed)),
        Sexpr::pair(
            "detecting-obligations",
            Sexpr::List(
                result
                    .detecting_obligations
                    .iter()
                    .map(|id| Sexpr::Str(id.clone()))
                    .collect(),
            ),
        ),
        Sexpr::pair(
            "degraded-obligations",
            Sexpr::List(
                result
                    .degraded_obligations
                    .iter()
                    .map(|id| Sexpr::Str(id.clone()))
                    .collect(),
            ),
        ),
        Sexpr::pair("description", Sexpr::Str(result.description.clone())),
    ])
}

fn blind_spot_to_sexpr(result: &MutantResult) -> Sexpr {
    Sexpr::list(vec![
        Sexpr::sym("blind-spot"),
        Sexpr::pair("mutant-id", Sexpr::Str(result.mutant_id.clone())),
        Sexpr::pair("class", Sexpr::sym(&result.class)),
        Sexpr::pair("description", Sexpr::Str(result.description.clone())),
    ])
}

/// Runs the full campaign: the baseline (lower + verify every
/// obligation over the unmutated IR) is computed ONCE, then each
/// mutant via `run_mutant`. The `campaign-result` root uses the NESTED
/// house convention with the fingerprint computed over the
/// fingerprint-free form and appended last — the same discipline as
/// every fingerprinted artifact since phase 7. The result is BOUND to
/// its subject (module + IR fingerprint). `pass` iff every critical
/// mutant was applied AND killed (an empty mutant list passes
/// vacuously; an inapplicable critical mutant blocks pass — the
/// campaign could not test that defect class); `degraded-only` counts
/// applied, unkilled mutants with at least one degradation;
/// `inapplicable` counts identity mutations (missing targets), which
/// are never survivors and never blind spots (phase-9 gate, finding
/// 1).
pub fn run_campaign(ir: &Ir, mutants: &[Mutant]) -> Sexpr {
    let obligations = lower_all_obligations(ir);
    let baseline: Vec<Sexpr> = obligations
        .iter()
        .map(|o| verify_obligation(ir, o))
        .collect();

    let results: Vec<MutantResult> = mutants
        .iter()
        .map(|m| run_mutant(ir, &baseline, m))
        .collect();

    let killed = results.iter().filter(|r| r.killed).count();
    // A SURVIVOR is a mutant that was APPLIED and not killed; an
    // inapplicable mutant was never tested and is counted separately
    // (phase-9 gate, finding 1).
    let survived: Vec<&MutantResult> = results.iter().filter(|r| r.applied && !r.killed).collect();
    let inapplicable = results.iter().filter(|r| !r.applied).count();
    let degraded_only = results
        .iter()
        .filter(|r| r.applied && !r.killed && !r.degraded_obligations.is_empty())
        .count();
    let critical_survived: Vec<&MutantResult> =
        survived.iter().copied().filter(|r| r.critical).collect();
    // `pass` means: every CRITICAL mutant was applied AND killed. A
    // critical mutant that survived is a blind spot; a critical mutant
    // that never applied means the campaign could not test that defect
    // class against this spec — neither is a pass (the empty mutant
    // list stays vacuously true, reference parity). Fabricated
    // pass-by-inapplicability was the phase-9 gate's blocker.
    let pass = results
        .iter()
        .all(|r| !r.critical || (r.applied && r.killed));

    let mut fields = vec![
        Sexpr::pair("schema", Sexpr::Str(ADEQUACY_SCHEMA.to_string())),
        // The campaign is BOUND to its subject (phase-9 gate, finding
        // 1): without this, the todo campaign result was byte-identical
        // — fingerprint included — over any spec, so the committed
        // golden could impersonate the adequacy evidence for anything.
        Sexpr::pair(
            "subject",
            Sexpr::list(vec![
                Sexpr::pair("module", Sexpr::Str(ir.module_name.clone())),
                Sexpr::pair("ir-fingerprint", Sexpr::Str(ir.fingerprint.clone())),
            ]),
        ),
        Sexpr::pair("total", Sexpr::Int(mutants.len() as i64)),
        Sexpr::pair("killed", Sexpr::Int(killed as i64)),
        Sexpr::pair("survived", Sexpr::Int(survived.len() as i64)),
        Sexpr::pair("inapplicable", Sexpr::Int(inapplicable as i64)),
        Sexpr::pair("degraded-only", Sexpr::Int(degraded_only as i64)),
        Sexpr::pair(
            "critical-survived",
            Sexpr::Int(critical_survived.len() as i64),
        ),
        Sexpr::pair("pass", bool_sexpr(pass)),
        Sexpr::pair(
            "results",
            Sexpr::List(results.iter().map(mutant_result_to_sexpr).collect()),
        ),
        Sexpr::pair(
            "blind-spots",
            Sexpr::List(
                critical_survived
                    .iter()
                    .map(|r| blind_spot_to_sexpr(r))
                    .collect(),
            ),
        ),
    ];

    let fingerprint_free = Sexpr::list(vec![
        Sexpr::sym("campaign-result"),
        Sexpr::list(fields.clone()),
    ]);
    let fp = fingerprint::fingerprint(&fingerprint_free);
    fields.push(Sexpr::pair("fingerprint", Sexpr::Str(fp)));

    Sexpr::list(vec![Sexpr::sym("campaign-result"), Sexpr::list(fields)])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ps(text: &str) -> Sexpr {
        crate::sexpr::parse(text).expect("parse test sexpr")
    }

    fn small_ir() -> Ir {
        let behavior = IrNode::new(
            "m/behavior/act".to_string(),
            "behavior",
            "act".to_string(),
            vec![
                (":on".to_string(), ps("(svc/act actor input)")),
                (":writes".to_string(), ps("(store)")),
            ],
            vec![ps("(requires (ok actor))"), ps("(fails (denied))")],
        );
        let invariant = IrNode::new(
            "m/invariant/cap".to_string(),
            "invariant",
            "cap".to_string(),
            vec![(":scope".to_string(), Sexpr::sym("store"))],
            vec![],
        );
        Ir::new(
            "gymnast.ir/0.1".to_string(),
            "m".to_string(),
            vec![],
            vec![],
            vec![behavior],
            vec![invariant],
            vec![],
            vec![],
        )
    }

    #[test]
    fn test_weaken_limit_inserts_absent_always_as_nil() {
        // Reference put-assoc parity: an invariant WITHOUT an :always
        // field gains (:always nil) — replace_limit(nil) is nil — and
        // the node is otherwise untouched. The target EXISTS, so this
        // is not the missing-target identity case.
        let ir = small_ir();
        let mutated = apply_mutation(
            &ir,
            &Mutation::WeakenLimit {
                invariant_name: "cap".to_string(),
                new_limit: 7,
            },
        );
        let node = mutated.find_node("m/invariant/cap").expect("node kept");
        assert_eq!(node.field(":always"), Some(&Sexpr::List(vec![])));
        assert_eq!(node.field(":scope"), Some(&Sexpr::sym("store")));
        // Canonical field order preserved after insertion.
        let keys: Vec<&str> = node.fields.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec![":always", ":scope"]);
    }

    #[test]
    fn test_replace_limit_rebuilds_exactly_three_elements() {
        // The reference's (list '<= (cadr p) new-limit) DROPS any
        // extra trailing elements; ported exactly.
        assert_eq!(replace_limit(&ps("(<= a 5 extra)"), 9), ps("(<= a 9)"));
    }

    #[test]
    fn test_replace_limit_third_position_must_be_int() {
        assert_eq!(replace_limit(&ps("(< a b)"), 9), ps("(< a b)"));
        assert_eq!(replace_limit(&ps("(< a \"5\")"), 9), ps("(< a \"5\")"));
    }

    #[test]
    fn test_boundary_interleaving_counts_down() {
        let ir = small_ir();
        let scenario = boundary_interleaving(&ir, 2).expect("writing transition exists");
        assert_eq!(
            scenario,
            ps("(interleaving-scenario (operation svc/act) (boundary 2) \
                (steps ((svc/act \"actor-2\" \"input-2\") (svc/act \"actor-1\" \"input-1\"))) \
                (expected-violations 0))")
        );
    }

    #[test]
    fn test_run_mutant_missing_target_survives() {
        // A mutant naming a missing target re-verifies an unchanged IR
        // against its own baseline: no new failures, no degradations.
        let ir = small_ir();
        let baseline: Vec<Sexpr> = lower_all_obligations(&ir)
            .iter()
            .map(|o| verify_obligation(&ir, o))
            .collect();
        let mutant = Mutant::new(
            "x1",
            "weaken-precondition",
            "no such behavior",
            Mutation::WeakenPrecondition {
                behavior_name: "ghost".to_string(),
            },
        );
        let result = run_mutant(&ir, &baseline, &mutant);
        assert!(!result.killed);
        assert!(result.detecting_obligations.is_empty());
        assert!(result.degraded_obligations.is_empty());
        assert!(result.critical);
    }

    #[test]
    fn test_campaign_shape_over_empty_ir_and_one_mutant() {
        let ir = small_ir();
        let mutant = Mutant::new(
            "x1",
            "skip-state-write",
            "act stops writing",
            Mutation::SkipStateWrite {
                behavior_name: "act".to_string(),
            },
        );
        let campaign = run_campaign(&ir, &[mutant]);
        let items = campaign.as_list().expect("list");
        assert_eq!(items[0].as_sym(), Some("campaign-result"));
        assert_eq!(items.len(), 2, "nested house convention");
        let fields = items[1].as_list().expect("fields");
        // Fingerprint self-consistency: last field, computed over the
        // fingerprint-free form.
        let (last, rest) = fields.split_last().expect("non-empty");
        let last_pair = last.as_list().expect("pair");
        assert_eq!(last_pair[0].as_sym(), Some("fingerprint"));
        let free = Sexpr::list(vec![
            Sexpr::sym("campaign-result"),
            Sexpr::list(rest.to_vec()),
        ]);
        assert_eq!(
            last_pair[1].as_str(),
            Some(fingerprint::fingerprint(&free).as_str())
        );
    }
}
