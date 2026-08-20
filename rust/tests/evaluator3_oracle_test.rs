//! Tests-of-record for the phase-7 tri-state evaluator and operation-match
//! rule (`docs/rust-port-plan-phase7.md`, sections A-C), authored from the
//! plan ALONE, BEFORE any implementation of `Verdict`/`eval_predicate3`/
//! `check_transition_refs`/the new `TraceStep::symbolic` field/the
//! ambiguous-operation dispatch exists (the committed-oracle upgrade:
//! Stage 1 commits this file to git before any implementation stage runs).
//! `src/transition.lisp` and the phase-6 Rust `transition.rs`/`verify.rs`
//! (already read in full) were consulted only for what section A says must
//! be PRESERVED exactly; every new-semantics shape comes from the phase-7
//! plan's explicit tables, never guessed from the boolean evaluator alone.
//!
//! Implementation stages MUST NOT edit this file. If an assertion here
//! turns out to conflict with a considered implementation choice, the
//! implementer reports the conflict; only the integrator resolves it.
//!
//! This file will not compile until `crate::transition` gains `Verdict`,
//! `eval_predicate3`, `check_transition_refs`, and `TraceStep::symbolic`,
//! and `crate::verify`'s dispatch/bundle gain the new statuses/fields --
//! that is expected at this stage.
//!
//! RESOLVED AMBIGUITIES (plan text under-specifies these; the
//! contract-consistent reading taken here is noted at each site too):
//!
//!  1. "Boolean-wrapper equivalence... under the phase-6 mapping" is NOT
//!     read as a single flat formula from `Verdict` to `(bool, checked)`.
//!     Hand-tracing the UNCHANGED `eval_predicate_inner` code (phase-6,
//!     read in full) against the new tri-state table shows the boolean
//!     value for a composite `Unknown` verdict is recursion-dependent, not
//!     constant: e.g. `(not atom)` has verdict `Unknown` (not stays
//!     Unknown per the table) but its EXISTING boolean value is `false`
//!     (`!true`), not the naive "Unknown -> true" the table's *base-case*
//!     row ("nil / any atom -> Unknown") might suggest in isolation. So
//!     every corpus item below carries an INDEPENDENTLY hand-traced
//!     expected `(verdict, bool, checked)` triple (shown in a comment at
//!     each entry), plus a general correspondence check that holds
//!     unconditionally regardless of recursion depth: `Holds` implies
//!     `(true, true)`, `Fails` implies `(false, true)`, and `Unknown`
//!     implies `checked == false` (the only universally-safe claim).
//!  2. `execute_trace`'s embedded `violation`/`counterexample`/`trace-step`
//!     Sexpr shapes are read the same way `transition_oracle_test.rs`
//!     read them (flat tag-plus-sibling-pairs, confirmed by the plan's own
//!     literal, paren-balanced `(violation (type ambiguous-operation)
//!     (operation s) (candidates (op1 op2 ...)))` example) -- the same
//!     tolerant `field()` helper is still used for defense-in-depth
//!     consistency with the sibling oracle files.
//!  3. The `candidates` list's ORDER inside an `ambiguous-operation`
//!     violation is not pinned by the plan (only that it names both
//!     candidates); asserted here as a SET (sorted comparison), not a
//!     position, to avoid gambling the test on an unstated iteration
//!     order.
//!  4. `TraceStep::symbolic` is read as: true iff any precondition or
//!     failure `:when` guard evaluated while applying the matched
//!     transition rested on a permissive (unchecked) default -- exactly
//!     the same "basis" concept `eval_predicate_basis` already reports for
//!     a single predicate, extended across every guard evaluated along the
//!     one `apply_transition` call that produced this step. A step that
//!     never applied a transition at all (no-match / ambiguous-operation)
//!     performed no guard evaluation, so it is NOT symbolic by this
//!     reading (vacuously grounded, not vacuously unknown).

use gymnast_rs::elaborate;
use gymnast_rs::ir::{Ir, IrNode};
use gymnast_rs::parser;
use gymnast_rs::sexpr::Sexpr;
use gymnast_rs::transition::{
    apply_transition, check_transition_refs, eval_predicate, eval_predicate3,
    eval_predicate_basis, execute_trace, extract_transitions, make_initial_state, State,
    Transition, Verdict,
};
use gymnast_rs::verify::{compile_verification, lower_all_obligations, verify_obligation};
use std::fs;

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

/// See sibling oracle files' identical helper and this file's ambiguity 2.
fn field<'a>(v: &'a Sexpr, key: &str) -> Option<&'a Sexpr> {
    if let Some(found) = v.assoc(key) {
        return Some(found);
    }
    v.as_list()
        .and_then(|items| items.get(1))
        .and_then(|inner| inner.assoc(key))
}

fn call(head: &str, args: Vec<Sexpr>) -> Sexpr {
    let mut v = vec![Sexpr::sym(head)];
    v.extend(args);
    Sexpr::list(v)
}

fn nil() -> Sexpr {
    Sexpr::list(vec![])
}

fn sample_state() -> State {
    vec![
        ("foo".to_string(), Sexpr::Int(42)),
        ("bar".to_string(), Sexpr::Str("baz".to_string())),
    ]
}

#[allow(clippy::too_many_arguments)]
fn make_transition(
    id: &str,
    operation: &str,
    actor: Option<&str>,
    input: Option<&str>,
    reads: Vec<&str>,
    writes: Vec<&str>,
    preconditions: Vec<Sexpr>,
    failures: Vec<Sexpr>,
) -> Transition {
    Transition {
        id: id.to_string(),
        operation: operation.to_string(),
        actor: actor.map(|s| s.to_string()),
        input: input.map(|s| s.to_string()),
        reads: reads.into_iter().map(|s| s.to_string()).collect(),
        writes: writes.into_iter().map(|s| s.to_string()).collect(),
        atomic: None,
        idempotency: None,
        preconditions,
        postconditions: Vec::new(),
        result: None,
        failures,
        emissions: Vec::new(),
    }
}

fn behavior_node_with_on(id: &str, name: &str, on_op: &str) -> IrNode {
    IrNode::new(
        id.to_string(),
        "behavior",
        name.to_string(),
        vec![(":on".to_string(), Sexpr::list(vec![Sexpr::sym(on_op)]))],
        vec![],
    )
}

fn obligation_id(o: &Sexpr) -> String {
    field(o, "id")
        .and_then(|s| s.as_str())
        .unwrap_or_else(|| panic!("obligation missing `id`: {:?}", o))
        .to_string()
}

fn result_status(r: &Sexpr) -> String {
    field(r, "status")
        .and_then(|s| s.as_sym())
        .unwrap_or_else(|| panic!("result missing `status`: {:?}", r))
        .to_string()
}

fn result_basis(r: &Sexpr) -> Option<String> {
    field(r, "basis").and_then(|s| s.as_sym()).map(String::from)
}

// =======================================================================
// 1. Every row of the tri-state table, including and/or Unknown-absorption
//    and not-Unknown.
// =======================================================================

#[test]
fn oracle_01a_verdict3_nil_and_atoms_are_unknown() {
    let state = sample_state();
    assert_eq!(eval_predicate3(&nil(), &state, None, None), Verdict::Unknown);
    assert_eq!(
        eval_predicate3(&Sexpr::sym("some_atom"), &state, None, None),
        Verdict::Unknown
    );
    assert_eq!(
        eval_predicate3(&Sexpr::Int(0), &state, None, None),
        Verdict::Unknown
    );
    assert_eq!(
        eval_predicate3(&Sexpr::Str("x".to_string()), &state, None, None),
        Verdict::Unknown
    );
}

#[test]
fn oracle_01b_verdict3_eq_holds_or_fails_structurally() {
    let state = sample_state();
    assert_eq!(
        eval_predicate3(
            &call("=", vec![Sexpr::Int(42), Sexpr::sym("foo")]),
            &state,
            None,
            None
        ),
        Verdict::Holds
    );
    assert_eq!(
        eval_predicate3(
            &call("=", vec![Sexpr::Int(1), Sexpr::sym("foo")]),
            &state,
            None,
            None
        ),
        Verdict::Fails
    );
}

#[test]
fn oracle_01c_verdict3_not_swaps_holds_fails_unknown_stays_unknown() {
    let state = sample_state();
    let holds = call("=", vec![Sexpr::Int(1), Sexpr::Int(1)]);
    let fails = call("=", vec![Sexpr::Int(1), Sexpr::Int(2)]);
    assert_eq!(
        eval_predicate3(&call("not", vec![holds]), &state, None, None),
        Verdict::Fails
    );
    assert_eq!(
        eval_predicate3(&call("not", vec![fails]), &state, None, None),
        Verdict::Holds
    );
    // not-Unknown: Unknown must stay Unknown, never swap to Holds/Fails.
    assert_eq!(
        eval_predicate3(&call("not", vec![Sexpr::sym("atom")]), &state, None, None),
        Verdict::Unknown
    );
    // Double negation of Unknown stays Unknown too.
    assert_eq!(
        eval_predicate3(
            &call("not", vec![call("not", vec![Sexpr::sym("atom")])]),
            &state,
            None,
            None
        ),
        Verdict::Unknown
    );
}

#[test]
fn oracle_01d_verdict3_and_fails_dominates_then_unknown_absorbs_then_holds() {
    let state = sample_state();
    let holds = call("=", vec![Sexpr::Int(1), Sexpr::Int(1)]);
    let fails = call("=", vec![Sexpr::Int(1), Sexpr::Int(2)]);
    let unknown = Sexpr::sym("atom");

    // All Holds -> Holds.
    assert_eq!(
        eval_predicate3(
            &call("and", vec![holds.clone(), holds.clone()]),
            &state,
            None,
            None
        ),
        Verdict::Holds
    );
    // Holds + Unknown, no Fails -> Unknown (absorption).
    assert_eq!(
        eval_predicate3(
            &call("and", vec![holds.clone(), unknown.clone()]),
            &state,
            None,
            None
        ),
        Verdict::Unknown
    );
    // Fails + Unknown -> Fails (Fails dominates Unknown).
    assert_eq!(
        eval_predicate3(
            &call("and", vec![fails.clone(), unknown.clone()]),
            &state,
            None,
            None
        ),
        Verdict::Fails
    );
    // Unknown + Fails (order reversed) -> still Fails.
    assert_eq!(
        eval_predicate3(
            &call("and", vec![unknown.clone(), fails.clone()]),
            &state,
            None,
            None
        ),
        Verdict::Fails
    );
    // Empty and(): vacuously Holds (no Fails, no Unknown among zero items).
    assert_eq!(
        eval_predicate3(&Sexpr::list(vec![Sexpr::sym("and")]), &state, None, None),
        Verdict::Holds
    );
}

#[test]
fn oracle_01e_verdict3_or_holds_dominates_then_unknown_absorbs_then_fails() {
    let state = sample_state();
    let holds = call("=", vec![Sexpr::Int(1), Sexpr::Int(1)]);
    let fails = call("=", vec![Sexpr::Int(1), Sexpr::Int(2)]);
    let unknown = Sexpr::sym("atom");

    // All Fails -> Fails.
    assert_eq!(
        eval_predicate3(
            &call("or", vec![fails.clone(), fails.clone()]),
            &state,
            None,
            None
        ),
        Verdict::Fails
    );
    // Fails + Unknown, no Holds -> Unknown (absorption).
    assert_eq!(
        eval_predicate3(
            &call("or", vec![fails.clone(), unknown.clone()]),
            &state,
            None,
            None
        ),
        Verdict::Unknown
    );
    // Holds + Unknown -> Holds (Holds dominates Unknown).
    assert_eq!(
        eval_predicate3(
            &call("or", vec![holds.clone(), unknown.clone()]),
            &state,
            None,
            None
        ),
        Verdict::Holds
    );
    // Unknown + Holds (order reversed) -> still Holds.
    assert_eq!(
        eval_predicate3(
            &call("or", vec![unknown.clone(), holds.clone()]),
            &state,
            None,
            None
        ),
        Verdict::Holds
    );
    // Empty or(): vacuously Fails (no Holds, no Unknown among zero items).
    assert_eq!(
        eval_predicate3(&Sexpr::list(vec![Sexpr::sym("or")]), &state, None, None),
        Verdict::Fails
    );
}

#[test]
fn oracle_01f_verdict3_lt_le_holds_fails_when_both_int_else_unknown() {
    let state = sample_state();
    assert_eq!(
        eval_predicate3(
            &call("<", vec![Sexpr::Int(1), Sexpr::Int(2)]),
            &state,
            None,
            None
        ),
        Verdict::Holds
    );
    assert_eq!(
        eval_predicate3(
            &call("<", vec![Sexpr::Int(2), Sexpr::Int(1)]),
            &state,
            None,
            None
        ),
        Verdict::Fails
    );
    assert_eq!(
        eval_predicate3(
            &call("<=", vec![Sexpr::Int(2), Sexpr::Int(2)]),
            &state,
            None,
            None
        ),
        Verdict::Holds
    );
    assert_eq!(
        eval_predicate3(
            &call("<=", vec![Sexpr::Int(3), Sexpr::Int(2)]),
            &state,
            None,
            None
        ),
        Verdict::Fails
    );
    // Non-Int operand on either side -> Unknown (not Fails, not Holds).
    assert_eq!(
        eval_predicate3(
            &call("<", vec![Sexpr::Str("a".to_string()), Sexpr::Int(2)]),
            &state,
            None,
            None
        ),
        Verdict::Unknown
    );
    assert_eq!(
        eval_predicate3(
            &call("<=", vec![Sexpr::sym("unbound_symbol"), Sexpr::Int(2)]),
            &state,
            None,
            None
        ),
        Verdict::Unknown
    );
}

#[test]
fn oracle_01g_verdict3_unrecognized_head_is_unknown() {
    let state = sample_state();
    assert_eq!(
        eval_predicate3(
            &call(
                "forall",
                vec![
                    Sexpr::list(vec![Sexpr::sym("x"), Sexpr::sym("TodoList")]),
                    Sexpr::sym("p")
                ]
            ),
            &state,
            None,
            None
        ),
        Verdict::Unknown
    );
    assert_eq!(
        eval_predicate3(
            &call("some_unregistered_call", vec![Sexpr::sym("x")]),
            &state,
            None,
            None
        ),
        Verdict::Unknown
    );
}

// =======================================================================
// 2. Boolean-wrapper equivalence: for a corpus of predicates,
//    eval_predicate == eval_predicate3 mapped under the phase-6 mapping.
//    See file header ambiguity 1 for why each entry's expected triple is
//    independently hand-traced rather than derived from one formula.
// =======================================================================

struct Corpus {
    name: &'static str,
    pred: Sexpr,
    verdict: Verdict,
    boolean: bool,
    checked: bool,
}

fn corpus() -> Vec<Corpus> {
    let holds = call("=", vec![Sexpr::Int(1), Sexpr::Int(1)]);
    let fails = call("=", vec![Sexpr::Int(1), Sexpr::Int(2)]);
    vec![
        // Base cases (row 1 of the table): true + unchecked.
        Corpus {
            name: "nil",
            pred: nil(),
            verdict: Verdict::Unknown,
            boolean: true,
            checked: false,
        },
        Corpus {
            name: "atom",
            pred: Sexpr::sym("foo_atom"),
            verdict: Verdict::Unknown,
            boolean: true,
            checked: false,
        },
        // Grounded equality.
        Corpus {
            name: "(= 1 1)",
            pred: holds.clone(),
            verdict: Verdict::Holds,
            boolean: true,
            checked: true,
        },
        Corpus {
            name: "(= 1 2)",
            pred: fails.clone(),
            verdict: Verdict::Fails,
            boolean: false,
            checked: true,
        },
        // not() over grounded operands: checked stays true (the `not`
        // wrapper itself touches nothing; the `=` branch never sets
        // `checked = false`).
        Corpus {
            name: "(not (= 1 2))",
            pred: call("not", vec![fails.clone()]),
            verdict: Verdict::Holds,
            boolean: true,
            checked: true,
        },
        Corpus {
            name: "(not (= 1 1))",
            pred: call("not", vec![holds.clone()]),
            verdict: Verdict::Fails,
            boolean: false,
            checked: true,
        },
        // not(atom): the inner "any atom" branch returns (true, unchecked)
        // BEFORE `not` runs; `not` only negates the boolean, so the result
        // is (false, unchecked) -- NOT (true, unchecked). This is exactly
        // the case ambiguity 1 calls out.
        Corpus {
            name: "(not atom)",
            pred: call("not", vec![Sexpr::sym("atom")]),
            verdict: Verdict::Unknown,
            boolean: false,
            checked: false,
        },
        // and(): Rust's `Iterator::all` short-circuits on the first
        // false, so `(and (= 1 2) atom)` never evaluates `atom` at all --
        // `checked` stays at its untouched initial value `true`.
        Corpus {
            name: "(and (= 1 1) (= 2 2))",
            pred: call("and", vec![holds.clone(), call("=", vec![Sexpr::Int(2), Sexpr::Int(2)])]),
            verdict: Verdict::Holds,
            boolean: true,
            checked: true,
        },
        Corpus {
            name: "(and (= 1 2) atom)",
            pred: call("and", vec![fails.clone(), Sexpr::sym("atom")]),
            verdict: Verdict::Fails,
            boolean: false,
            checked: true,
        },
        Corpus {
            name: "(and (= 1 1) atom)",
            pred: call("and", vec![holds.clone(), Sexpr::sym("atom")]),
            verdict: Verdict::Unknown,
            boolean: true,
            checked: false,
        },
        // or(): `Iterator::any` short-circuits on the first true.
        Corpus {
            name: "(or (= 1 1) atom)",
            pred: call("or", vec![holds.clone(), Sexpr::sym("atom")]),
            verdict: Verdict::Holds,
            boolean: true,
            checked: true,
        },
        Corpus {
            name: "(or (= 1 2) atom)",
            pred: call("or", vec![fails.clone(), Sexpr::sym("atom")]),
            verdict: Verdict::Unknown,
            boolean: true,
            checked: false,
        },
        Corpus {
            name: "(or (= 1 2) (= 3 4))",
            pred: call("or", vec![fails.clone(), call("=", vec![Sexpr::Int(3), Sexpr::Int(4)])]),
            verdict: Verdict::Fails,
            boolean: false,
            checked: true,
        },
        // Comparison delta: non-Int -> Unknown, boolean FALSE + unchecked
        // (the one row where the base default is false, not true).
        Corpus {
            name: "(< \"a\" 2)",
            pred: call("<", vec![Sexpr::Str("a".to_string()), Sexpr::Int(2)]),
            verdict: Verdict::Unknown,
            boolean: false,
            checked: false,
        },
        Corpus {
            name: "(<= 2 2)",
            pred: call("<=", vec![Sexpr::Int(2), Sexpr::Int(2)]),
            verdict: Verdict::Holds,
            boolean: true,
            checked: true,
        },
    ]
}

#[test]
fn oracle_02_boolean_wrapper_equivalence_corpus() {
    let state = sample_state();
    for item in corpus() {
        let v3 = eval_predicate3(&item.pred, &state, None, None);
        assert_eq!(
            v3, item.verdict,
            "{}: eval_predicate3 verdict mismatch",
            item.name
        );

        let b = eval_predicate(&item.pred, &state, None, None);
        let (b2, checked) = eval_predicate_basis(&item.pred, &state, None, None);
        assert_eq!(b, b2, "{}: eval_predicate/eval_predicate_basis disagree on the boolean value -- basis must never diverge from the plain wrapper", item.name);
        assert_eq!(
            b, item.boolean,
            "{}: eval_predicate boolean value mismatch (existing phase-6 behavior must be preserved exactly)",
            item.name
        );
        assert_eq!(
            checked, item.checked,
            "{}: eval_predicate_basis checked-flag mismatch",
            item.name
        );

        // Universal correspondence, independent of recursion depth (see
        // ambiguity 1): Holds/Fails always ground the boolean; Unknown
        // always leaves it unchecked.
        match v3 {
            Verdict::Holds => assert!(
                b && checked,
                "{}: Holds must map to (true, checked)",
                item.name
            ),
            Verdict::Fails => assert!(
                !b && checked,
                "{}: Fails must map to (false, checked)",
                item.name
            ),
            Verdict::Unknown => assert!(
                !checked,
                "{}: Unknown must always be unchecked (symbolic)",
                item.name
            ),
        }
    }
}

// =======================================================================
// 3. todo.gym end-to-end statuses re-derived under sections A+B+C.
// =======================================================================

#[test]
fn oracle_03a_both_invariants_become_indeterminate_symbolic_basis() {
    let ir = load_todo_ir();
    let obligations = lower_all_obligations(&ir);
    // owner_isolation's :always is the bare atom
    // `no_observation_without_active_membership` -> row 1 of the tri-state
    // table (any atom) -> Unknown at the deciding evaluation.
    //
    // sharing_limit's :always is `(forall ((list TodoList)) (<= ...))`,
    // headed by `forall` -- not one of eval_predicate3's special heads
    // (=, not, and, or, <, <=) -- so it is also Unknown (unrecognized
    // head), independent of any state.
    //
    // Section A: an invariant obligation whose deciding evaluation is
    // Unknown now yields `indeterminate` (never `passed`), with `basis`
    // always `symbolic` there (never the redundant `checked`).
    for id in [
        "todo/invariant/owner_isolation/invariant-check",
        "todo/invariant/sharing_limit/invariant-check",
    ] {
        let ob = obligations
            .iter()
            .find(|o| obligation_id(o) == id)
            .unwrap_or_else(|| panic!("missing obligation {}", id));
        let result = verify_obligation(&ir, ob);
        assert_eq!(
            result_status(&result),
            "indeterminate",
            "{}: expected indeterminate, got {:?}",
            id,
            result
        );
        assert_eq!(
            result_basis(&result).as_deref(),
            Some("symbolic"),
            "{}: indeterminate results must carry basis=symbolic",
            id
        );
    }
}

// DERIVATION (section B's suffix match rule against
// tests/fixtures/todo-ir.sexpr's clause shapes):
//
// create_then_read: execute steps `create_task` then `query_tasks`.
//   - "create_task": `todo_service/create_task` ends with "/create_task"
//     and no other transition operation does -> unique match. The
//     matched transition's preconditions (`authenticated`/`may_edit_list`)
//     are unrecognized-head calls, so apply_transition (which keeps
//     BOOLEAN semantics, unchanged from phase 6) evaluates them true via
//     the permissive default; the one failure clause's `:when` is
//     `(not (may_edit_list ...))`, which evaluates to `(not true)` =
//     false, so it never matches; preconditions hold -> `(succeeded)`.
//     This step is SYMBOLIC (its guard evaluations hit permissive
//     defaults).
//   - "query_tasks": no transition operation equals or ends with
//     "/query_tasks" (query_tasks is an interface query, not a behavior)
//     -> zero matches -> unchanged `no-matching-transition`, a violation.
//   -> 1 violation -> property FAILED. basis: the create_task step WAS
//      symbolic, so "any executed step was symbolic" -> basis=symbolic.
//
// viewer_cannot_mutate: execute is the single step `create_task`, which
//   matches and succeeds exactly as above, with zero violations ->
//   property PASSED, basis=symbolic (its one executed step was symbolic).
//
// sharing_boundary (scenario): steps are the two `when` actions,
//   `invite_distinct(owner, 256)` and `invite_distinct(owner, 257)`.
//   "invite_distinct" is a helper name that neither equals nor is a
//   "/"-suffix of any transition operation (`todo_service/invite` does
//   NOT end with "/invite_distinct") -> both steps are zero-match ->
//   2 violations -> scenario FAILED. Neither step ever applied a
//   transition, so NO guard evaluation happened at all -> "any executed
//   step was symbolic" is vacuously false -> basis=checked (not
//   symbolic), despite the obligation failing.
#[test]
fn oracle_03b_property_and_scenario_statuses_and_basis_under_new_match_rule() {
    let ir = load_todo_ir();
    let obligations = lower_all_obligations(&ir);
    let by_id = |id: &str| -> &Sexpr {
        obligations
            .iter()
            .find(|o| obligation_id(o) == id)
            .unwrap_or_else(|| panic!("missing obligation {}", id))
    };

    let expected: &[(&str, &str, &str)] = &[
        (
            "todo/acceptance/production/property/create_then_read",
            "failed",
            "symbolic",
        ),
        (
            "todo/acceptance/production/property/viewer_cannot_mutate",
            "passed",
            "symbolic",
        ),
        (
            "todo/acceptance/production/scenario/sharing_boundary",
            "failed",
            "checked",
        ),
    ];
    for (id, status, basis) in expected {
        let ob = by_id(id);
        let result = verify_obligation(&ir, ob);
        assert_eq!(
            result_status(&result),
            *status,
            "{}: status mismatch, got {:?}",
            id,
            result
        );
        assert_eq!(
            result_basis(&result).as_deref(),
            Some(*basis),
            "{}: basis mismatch, got {:?}",
            id,
            result
        );
    }
}

// Bundle summary, derived from oracle_03a/03b plus the unchanged
// concurrency/fault/coverage/constraint skips (4 total, untouched by
// phase 7): total 9, passed 1 (viewer_cannot_mutate only -- the two
// invariants moved OFF `passed` into `indeterminate`), failed 2
// (create_then_read, sharing_boundary), skipped 4, indeterminate 2.
// 1 + 2 + 4 + 2 == 9.
#[test]
fn oracle_03c_bundle_summary_gains_indeterminate_and_totals_match() {
    let ir = load_todo_ir();
    let bundle = compile_verification(&ir);
    let summary = field(&bundle, "summary").expect("bundle must carry a summary");
    let get = |k: &str| field(summary, k).and_then(|v| v.as_int()).unwrap();
    assert_eq!(get("total"), 9);
    assert_eq!(get("passed"), 1);
    assert_eq!(get("failed"), 2);
    assert_eq!(get("skipped"), 4);
    assert_eq!(
        get("indeterminate"),
        2,
        "summary must gain an (indeterminate N) entry after skipped"
    );
    assert_eq!(get("passed") + get("failed") + get("skipped") + get("indeterminate"), get("total"));
}

// DERIVATION (section C): the only `state` node is `todo_state`; every
// `reads`/`writes` entry on either behavior names something else, so
// EVERY entry is unresolved:
//   create_task: reads (memberships, todo_lists), writes (tasks) -> 3
//   invite_user: reads (memberships, invitations), writes (invitations) -> 3
// Total 6, with "invitations" appearing twice for invite_user (once from
// reads, once from writes) -- matching the plan's own arithmetic exactly.
#[test]
fn oracle_03d_check_transition_refs_exactly_six_w406_over_todo_gym() {
    let ir = load_todo_ir();
    let warnings = check_transition_refs(&ir);
    assert_eq!(
        warnings.len(),
        6,
        "expected exactly 6 W406 warnings, got {:?}",
        warnings
    );
    for w in &warnings {
        assert_eq!(w.assoc("code").and_then(|c| c.as_str()), Some("W406"));
        assert_eq!(w.assoc("severity").and_then(|s| s.as_sym()), Some("warning"));
    }
    let expected_pairs: &[(&str, &str, usize)] = &[
        ("todo/behavior/create_task", "memberships", 1),
        ("todo/behavior/create_task", "todo_lists", 1),
        ("todo/behavior/create_task", "tasks", 1),
        ("todo/behavior/invite_user", "memberships", 1),
        ("todo/behavior/invite_user", "invitations", 2),
    ];
    for (transition_id, state_ref, expected_count) in expected_pairs {
        let count = warnings
            .iter()
            .filter(|w| {
                let msg = w.assoc("message").and_then(|m| m.as_str()).unwrap_or("");
                msg.contains(transition_id) && msg.contains(state_ref)
            })
            .count();
        assert_eq!(
            count, *expected_count,
            "({}, {}): expected {} W406 warning(s) naming both the transition id and the ref, got {}",
            transition_id, state_ref, expected_count, count
        );
    }
}

#[test]
fn oracle_03e_bundle_transition_diagnostics_after_environment_diagnostics_e601_absent() {
    let ir = load_todo_ir();
    let bundle = compile_verification(&ir);
    let inner = bundle
        .as_list()
        .and_then(|l| l.get(1))
        .and_then(|v| v.as_list())
        .expect("bundle must nest one field list");
    let key_at = |k: &str| {
        inner.iter().position(|p| {
            p.as_list().and_then(|x| x.first()).and_then(|s| s.as_sym()) == Some(k)
        })
    };
    let env_idx = key_at("environment-diagnostics").expect("bundle must carry environment-diagnostics");
    let trans_idx = key_at("transition-diagnostics").expect("bundle must carry transition-diagnostics");
    assert!(
        trans_idx > env_idx,
        "transition-diagnostics must be placed after environment-diagnostics"
    );

    let bundled: Vec<Sexpr> = field(&bundle, "transition-diagnostics")
        .and_then(|d| d.as_list())
        .expect("transition-diagnostics must be a list")
        .to_vec();
    let direct = check_transition_refs(&ir);
    assert_eq!(
        bundled, direct,
        "the bundle's transition-diagnostics must equal check_transition_refs(ir) exactly"
    );

    // todo.gym's 9 obligation ids are all distinct (see verify_oracle_test
    // .rs's oracle_02a), so E601 (duplicate-obligation-id) must be absent
    // from the bundle's own (new, section D) diagnostics field for todo.
    if let Some(diags) = field(&bundle, "diagnostics").and_then(|d| d.as_list()) {
        let e601: Vec<&Sexpr> = diags
            .iter()
            .filter(|d| d.assoc("code").and_then(|c| c.as_str()) == Some("E601"))
            .collect();
        assert!(
            e601.is_empty(),
            "todo.gym has no duplicate obligation ids; E601 must not fire, got {:?}",
            e601
        );
    }
}

// =======================================================================
// 4. Ambiguous-op test: two behaviors a/op and b/op, step op ->
//    ambiguous-operation violation naming both candidates.
// =======================================================================

#[test]
fn oracle_04_ambiguous_operation_two_suffix_matches_records_violation_naming_both() {
    let a = behavior_node_with_on("m/behavior/a", "a", "a/op");
    let b = behavior_node_with_on("m/behavior/b", "b", "b/op");
    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "m".to_string(),
        vec![],
        vec![],
        vec![a, b],
        vec![],
        vec![],
        vec![],
    );
    let step = Sexpr::list(vec![Sexpr::sym("op")]);
    let trace = execute_trace(&ir, &[step]);

    assert_eq!(trace.steps.len(), 1);
    let s = &trace.steps[0];
    assert_eq!(
        s.outcome,
        Sexpr::list(vec![Sexpr::sym("ambiguous-operation"), Sexpr::sym("op")])
    );
    assert_eq!(s.pre_state, s.post_state, "an ambiguous step never mutates state");

    assert_eq!(trace.violations.len(), 1);
    let v = &trace.violations[0];
    assert_eq!(
        field(v, "type").and_then(|s| s.as_sym()),
        Some("ambiguous-operation")
    );
    assert_eq!(
        field(v, "operation").and_then(|s| s.as_sym()),
        Some("op")
    );
    let candidates: Vec<&str> = field(v, "candidates")
        .and_then(|c| c.as_list())
        .expect("candidates must be a list")
        .iter()
        .filter_map(|c| c.as_sym())
        .collect();
    let mut sorted = candidates.clone();
    sorted.sort();
    assert_eq!(
        sorted,
        vec!["a/op", "b/op"],
        "candidates must name both matching operations (order unspecified by the plan), got {:?}",
        candidates
    );
}

#[test]
fn oracle_04b_zero_matches_unchanged_no_matching_transition() {
    let a = behavior_node_with_on("m/behavior/a", "a", "a/op");
    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "m".to_string(),
        vec![],
        vec![],
        vec![a],
        vec![],
        vec![],
        vec![],
    );
    let step = Sexpr::list(vec![Sexpr::sym("nothing_matches")]);
    let trace = execute_trace(&ir, &[step]);
    assert_eq!(
        trace.steps[0].outcome,
        Sexpr::list(vec![
            Sexpr::sym("no-matching-transition"),
            Sexpr::sym("nothing_matches")
        ])
    );
}

// =======================================================================
// 5. Suffix-match uniqueness: step create_task matches exactly
//    todo_service/create_task.
// =======================================================================

#[test]
fn oracle_05_suffix_match_uniqueness_create_task_over_todo_gym() {
    let ir = load_todo_ir();
    let step = Sexpr::list(vec![
        Sexpr::sym("create_task"),
        Sexpr::sym("actor"),
        Sexpr::sym("task"),
    ]);
    let trace = execute_trace(&ir, &[step]);
    assert_eq!(trace.steps.len(), 1);
    assert_eq!(
        trace.steps[0].transition_id, "todo/behavior/create_task",
        "\"create_task\" must match exactly todo_service/create_task (the only operation \
         ending with \"/create_task\"), not report unknown or ambiguous"
    );
    assert!(
        trace.violations.is_empty(),
        "a unique suffix match must not record a violation"
    );
}

// =======================================================================
// 6. TraceStep basis field present and symbolic for an abstract-
//    precondition transition, checked for a grounded one.
// =======================================================================

#[test]
fn oracle_06a_symbolic_true_for_todo_create_task_abstract_preconditions() {
    let ir = load_todo_ir();
    let transitions = extract_transitions(&ir);
    let ct = transitions
        .iter()
        .find(|t| t.id == "todo/behavior/create_task")
        .expect("todo.gym must have create_task");
    let state = make_initial_state(&ir);
    let actor = Sexpr::sym("user1");
    let input = Sexpr::sym("req1");
    let step = apply_transition(ct, &state, Some(&actor), Some(&input));
    assert_eq!(step.outcome, Sexpr::list(vec![Sexpr::sym("succeeded")]));
    assert!(
        step.symbolic,
        "create_task's preconditions (authenticated/may_edit_list) are unrecognized-head \
         calls, which hit the evaluator's permissive default -- the step must be symbolic"
    );
}

#[test]
fn oracle_06b_checked_false_for_a_grounded_precondition_transition() {
    let grounded = make_transition(
        "m/behavior/grounded",
        "svc/grounded_op",
        None,
        None,
        vec![],
        vec!["out"],
        vec![call("=", vec![Sexpr::Int(1), Sexpr::Int(1)])],
        vec![],
    );
    let state: State = vec![];
    let step = apply_transition(&grounded, &state, None, None);
    assert_eq!(step.outcome, Sexpr::list(vec![Sexpr::sym("succeeded")]));
    assert!(
        !step.symbolic,
        "a grounded (=) precondition evaluates every branch -- the step must not be symbolic"
    );
}

#[test]
fn oracle_06c_symbolic_true_when_the_matched_failure_clause_guard_is_abstract() {
    // The failure clause's :when is an unrecognized-head call; it must
    // still evaluate (boolean semantics unchanged) to decide the outcome,
    // and that evaluation is exactly the kind of permissive default that
    // makes the step symbolic.
    let t = make_transition(
        "m/behavior/abstract_fail",
        "svc/op",
        None,
        None,
        vec![],
        vec!["out"],
        vec![],
        vec![Sexpr::list(vec![
            Sexpr::sym("some_error"),
            Sexpr::sym(":when"),
            Sexpr::sym("abstract_guard"),
        ])],
    );
    let state: State = vec![];
    let step = apply_transition(&t, &state, None, None);
    assert_eq!(
        step.outcome,
        Sexpr::list(vec![Sexpr::sym("failed"), Sexpr::sym("some_error")])
    );
    assert!(
        step.symbolic,
        "an abstract (unrecognized-head) failure guard must mark the step symbolic"
    );
}
