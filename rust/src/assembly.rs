//! Assembly and promotion evidence bundles (`docs/rust-port-plan-phase8.md`,
//! sections A-B). Ports `src/assembly.lisp`'s behavioral intent onto the
//! Rust IR/plan/execution contracts.
//!
//! Principle carried over from the reference, verbatim: the assembler
//! only collects evidence; it never decides whether the evidence is
//! sufficient. Promotion policy is a separate object evaluated over the
//! assembled bundle. No model output participates anywhere in this
//! module.
//!
//! Every function here is total: no panics on any input. Promotion
//! checks are fail-closed — a missing or malformed bundle field
//! evaluates its check to `nil`, never to a fabricated pass (with the
//! one documented vacuous case: a bundle with NO verification section
//! passes both verification checks vacuously, exactly as the plan's
//! check table states).
//!
//! Deliberate deltas from `src/assembly.lisp` (see
//! `docs/ir-contract-deltas.md`, phase-8 section):
//! - the bundle carries a trailing `fingerprint` field over its
//!   fingerprint-free form (the reference bundle has none);
//! - `evaluate_promotion` computes a FIFTH check,
//!   `no-indeterminate-verification` — phase 7 made undecidable
//!   verdicts honest, and promotion must not launder a
//!   fully-indeterminate verification into `promote`;
//! - the Rust `ExecutionStatus` has no `passed` variant, so the
//!   reference's `passed` arm in the succeeded tally is dead here;
//!   `deferred` counts toward neither tally (Lamedh parity — a deferred
//!   node does NOT block `all-nodes-succeeded`);
//! - diagnostics here use the nested house shape
//!   `(diagnostic ((severity s) (code c) (subject "...") (message "...")))`
//!   with severity/code as bare symbols, per the phase-8 plan's printed
//!   sketch; the reference's trailing `details` field (which duplicates
//!   `subject` at every call site) is not carried.

use crate::fingerprint;
use crate::ir::{Ir, IrNode};
use crate::plan::{Plan, PlanNode};
use crate::recipe::{ExecutionResult, ExecutionStatus};
use crate::sexpr::Sexpr;

pub const BUNDLE_SCHEMA: &str = "gymnast.bundle/0.1";

/// One produced file, linked to the plan node that produced it.
#[derive(Debug, Clone, PartialEq)]
pub struct Artifact {
    /// Relative path from the candidate's `files` entry.
    pub path: String,
    /// Plan node that produced it (the RESULT's node-id, never the
    /// candidate's own claim).
    pub node_id: String,
    /// `fingerprint_string(content)` — FNV-1a, same as everywhere.
    pub digest: String,
    /// Content length in BYTES.
    pub size: i64,
}

impl Artifact {
    /// `(artifact ((path "...") (node-id "...") (digest "fnv1a64:...")
    /// (size N)))` — canonical field order as declared.
    pub fn to_sexpr(&self) -> Sexpr {
        Sexpr::list(vec![
            Sexpr::sym("artifact"),
            Sexpr::list(vec![
                Sexpr::pair("path", Sexpr::Str(self.path.clone())),
                Sexpr::pair("node-id", Sexpr::Str(self.node_id.clone())),
                Sexpr::pair("digest", Sexpr::Str(self.digest.clone())),
                Sexpr::pair("size", Sexpr::Int(self.size)),
            ]),
        ])
    }
}

/// Source -> IR -> plan node -> evidence linkage for one IR node.
#[derive(Debug, Clone, PartialEq)]
pub struct TraceabilityEntry {
    /// IR node id.
    pub semantic_id: String,
    /// IR node kind.
    pub kind: String,
    /// Plan node ids whose `inputs` contain `semantic_id`, in plan order.
    pub plan_nodes: Vec<String>,
    /// `!plan_nodes.is_empty()`.
    pub has_implementation: bool,
    /// Some execution result's node_id is in `plan_nodes` — status-blind
    /// (Lamedh parity: the reference filters by node-id membership
    /// alone, so a deferred result is evidence-presence too).
    pub has_evidence: bool,
}

impl TraceabilityEntry {
    /// `(traceability-entry ((semantic-id "...") (kind "...")
    /// (plan-nodes ("..." ...)) (has-implementation t|nil)
    /// (has-evidence t|nil)))`.
    pub fn to_sexpr(&self) -> Sexpr {
        Sexpr::list(vec![
            Sexpr::sym("traceability-entry"),
            Sexpr::list(vec![
                Sexpr::pair("semantic-id", Sexpr::Str(self.semantic_id.clone())),
                Sexpr::pair("kind", Sexpr::Str(self.kind.clone())),
                Sexpr::pair(
                    "plan-nodes",
                    Sexpr::list(
                        self.plan_nodes
                            .iter()
                            .map(|id| Sexpr::Str(id.clone()))
                            .collect(),
                    ),
                ),
                Sexpr::pair("has-implementation", bool_sexpr(self.has_implementation)),
                Sexpr::pair("has-evidence", bool_sexpr(self.has_evidence)),
            ]),
        ])
    }
}

/// The crate's boolean serialization convention: sym `t` for true, the
/// empty list (printing as `nil`) for false.
fn bool_sexpr(b: bool) -> Sexpr {
    if b {
        Sexpr::sym("t")
    } else {
        Sexpr::List(vec![])
    }
}

/// A `t`/`nil`-encoded boolean read back, fail-closed: only the bare
/// symbol `t` reads as true; `nil`, any other value, and a missing
/// field (`None`) all read as false.
fn bool_of(v: Option<&Sexpr>) -> bool {
    v.and_then(|s| s.as_sym()) == Some("t")
}

/// Assembly diagnostic in the nested house shape the phase-8 plan
/// sketches: `(diagnostic ((severity s) (code c) (subject "...")
/// (message "...")))`, severity and code as bare symbols.
fn diagnostic(severity: &str, code: &str, subject: &str, message: &str) -> Sexpr {
    Sexpr::list(vec![
        Sexpr::sym("diagnostic"),
        Sexpr::list(vec![
            Sexpr::pair("severity", Sexpr::sym(severity)),
            Sexpr::pair("code", Sexpr::sym(code)),
            Sexpr::pair("subject", Sexpr::Str(subject.to_string())),
            Sexpr::pair("message", Sexpr::Str(message.to_string())),
        ]),
    ])
}

/// Collects every candidate file output as an `Artifact`, in result
/// order (mirrors `gymnast-collect-artifacts`, with one deliberate
/// delta: only SUCCEEDED results contribute — phase-8 gate, finding 3;
/// see the delta doc). A result whose status is not `Succeeded`, whose
/// `candidate` is `None`, whose candidate is not a well-formed
/// two-element `(candidate (...))` tagged form, or whose `files` field
/// is missing or empty contributes nothing — never an error here: the
/// firewall already ruled on candidates; assembly only collects. An
/// individual `files` entry that is not a `(string string)` pair is
/// likewise skipped. The digest is over the file CONTENT string alone
/// (never the `(path content)` pair); the size is the content's length
/// in bytes. The artifact's `node_id` is the RESULT's node-id, never
/// the candidate's own claim.
pub fn collect_artifacts(results: &[ExecutionResult]) -> Vec<Artifact> {
    let mut artifacts = Vec::new();
    for result in results {
        // Only SUCCEEDED results contribute artifacts (phase-8 gate,
        // finding 3; documented delta — the reference collects
        // blindly): a Failed result still carries its firewall-REJECTED
        // candidate for provenance, and recording rejected untrusted
        // output as a produced artifact would suppress the
        // missing-artifact warning for a path that was never written.
        if result.status != ExecutionStatus::Succeeded {
            continue;
        }
        let body = match candidate_body(result.candidate.as_ref()) {
            Some(body) => body,
            None => continue,
        };
        let files = match body.assoc("files").and_then(|f| f.as_list()) {
            Some(files) => files,
            None => continue,
        };
        for entry in files {
            let pair = match entry.as_list() {
                Some(pair) => pair,
                None => continue,
            };
            let (path, content) = match (
                pair.first().and_then(|p| p.as_str()),
                pair.get(1).and_then(|c| c.as_str()),
            ) {
                (Some(path), Some(content)) => (path, content),
                _ => continue,
            };
            artifacts.push(Artifact {
                path: path.to_string(),
                node_id: result.node_id.clone(),
                digest: fingerprint::fingerprint_string(content),
                size: content.len() as i64,
            });
        }
    }
    artifacts
}

/// The field-pairs body of a well-formed `(candidate (...))` tagged
/// form; `None` for anything else (the same shape rule
/// `candidate::candidate_shape` enforces).
fn candidate_body(candidate: Option<&Sexpr>) -> Option<&Sexpr> {
    let items = candidate?.as_list()?;
    if items.len() != 2 || items[0].as_sym() != Some("candidate") {
        return None;
    }
    items[1].as_list()?;
    Some(&items[1])
}

/// Validates artifacts against the plan's declared may-write paths
/// (mirrors `gymnast-validate-artifacts`): declared = the concatenation
/// of every plan node's `may_write` in plan-node order; actual = the
/// artifacts' paths in order. One error per actual path not in declared
/// (`untracked-artifact`), one warning per declared path not in actual
/// (`missing-artifact`). All untracked first (artifact order), then all
/// missing (declared order). Duplicates are NOT deduplicated (Lamedh
/// parity: `filter` over the raw lists).
pub fn validate_artifacts(plan: &Plan, artifacts: &[Artifact]) -> Vec<Sexpr> {
    let declared: Vec<&String> = plan.nodes.iter().flat_map(|n| &n.may_write).collect();
    let actual: Vec<&String> = artifacts.iter().map(|a| &a.path).collect();

    let mut diags = Vec::new();
    for path in &actual {
        if !declared.contains(path) {
            diags.push(diagnostic(
                "error",
                "untracked-artifact",
                path,
                "artifact not declared in any plan node",
            ));
        }
    }
    for path in &declared {
        if !actual.contains(path) {
            diags.push(diagnostic(
                "warning",
                "missing-artifact",
                path,
                "declared artifact not produced",
            ));
        }
    }
    diags
}

/// Capability edge validation (mirrors
/// `gymnast-validate-capability-edges`): one error per capability
/// OCCURRENCE that appears in the concatenated `capabilities` of all
/// nodes AND the concatenated `prohibitions` of all nodes (capability
/// order; no dedup).
pub fn validate_capability_edges(plan: &Plan) -> Vec<Sexpr> {
    let all_capabilities: Vec<&String> = plan.nodes.iter().flat_map(|n| &n.capabilities).collect();
    let all_prohibitions: Vec<&String> = plan.nodes.iter().flat_map(|n| &n.prohibitions).collect();
    all_capabilities
        .iter()
        .filter(|cap| all_prohibitions.contains(cap))
        .map(|cap| {
            diagnostic(
                "error",
                "prohibited-capability",
                cap,
                "capability is both used and prohibited",
            )
        })
        .collect()
}

/// One traceability entry for one IR node (mirrors
/// `gymnast-traceability-entry`): `plan_nodes` is the ids of the plan
/// nodes whose `inputs` contain the IR node's id, in plan order;
/// `has_evidence` is status-blind membership of some result's node-id
/// in that list.
pub fn traceability_entry(
    ir_node: &IrNode,
    plan: &Plan,
    results: &[ExecutionResult],
) -> TraceabilityEntry {
    let plan_nodes: Vec<String> = plan
        .nodes
        .iter()
        .filter(|n| n.inputs.contains(&ir_node.id))
        .map(|n| n.id.clone())
        .collect();
    let has_evidence = results.iter().any(|r| plan_nodes.contains(&r.node_id));
    TraceabilityEntry {
        semantic_id: ir_node.id.clone(),
        kind: ir_node.kind.clone(),
        has_implementation: !plan_nodes.is_empty(),
        has_evidence,
        plan_nodes,
    }
}

/// One entry per IR node, in `Ir::all_nodes()` order (every partition —
/// design, transitions, obligations, synthesis).
pub fn build_traceability_map(
    ir: &Ir,
    plan: &Plan,
    results: &[ExecutionResult],
) -> Vec<TraceabilityEntry> {
    ir.all_nodes()
        .into_iter()
        .map(|node| traceability_entry(node, plan, results))
        .collect()
}

/// One `unimplemented-semantic-node` warning per entry with no
/// implementation path. `has_evidence` does NOT diagnose here —
/// promotion checks it.
pub fn traceability_diagnostics(map: &[TraceabilityEntry]) -> Vec<Sexpr> {
    map.iter()
        .filter(|entry| !entry.has_implementation)
        .map(|entry| {
            diagnostic(
                "warning",
                "unimplemented-semantic-node",
                &entry.semantic_id,
                "semantic node has no implementation path",
            )
        })
        .collect()
}

/// Dependency lock: a snapshot of each plan node's recipe, model, and
/// contract fingerprint, in plan order (mirrors
/// `gymnast-dependency-lock`). `recipe` prints as a bare symbol (the
/// vocabulary-term convention `PlanNode::field_pairs` establishes);
/// `model` is the node's model sexpr verbatim.
pub fn dependency_lock(plan: &Plan) -> Sexpr {
    let node_locks: Vec<Sexpr> = plan.nodes.iter().map(node_lock).collect();
    Sexpr::list(vec![
        Sexpr::sym("dependency-lock"),
        Sexpr::list(vec![
            Sexpr::pair("plan-fingerprint", Sexpr::Str(plan.fingerprint.clone())),
            Sexpr::pair("node-locks", Sexpr::list(node_locks)),
        ]),
    ])
}

fn node_lock(node: &PlanNode) -> Sexpr {
    Sexpr::list(vec![
        Sexpr::sym("node-lock"),
        Sexpr::list(vec![
            Sexpr::pair("node-id", Sexpr::Str(node.id.clone())),
            Sexpr::pair("recipe", Sexpr::sym(&node.recipe)),
            Sexpr::pair("model", node.model.clone()),
            Sexpr::pair("fingerprint", Sexpr::Str(node.fingerprint.clone())),
        ]),
    ])
}

/// The default promotion policy. Its `requires` list is descriptive
/// policy metadata (Lamedh parity): `evaluate_promotion`'s computed
/// checks are the contract, not this list.
pub fn default_promotion_policy() -> Sexpr {
    let requires: Vec<Sexpr> = [
        "all-artifacts-present",
        "no-untracked-artifacts",
        "no-capability-violations",
        "all-nodes-succeeded",
        "verification-passed",
        "traceability-complete",
    ]
    .iter()
    .map(|name| Sexpr::pair(name, Sexpr::sym("t")))
    .collect();
    Sexpr::list(vec![
        Sexpr::sym("promotion-policy"),
        Sexpr::list(vec![
            Sexpr::pair("name", Sexpr::sym("default")),
            Sexpr::pair("requires", Sexpr::list(requires)),
        ]),
    ])
}

/// Evaluates a promotion policy over an assembled evidence bundle,
/// computing exactly SIX checks (the policy's `requires` list is
/// metadata — the computed checks are the contract):
///
/// | check | rule |
/// |---|---|
/// | `no-error-diagnostics` | bundle `diagnostics` contains no `(severity error)`, AND the nested verification section (when present) carries no error-severity diagnostic anywhere (`verify::bundle_error_diagnostics` — E601, source E3xx, ...; phase-8 gate, finding 5) |
/// | `all-artifacts-present` | bundle `diagnostics` contains no `missing-artifact` warning (phase-8 gate, finding 4: the advertised `requires` line now has a computed check; a build with declared-but-unproduced artifacts must not promote) |
/// | `all-nodes-succeeded` | summary `failed-nodes == 0` (deferred does NOT block) |
/// | `verification-passed` | no verification section, OR its summary has `total > 0` AND `failed == 0` (phase-8 gate, finding 4: a zero-obligation verification is no evidence at all) |
/// | `no-indeterminate-verification` | no verification section, OR summary `indeterminate == 0` (phase 7 made undecidable verdicts honest; promotion must not launder them) |
/// | `traceability-complete` | every traceability entry has both `has-implementation` and `has-evidence` |
///
/// `decision` is `promote` iff all six are `t`. Missing/malformed
/// bundle fields evaluate their check to `nil` (fail-closed), never a
/// panic. The one vacuous direction is deliberate: an explicit
/// `(verification nil)` pair — the assembled shape when no
/// verification bundle was supplied — passes both verification checks;
/// a bundle MISSING the pair entirely (not a shape `assemble_bundle`
/// ever emits) fails them closed, and a present section whose summary
/// cannot be read fails both.
///
/// SHADOW-PROOF READS (phase-8 gate, finding 2): every field this
/// function consults is read with `assoc_unique` — a key that appears
/// MORE than once in the bundle body (or inside the summary) makes the
/// affected reads fail and their checks evaluate `nil`. A first-wins
/// `assoc` would let one prepended `(verification nil)` pair shadow
/// the real section and flip `hold` to `promote`.
///
/// AUTHENTICITY is the caller's obligation, not this function's: the
/// promotion path inside `compile`/`synthesize` evaluates the bundle
/// it just assembled in-process. Any consumer reading a bundle back
/// from disk MUST call `verify_bundle_fingerprint` first — the
/// fingerprint is the tamper-evidence; this function checks structure,
/// not provenance (documented in `docs/ir-contract-deltas.md`).
pub fn evaluate_promotion(policy: &Sexpr, bundle: &Sexpr) -> Sexpr {
    let body = bundle_body(bundle);

    let own_diags_clean = match body.and_then(|b| assoc_unique(b, "diagnostics")) {
        Some(diags) => match diags.as_list() {
            Some(items) => !items.iter().any(is_error_diagnostic),
            None => false,
        },
        None => false,
    };
    // Fold the nested verification section's own error census in
    // (E601, source diagnostics, ...): an error the verification
    // bundle already carries must not read as "no error diagnostics"
    // one level up.
    let nested_verification_clean = match body.and_then(|b| assoc_unique(b, "verification")) {
        None => false, // body present but field missing/duplicated: fail closed
        Some(section) => {
            if section.as_list().is_some_and(|items| items.is_empty()) {
                true
            } else {
                crate::verify::bundle_error_diagnostics(section).is_empty()
            }
        }
    };
    let no_error_diagnostics = own_diags_clean && nested_verification_clean;

    let all_artifacts_present = match body.and_then(|b| assoc_unique(b, "diagnostics")) {
        Some(diags) => match diags.as_list() {
            Some(items) => !items.iter().any(is_missing_artifact_diagnostic),
            None => false,
        },
        None => false,
    };

    let all_succeeded = body
        .and_then(|b| assoc_unique(b, "summary"))
        .and_then(|s| assoc_unique(s, "failed-nodes"))
        .and_then(|n| n.as_int())
        == Some(0);

    let (verification_passed, no_indeterminate) = verification_checks(body);

    let traceability_complete = match body
        .and_then(|b| assoc_unique(b, "traceability"))
        .and_then(|t| t.as_list())
    {
        Some(entries) => entries.iter().all(entry_traced),
        None => false,
    };

    let checks = [
        ("no-error-diagnostics", no_error_diagnostics),
        ("all-artifacts-present", all_artifacts_present),
        ("all-nodes-succeeded", all_succeeded),
        ("verification-passed", verification_passed),
        ("no-indeterminate-verification", no_indeterminate),
        ("traceability-complete", traceability_complete),
    ];
    let all_pass = checks.iter().all(|(_, pass)| *pass);

    let policy_name = policy
        .as_list()
        .and_then(|items| items.get(1))
        .and_then(|body| body.assoc("name"))
        .cloned()
        .unwrap_or(Sexpr::List(vec![]));

    Sexpr::list(vec![
        Sexpr::sym("promotion-result"),
        Sexpr::list(vec![
            Sexpr::pair("policy", policy_name),
            Sexpr::pair(
                "decision",
                Sexpr::sym(if all_pass { "promote" } else { "hold" }),
            ),
            Sexpr::pair(
                "checks",
                Sexpr::list(
                    checks
                        .iter()
                        .map(|(name, pass)| Sexpr::pair(name, bool_sexpr(*pass)))
                        .collect(),
                ),
            ),
        ]),
    ])
}

/// The field-pairs body of a well-formed two-element
/// `(evidence-bundle (...))` form; `None` for anything else.
fn bundle_body(bundle: &Sexpr) -> Option<&Sexpr> {
    let items = bundle.as_list()?;
    if items.len() != 2 || items[0].as_sym() != Some("evidence-bundle") {
        return None;
    }
    items[1].as_list()?;
    Some(&items[1])
}

/// Fail-closed severity read: `true` when the diagnostic's severity is
/// `error` OR cannot be read at all (an unreadable diagnostic must
/// never pass for a clean one). Handles both the nested assembly shape
/// `(diagnostic ((severity s) ...))` and the flat `diag::diag_sexpr`
/// shape `(diagnostic (severity s) ...)`.
fn is_error_diagnostic(d: &Sexpr) -> bool {
    let severity = d
        .assoc("severity")
        .or_else(|| {
            d.as_list()
                .and_then(|items| items.get(1))
                .and_then(|body| body.assoc("severity"))
        })
        .and_then(|s| s.as_sym());
    match severity {
        Some(severity) => severity == "error",
        None => true,
    }
}

/// Alist lookup that REJECTS shadowing: the value of the pair whose
/// head symbol equals `key`, but only when exactly ONE such pair
/// exists. A duplicated key returns `None`, so every promotion check
/// reading through this fails closed instead of silently taking the
/// first (or any) occurrence (phase-8 gate, finding 2 — the same
/// parser-differential rule the strict runner readback enforces).
fn assoc_unique<'a>(form: &'a Sexpr, key: &str) -> Option<&'a Sexpr> {
    let items = form.as_list()?;
    let mut found: Option<&Sexpr> = None;
    for item in items {
        if let Sexpr::List(pair) = item {
            if pair.len() == 2 && pair[0].as_sym() == Some(key) {
                if found.is_some() {
                    return None; // duplicate: shadow attempt, fail closed
                }
                found = Some(&pair[1]);
            }
        }
    }
    found
}

/// `true` for a diagnostic whose `code` reads as `missing-artifact`
/// (either diagnostic shape, same lookup convention as
/// `is_error_diagnostic`). An unreadable code is NOT counted here —
/// `is_error_diagnostic` already fails the bundle closed on any
/// unreadable diagnostic, so this check stays specific.
fn is_missing_artifact_diagnostic(d: &Sexpr) -> bool {
    let code = d
        .assoc("code")
        .or_else(|| {
            d.as_list()
                .and_then(|items| items.get(1))
                .and_then(|body| body.assoc("code"))
        })
        .and_then(|s| s.as_sym());
    code == Some("missing-artifact")
}

/// Recomputes the bundle's fingerprint over its fingerprint-free form
/// and compares: `true` only for a well-formed bundle whose trailing
/// `fingerprint` field matches its own content. The tamper-evidence
/// check for any consumer reading a bundle back from an untrusted
/// medium (phase-8 gate, finding 2); the in-process
/// assemble-then-evaluate path does not need it.
pub fn verify_bundle_fingerprint(bundle: &Sexpr) -> bool {
    let items = match bundle.as_list() {
        Some(items) if items.len() == 2 && items[0].as_sym() == Some("evidence-bundle") => items,
        _ => return false,
    };
    let body = match items[1].as_list() {
        Some(body) => body,
        None => return false,
    };
    let claimed = match body.last().and_then(|pair| pair.as_list()) {
        Some(pair)
            if pair.len() == 2
                && pair[0].as_sym() == Some("fingerprint")
                && body
                    .iter()
                    .filter(|p| {
                        p.as_list().and_then(|x| x.first()).and_then(|s| s.as_sym())
                            == Some("fingerprint")
                    })
                    .count()
                    == 1 =>
        {
            match pair[1].as_str() {
                Some(s) => s.to_string(),
                None => return false,
            }
        }
        _ => return false,
    };
    let without = Sexpr::list(vec![
        Sexpr::sym("evidence-bundle"),
        Sexpr::List(body[..body.len() - 1].to_vec()),
    ]);
    fingerprint::fingerprint(&without) == claimed
}

/// The two verification checks, computed together over the bundle's
/// `verification` field: an absent field or a `nil` value is "no
/// verification section" (both vacuously true); a present section
/// whose summary cannot be read fails both (fail-closed); otherwise
/// `total > 0 && failed == 0` / `indeterminate == 0` respectively.
/// The `total > 0` condition is the phase-8 gate's finding 4: a
/// zero-obligation verification section is not evidence that anything
/// passed — without it, a spec with no obligations at all would
/// launder into `promote` exactly the way a fully-indeterminate one
/// would have before the `no-indeterminate-verification` check.
fn verification_checks(body: Option<&Sexpr>) -> (bool, bool) {
    let section = match body.map(|b| assoc_unique(b, "verification")) {
        Some(Some(v)) => v,
        Some(None) => return (false, false), // missing or duplicated: fail closed
        None => return (false, false),       // no readable bundle body at all
    };
    if section.as_list().is_some_and(|items| items.is_empty()) {
        return (true, true);
    }
    match crate::verify::bundle_summary(section) {
        Some(summary) => (
            summary.total > 0 && summary.failed == 0,
            summary.indeterminate == 0,
        ),
        None => (false, false),
    }
}

/// Fail-closed traceability-entry read: `true` only for a well-formed
/// two-element `(traceability-entry (...))` form whose
/// `has-implementation` and `has-evidence` both read as `t`.
fn entry_traced(entry: &Sexpr) -> bool {
    let body = match entry.as_list() {
        Some(items) if items.len() == 2 && items[0].as_sym() == Some("traceability-entry") => {
            &items[1]
        }
        _ => return false,
    };
    bool_of(body.assoc("has-implementation")) && bool_of(body.assoc("has-evidence"))
}

/// Assembles the evidence bundle (mirrors `gymnast-assemble-bundle`):
/// artifacts, traceability, dependency lock, the verification bundle
/// verbatim (or `nil`), a summary, and the concatenated diagnostics
/// (artifact ++ capability ++ traceability, in that order), plus a
/// trailing `fingerprint` over the fingerprint-free form (phase-7
/// pattern verbatim; a deliberate delta — the reference bundle has no
/// fingerprint).
///
/// `succeeded-nodes` counts results with status `succeeded` (the Rust
/// enum has no `passed`, so the reference's `passed` arm is dead here);
/// `failed-nodes` counts `failed`; `deferred` counts toward neither.
pub fn assemble_bundle(
    ir: &Ir,
    plan: &Plan,
    results: &[ExecutionResult],
    verification: Option<&Sexpr>,
) -> Sexpr {
    let without = assemble_bundle_without_fingerprint(ir, plan, results, verification);
    let fp = fingerprint::fingerprint(&without);
    match without {
        Sexpr::List(mut items) => {
            if let Some(Sexpr::List(body)) = items.get_mut(1) {
                body.push(Sexpr::pair("fingerprint", Sexpr::Str(fp)));
            }
            Sexpr::List(items)
        }
        // Unreachable: assemble_bundle_without_fingerprint always builds
        // a two-element list. Kept total regardless.
        other => other,
    }
}

/// The bundle WITHOUT its `fingerprint` field — the exact form the
/// fingerprint is computed over, so the two can never drift.
fn assemble_bundle_without_fingerprint(
    ir: &Ir,
    plan: &Plan,
    results: &[ExecutionResult],
    verification: Option<&Sexpr>,
) -> Sexpr {
    let artifacts = collect_artifacts(results);
    let artifact_diags = validate_artifacts(plan, &artifacts);
    let cap_diags = validate_capability_edges(plan);
    let traceability = build_traceability_map(ir, plan, results);
    let trace_diags = traceability_diagnostics(&traceability);
    let lock = dependency_lock(plan);

    let succeeded = results
        .iter()
        .filter(|r| r.status == ExecutionStatus::Succeeded)
        .count();
    let failed = results
        .iter()
        .filter(|r| r.status == ExecutionStatus::Failed)
        .count();

    let mut all_diags = artifact_diags;
    all_diags.extend(cap_diags);
    all_diags.extend(trace_diags);

    let summary = Sexpr::list(vec![
        Sexpr::pair("total-nodes", Sexpr::Int(plan.nodes.len() as i64)),
        Sexpr::pair("artifacts-produced", Sexpr::Int(artifacts.len() as i64)),
        Sexpr::pair("succeeded-nodes", Sexpr::Int(succeeded as i64)),
        Sexpr::pair("failed-nodes", Sexpr::Int(failed as i64)),
        Sexpr::pair("has-verification", bool_sexpr(verification.is_some())),
    ]);

    Sexpr::list(vec![
        Sexpr::sym("evidence-bundle"),
        Sexpr::list(vec![
            Sexpr::pair("schema", Sexpr::Str(BUNDLE_SCHEMA.to_string())),
            Sexpr::pair("ir-fingerprint", Sexpr::Str(ir.fingerprint.clone())),
            Sexpr::pair("plan-fingerprint", Sexpr::Str(plan.fingerprint.clone())),
            Sexpr::pair(
                "artifacts",
                Sexpr::list(artifacts.iter().map(Artifact::to_sexpr).collect()),
            ),
            Sexpr::pair(
                "traceability",
                Sexpr::list(
                    traceability
                        .iter()
                        .map(TraceabilityEntry::to_sexpr)
                        .collect(),
                ),
            ),
            Sexpr::pair("dependency-lock", lock),
            Sexpr::pair(
                "verification",
                verification.cloned().unwrap_or(Sexpr::List(vec![])),
            ),
            Sexpr::pair("summary", summary),
            Sexpr::pair("diagnostics", Sexpr::list(all_diags)),
        ]),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bool_sexpr_prints_t_and_nil() {
        assert_eq!(bool_sexpr(true).print(), "t");
        assert_eq!(bool_sexpr(false).print(), "nil");
    }

    #[test]
    fn test_bool_of_is_fail_closed() {
        assert!(bool_of(Some(&Sexpr::sym("t"))));
        assert!(!bool_of(Some(&Sexpr::List(vec![]))));
        assert!(!bool_of(Some(&Sexpr::Str("t".to_string()))));
        assert!(!bool_of(None));
    }

    #[test]
    fn test_is_error_diagnostic_fail_closed_on_unreadable() {
        // Nested assembly shape.
        assert!(is_error_diagnostic(&diagnostic("error", "c", "s", "m")));
        assert!(!is_error_diagnostic(&diagnostic("warning", "c", "s", "m")));
        // Flat diag_sexpr shape.
        let flat = crate::diag::diag_sexpr("error", "E999", (0, 0), "x".to_string());
        assert!(is_error_diagnostic(&flat));
        // Unreadable severity counts as an error.
        assert!(is_error_diagnostic(&Sexpr::sym("garbage")));
    }

    #[test]
    fn test_evaluate_promotion_total_on_garbage_inputs() {
        // Neither input is remotely bundle-shaped: every check fails
        // closed, the decision is hold, and nothing panics.
        // PHASE-8 GATE UPDATE (finding 4): a garbage bundle now fails
        // ALL SIX checks — the verification checks are vacuously true
        // only for an explicit `(verification nil)` pair in a readable
        // body, never for a bundle with no readable body at all.
        let result = evaluate_promotion(&Sexpr::sym("junk"), &Sexpr::Int(7));
        assert_eq!(
            result.print(),
            "(promotion-result ((policy nil) (decision hold) (checks \
             ((no-error-diagnostics nil) (all-artifacts-present nil) \
             (all-nodes-succeeded nil) (verification-passed nil) \
             (no-indeterminate-verification nil) (traceability-complete nil)))))"
        );
    }

    #[test]
    fn test_collect_artifacts_skips_malformed_file_entries() {
        use crate::recipe::{ExecutionResult, ExecutionStatus};
        let candidate = crate::sexpr::parse(
            r#"(candidate ((files (("ok.rb" "body") ("no-content") (17) not-a-pair))))"#,
        )
        .expect("candidate parses");
        let results = vec![ExecutionResult {
            node_id: "m/plan/a".to_string(),
            status: ExecutionStatus::Succeeded,
            candidate: Some(candidate),
            recipe_identity: None,
            diagnostics: vec![],
        }];
        let artifacts = collect_artifacts(&results);
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].path, "ok.rb");
        assert_eq!(artifacts[0].size, 4);
    }

    #[test]
    fn test_verification_checks_fail_closed_on_unreadable_summary() {
        // Present verification section whose summary is missing: both
        // checks fail closed.
        let body = crate::sexpr::parse(
            r#"((verification (verification-bundle ((schema "gymnast.verify/0.1")))))"#,
        )
        .expect("body parses");
        assert_eq!(verification_checks(Some(&body)), (false, false));
    }
}
