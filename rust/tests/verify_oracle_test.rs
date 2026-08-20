//! Tests-of-record for `verify.rs`, authored from
//! `docs/rust-port-plan-phase6.md` section B and its "Oracle tests" ->
//! `verify_oracle_test.rs` list ALONE, BEFORE any implementation of
//! `crate::verify` exists (the phase-4+ committed-oracle upgrade: Stage 1
//! commits this file to git before any implementation stage runs).
//! `src/verify.lisp` was consulted only for BEHAVIORAL INTENT; every
//! Rust-IR shape adaptation comes from the phase-6 plan's explicit table
//! and `docs/ir-contract-deltas.md`, never guessed from the Lamedh
//! golden.
//!
//! Implementation stages MUST NOT edit this file. If an assertion here
//! turns out to conflict with a considered implementation choice, the
//! implementer reports the conflict; only the integrator resolves it.
//!
//! This file will not compile until `crate::verify` exists and
//! `main.rs` gains a `verify` subcommand -- that is expected at this
//! stage.
//!
//! Numbering follows the plan's `verify_oracle_test.rs` list items 1-7
//! exactly; each gets one or more `#[test]`s, none merged or dropped.
//!
//! For items 3 and 5, the plan explicitly hands the oracle author the
//! job of DERIVING and pinning exact values by applying the plan's
//! stated semantics to `tests/fixtures/todo-ir.sexpr`; those derivations
//! are written out in full as comments at each site (not just asserted).
//!
//! RESOLVED AMBIGUITIES (contract-consistent readings; each is also
//! called out at its use site):
//!
//!  1. WRAPPING DEPTH of the small ad-hoc shapes verify.rs builds
//!     (`verification-obligation`, `verification-result`, `violation`,
//!     `divergence`, `normalized-counterexample`,
//!     `trace-equivalence-result`, `coverage-analysis`,
//!     `execution-environment`). The plan gives only ONE shape with
//!     enough literal parentheses to pin definitively: the bundle,
//!     `(verification-bundle ((schema ...) (obligations (...)) (results
//!     (...)) (summary ((total N) ...)) (coverage (...))
//!     (environment-diagnostics (...))))` -- counting parens confirms
//!     this is NESTED one level (tag, then ONE sublist holding every
//!     field pair), matching this crate's established
//!     `IrNode`/`PlanNode`/`ExecutionResult` house convention. But
//!     section A's `(violation (invariant id) (predicate p) (state
//!     ...))` example, counted the same way, is FLAT (tag directly
//!     followed by sibling pairs, no extra wrapping list) -- and
//!     `src/verify.lisp` builds every one of the shapes above via plain
//!     `(list 'tag (list 'k v) ...)` calls with no `defrecord` behind
//!     any of them (unlike `transition.lisp`'s `gymnast-transition` /
//!     `gymnast-trace-step`, which DO use `defrecord` and DO get real
//!     Rust structs in section A). That is real, textually-supported
//!     ambiguity in the plan for the smaller shapes, not a guess: the
//!     `field()` helper below tries the flat reading first (the
//!     Lisp-mirroring, no-struct-behind-it default) and falls back to
//!     one level of nesting, so every test here pins the SUBSTANCE the
//!     plan actually specifies (ids, kinds, statuses, counts, flags,
//!     divergence kinds) without gambling the whole file on the
//!     unpinned wrapping-depth question. `verification-bundle` fields
//!     are looked up the same way for a single consistent idiom, even
//!     though its nesting is not actually ambiguous.
//!  2. Coverage-obligation field NAMES: the plan's obligation-lowering
//!     table says coverage obligations carry "the five flags read by
//!     OUR underscore keys (`:every_operation`, ...)" -- read here as
//!     pinning the OBLIGATION's own field names to the same underscore
//!     spelling used to read them off the source clause (not renamed to
//!     the hyphenated style verify.lisp otherwise uses for multi-word
//!     tags like `obligation-id`), consistent with the delta doc's
//!     general "Rust preserves author/source spelling" theme.
//!  3. Ids (`obligation-id`, `operation`/transition-id echoes) are
//!     asserted as `Sexpr::Str`, matching every other semantic id in
//!     this crate.
//!  4. Boolean-valued Sexpr fields (coverage flags, `equivalent`) use
//!     the Lisp t/nil convention `Sexpr::sym("t")` / `Sexpr::list(vec![])`
//!     since `Sexpr` has no `Bool` variant -- the only faithful
//!     encoding available, not really a free choice.
//!  5. The `execute`-value step-splitting rule (property obligations):
//!     "our IR's execute is a Nested/List of step forms -- each list
//!     element is one step; a single non-list execute value is one
//!     step." Read as: if `execute`'s FIRST element is itself a list,
//!     `execute` is the multi-step case (each element is one step);
//!     otherwise the whole `execute` value is the one step. This reading
//!     is what makes todo.gym's two shapes
//!     (`((create_task actor task) (query_tasks actor task/list))` vs.
//!     `(create_task actor task)`) actually distinguishable, and it is
//!     exactly what oracle_03d below works out and pins.

use gymnast_rs::elaborate;
use gymnast_rs::ir::{Ir, IrNode};
use gymnast_rs::parser;
use gymnast_rs::sexpr::{self, canonical_serialize, Sexpr};
use gymnast_rs::transition::{State, Trace, TraceStep};
use gymnast_rs::verify::{
    compare_traces, compile_verification, coverage_gaps, env_deterministic, env_diagnostics,
    extract_execution_env, lower_all_obligations, normalize_counterexample,
    normalize_counterexamples, trace_equivalence_result, verify_obligation,
};
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

/// See the file header's "Resolved ambiguities" item 1.
fn field<'a>(v: &'a Sexpr, key: &str) -> Option<&'a Sexpr> {
    if let Some(found) = v.assoc(key) {
        return Some(found);
    }
    v.as_list()
        .and_then(|items| items.get(1))
        .and_then(|inner| inner.assoc(key))
}

fn nil() -> Sexpr {
    Sexpr::list(vec![])
}

fn truthy() -> Sexpr {
    Sexpr::sym("t")
}

fn state_sexpr(state: &State) -> Sexpr {
    Sexpr::list(
        state
            .iter()
            .map(|entry: &(String, Sexpr)| Sexpr::list(vec![Sexpr::sym(&entry.0), entry.1.clone()]))
            .collect(),
    )
}

fn find_node<'a>(ir: &'a Ir, id: &str) -> &'a IrNode {
    ir.find_node(id)
        .unwrap_or_else(|| panic!("no IR node {}", id))
}

fn obligation_id(o: &Sexpr) -> String {
    field(o, "id")
        .and_then(|s| s.as_str())
        .unwrap_or_else(|| panic!("obligation missing `id`: {:?}", o))
        .to_string()
}

fn obligation_kind(o: &Sexpr) -> String {
    field(o, "kind")
        .and_then(|s| s.as_sym())
        .unwrap_or_else(|| panic!("obligation missing `kind`: {:?}", o))
        .to_string()
}

fn result_status(r: &Sexpr) -> String {
    field(r, "status")
        .and_then(|s| s.as_sym())
        .unwrap_or_else(|| panic!("result missing `status`: {:?}", r))
        .to_string()
}

fn step(transition_id: &str, outcome: Sexpr, pre: State, post: State) -> TraceStep {
    TraceStep {
        transition_id: transition_id.to_string(),
        actor: Some(Sexpr::sym("user1")),
        input: Some(Sexpr::sym("payload")),
        pre_state: pre,
        post_state: post,
        result: None,
        outcome,
    }
}

fn succeeded() -> Sexpr {
    Sexpr::list(vec![Sexpr::sym("succeeded")])
}

fn failed(err: &str) -> Sexpr {
    Sexpr::list(vec![Sexpr::sym("failed"), Sexpr::sym(err)])
}

// =======================================================================
// 1. Env extraction from todo.gym: virtual/seeded/controlled/"UTC",
//    deterministic -> zero env warnings; a hand-built acceptance node
//    with defaults -> three warnings naming the acceptance id.
// =======================================================================

#[test]
fn oracle_01a_env_extraction_from_todo_gym_matches_execution_clause() {
    let ir = load_todo_ir();
    let acc = find_node(&ir, "todo/acceptance/production");
    let env = extract_execution_env(acc);

    assert_eq!(
        field(&env, "clock").and_then(|s| s.as_sym()),
        Some("virtual")
    );
    assert_eq!(
        field(&env, "randomness").and_then(|s| s.as_sym()),
        Some("seeded")
    );
    assert_eq!(
        field(&env, "network").and_then(|s| s.as_sym()),
        Some("controlled")
    );
    // todo.gym's `execution` clause never sets `:locale` -> default.
    assert_eq!(
        field(&env, "locale").and_then(|s| s.as_str()),
        Some("en-US")
    );
    assert_eq!(
        field(&env, "timezone").and_then(|s| s.as_str()),
        Some("UTC")
    );
}

#[test]
fn oracle_01b_env_deterministic_true_zero_warnings_for_todo_gym() {
    let ir = load_todo_ir();
    let acc = find_node(&ir, "todo/acceptance/production");
    let env = extract_execution_env(acc);

    assert!(env_deterministic(&env));
    let diags = env_diagnostics(&env, "todo/acceptance/production");
    assert!(
        diags.is_empty(),
        "a fully virtual/seeded/controlled environment must yield zero env warnings, got {:?}",
        diags
    );
}

#[test]
fn oracle_01c_env_defaults_and_three_warnings_naming_acceptance_id() {
    // A hand-built acceptance node with no `execution` clause at all.
    let acc = IrNode::new(
        "hand/acceptance/defaults".to_string(),
        "acceptance",
        "defaults".to_string(),
        vec![(":subject".to_string(), Sexpr::sym("app"))],
        vec![],
    );
    let env = extract_execution_env(&acc);

    assert_eq!(
        field(&env, "clock").and_then(|s| s.as_sym()),
        Some("system")
    );
    assert_eq!(
        field(&env, "randomness").and_then(|s| s.as_sym()),
        Some("system")
    );
    assert_eq!(
        field(&env, "network").and_then(|s| s.as_sym()),
        Some("system")
    );
    assert_eq!(
        field(&env, "locale").and_then(|s| s.as_str()),
        Some("en-US")
    );
    assert_eq!(
        field(&env, "timezone").and_then(|s| s.as_str()),
        Some("UTC")
    );

    assert!(!env_deterministic(&env));
    let diags = env_diagnostics(&env, "hand/acceptance/defaults");
    assert_eq!(
        diags.len(),
        3,
        "clock/randomness/network all default to non-deterministic -> exactly 3 warnings, got {:?}",
        diags
    );
    for d in &diags {
        let d: &Sexpr = d;
        assert_eq!(
            d.assoc("severity").and_then(|s| s.as_sym()),
            Some("warning")
        );
        let message = d.assoc("message").and_then(|s| s.as_str()).unwrap_or("");
        assert!(
            message.contains("hand/acceptance/defaults"),
            "each env warning must name the acceptance id in its message, got: {}",
            message
        );
    }
}

// =======================================================================
// 2. Lowering over todo.gym: obligation ids exactly the plan's set;
//    coverage obligation's five flags all truthy; fault obligation's
//    after/inject/assertion all present and distinct.
// =======================================================================

const EXPECTED_OBLIGATION_IDS: &[&str] = &[
    "todo/acceptance/production/property/create_then_read",
    "todo/acceptance/production/property/viewer_cannot_mutate",
    "todo/acceptance/production/scenario/sharing_boundary",
    "todo/acceptance/production/concurrency/boundary_race",
    "todo/acceptance/production/fault/durable_restart",
    "todo/acceptance/production/coverage",
    "todo/invariant/owner_isolation/invariant-check",
    "todo/invariant/sharing_limit/invariant-check",
    "todo/constraint/collaborative_capacity/constraint-check",
];

#[test]
fn oracle_02a_lowering_over_todo_gym_ids_exactly_and_in_order() {
    let ir = load_todo_ir();
    let obligations = lower_all_obligations(&ir);
    assert_eq!(
        obligations.len(),
        9,
        "lower_all_obligations(todo.gym) must yield exactly 9 obligations"
    );
    let ids: Vec<String> = obligations.iter().map(obligation_id).collect();
    let expected: Vec<String> = EXPECTED_OBLIGATION_IDS
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(
        ids, expected,
        "obligation ids and order must exactly match acceptance-clause order, then invariant, then constraint"
    );
}

#[test]
fn oracle_02b_coverage_obligation_all_five_flags_truthy() {
    let ir = load_todo_ir();
    let obligations = lower_all_obligations(&ir);
    let cov = obligations
        .iter()
        .find(|o| obligation_kind(o) == "coverage")
        .expect("todo.gym must lower a coverage obligation");

    for key in [
        "every_operation",
        "every_error",
        "every_transition",
        "every_invariant",
        "boundaries",
    ] {
        let v =
            field(cov, key).unwrap_or_else(|| panic!("coverage obligation missing flag {}", key));
        assert_eq!(v, &truthy(), "flag {} must be truthy", key);
    }
}

#[test]
fn oracle_02c_fault_obligation_after_inject_assertion_present_and_distinct() {
    let ir = load_todo_ir();
    let obligations = lower_all_obligations(&ir);
    let fault = obligations
        .iter()
        .find(|o| obligation_kind(o) == "fault")
        .expect("todo.gym must lower a fault obligation");

    let after = field(fault, "after").expect("fault obligation must carry `after`");
    let inject = field(fault, "inject").expect("fault obligation must carry `inject`");
    let assertion = field(fault, "assertion").expect("fault obligation must carry `assertion`");

    assert_eq!(after, &Sexpr::sym("acknowledged_write"));
    assert_eq!(inject, &Sexpr::sym("restart"));
    assert_eq!(assertion, &Sexpr::sym("read_your_acknowledged_write"));

    // The phase-4 fault-loss regression guard, at the obligation level:
    // after/inject/assertion must all be distinct (none silently
    // collapsed onto another during lowering).
    assert_ne!(after, inject);
    assert_ne!(after, assertion);
    assert_ne!(inject, assertion);
}

// =======================================================================
// 3. Dispatch statuses over todo.gym: both invariants passed;
//    concurrency/fault/coverage/model/constraint skipped with
//    deferred-verification; property/scenario statuses pinned to
//    whatever the reference semantics yield (DERIVED below).
// =======================================================================

#[test]
fn oracle_03a_both_invariants_pass_over_todo_gym() {
    let ir = load_todo_ir();
    let obligations = lower_all_obligations(&ir);
    let invariant_obs: Vec<&Sexpr> = obligations
        .iter()
        .filter(|o| obligation_kind(o) == "invariant")
        .collect();
    assert_eq!(invariant_obs.len(), 2);

    // owner_isolation's :always is the bare atom
    // `no_observation_without_active_membership` -> holds trivially
    // (row 1 of the predicate table) both initially and after every
    // transition, regardless of the transition's effect.
    //
    // sharing_limit's :always is `(forall ((list TodoList)) (<= ...))`
    // -- headed by `forall`, which is not one of the closed evaluator's
    // special heads (=, not, and, or, <, <=) -- so it falls into "row 6"
    // (anything else defaults to true) and ALSO holds trivially, always.
    //
    // So both invariant obligations must dispatch to `passed`.
    for ob in invariant_obs {
        let result = verify_obligation(&ir, ob);
        assert_eq!(
            result_status(&result),
            "passed",
            "{}: expected passed, got {:?}",
            obligation_id(ob),
            result
        );
    }
}

#[test]
fn oracle_03b_concurrency_fault_coverage_constraint_skipped_deferred_verification() {
    let ir = load_todo_ir();
    let obligations = lower_all_obligations(&ir);
    for kind in ["concurrency", "fault", "coverage", "constraint"] {
        let ob = obligations
            .iter()
            .find(|o| obligation_kind(o) == kind)
            .unwrap_or_else(|| panic!("todo.gym must lower a {} obligation", kind));
        let result = verify_obligation(&ir, ob);
        assert_eq!(
            result_status(&result),
            "skipped",
            "{} obligation must be skipped",
            kind
        );
        let diags = field(&result, "diagnostics")
            .and_then(|d| d.as_list())
            .unwrap_or(&[]);
        assert!(
            !diags.is_empty(),
            "{}: skipped result must carry a deferred-verification diagnostic",
            kind
        );
        let joined: String = diags
            .iter()
            .map(|d| d.print())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            joined.contains("deferred-verification"),
            "{}: diagnostic must reference deferred-verification, got {}",
            kind,
            joined
        );
        assert!(
            joined.contains(kind),
            "{}: diagnostic message must name the obligation kind, got {}",
            kind,
            joined
        );
        assert!(
            joined.contains("requires runtime execution"),
            "{}: got {}",
            kind,
            joined
        );
    }
}

#[test]
fn oracle_03c_model_obligation_skipped_deferred_verification_hand_built() {
    // `model` obligations do not occur in todo.gym; dispatch is verified
    // with a hand-built one so the plan's full
    // "concurrency/fault/coverage/model/constraint skipped" clause is
    // actually exercised end to end.
    let ir = load_todo_ir();
    let model_ob = Sexpr::list(vec![
        Sexpr::sym("verification-obligation"),
        Sexpr::pair("id", Sexpr::Str("hand/acceptance/x/model/m".to_string())),
        Sexpr::pair("kind", Sexpr::sym("model")),
        Sexpr::pair("source", Sexpr::Str("hand/acceptance/x".to_string())),
        Sexpr::pair("name", Sexpr::sym("m")),
        Sexpr::pair("spec", nil()),
        Sexpr::pair("environment", nil()),
    ]);
    let result = verify_obligation(&ir, &model_ob);
    assert_eq!(result_status(&result), "skipped");
    let diags = field(&result, "diagnostics")
        .and_then(|d| d.as_list())
        .unwrap_or(&[]);
    let joined: String = diags
        .iter()
        .map(|d| d.print())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(joined.contains("deferred-verification"));
    assert!(joined.contains("model"));
}

// DERIVATION (see the file header, ambiguity 5, and
// `tests/fixtures/todo-ir.sexpr` for the raw clause shapes these read):
//
// create_then_read: :execute ((create_task actor task) (query_tasks
//   actor task/list)). Its first element, (create_task actor task), IS
//   itself a list -> the multi-step case: two trace steps,
//   (create_task actor task) and (query_tasks actor task/list). Each
//   step's op-name (car) is a BARE symbol ("create_task"/"query_tasks")
//   while the only behavior transitions carry QUALIFIED operations
//   ("todo_service/create_task"; there is no "query_tasks" behavior
//   transition at all -- query_tasks is a query in the interface, not a
//   behavior). Operation matching is EXACT (section A) -> both steps
//   produce a no-matching-transition violation -> the trace has 2
//   violations -> the property is `failed`, with 2 counterexamples.
//
// viewer_cannot_mutate: :execute (create_task actor task). Its first
//   element, the symbol `create_task`, is NOT a list -> the single-step
//   case: the whole execute value is the one step, (create_task actor
//   task). Same bare-vs-qualified mismatch -> 1 violation -> `failed`
//   with 1 counterexample.
//
// sharing_boundary (scenario): clause tail is (given ((owner
//   authenticated_owner))) (when (invite_distinct owner 256)) (then
//   succeeds) (when (invite_distinct owner 257)) (then (fails_with
//   sharing_limit)). Per section B, scenario steps are the `when`
//   entries' action lists (each `when`'s second element, here always a
//   list): (invite_distinct owner 256) and (invite_distinct owner 257).
//   op-name "invite_distinct" never matches the qualified
//   "todo_service/invite" -> 2 violations -> `failed` with 2
//   counterexamples.
#[test]
fn oracle_03d_property_and_scenario_statuses_pinned_failed_no_matching_transition() {
    let ir = load_todo_ir();
    let obligations = lower_all_obligations(&ir);
    let by_id = |id: &str| -> &Sexpr {
        obligations
            .iter()
            .find(|o| obligation_id(o) == id)
            .unwrap_or_else(|| panic!("missing obligation {}", id))
    };

    for id in [
        "todo/acceptance/production/property/create_then_read",
        "todo/acceptance/production/property/viewer_cannot_mutate",
        "todo/acceptance/production/scenario/sharing_boundary",
    ] {
        let ob = by_id(id);
        let result = verify_obligation(&ir, ob);
        assert_eq!(
            result_status(&result),
            "failed",
            "{}: expected failed (no-matching-transition), got {:?}",
            id,
            result
        );
    }

    let ctr = verify_obligation(
        &ir,
        by_id("todo/acceptance/production/property/create_then_read"),
    );
    let ctr_ces = field(&ctr, "counterexamples")
        .and_then(|c| c.as_list())
        .expect("counterexamples must be a list");
    assert_eq!(
        ctr_ces.len(),
        2,
        "create_then_read: two unmatched steps -> two counterexamples"
    );

    let vcm = verify_obligation(
        &ir,
        by_id("todo/acceptance/production/property/viewer_cannot_mutate"),
    );
    let vcm_ces = field(&vcm, "counterexamples")
        .and_then(|c| c.as_list())
        .expect("counterexamples must be a list");
    assert_eq!(
        vcm_ces.len(),
        1,
        "viewer_cannot_mutate: one unmatched step -> one counterexample"
    );

    let sb = verify_obligation(
        &ir,
        by_id("todo/acceptance/production/scenario/sharing_boundary"),
    );
    let sb_ces = field(&sb, "counterexamples")
        .and_then(|c| c.as_list())
        .expect("counterexamples must be a list");
    assert_eq!(
        sb_ces.len(),
        2,
        "sharing_boundary: two unmatched `when` steps -> two counterexamples"
    );
}

// =======================================================================
// 4. Trace equivalence: equal traces -> equivalent, no divergences; an
//    outcome mismatch, a state mismatch, and length mismatches each
//    produce their divergence kind; normalized counterexamples carry
//    obligation id, divergence type, and the step projections.
// =======================================================================

#[test]
fn oracle_04a_equal_traces_are_equivalent_no_divergences() {
    let s1 = step(
        "m/behavior/x",
        succeeded(),
        vec![("k".to_string(), Sexpr::Int(1))],
        vec![("k".to_string(), Sexpr::Int(2))],
    );
    let ref_steps = vec![s1.clone()];
    let impl_steps = vec![s1];
    let divs = compare_traces(&ref_steps, &impl_steps);
    assert!(divs.is_empty());
}

#[test]
fn oracle_04b_trace_equivalence_result_true_for_identical_traces() {
    let ir = load_todo_ir();
    let s1 = step(
        "m/behavior/x",
        succeeded(),
        vec![("k".to_string(), Sexpr::Int(1))],
        vec![("k".to_string(), Sexpr::Int(2))],
    );
    let final_state = vec![("k".to_string(), Sexpr::Int(2))];
    let t1 = Trace {
        steps: vec![s1.clone()],
        violations: vec![],
        final_state: final_state.clone(),
    };
    let t2 = Trace {
        steps: vec![s1],
        violations: vec![],
        final_state,
    };

    let result = trace_equivalence_result(&ir, &t1, &t2, "ob-1");
    assert_eq!(
        field(&result, "obligation-id").and_then(|s| s.as_str()),
        Some("ob-1")
    );
    assert_eq!(field(&result, "equivalent"), Some(&truthy()));
    let divs = field(&result, "divergences")
        .and_then(|d| d.as_list())
        .expect("divergences must be a list");
    assert!(divs.is_empty());
    assert!(field(&result, "reference-violations")
        .and_then(|v| v.as_list())
        .expect("reference-violations must be a list")
        .is_empty());
    assert!(field(&result, "implementation-violations")
        .and_then(|v| v.as_list())
        .expect("implementation-violations must be a list")
        .is_empty());
}

#[test]
fn oracle_04c_trace_equivalence_result_false_with_divergences_when_traces_differ() {
    let ir = load_todo_ir();
    let ref_step = step(
        "m/behavior/x",
        succeeded(),
        vec![],
        vec![("k".to_string(), Sexpr::Int(2))],
    );
    let impl_step = step("m/behavior/x", failed("forbidden"), vec![], vec![]);
    let t1 = Trace {
        steps: vec![ref_step],
        violations: vec![],
        final_state: vec![],
    };
    let t2 = Trace {
        steps: vec![impl_step],
        violations: vec![],
        final_state: vec![],
    };

    let result = trace_equivalence_result(&ir, &t1, &t2, "ob-2");
    assert_eq!(field(&result, "equivalent"), Some(&nil()));
    let divs = field(&result, "divergences")
        .and_then(|d| d.as_list())
        .expect("divergences must be a list");
    assert_eq!(divs.len(), 1);
    assert_eq!(
        field(&divs[0], "type").and_then(|s| s.as_sym()),
        Some("outcome-mismatch")
    );
}

#[test]
fn oracle_04d_outcome_mismatch_divergence_kind() {
    let pre = vec![("k".to_string(), Sexpr::Int(1))];
    let post = vec![("k".to_string(), Sexpr::Int(2))];
    let ref_step = step("m/behavior/x", succeeded(), pre.clone(), post.clone());
    let impl_step = step("m/behavior/x", failed("forbidden"), pre, post);

    let divs = compare_traces(&[ref_step], &[impl_step]);
    assert_eq!(divs.len(), 1);
    assert_eq!(
        field(&divs[0], "type").and_then(|s| s.as_sym()),
        Some("outcome-mismatch")
    );
    assert_eq!(field(&divs[0], "reference"), Some(&succeeded()));
    assert_eq!(
        field(&divs[0], "implementation"),
        Some(&failed("forbidden"))
    );
}

#[test]
fn oracle_04e_state_mismatch_divergence_kind() {
    let pre = vec![("k".to_string(), Sexpr::Int(1))];
    let ref_step = step(
        "m/behavior/x",
        succeeded(),
        pre.clone(),
        vec![("k".to_string(), Sexpr::Int(2))],
    );
    let impl_step = step(
        "m/behavior/x",
        succeeded(),
        pre,
        vec![("k".to_string(), Sexpr::Int(99))],
    );

    let divs = compare_traces(&[ref_step], &[impl_step]);
    assert_eq!(divs.len(), 1);
    assert_eq!(
        field(&divs[0], "type").and_then(|s| s.as_sym()),
        Some("state-mismatch")
    );
}

#[test]
fn oracle_04f_missing_implementation_steps_when_impl_shorter() {
    let s = step("m/behavior/x", succeeded(), vec![], vec![]);
    let ref_steps = vec![s.clone(), s];
    let divs = compare_traces(&ref_steps, &[]);
    assert_eq!(divs.len(), 1);
    assert_eq!(
        field(&divs[0], "type").and_then(|s| s.as_sym()),
        Some("missing-implementation-steps")
    );
    assert_eq!(field(&divs[0], "count"), Some(&Sexpr::Int(2)));
}

#[test]
fn oracle_04g_extra_implementation_steps_when_impl_longer() {
    let s = step("m/behavior/x", succeeded(), vec![], vec![]);
    let impl_steps = vec![s.clone(), s.clone(), s];
    let divs = compare_traces(&[], &impl_steps);
    assert_eq!(divs.len(), 1);
    assert_eq!(
        field(&divs[0], "type").and_then(|s| s.as_sym()),
        Some("extra-implementation-steps")
    );
    assert_eq!(field(&divs[0], "count"), Some(&Sexpr::Int(3)));
}

#[test]
fn oracle_04h_normalize_counterexample_outcome_mismatch_carries_step_projections() {
    let pre = vec![("k".to_string(), Sexpr::Int(1))];
    let post = vec![("k".to_string(), Sexpr::Int(2))];
    let ref_step = step("m/behavior/x", succeeded(), pre.clone(), post.clone());
    let impl_step = step("m/behavior/x", failed("forbidden"), pre.clone(), post);

    let divs = compare_traces(&[ref_step.clone()], &[impl_step]);
    assert_eq!(divs.len(), 1);

    let ce = normalize_counterexample(&divs[0], "ob-outcome");
    assert_eq!(
        field(&ce, "obligation-id").and_then(|s| s.as_str()),
        Some("ob-outcome")
    );
    assert_eq!(
        field(&ce, "divergence-type").and_then(|s| s.as_sym()),
        Some("outcome-mismatch")
    );
    assert_eq!(
        field(&ce, "operation").and_then(|s| s.as_str()),
        Some("m/behavior/x"),
        "operation must be the REFERENCE step's transition id"
    );
    assert_eq!(field(&ce, "actor"), Some(ref_step.actor.as_ref().unwrap()));
    assert_eq!(field(&ce, "input"), Some(ref_step.input.as_ref().unwrap()));
    assert_eq!(
        field(&ce, "pre-state"),
        Some(&state_sexpr(&ref_step.pre_state))
    );
    assert_eq!(field(&ce, "expected"), Some(&succeeded()));
    assert_eq!(field(&ce, "actual"), Some(&failed("forbidden")));
}

#[test]
fn oracle_04i_normalize_counterexample_state_mismatch_expected_actual_empty_reference_quirk() {
    // KNOWN REFERENCE QUIRK, ported verbatim (the plan: "port
    // structurally 1:1"): normalize-counterexample always reads keys
    // 'reference / 'implementation off the divergence, but a
    // state-mismatch divergence stores its values under
    // 'reference-state / 'implementation-state instead -- so
    // `expected`/`actual` come back empty (nil) for a state-mismatch
    // counterexample, even though the divergence itself does carry the
    // real states, just under different keys.
    let pre = vec![("k".to_string(), Sexpr::Int(1))];
    let ref_step = step(
        "m/behavior/x",
        succeeded(),
        pre.clone(),
        vec![("k".to_string(), Sexpr::Int(2))],
    );
    let impl_step = step(
        "m/behavior/x",
        succeeded(),
        pre,
        vec![("k".to_string(), Sexpr::Int(99))],
    );

    let divs = compare_traces(&[ref_step.clone()], &[impl_step]);
    assert_eq!(divs.len(), 1);

    let ce = normalize_counterexample(&divs[0], "ob-state");
    assert_eq!(
        field(&ce, "obligation-id").and_then(|s| s.as_str()),
        Some("ob-state")
    );
    assert_eq!(
        field(&ce, "divergence-type").and_then(|s| s.as_sym()),
        Some("state-mismatch")
    );
    assert_eq!(
        field(&ce, "operation").and_then(|s| s.as_str()),
        Some("m/behavior/x")
    );
    assert_eq!(
        field(&ce, "pre-state"),
        Some(&state_sexpr(&ref_step.pre_state))
    );
    assert_eq!(
        field(&ce, "expected"),
        Some(&nil()),
        "reference quirk: looked up under the wrong key, so this is nil"
    );
    assert_eq!(
        field(&ce, "actual"),
        Some(&nil()),
        "reference quirk: looked up under the wrong key, so this is nil"
    );
}

#[test]
fn oracle_04j_normalize_counterexample_length_mismatch_has_no_step_projections() {
    let s = step("m/behavior/x", succeeded(), vec![], vec![]);
    let divs = compare_traces(&[s.clone(), s], &[]);
    assert_eq!(divs.len(), 1);

    let ce = normalize_counterexample(&divs[0], "ob-len");
    assert_eq!(
        field(&ce, "divergence-type").and_then(|s| s.as_sym()),
        Some("missing-implementation-steps")
    );
    assert_eq!(field(&ce, "operation"), Some(&nil()));
    assert_eq!(field(&ce, "actor"), Some(&nil()));
    assert_eq!(field(&ce, "input"), Some(&nil()));
    assert_eq!(field(&ce, "pre-state"), Some(&nil()));
}

#[test]
fn oracle_04k_normalize_counterexamples_plural_maps_over_all_divergences() {
    let s = step("m/behavior/x", succeeded(), vec![], vec![]);
    let length_divs = compare_traces(&[s.clone(), s], &[]);

    let s2 = step(
        "m/behavior/y",
        succeeded(),
        vec![],
        vec![("k".to_string(), Sexpr::Int(1))],
    );
    let s2b = step(
        "m/behavior/y",
        failed("x"),
        vec![],
        vec![("k".to_string(), Sexpr::Int(1))],
    );
    let outcome_divs = compare_traces(&[s2], &[s2b]);

    let mut all = length_divs;
    all.extend(outcome_divs);
    assert_eq!(all.len(), 2);

    let ces = normalize_counterexamples(&all, "ob-multi");
    assert_eq!(ces.len(), 2);
    for ce in &ces {
        assert_eq!(
            field(ce, "obligation-id").and_then(|s| s.as_str()),
            Some("ob-multi")
        );
    }
    assert_eq!(
        field(&ces[0], "divergence-type").and_then(|s| s.as_sym()),
        Some("missing-implementation-steps")
    );
    assert_eq!(
        field(&ces[1], "divergence-type").and_then(|s| s.as_sym()),
        Some("outcome-mismatch")
    );
}

// =======================================================================
// 5. Coverage analysis over todo.gym: counts (2 property + 1 scenario +
//    1 fault = 4 total, 2 transitions, 2 invariants) and the resulting
//    gaps list computed per the reference logic (DERIVED below).
// =======================================================================

#[test]
fn oracle_05_coverage_analysis_over_todo_gym_counts_and_single_gap() {
    let ir = load_todo_ir();
    let obligations = lower_all_obligations(&ir);
    let analysis = coverage_gaps(&ir, &obligations);

    assert_eq!(
        field(&analysis, "property-obligations"),
        Some(&Sexpr::Int(2))
    );
    assert_eq!(
        field(&analysis, "scenario-obligations"),
        Some(&Sexpr::Int(1))
    );
    assert_eq!(field(&analysis, "fault-obligations"), Some(&Sexpr::Int(1)));
    // "total-obligations" here is the reference's `covered-count`
    // (property + scenario + fault = 2 + 1 + 1 = 4) -- NOT the bundle's
    // overall obligation total of 9 (see oracle_06 for that one).
    assert_eq!(field(&analysis, "total-obligations"), Some(&Sexpr::Int(4)));
    assert_eq!(
        field(&analysis, "transitions-defined"),
        Some(&Sexpr::Int(2))
    );
    assert_eq!(field(&analysis, "invariants-defined"), Some(&Sexpr::Int(2)));

    // DERIVATION of the single expected gap (todo.gym's coverage clause
    // sets every_operation / every_error / every_transition /
    // every_invariant all to truthy):
    //   uncovered-transitions: transitions(2) > covered(4)? false -> no gap.
    //   uncovered-operations:  behaviors(2) > (property+scenario=3)? false -> no gap.
    //   uncovered-error-paths: behaviors(2) > fault_obs(1)? TRUE -> gap, count = 2-1 = 1.
    //   uncovered-invariants:  invariants(2) > invariant_obs(2)? false -> no gap.
    // So gaps == [(gap uncovered-error-paths 1)] exactly.
    let gaps = field(&analysis, "gaps")
        .and_then(|g| g.as_list())
        .expect("gaps must be a list");
    assert_eq!(
        gaps.len(),
        1,
        "exactly one gap: uncovered-error-paths, got {:?}",
        gaps
    );
    let items = gaps[0].as_list().expect("gap entry must be a list");
    assert_eq!(items.first().and_then(|s| s.as_sym()), Some("gap"));
    assert_eq!(
        items.get(1).and_then(|s| s.as_sym()),
        Some("uncovered-error-paths")
    );
    assert_eq!(items.get(2), Some(&Sexpr::Int(1)));
}

#[test]
fn oracle_05b_coverage_analysis_absent_when_no_coverage_obligation() {
    // With no coverage obligation present at all, the reference returns
    // nil (no coverage-analysis form). Constructed by filtering todo.gym's
    // own obligations list down to exclude the coverage one.
    let ir = load_todo_ir();
    let obligations: Vec<Sexpr> = lower_all_obligations(&ir)
        .into_iter()
        .filter(|o| obligation_kind(o) != "coverage")
        .collect();
    let analysis = coverage_gaps(&ir, &obligations);
    assert_eq!(
        analysis,
        nil(),
        "no coverage obligation -> nil coverage-analysis"
    );
}

// =======================================================================
// 6. Bundle: summary total == obligations len == 9; passed+failed+
//    skipped == total; determinism (two compile_verification runs
//    byte-identical); schema present.
// =======================================================================

#[test]
fn oracle_06a_bundle_summary_and_schema_over_todo_gym() {
    let ir = load_todo_ir();
    let bundle = compile_verification(&ir);

    assert_eq!(
        field(&bundle, "schema").and_then(|s| s.as_str()),
        Some("gymnast.verify/0.1")
    );

    let obligations = field(&bundle, "obligations")
        .and_then(|o| o.as_list())
        .expect("obligations must be a list");
    assert_eq!(obligations.len(), 9);

    let summary = field(&bundle, "summary").expect("bundle must carry a summary");
    let total = field(summary, "total").and_then(|s| s.as_int()).unwrap();
    let passed = field(summary, "passed").and_then(|s| s.as_int()).unwrap();
    let failed = field(summary, "failed").and_then(|s| s.as_int()).unwrap();
    let skipped = field(summary, "skipped").and_then(|s| s.as_int()).unwrap();

    assert_eq!(total, 9);
    assert_eq!(passed + failed + skipped, total);

    // Derived exactly (see oracle_03* above): 2 invariants pass, 2
    // properties + 1 scenario fail (bare-vs-qualified
    // no-matching-transition), 1 concurrency + 1 fault + 1 coverage + 1
    // constraint are skipped.
    assert_eq!(passed, 2);
    assert_eq!(failed, 3);
    assert_eq!(skipped, 4);

    let results = field(&bundle, "results")
        .and_then(|r| r.as_list())
        .expect("results must be a list");
    assert_eq!(results.len(), 9);
}

#[test]
fn oracle_06b_bundle_determinism_two_runs_byte_identical() {
    let ir1 = load_todo_ir();
    let ir2 = load_todo_ir();
    let b1 = compile_verification(&ir1);
    let b2 = compile_verification(&ir2);
    assert_eq!(canonical_serialize(&b1), canonical_serialize(&b2));
}

// =======================================================================
// 7. CLI: verify on todo.gym exits 0 and stdout parses via sexpr::parse;
//    verify on a spec with an IR error exits 1.
// =======================================================================

fn run_verify_cli(source_path: &str) -> (i32, String, String) {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_gymnast-rs"))
        .args(["verify", source_path])
        .output()
        .expect("run gymnast-rs verify");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn oracle_07a_cli_verify_todo_gym_exits_0_stdout_parses() {
    let (code, stdout, stderr) = run_verify_cli("../examples/todo.gym");
    assert_eq!(
        code, 0,
        "verify on a clean spec must exit 0, stderr: {}",
        stderr
    );
    let parsed = sexpr::parse(stdout.trim_end());
    assert!(
        parsed.is_ok(),
        "verify stdout must parse as a canonical Sexpr, got err: {:?}\nstdout: {}",
        parsed.err(),
        stdout
    );
    let bundle = parsed.unwrap();
    assert_eq!(
        bundle
            .as_list()
            .and_then(|l| l.first())
            .and_then(|s| s.as_sym()),
        Some("verification-bundle")
    );
}

#[test]
fn oracle_07b_cli_verify_exits_1_on_ir_error() {
    // Same known-bad fixture family used by plan_oracle_test.rs /
    // recipe_oracle_test.rs: a duplicate semantic id (E301).
    const KNOWN_BAD_SPEC: &str = "spec t = v 0.1 owner o exports A\n\nmode A = opaque text\ninv a = on s always p\ninv a = on s always q\n";
    let unique = std::process::id();
    let bad_path = std::env::temp_dir().join(format!("gymnast-verify-bad-{}.gym", unique));
    fs::write(&bad_path, KNOWN_BAD_SPEC).unwrap();

    let (code, _stdout, stderr) = run_verify_cli(bad_path.to_str().unwrap());
    assert_eq!(
        code, 1,
        "verify must exit 1 on invalid IR, stderr: {}",
        stderr
    );

    fs::remove_file(&bad_path).ok();
}
