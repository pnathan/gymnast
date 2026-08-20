//! Tests-of-record for `adequacy.rs`, authored from
//! `docs/rust-port-plan-phase9.md` sections A-E and its "Oracle tests"
//! list (items 01-08) ALONE, BEFORE any implementation of
//! `crate::adequacy` exists (the committed-oracle process of phases
//! 4-8: Stage 1 commits this file to git red; implementation stages
//! MUST NOT edit it; only the integrator arbitrates a conflict).
//! `src/adequacy.lisp` was consulted only for BEHAVIORAL INTENT; every
//! Rust-IR shape adaptation comes from the phase-9 plan and
//! `docs/ir-contract-deltas.md`, never guessed from Lamedh output.
//!
//! THE ONE DELIBERATE SEMANTIC DELTA (plan, binding): detection is
//! BASELINE-AWARE. The reference counts a mutant killed when ANY
//! obligation is `failed` after mutation; against todo.gym that is
//! vacuous, because the baseline already has two `failed` obligations.
//! Here a mutant is killed iff some obligation is `failed` in the
//! mutated results AND was not `failed` in the baseline (a NEW failure,
//! including an obligation id that only exists post-mutation).
//! Obligations whose status moved to `indeterminate` from anything else
//! are reported as DEGRADED (visibility, not detection). The derived
//! consequence pinned throughout: all five standard todo.gym mutants
//! survive -- `(pass nil)` with five blind spots -- where the
//! reference's vacuous rule would have laundered the same facts into
//! `pass t`.
//!
//! BASELINE FACTS (derived from the CURRENT phase-8 binary, not from
//! the plan text alone -- phases 7-8 landed changes after the plan was
//! written). `cargo run -- verify ../examples/todo.gym` today is
//! byte-identical to the committed `tests/fixtures/todo-verify.sexpr`,
//! whose summary is (total 9) (passed 1) (failed 2) (skipped 4)
//! (indeterminate 2). Arithmetic: 9 obligations = 6 acceptance-lowered
//! (2 property + 1 scenario + 1 concurrency + 1 fault + 1 coverage)
//! + 2 invariant-checks + 1 constraint-check. Per-obligation statuses:
//!
//!   todo/acceptance/production/property/create_then_read      failed
//!   todo/acceptance/production/property/viewer_cannot_mutate  passed
//!   todo/acceptance/production/scenario/sharing_boundary      failed
//!   todo/acceptance/production/concurrency/boundary_race      skipped
//!   todo/acceptance/production/fault/durable_restart          skipped
//!   todo/acceptance/production/coverage                       skipped
//!   todo/invariant/owner_isolation/invariant-check       indeterminate
//!   todo/invariant/sharing_limit/invariant-check         indeterminate
//!   todo/constraint/collaborative_capacity/constraint-check   skipped
//!
//! So the baseline FAILED set is {create_then_read, sharing_boundary};
//! only a `failed` status outside that set (or under a new obligation
//! id) can kill a mutant.
//!
//! SHAPE CONVENTIONS (plan section D, pinned strictly): the
//! `campaign-result` root uses the NESTED house convention
//! `(tag ((k v) ...))` with the fingerprint over the fingerprint-free
//! form appended last; `mutant-result`, `blind-spot`,
//! `interleaving-scenario`, and `fault-scenario` forms are FLAT
//! `(tag (k v) ...)` -- the phase-6 record-projection convention split,
//! already documented in `docs/ir-contract-deltas.md`. Ids
//! (`mutant-id`, obligation ids in `detecting-obligations` /
//! `degraded-obligations`) are `Sexpr::Str`, matching every other
//! semantic id in this crate; `class` and other enum-ish tags are
//! symbols; booleans use the t/nil convention (`Sexpr::sym("t")` /
//! `Sexpr::List(vec![])`) since `Sexpr` has no Bool variant.
//!
//! RESOLVED API READINGS (the plan's signatures, made concrete where it
//! leaves latitude; each is also called out at its use site):
//!  1. `replace_limit` must be `pub` (oracle item 02 tests it
//!     directly); signature read as
//!     `replace_limit(predicate: &Sexpr, new_limit: i64) -> Sexpr`,
//!     the direct transliteration of the reference's
//!     `(gymnast-replace-limit predicate new-limit)`.
//!  2. `run_mutant`'s `baseline: &[Sexpr]` is the slice of baseline
//!     VERIFICATION-RESULT forms (the plan: "run verification over the
//!     BASELINE IR once" then compare statuses) -- computed here as
//!     `verify_obligation` over `lower_all_obligations`, the same
//!     baseline `run_campaign` computes internally.
//!  3. `apply_mutation` does NOT re-fingerprint the mutated IR (plan
//!     section B, explicit): the `Ir.fingerprint` field of the mutated
//!     value still carries the ORIGINAL fingerprint. Pinned in item 01
//!     -- the mutated IR is a transient verification input, never
//!     serialized.
//!  4. `boundary_interleaving`'s scenario embeds the operation as a
//!     bare symbol (operations print as symbols everywhere else in the
//!     crate, e.g. `(operation query_tasks)` in trace violations) and
//!     the generated actor/input as STRINGS ("actor-N"/"input-N"), the
//!     reference's `concat` output.

use gymnast_rs::adequacy::{
    apply_mutation, boundary_interleaving, replace_limit, run_campaign, run_mutant,
    standard_fault_scenarios, standard_todo_mutants, Mutant, Mutation, ADEQUACY_SCHEMA,
};
use gymnast_rs::elaborate;
use gymnast_rs::fingerprint;
use gymnast_rs::ir::{Ir, IrNode};
use gymnast_rs::parser;
use gymnast_rs::sexpr::{self, canonical_serialize, Sexpr};
use gymnast_rs::verify::{lower_all_obligations, verify_obligation};
use std::fs;
use std::process::Command;

// ---------------------------------------------------------------------
// Shared fixtures / helpers (not tests themselves).
// ---------------------------------------------------------------------

fn load_todo_ir() -> Ir {
    let src = fs::read_to_string("../examples/todo.gym").expect("read ../examples/todo.gym");
    let (ast, parse_diags) = parser::parse(&src);
    let file = ast.expect("parse todo.gym");
    let (ir, _all_diags) = elaborate::elaborate_with_parse_diags(&file, &parse_diags);
    ir
}

fn ps(text: &str) -> Sexpr {
    sexpr::parse(text).unwrap_or_else(|e| panic!("parse {:?}: {}", text, e))
}

fn nil() -> Sexpr {
    Sexpr::List(vec![])
}

fn truthy() -> Sexpr {
    Sexpr::sym("t")
}

/// The baseline verification results `run_mutant` compares against
/// (resolved API reading 2 in the file header).
fn baseline_results(ir: &Ir) -> Vec<Sexpr> {
    lower_all_obligations(ir)
        .iter()
        .map(|o| verify_obligation(ir, o))
        .collect()
}

/// Field lookup on a FLAT form: `(tag (k v) (k v) ...)`.
fn flat_field<'a>(v: &'a Sexpr, key: &str) -> &'a Sexpr {
    v.assoc(key)
        .unwrap_or_else(|| panic!("no flat field {:?} in {}", key, v.print()))
}

/// Field lookup on the NESTED campaign root: `(tag ((k v) ...))`.
fn nested_field<'a>(v: &'a Sexpr, key: &str) -> &'a Sexpr {
    v.as_list()
        .and_then(|items| items.get(1))
        .and_then(|inner| inner.assoc(key))
        .unwrap_or_else(|| panic!("no nested field {:?} in {}", key, v.print()))
}

/// The key sequence of a pair list, for pinning field ORDER (the plan
/// spells the campaign-result and mutant-result field orders out
/// literally, so they are pinned, not just the field VALUES).
fn pair_keys(pairs: &[Sexpr]) -> Vec<String> {
    pairs
        .iter()
        .filter_map(|p| p.as_list())
        .filter(|p| p.len() == 2)
        .filter_map(|p| p[0].as_sym().map(|s| s.to_string()))
        .collect()
}

fn head_sym(v: &Sexpr) -> &str {
    v.as_list()
        .and_then(|items| items.first())
        .and_then(|h| h.as_sym())
        .unwrap_or_else(|| panic!("no head symbol in {}", v.print()))
}

/// Clause head symbols of an IR node, for pinning which clauses a
/// mutation dropped (clause order is preserved by the IR contract).
fn clause_heads(node: &IrNode) -> Vec<String> {
    node.clauses
        .iter()
        .map(|c| head_sym(c).to_string())
        .collect()
}

// ---------------------------------------------------------------------
// Synthetic IRs for the baseline-aware detection pins (item 05).
// ---------------------------------------------------------------------

/// An IR whose one invariant `(< count 10)` is `passed` at baseline:
/// initial state binds `count` to Int 0 (0 < 10 -> Holds, grounded),
/// and the only behavior writes ELSEWHERE (`log`), so the
/// post-transition check point is also 0 < 10 -> Holds. Under
/// `WeakenLimit { new_limit: -1 }` the predicate becomes
/// `(< count -1)`: 0 < -1 is a grounded Fails at the INITIAL state, so
/// the obligation flips passed -> failed -- a NEW failure, proving the
/// detection path has teeth (plan section D's synthetic killed case).
fn synthetic_killed_ir() -> Ir {
    let state = IrNode::new(
        "syn/state/count".to_string(),
        "state",
        "count".to_string(),
        vec![(":initial".to_string(), Sexpr::Int(0))],
        vec![],
    );
    let behavior = IrNode::new(
        "syn/behavior/log_write".to_string(),
        "behavior",
        "log_write".to_string(),
        vec![
            (":on".to_string(), ps("(svc/log_write actor input)")),
            (":writes".to_string(), ps("(log)")),
        ],
        vec![],
    );
    let invariant = IrNode::new(
        "syn/invariant/cap".to_string(),
        "invariant",
        "cap".to_string(),
        vec![
            (":always".to_string(), ps("(< count 10)")),
            (":scope".to_string(), Sexpr::sym("count")),
        ],
        vec![],
    );
    Ir::new(
        "gymnast.ir/0.1".to_string(),
        "syn".to_string(),
        vec![],
        vec![state],
        vec![behavior],
        vec![invariant],
        vec![],
        vec![],
    )
}

/// An IR whose one invariant `(< count 10)` is `passed` at baseline but
/// goes `indeterminate` (NOT failed) under `WeakenPrecondition`:
///
///  - baseline: behavior `bump` has `requires (= 1 2)` -- a grounded
///    Fails (both sides resolved Ints, 1 != 2) -- so applying it yields
///    `(precondition-failed)` and post = pre; both check points see
///    count = 0, 0 < 10 -> Holds -> `passed` (basis checked).
///  - mutated: the requires clause is dropped, `bump` now succeeds and
///    appends its (nil) input to its `:writes` entry `count`, so the
///    post-transition state binds `count` to the LIST (nil); `<` over a
///    non-Int is Verdict::Unknown -> status `indeterminate`.
///
/// passed -> indeterminate is a DEGRADATION (an undecidable verdict
/// detects nothing), never a kill -- the delta the plan adds over the
/// reference.
fn synthetic_degraded_ir() -> Ir {
    let state = IrNode::new(
        "syn/state/count".to_string(),
        "state",
        "count".to_string(),
        vec![(":initial".to_string(), Sexpr::Int(0))],
        vec![],
    );
    let behavior = IrNode::new(
        "syn/behavior/bump".to_string(),
        "behavior",
        "bump".to_string(),
        vec![
            (":on".to_string(), ps("(svc/bump actor input)")),
            (":writes".to_string(), ps("(count)")),
        ],
        vec![ps("(requires (= 1 2))")],
    );
    let invariant = IrNode::new(
        "syn/invariant/cap".to_string(),
        "invariant",
        "cap".to_string(),
        vec![
            (":always".to_string(), ps("(< count 10)")),
            (":scope".to_string(), Sexpr::sym("count")),
        ],
        vec![],
    );
    Ir::new(
        "gymnast.ir/0.1".to_string(),
        "syn".to_string(),
        vec![],
        vec![state],
        vec![behavior],
        vec![invariant],
        vec![],
        vec![],
    )
}

/// An IR with one behavior and NO `:writes` field at all -- no writing
/// transitions, so `boundary_interleaving` has nothing to build a
/// scenario over.
fn synthetic_no_writes_ir() -> Ir {
    let behavior = IrNode::new(
        "syn/behavior/peek".to_string(),
        "behavior",
        "peek".to_string(),
        vec![(":on".to_string(), ps("(svc/peek actor input)"))],
        vec![],
    );
    Ir::new(
        "gymnast.ir/0.1".to_string(),
        "syn".to_string(),
        vec![],
        vec![],
        vec![behavior],
        vec![],
        vec![],
        vec![],
    )
}

// =======================================================================
// 01. apply_mutation per operator over the todo IR: node-level effects
//     (clauses dropped, node removed, limit rewritten inside forall,
//     writes emptied), untouched nodes byte-identical, missing targets
//     total.
// =======================================================================

#[test]
fn oracle_01a_weaken_precondition_drops_requires_clauses_only() {
    let ir = load_todo_ir();
    let mutated = apply_mutation(
        &ir,
        &Mutation::WeakenPrecondition {
            behavior_name: "create_task".to_string(),
        },
    );

    // todo/behavior/create_task's committed clause list (todo-ir.sexpr,
    // order-preserved): requires, requires, ensures, returns, fails,
    // emits. Filtering head == requires leaves 4 clauses.
    let node = mutated
        .find_node("todo/behavior/create_task")
        .expect("mutated IR keeps the node under its id");
    assert_eq!(
        clause_heads(node),
        vec!["ensures", "returns", "fails", "emits"]
    );
    let original = ir.find_node("todo/behavior/create_task").unwrap();
    let expected: Vec<Sexpr> = original
        .clauses
        .iter()
        .filter(|c| head_sym(c) != "requires")
        .cloned()
        .collect();
    assert_eq!(node.clauses, expected);

    // Everything except the clause list is untouched on the target...
    assert_eq!(node.id, original.id);
    assert_eq!(node.kind, original.kind);
    assert_eq!(node.name, original.name);
    assert_eq!(node.fields, original.fields);
    assert_eq!(node.mechanism, original.mechanism);

    // ...and every OTHER node is byte-identical: only the transitions
    // partition (where behavior nodes live) may differ, and there only
    // in the one target node.
    assert_eq!(mutated.design, ir.design);
    assert_eq!(mutated.obligations, ir.obligations);
    assert_eq!(mutated.synthesis, ir.synthesis);
    assert_eq!(
        mutated.find_node("todo/behavior/invite_user"),
        ir.find_node("todo/behavior/invite_user")
    );

    // Resolved API reading 3: the mutated IR is NEVER re-fingerprinted
    // (plan section B) -- it is a transient verification input, never
    // serialized, so the field still carries the original value.
    assert_eq!(mutated.fingerprint, ir.fingerprint);
}

#[test]
fn oracle_01b_remove_invariant_drops_the_node_from_every_partition() {
    let ir = load_todo_ir();
    let mutated = apply_mutation(
        &ir,
        &Mutation::RemoveInvariant {
            invariant_name: "sharing_limit".to_string(),
        },
    );

    assert!(mutated.find_node("todo/invariant/sharing_limit").is_none());
    // Invariants live in the obligations partition (elaborate.rs's
    // partitioning); exactly one node disappears there and nothing else
    // moves.
    assert_eq!(mutated.obligations.len(), ir.obligations.len() - 1);
    let expected: Vec<IrNode> = ir
        .obligations
        .iter()
        .filter(|n| n.id != "todo/invariant/sharing_limit")
        .cloned()
        .collect();
    assert_eq!(mutated.obligations, expected);
    assert!(mutated
        .find_node("todo/invariant/owner_isolation")
        .is_some());
    assert_eq!(mutated.design, ir.design);
    assert_eq!(mutated.transitions, ir.transitions);
    assert_eq!(mutated.synthesis, ir.synthesis);
    assert_eq!(mutated.fingerprint, ir.fingerprint);
}

#[test]
fn oracle_01c_weaken_limit_rewrites_the_limit_inside_forall() {
    let ir = load_todo_ir();
    let mutated = apply_mutation(
        &ir,
        &Mutation::WeakenLimit {
            invariant_name: "sharing_limit".to_string(),
            new_limit: 512,
        },
    );

    // :always is (forall ((list TodoList)) (<= (other_principal_count
    // list) 256)); replace_limit recurses through the forall head into
    // the body, whose third position is Int 256 -> rewritten to 512.
    let node = mutated
        .find_node("todo/invariant/sharing_limit")
        .expect("mutated IR keeps the node");
    assert_eq!(
        node.field(":always"),
        Some(&ps(
            "(forall ((list TodoList)) (<= (other_principal_count list) 512))"
        ))
    );
    // Every other field/attribute of the node is untouched.
    let original = ir.find_node("todo/invariant/sharing_limit").unwrap();
    assert_eq!(node.field(":scope"), original.field(":scope"));
    assert_eq!(node.id, original.id);
    assert_eq!(node.kind, "invariant");
    assert_eq!(node.name, original.name);
    assert_eq!(node.clauses, original.clauses);
    // Untouched partitions byte-identical.
    assert_eq!(mutated.design, ir.design);
    assert_eq!(mutated.transitions, ir.transitions);
    assert_eq!(mutated.synthesis, ir.synthesis);
    assert_eq!(mutated.fingerprint, ir.fingerprint);
}

#[test]
fn oracle_01d_remove_failure_mode_drops_fails_clauses_only() {
    let ir = load_todo_ir();
    let mutated = apply_mutation(
        &ir,
        &Mutation::RemoveFailureMode {
            behavior_name: "invite_user".to_string(),
        },
    );

    // todo/behavior/invite_user's committed clause list: requires,
    // requires, ensures, fails. Filtering head == fails leaves 3.
    let node = mutated
        .find_node("todo/behavior/invite_user")
        .expect("mutated IR keeps the node");
    assert_eq!(clause_heads(node), vec!["requires", "requires", "ensures"]);
    let original = ir.find_node("todo/behavior/invite_user").unwrap();
    let expected: Vec<Sexpr> = original
        .clauses
        .iter()
        .filter(|c| head_sym(c) != "fails")
        .cloned()
        .collect();
    assert_eq!(node.clauses, expected);
    assert_eq!(node.fields, original.fields);
    assert_eq!(
        mutated.find_node("todo/behavior/create_task"),
        ir.find_node("todo/behavior/create_task")
    );
    assert_eq!(mutated.design, ir.design);
    assert_eq!(mutated.obligations, ir.obligations);
    assert_eq!(mutated.fingerprint, ir.fingerprint);
}

#[test]
fn oracle_01e_skip_state_write_empties_the_writes_field() {
    let ir = load_todo_ir();
    let mutated = apply_mutation(
        &ir,
        &Mutation::SkipStateWrite {
            behavior_name: "create_task".to_string(),
        },
    );

    // :writes (tasks) -> the empty list (nil), everything else intact.
    let node = mutated
        .find_node("todo/behavior/create_task")
        .expect("mutated IR keeps the node");
    assert_eq!(node.field(":writes"), Some(&nil()));
    let original = ir.find_node("todo/behavior/create_task").unwrap();
    assert_eq!(node.field(":reads"), original.field(":reads"));
    assert_eq!(node.field(":on"), original.field(":on"));
    assert_eq!(node.field(":atomic"), original.field(":atomic"));
    assert_eq!(node.field(":idempotency"), original.field(":idempotency"));
    assert_eq!(node.clauses, original.clauses);
    assert_eq!(
        mutated.find_node("todo/behavior/invite_user"),
        ir.find_node("todo/behavior/invite_user")
    );
    assert_eq!(mutated.design, ir.design);
    assert_eq!(mutated.obligations, ir.obligations);
    assert_eq!(mutated.fingerprint, ir.fingerprint);
}

#[test]
fn oracle_01f_missing_targets_leave_the_ir_unchanged_never_panic() {
    // Plan edge table: a mutant naming a missing behavior/invariant
    // returns the IR unchanged (first-match-only targeting found
    // nothing) -- total, never a panic. Includes the "RemoveInvariant
    // of a name shared by zero nodes" row.
    let ir = load_todo_ir();
    let missing: Vec<Mutation> = vec![
        Mutation::WeakenPrecondition {
            behavior_name: "no_such_behavior".to_string(),
        },
        Mutation::RemoveInvariant {
            invariant_name: "no_such_invariant".to_string(),
        },
        Mutation::WeakenLimit {
            invariant_name: "no_such_invariant".to_string(),
            new_limit: 5,
        },
        Mutation::RemoveFailureMode {
            behavior_name: "no_such_behavior".to_string(),
        },
        Mutation::SkipStateWrite {
            behavior_name: "no_such_behavior".to_string(),
        },
    ];
    for mutation in &missing {
        assert_eq!(
            apply_mutation(&ir, mutation),
            ir,
            "missing target must leave the IR unchanged: {:?}",
            mutation
        );
    }
    // Kind-scoped matching: "create_task" names a BEHAVIOR, so an
    // invariant-targeting mutation naming it finds no target either.
    assert_eq!(
        apply_mutation(
            &ir,
            &Mutation::RemoveInvariant {
                invariant_name: "create_task".to_string(),
            }
        ),
        ir
    );
}

// =======================================================================
// 02. replace_limit table: all six rows, including forall recursion and
//     the fallthrough.
// =======================================================================

#[test]
fn oracle_02_replace_limit_table() {
    // Row 1: nil predicate -> unchanged (the reference's (null p) arm).
    assert_eq!(replace_limit(&nil(), 99), nil());
    // Row 2: atom predicate -> unchanged (the atom arm).
    assert_eq!(
        replace_limit(&Sexpr::sym("always_safe"), 99),
        Sexpr::sym("always_safe")
    );
    // Row 3: (<= a N) with an Int in third position -> new limit.
    assert_eq!(
        replace_limit(&ps("(<= (other_principal_count list) 256)"), 512),
        ps("(<= (other_principal_count list) 512)")
    );
    // Row 4: (< a N) with an Int in third position -> new limit.
    assert_eq!(replace_limit(&ps("(< count 10)"), -1), ps("(< count -1)"));
    // Row 5: (forall binders body) recurses into the body -- binders
    // untouched, only the body's limit rewritten.
    assert_eq!(
        replace_limit(
            &ps("(forall ((list TodoList)) (<= (other_principal_count list) 256))"),
            512
        ),
        ps("(forall ((list TodoList)) (<= (other_principal_count list) 512))")
    );
    // Row 6 (fallthrough): anything else unchanged -- an unrecognized
    // head, and a comparison whose third position is NOT an Int (the
    // reference's numberp guard).
    assert_eq!(
        replace_limit(&ps("(and (< a 5) (< b 5))"), 99),
        ps("(and (< a 5) (< b 5))")
    );
    assert_eq!(replace_limit(&ps("(<= a b)"), 99), ps("(<= a b)"));
}

// =======================================================================
// 03. boundary_interleaving over todo + the two edge rows.
// =======================================================================

#[test]
fn oracle_03a_boundary_interleaving_over_todo() {
    // The FIRST transition with a non-empty write set: behavior nodes
    // are extracted in id order, and "todo/behavior/create_task" <
    // "todo/behavior/invite_user" (byte order, 'c' < 'i'); create_task's
    // :writes is (tasks), non-empty, so it is chosen and invite_user is
    // never consulted. Its operation (first :on element) is
    // todo_service/create_task. Steps count DOWN from boundary_count to
    // 1 -- the reference's recursion order (cons N before recursing on
    // N-1) -- with "actor-N"/"input-N" as strings.
    let ir = load_todo_ir();
    let scenario = boundary_interleaving(&ir, 3).expect("todo has a writing transition");
    assert_eq!(
        scenario,
        ps(
            "(interleaving-scenario (operation todo_service/create_task) (boundary 3) \
            (steps ((todo_service/create_task \"actor-3\" \"input-3\") \
                    (todo_service/create_task \"actor-2\" \"input-2\") \
                    (todo_service/create_task \"actor-1\" \"input-1\"))) \
            (expected-violations 0))"
        )
    );
    // Flat-form field order pinned: operation, boundary, steps,
    // expected-violations.
    let items = scenario.as_list().unwrap();
    assert_eq!(items[0].as_sym(), Some("interleaving-scenario"));
    assert_eq!(
        pair_keys(&items[1..]),
        vec!["operation", "boundary", "steps", "expected-violations"]
    );
}

#[test]
fn oracle_03b_boundary_interleaving_none_without_writing_transitions() {
    // Plan edge table: an IR with no writing transitions -> None. The
    // synthetic IR's one behavior has no :writes field at all, so its
    // extracted write set is empty.
    let ir = synthetic_no_writes_ir();
    assert_eq!(boundary_interleaving(&ir, 3), None);
}

#[test]
fn oracle_03c_boundary_count_zero_or_negative_yields_empty_steps() {
    // Plan edge table: boundary_count <= 0 -> Some scenario with EMPTY
    // steps (the reference recursion's base case), the boundary echoed
    // verbatim.
    let ir = load_todo_ir();
    assert_eq!(
        boundary_interleaving(&ir, 0),
        Some(ps(
            "(interleaving-scenario (operation todo_service/create_task) (boundary 0) \
             (steps nil) (expected-violations 0))"
        ))
    );
    assert_eq!(
        boundary_interleaving(&ir, -3),
        Some(ps(
            "(interleaving-scenario (operation todo_service/create_task) (boundary -3) \
             (steps nil) (expected-violations 0))"
        ))
    );
}

// =======================================================================
// 04. standard_fault_scenarios: the exact four forms.
// =======================================================================

#[test]
fn oracle_04_standard_fault_scenarios_exact_four_forms() {
    // Reference parity, data only (plan section C): four flat
    // (fault-scenario ...) forms in the reference's order, every
    // `expected` fixed to `detected` by the constructor.
    assert_eq!(
        standard_fault_scenarios(),
        vec![
            ps("(fault-scenario (name restart-after-write) (type restart) \
                (after acknowledged-write) (expected detected))"),
            ps(
                "(fault-scenario (name timeout-mid-operation) (type timeout) \
                (after operation-start) (expected detected))"
            ),
            ps(
                "(fault-scenario (name duplicate-delivery) (type duplicate-delivery) \
                (after acknowledged-write) (expected detected))"
            ),
            ps("(fault-scenario (name stale-version) (type stale-version) \
                (after read) (expected detected))"),
        ]
    );
}

// =======================================================================
// 05. run_mutant baseline-aware semantics: a synthetic killed case and
//     a synthetic degraded case.
// =======================================================================

#[test]
fn oracle_05a_synthetic_killed_case_detects_with_exact_obligation_id() {
    // See synthetic_killed_ir's doc comment for the full derivation:
    // baseline `syn/invariant/cap/invariant-check` is `passed` (0 < 10
    // Holds at both check points, grounded), the mutated predicate
    // (< count -1) is a grounded Fails at the initial state -> `failed`.
    // failed AND not-failed-in-baseline = a NEW failure -> killed, with
    // exactly that obligation id detecting.
    let ir = synthetic_killed_ir();
    let baseline = baseline_results(&ir);

    // Baseline sanity (this is the CURRENT phase-8 verify semantics the
    // pin is derived from): exactly one obligation, status passed.
    assert_eq!(baseline.len(), 1);
    assert_eq!(
        baseline[0].assoc("status").and_then(|s| s.as_sym()),
        Some("passed")
    );

    let mutant = Mutant {
        id: "syn-k1".to_string(),
        class: "weaken-limit".to_string(),
        description: "weaken cap limit to -1".to_string(),
        mutation: Mutation::WeakenLimit {
            invariant_name: "cap".to_string(),
            new_limit: -1,
        },
        critical: true,
    };
    let result = run_mutant(&ir, &baseline, &mutant);
    assert_eq!(result.mutant_id, "syn-k1");
    assert_eq!(result.class, "weaken-limit");
    assert!(result.critical);
    assert!(result.killed, "a NEW failure must kill the mutant");
    assert_eq!(
        result.detecting_obligations,
        vec!["syn/invariant/cap/invariant-check".to_string()]
    );
    assert_eq!(result.degraded_obligations, Vec::<String>::new());
    assert_eq!(result.description, "weaken cap limit to -1");
}

#[test]
fn oracle_05b_synthetic_degraded_case_is_degraded_not_killed() {
    // See synthetic_degraded_ir's doc comment for the full derivation:
    // baseline `passed` (grounded precondition failure keeps count at
    // Int 0), mutated `indeterminate` (the now-unguarded write turns
    // count into a list, `<` over a non-Int is Unknown). passed ->
    // indeterminate is a DEGRADATION -- an undecidable verdict detects
    // nothing -- so the mutant is NOT killed (the plan's delta over the
    // reference's any-failure rule).
    let ir = synthetic_degraded_ir();
    let baseline = baseline_results(&ir);
    assert_eq!(baseline.len(), 1);
    assert_eq!(
        baseline[0].assoc("status").and_then(|s| s.as_sym()),
        Some("passed")
    );

    let mutant = Mutant {
        id: "syn-d1".to_string(),
        class: "weaken-precondition".to_string(),
        description: "drop bump's requires clauses".to_string(),
        mutation: Mutation::WeakenPrecondition {
            behavior_name: "bump".to_string(),
        },
        critical: true,
    };
    let result = run_mutant(&ir, &baseline, &mutant);
    assert!(
        !result.killed,
        "a status move to indeterminate is visibility loss, not detection"
    );
    assert_eq!(result.detecting_obligations, Vec::<String>::new());
    assert_eq!(
        result.degraded_obligations,
        vec!["syn/invariant/cap/invariant-check".to_string()]
    );
}

// =======================================================================
// 06. The five standard todo mutants: structure, then each SURVIVES
//     with empty detecting-obligations under the baseline-aware rule.
// =======================================================================

#[test]
fn oracle_06a_standard_todo_mutants_structure() {
    let mutants = standard_todo_mutants();
    assert_eq!(mutants.len(), 5);

    let expected_ids = ["m1", "m2", "m3", "m4", "m5"];
    let expected_classes = [
        "weaken-precondition",
        "remove-invariant",
        "weaken-limit",
        "remove-failure-mode",
        "skip-state-write",
    ];
    for (i, m) in mutants.iter().enumerate() {
        assert_eq!(m.id, expected_ids[i]);
        assert_eq!(m.class, expected_classes[i]);
        // The constructor sets critical, matching the reference's
        // `gymnast-mutant` (plan section A).
        assert!(m.critical, "mutant {} must be critical", m.id);
        assert!(
            !m.description.is_empty(),
            "mutant {} needs a description",
            m.id
        );
    }

    // The mutation payloads, matched structurally (no PartialEq
    // assumed on Mutation).
    match &mutants[0].mutation {
        Mutation::WeakenPrecondition { behavior_name } => {
            assert_eq!(behavior_name, "create_task")
        }
        other => panic!("m1 must weaken create_task's precondition, got {:?}", other),
    }
    match &mutants[1].mutation {
        Mutation::RemoveInvariant { invariant_name } => {
            assert_eq!(invariant_name, "sharing_limit")
        }
        other => panic!("m2 must remove sharing_limit, got {:?}", other),
    }
    match &mutants[2].mutation {
        Mutation::WeakenLimit {
            invariant_name,
            new_limit,
        } => {
            assert_eq!(invariant_name, "sharing_limit");
            assert_eq!(*new_limit, 512);
        }
        other => panic!("m3 must weaken sharing_limit to 512, got {:?}", other),
    }
    match &mutants[3].mutation {
        Mutation::RemoveFailureMode { behavior_name } => {
            assert_eq!(behavior_name, "invite_user")
        }
        other => panic!(
            "m4 must remove invite_user's failure modes, got {:?}",
            other
        ),
    }
    match &mutants[4].mutation {
        Mutation::SkipStateWrite { behavior_name } => {
            assert_eq!(behavior_name, "create_task")
        }
        other => panic!("m5 must skip create_task's state write, got {:?}", other),
    }
}

#[test]
fn oracle_06b_all_five_standard_todo_mutants_survive() {
    // Derivations, one per mutant, against the CURRENT phase-7/8
    // verification semantics (baseline statuses in the file header;
    // baseline failed set = {create_then_read, sharing_boundary}):
    //
    // m1 WeakenPrecondition create_task: both requires clauses --
    //    (authenticated user) and (may_edit_list pre user request/list)
    //    -- are unrecognized-head calls, so eval_predicate holds them
    //    SYMBOLICALLY at every baseline evaluation; dropping them
    //    leaves preconditions vacuously holding. Every trace step that
    //    reached create_task already had preconditions-hold = true, so
    //    every outcome, violation, and status is identical to baseline
    //    (the fails-guard still evaluates, so steps even stay
    //    symbolic). No new failure, nothing moves to indeterminate.
    //
    // m2 RemoveInvariant sharing_limit: the lowered obligation
    //    todo/invariant/sharing_limit/invariant-check simply VANISHES
    //    from the mutated run. It was `indeterminate` at baseline, not
    //    `failed` -- and a REMOVED indeterminate is neither a new
    //    failure nor a degradation. In-trace invariant checks: its
    //    forall predicate was Verdict::Unknown at every state (never a
    //    violation), so removing it changes no other obligation's
    //    status either.
    //
    // m3 WeakenLimit sharing_limit 512: the predicate becomes (forall
    //    ((list TodoList)) (<= (other_principal_count list) 512)) --
    //    still forall-headed, which the closed evaluator maps to
    //    Verdict::Unknown REGARDLESS of the limit -> the invariant
    //    check is `indeterminate` before and after (decided at the
    //    initial-state check point, transitions never consulted). No
    //    status changes anywhere.
    //
    // m4 RemoveFailureMode invite_user: the only steps that could
    //    exercise invite_user's fails clause are sharing_boundary's
    //    (invite_distinct owner 256/257). Under the phase-7 suffix
    //    rule the candidate operations are todo_service/create_task
    //    and todo_service/invite; neither equals invite_distinct nor
    //    ends with "/invite_distinct", so both steps were
    //    no-matching-transition at BASELINE (sharing_boundary already
    //    `failed`) and still are after the mutation (matching consults
    //    :on, which this mutation does not touch). An already-failed
    //    obligation failing again is not a NEW failure.
    //
    // m5 SkipStateWrite create_task: viewer_cannot_mutate's single
    //    create_task step still takes the (succeeded) path (fails-guard
    //    false, preconditions hold symbolically); post = pre now, but
    //    no violation ever depended on the tasks append (both
    //    invariants are Unknown at every state), so it still `passed`.
    //    create_then_read still fails ONLY on query_tasks'
    //    no-matching-transition (already failed at baseline). Both
    //    invariant obligations go indeterminate at the INITIAL state
    //    check point, before any transition (and hence any write set)
    //    is consulted -- unchanged.
    //
    // -> all five survived, detecting-obligations and
    //    degraded-obligations empty for each.
    let ir = load_todo_ir();
    let baseline = baseline_results(&ir);
    // Baseline arithmetic check (file header): 9 results, failed set of
    // size 2.
    assert_eq!(baseline.len(), 9);
    let baseline_failed: Vec<String> = baseline
        .iter()
        .filter(|r| r.assoc("status").and_then(|s| s.as_sym()) == Some("failed"))
        .filter_map(|r| r.assoc("obligation-id").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .collect();
    assert_eq!(
        baseline_failed,
        vec![
            "todo/acceptance/production/property/create_then_read".to_string(),
            "todo/acceptance/production/scenario/sharing_boundary".to_string(),
        ]
    );

    for m in &standard_todo_mutants() {
        let result = run_mutant(&ir, &baseline, m);
        assert_eq!(result.mutant_id, m.id);
        assert_eq!(result.class, m.class);
        assert!(result.critical);
        assert!(
            !result.killed,
            "standard mutant {} must SURVIVE under baseline-aware detection",
            m.id
        );
        assert_eq!(
            result.detecting_obligations,
            Vec::<String>::new(),
            "mutant {} has no NEW failures",
            m.id
        );
        assert_eq!(
            result.degraded_obligations,
            Vec::<String>::new(),
            "mutant {} moves nothing to indeterminate",
            m.id
        );
        assert_eq!(result.description, m.description);
    }
}

// =======================================================================
// 07. run_campaign over todo: summary counts, pass nil, blind spots,
//     fingerprint self-consistency, byte-stability, empty-mutant edge.
// =======================================================================

#[test]
fn oracle_07a_campaign_summary_counts_and_field_order() {
    // Arithmetic (from oracle_06b): 5 mutants, 0 killed, 5 survived,
    // 0 degraded-only (no mutant has a degradation), all 5 critical
    // (constructor) and surviving -> critical-survived 5 -> pass nil.
    let ir = load_todo_ir();
    let campaign = run_campaign(&ir, &standard_todo_mutants());

    let items = campaign.as_list().expect("campaign-result is a list");
    assert_eq!(items[0].as_sym(), Some("campaign-result"));
    assert_eq!(items.len(), 2, "nested house convention: tag + one pack");
    let fields = items[1].as_list().expect("nested field list");
    assert_eq!(
        pair_keys(fields),
        vec![
            "schema",
            "total",
            "killed",
            "survived",
            "degraded-only",
            "critical-survived",
            "pass",
            "results",
            "blind-spots",
            "fingerprint"
        ]
    );

    assert_eq!(ADEQUACY_SCHEMA, "gymnast.adequacy/0.1");
    assert_eq!(
        nested_field(&campaign, "schema"),
        &Sexpr::Str("gymnast.adequacy/0.1".to_string())
    );
    assert_eq!(nested_field(&campaign, "total"), &Sexpr::Int(5));
    assert_eq!(nested_field(&campaign, "killed"), &Sexpr::Int(0));
    assert_eq!(nested_field(&campaign, "survived"), &Sexpr::Int(5));
    assert_eq!(nested_field(&campaign, "degraded-only"), &Sexpr::Int(0));
    assert_eq!(nested_field(&campaign, "critical-survived"), &Sexpr::Int(5));
    assert_eq!(nested_field(&campaign, "pass"), &nil());

    // The five mutant-result forms, FLAT (reference record projection),
    // in mutant order, each with the plan's exact field order and empty
    // detecting/degraded lists.
    let results = nested_field(&campaign, "results")
        .as_list()
        .expect("results list");
    assert_eq!(results.len(), 5);
    let expected = [
        ("m1", "weaken-precondition"),
        ("m2", "remove-invariant"),
        ("m3", "weaken-limit"),
        ("m4", "remove-failure-mode"),
        ("m5", "skip-state-write"),
    ];
    for (r, (id, class)) in results.iter().zip(expected.iter()) {
        let r_items = r.as_list().expect("mutant-result is a list");
        assert_eq!(r_items[0].as_sym(), Some("mutant-result"));
        assert_eq!(
            pair_keys(&r_items[1..]),
            vec![
                "mutant-id",
                "class",
                "critical",
                "killed",
                "detecting-obligations",
                "degraded-obligations",
                "description"
            ]
        );
        assert_eq!(flat_field(r, "mutant-id"), &Sexpr::Str(id.to_string()));
        assert_eq!(flat_field(r, "class"), &Sexpr::sym(class));
        assert_eq!(flat_field(r, "critical"), &truthy());
        assert_eq!(flat_field(r, "killed"), &nil());
        assert_eq!(flat_field(r, "detecting-obligations"), &nil());
        assert_eq!(flat_field(r, "degraded-obligations"), &nil());
        assert!(flat_field(r, "description").as_str().is_some());
    }
}

#[test]
fn oracle_07b_campaign_blind_spots_name_all_five_survivors() {
    let ir = load_todo_ir();
    let campaign = run_campaign(&ir, &standard_todo_mutants());
    let blind_spots = nested_field(&campaign, "blind-spots")
        .as_list()
        .expect("blind-spots list");
    assert_eq!(blind_spots.len(), 5);
    let expected = [
        ("m1", "weaken-precondition"),
        ("m2", "remove-invariant"),
        ("m3", "weaken-limit"),
        ("m4", "remove-failure-mode"),
        ("m5", "skip-state-write"),
    ];
    for (b, (id, class)) in blind_spots.iter().zip(expected.iter()) {
        let b_items = b.as_list().expect("blind-spot is a list");
        assert_eq!(b_items[0].as_sym(), Some("blind-spot"));
        assert_eq!(
            pair_keys(&b_items[1..]),
            vec!["mutant-id", "class", "description"]
        );
        assert_eq!(flat_field(b, "mutant-id"), &Sexpr::Str(id.to_string()));
        assert_eq!(flat_field(b, "class"), &Sexpr::sym(class));
        assert!(flat_field(b, "description").as_str().is_some());
    }
}

#[test]
fn oracle_07c_campaign_fingerprint_self_consistency() {
    // The trailing fingerprint is computed over the fingerprint-free
    // form -- the same Ir/Plan/verification-bundle/evidence-bundle
    // discipline used everywhere in the crate since phase 7.
    let ir = load_todo_ir();
    let campaign = run_campaign(&ir, &standard_todo_mutants());

    let items = campaign.as_list().unwrap();
    let fields = items[1].as_list().unwrap();
    let (last, rest) = fields.split_last().expect("non-empty field list");
    let last_pair = last.as_list().expect("fingerprint pair");
    assert_eq!(last_pair[0].as_sym(), Some("fingerprint"));
    let fp = last_pair[1].as_str().expect("fingerprint is a string");
    assert!(fp.starts_with("fnv1a64:"), "got {:?}", fp);

    let fingerprint_free = Sexpr::list(vec![
        Sexpr::sym("campaign-result"),
        Sexpr::list(rest.to_vec()),
    ]);
    assert_eq!(fingerprint::fingerprint(&fingerprint_free), fp);

    // Mutating a copy of the fingerprint-free form changes the
    // recomputed fingerprint: (total 5) -> (total 6).
    let tampered_fields: Vec<Sexpr> = rest
        .iter()
        .map(|p| {
            if p.as_list()
                .and_then(|pp| pp.first())
                .and_then(|k| k.as_sym())
                == Some("total")
            {
                Sexpr::pair("total", Sexpr::Int(6))
            } else {
                p.clone()
            }
        })
        .collect();
    let tampered = Sexpr::list(vec![
        Sexpr::sym("campaign-result"),
        Sexpr::list(tampered_fields),
    ]);
    assert_ne!(fingerprint::fingerprint(&tampered), fp);
}

#[test]
fn oracle_07d_campaign_byte_stable_across_two_runs() {
    let ir1 = load_todo_ir();
    let ir2 = load_todo_ir();
    let one = canonical_serialize(&run_campaign(&ir1, &standard_todo_mutants()));
    let two = canonical_serialize(&run_campaign(&ir2, &standard_todo_mutants()));
    assert_eq!(one, two, "campaign result must be byte-identical");
}

#[test]
fn oracle_07e_empty_mutant_list_passes_vacuously() {
    // Plan edge table: empty mutant list -> total 0, pass t (no
    // critical survivors), empty results and blind-spots.
    let ir = load_todo_ir();
    let campaign = run_campaign(&ir, &[]);
    assert_eq!(nested_field(&campaign, "total"), &Sexpr::Int(0));
    assert_eq!(nested_field(&campaign, "killed"), &Sexpr::Int(0));
    assert_eq!(nested_field(&campaign, "survived"), &Sexpr::Int(0));
    assert_eq!(nested_field(&campaign, "degraded-only"), &Sexpr::Int(0));
    assert_eq!(nested_field(&campaign, "critical-survived"), &Sexpr::Int(0));
    assert_eq!(nested_field(&campaign, "pass"), &truthy());
    assert_eq!(nested_field(&campaign, "results"), &nil());
    assert_eq!(nested_field(&campaign, "blind-spots"), &nil());
}

// =======================================================================
// 08. Golden: `adequacy ../examples/todo.gym` matches
//     tests/fixtures/todo-adequacy.sexpr byte-for-byte. RED until Stage
//     3 lands the subcommand and the fixture.
// =======================================================================

#[test]
fn oracle_08_cli_adequacy_matches_golden_fixture() {
    let out = Command::new(env!("CARGO_BIN_EXE_gymnast-rs"))
        .args(["adequacy", "../examples/todo.gym"])
        .output()
        .expect("run gymnast-rs adequacy");
    // A failing campaign (pass nil) is evidence data, not a compiler
    // error: exit 0, same rationale as `hold` in phase 8 (plan section
    // E). Exit 1 is reserved for parse/IR errors, which todo.gym does
    // not have.
    assert!(
        out.status.success(),
        "adequacy over a valid spec must exit 0 even when the campaign \
         fails; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let golden = fs::read_to_string("tests/fixtures/todo-adequacy.sexpr")
        .or_else(|_| fs::read_to_string("fixtures/todo-adequacy.sexpr"))
        .expect("read tests/fixtures/todo-adequacy.sexpr");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        golden,
        "CLI adequacy stdout must match the committed fixture byte-for-byte"
    );
}
