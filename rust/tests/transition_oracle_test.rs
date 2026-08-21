//! Tests-of-record for `transition.rs`, authored from
//! `docs/rust-port-plan-phase6.md` section A and its "Oracle tests" ->
//! `transition_oracle_test.rs` list ALONE, BEFORE any implementation of
//! `crate::transition` exists (the phase-4+ committed-oracle upgrade:
//! Stage 1 commits this file to git before any implementation stage
//! runs). `src/transition.lisp` was consulted only for BEHAVIORAL
//! INTENT; every Rust-IR shape adaptation comes from the phase-6 plan's
//! explicit table, never guessed from the Lamedh golden.
//!
//! Implementation stages MUST NOT edit this file. If an assertion here
//! turns out to conflict with a considered implementation choice, the
//! implementer reports the conflict; only the integrator resolves it.
//!
//! This file will not compile until `crate::transition` exists -- that
//! is expected at this stage.
//!
//! Numbering follows the plan's `transition_oracle_test.rs` list items
//! 1-6 exactly; each gets one or more `#[test]`s, none merged or
//! dropped. A few `bonus_*` tests exercise adjacent behavior the plan
//! documents in the same prose paragraph as a numbered item but does
//! not itself number; they are supplementary, not substitutes.
//!
//! RESOLVED AMBIGUITIES (plan text under-specifies these; the
//! contract-consistent reading taken here is noted at each site and
//! summarized in the stage report):
//!
//!  1. Trace-step embedded Sexpr shapes for `violation` / `counterexample`
//!     / `execute_trace`'s `Trace` type are exercised strictly through
//!     the public functions (`check_invariants`, `execute_trace`) rather
//!     than hand-assembled, so no assumption about their internal
//!     encoding is required except where the plan gives a literal,
//!     paren-balanced example -- `(violation (invariant id) (predicate
//!     p) (state ...))` is one (a FLAT tag-plus-sibling-pairs shape, no
//!     extra wrapping list), confirmed by counting the plan text's own
//!     parentheses; that flat shape is used directly via `Sexpr::assoc`
//!     below. A resilient `field()` helper (tries flat first, then one
//!     level of nesting) is still used everywhere for defense-in-depth
//!     consistency with `verify_oracle_test.rs`, where the ambiguity is
//!     real (see that file's header) -- it costs nothing when the flat
//!     reading already holds.
//!  2. `Transition`'s `id`/obligation-style string fields are asserted as
//!     `Sexpr::Str`, matching this crate's established convention that
//!     every semantic id (`IrNode::id`, plan-node ids, obligation ids
//!     via the Lamedh reference's `concat`) serializes as `Sexpr::Str`,
//!     never `Sexpr::Sym`, even though ids contain only symbol-safe
//!     characters.

use gymnast_rs::elaborate;
use gymnast_rs::ir::{Ir, IrNode};
use gymnast_rs::parser;
use gymnast_rs::sexpr::Sexpr;
use gymnast_rs::transition::{
    apply_transition, check_invariants, eval_expr, eval_predicate, execute_trace,
    extract_transitions, make_initial_state, State, Transition, TRACE_BOUND,
};
use std::fs;

// ---------------------------------------------------------------------
// Shared fixtures / helpers (not tests themselves).
// ---------------------------------------------------------------------

/// Parses and elaborates `examples/todo.gym` fresh.
fn load_todo_ir() -> Ir {
    let src = fs::read_to_string("../examples/todo.gym").expect("read ../examples/todo.gym");
    let (ast, parse_diags) = parser::parse(&src);
    let file = ast.expect("parse todo.gym");
    let (ir, _all_diags) = elaborate::elaborate_with_parse_diags(&file, &parse_diags);
    ir
}

/// Field lookup that tolerates either FLAT (`(tag (k1 v1) (k2 v2) ...)`)
/// or one-level-NESTED (`(tag ((k1 v1) (k2 v2) ...))`) renderings of an
/// ad-hoc verify/transition Sexpr shape. See the file header note 1 and
/// `verify_oracle_test.rs`'s header for why this matters and where it is
/// actually load-bearing (mostly the sibling verify file; kept here too
/// for a single consistent idiom across both oracle files).
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

fn state_sexpr(state: &State) -> Sexpr {
    Sexpr::list(
        state
            .iter()
            .map(|entry: &(String, Sexpr)| Sexpr::list(vec![Sexpr::sym(&entry.0), entry.1.clone()]))
            .collect(),
    )
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

fn invariant_node(id: &str, name: &str, always: Sexpr) -> IrNode {
    IrNode::new(
        id.to_string(),
        "invariant",
        name.to_string(),
        vec![
            (":always".to_string(), always),
            (":scope".to_string(), Sexpr::sym("some_state")),
        ],
        vec![],
    )
}

fn state_node(id: &str, name: &str, initial: Sexpr) -> IrNode {
    IrNode::new(
        id.to_string(),
        "state",
        name.to_string(),
        vec![(":initial".to_string(), initial)],
        vec![],
    )
}

fn empty_ir(module: &str) -> Ir {
    Ir::new(
        "gymnast.ir/0.1".to_string(),
        module.to_string(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    )
}

// =======================================================================
// 1. Extraction over todo.gym: 2 transitions; create_task's operation
//    `todo_service/create_task`, actor `user`, input `request`, reads
//    `[memberships, todo_lists]`, writes `[tasks]`, 2 preconditions,
//    1 postcondition, result present, 1 failure, 1 emission.
// =======================================================================

#[test]
fn oracle_01a_extraction_over_todo_yields_exactly_two_transitions() {
    let ir = load_todo_ir();
    let transitions = extract_transitions(&ir);
    assert_eq!(
        transitions.len(),
        2,
        "todo.gym has exactly two `behavior` nodes"
    );
    // The transitions partition is id-sorted ("behavior kind, all_nodes
    // order"); "todo/behavior/create_task" < "todo/behavior/invite_user"
    // byte-wise, so create_task comes first.
    assert_eq!(transitions[0].id, "todo/behavior/create_task");
    assert_eq!(transitions[1].id, "todo/behavior/invite_user");
}

#[test]
fn oracle_01b_create_task_on_spec_operation_actor_input() {
    let ir = load_todo_ir();
    let transitions = extract_transitions(&ir);
    let ct = &transitions[0];
    // :on (todo_service/create_task user request) -- the slash is
    // already joined into one symbol in the Rust surface.
    assert_eq!(ct.operation, "todo_service/create_task");
    assert_eq!(ct.actor.as_deref(), Some("user"));
    assert_eq!(ct.input.as_deref(), Some("request"));
}

#[test]
fn oracle_01c_create_task_reads_and_writes() {
    let ir = load_todo_ir();
    let transitions = extract_transitions(&ir);
    let ct = &transitions[0];
    assert_eq!(
        ct.reads,
        vec!["memberships".to_string(), "todo_lists".to_string()]
    );
    assert_eq!(ct.writes, vec!["tasks".to_string()]);
}

#[test]
fn oracle_01d_create_task_precondition_postcondition_counts_and_content() {
    let ir = load_todo_ir();
    let transitions = extract_transitions(&ir);
    let ct = &transitions[0];

    assert_eq!(ct.preconditions.len(), 2);
    assert_eq!(
        ct.preconditions[0],
        call("authenticated", vec![Sexpr::sym("user")])
    );
    assert_eq!(
        ct.preconditions[1],
        call(
            "may_edit_list",
            vec![
                Sexpr::sym("pre"),
                Sexpr::sym("user"),
                Sexpr::sym("request/list")
            ]
        )
    );

    assert_eq!(ct.postconditions.len(), 1);
    assert_eq!(
        ct.postconditions[0],
        call(
            "=",
            vec![
                Sexpr::sym("post"),
                call(
                    "insert_task",
                    vec![
                        Sexpr::sym("pre"),
                        Sexpr::sym("request"),
                        Sexpr::sym("result")
                    ]
                )
            ]
        )
    );
}

#[test]
fn oracle_01e_create_task_result_present_and_unwrapped() {
    let ir = load_todo_ir();
    let transitions = extract_transitions(&ir);
    let ct = &transitions[0];
    // `(returns result)` -> the clause's expr, unwrapped to the bare
    // symbol (the Rust adaptation: "(returns <expr>) -> the expr").
    assert_eq!(ct.result, Some(Sexpr::sym("result")));
}

#[test]
fn oracle_01f_create_task_failure_and_emission_kept_whole() {
    let ir = load_todo_ir();
    let transitions = extract_transitions(&ir);
    let ct = &transitions[0];

    assert_eq!(ct.failures.len(), 1);
    // (fails forbidden :when (not (may_edit_list pre user request/list))
    //  :preserves all_state) -> the whole tail after `fails`, kept whole.
    assert_eq!(
        ct.failures[0],
        Sexpr::list(vec![
            Sexpr::sym("forbidden"),
            Sexpr::sym(":when"),
            call(
                "not",
                vec![call(
                    "may_edit_list",
                    vec![
                        Sexpr::sym("pre"),
                        Sexpr::sym("user"),
                        Sexpr::sym("request/list")
                    ]
                )]
            ),
            Sexpr::sym(":preserves"),
            Sexpr::sym("all_state"),
        ])
    );

    assert_eq!(ct.emissions.len(), 1);
    // (emits task_created exactly_once_logically) -> the whole tail.
    assert_eq!(
        ct.emissions[0],
        Sexpr::list(vec![
            Sexpr::sym("task_created"),
            Sexpr::sym("exactly_once_logically"),
        ])
    );
}

#[test]
fn oracle_01g_invite_user_shape_sanity() {
    // The plan's item 1 focuses on create_task; invite_user is checked
    // lightly here for the shared shape rules (operation/actor/input/
    // reads/writes), still part of "extraction over todo.gym".
    let ir = load_todo_ir();
    let transitions = extract_transitions(&ir);
    let iu = &transitions[1];
    assert_eq!(iu.id, "todo/behavior/invite_user");
    assert_eq!(iu.operation, "todo_service/invite");
    assert_eq!(iu.actor.as_deref(), Some("user"));
    assert_eq!(iu.input.as_deref(), Some("request"));
    assert_eq!(
        iu.reads,
        vec!["memberships".to_string(), "invitations".to_string()]
    );
    assert_eq!(iu.writes, vec!["invitations".to_string()]);
    assert_eq!(iu.preconditions.len(), 2);
    assert_eq!(iu.postconditions.len(), 1);
    assert_eq!(iu.result, None, "invite_user has no `returns` clause");
    assert_eq!(iu.failures.len(), 1);
    assert_eq!(iu.emissions.len(), 0, "invite_user has no `emits` clause");
}

// =======================================================================
// 2. Evaluator table: every row of the pred/expr tables as direct
//    assertions, including the non-Int comparison -> false delta and the
//    unknown-predicate -> true default.
// =======================================================================

#[test]
fn oracle_02a_pred_nil_and_atoms_are_true() {
    let state = sample_state();
    assert!(eval_predicate(&nil(), &state, None, None), "nil -> true");
    assert!(eval_predicate(&Sexpr::sym("some_atom"), &state, None, None));
    assert!(eval_predicate(&Sexpr::Int(0), &state, None, None));
    assert!(eval_predicate(
        &Sexpr::Str("x".to_string()),
        &state,
        None,
        None
    ));
}

#[test]
fn oracle_02b_pred_eq_structural() {
    let state = sample_state();
    // foo evals to 42 (state lookup); compared against the literal 42.
    assert!(eval_predicate(
        &call("=", vec![Sexpr::Int(42), Sexpr::sym("foo")]),
        &state,
        None,
        None
    ));
    assert!(!eval_predicate(
        &call("=", vec![Sexpr::Int(1), Sexpr::sym("foo")]),
        &state,
        None,
        None
    ));
    // Structural equality over lists too (eval_expr of a list is
    // verbatim, so two syntactically-equal lists compare equal).
    let l = Sexpr::list(vec![Sexpr::sym("a"), Sexpr::Int(1)]);
    assert!(eval_predicate(
        &call("=", vec![l.clone(), l]),
        &state,
        None,
        None
    ));
}

#[test]
fn oracle_02c_pred_not() {
    let state = sample_state();
    assert!(eval_predicate(
        &call("not", vec![call("=", vec![Sexpr::Int(1), Sexpr::Int(2)])]),
        &state,
        None,
        None
    ));
    assert!(!eval_predicate(
        &call("not", vec![call("=", vec![Sexpr::Int(1), Sexpr::Int(1)])]),
        &state,
        None,
        None
    ));
}

#[test]
fn oracle_02d_pred_and_or() {
    let state = sample_state();

    // and: all true (atoms are always true).
    assert!(eval_predicate(
        &call("and", vec![Sexpr::sym("a1"), Sexpr::sym("a2")]),
        &state,
        None,
        None
    ));
    // and: one false.
    assert!(!eval_predicate(
        &call(
            "and",
            vec![
                Sexpr::sym("a1"),
                call("=", vec![Sexpr::Int(1), Sexpr::Int(2)])
            ]
        ),
        &state,
        None,
        None
    ));
    // and with zero args: vacuously true (all of an empty tail holds).
    assert!(eval_predicate(
        &Sexpr::list(vec![Sexpr::sym("and")]),
        &state,
        None,
        None
    ));

    // or: any true.
    assert!(eval_predicate(
        &call(
            "or",
            vec![
                call("=", vec![Sexpr::Int(1), Sexpr::Int(2)]),
                Sexpr::sym("a1")
            ]
        ),
        &state,
        None,
        None
    ));
    // or: all false.
    assert!(!eval_predicate(
        &call("or", vec![call("=", vec![Sexpr::Int(1), Sexpr::Int(2)])]),
        &state,
        None,
        None
    ));
    // or with zero args: vacuously false (any of an empty tail fails).
    assert!(!eval_predicate(
        &Sexpr::list(vec![Sexpr::sym("or")]),
        &state,
        None,
        None
    ));
}

#[test]
fn oracle_02e_pred_lt_le_int_only_else_false_delta() {
    let state = sample_state();

    assert!(eval_predicate(
        &call("<", vec![Sexpr::Int(1), Sexpr::Int(2)]),
        &state,
        None,
        None
    ));
    assert!(!eval_predicate(
        &call("<", vec![Sexpr::Int(2), Sexpr::Int(1)]),
        &state,
        None,
        None
    ));
    assert!(!eval_predicate(
        &call("<", vec![Sexpr::Int(2), Sexpr::Int(2)]),
        &state,
        None,
        None
    ));
    assert!(eval_predicate(
        &call("<=", vec![Sexpr::Int(2), Sexpr::Int(2)]),
        &state,
        None,
        None
    ));
    assert!(!eval_predicate(
        &call("<=", vec![Sexpr::Int(3), Sexpr::Int(2)]),
        &state,
        None,
        None
    ));

    // DELTA (documented in the plan): Lamedh would error on a
    // non-number; the Rust evaluator is TOTAL and returns false when
    // either side does not evaluate to an Int.
    assert!(!eval_predicate(
        &call("<", vec![Sexpr::Str("a".to_string()), Sexpr::Int(2)]),
        &state,
        None,
        None
    ));
    // `bar` evals (via state lookup) to a Str, not an Int.
    assert!(!eval_predicate(
        &call("<", vec![Sexpr::Int(1), Sexpr::sym("bar")]),
        &state,
        None,
        None
    ));
    // An unbound symbol evals to itself (a Sym), not an Int.
    assert!(!eval_predicate(
        &call("<=", vec![Sexpr::sym("unbound_symbol"), Sexpr::Int(2)]),
        &state,
        None,
        None
    ));
}

#[test]
fn oracle_02f_pred_unknown_head_defaults_true() {
    let state = sample_state();
    // forall / any unrecognized call head: symbolic default of true.
    assert!(eval_predicate(
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
    ));
    assert!(eval_predicate(
        &call(
            "some_unregistered_call",
            vec![Sexpr::sym("x"), Sexpr::sym("y")]
        ),
        &state,
        None,
        None
    ));
}

#[test]
fn oracle_02g_expr_int_str_evaluate_to_themselves() {
    let state = sample_state();
    assert_eq!(eval_expr(&Sexpr::Int(7), &state, None, None), Sexpr::Int(7));
    assert_eq!(
        eval_expr(&Sexpr::Str("hi".to_string()), &state, None, None),
        Sexpr::Str("hi".to_string())
    );
}

#[test]
fn oracle_02h_expr_pre_post_yield_state_as_assoc_sexpr() {
    let state = sample_state();
    let expected = state_sexpr(&state);
    assert_eq!(eval_expr(&Sexpr::sym("pre"), &state, None, None), expected);
    assert_eq!(eval_expr(&Sexpr::sym("post"), &state, None, None), expected);
}

#[test]
fn oracle_02i_expr_actor_input_given_value_or_nil() {
    let state = sample_state();
    let actor_val = Sexpr::sym("user-123");
    let input_val = Sexpr::sym("req-456");
    assert_eq!(
        eval_expr(
            &Sexpr::sym("actor"),
            &state,
            Some(&actor_val),
            Some(&input_val)
        ),
        actor_val.clone()
    );
    assert_eq!(
        eval_expr(
            &Sexpr::sym("input"),
            &state,
            Some(&actor_val),
            Some(&input_val)
        ),
        input_val.clone()
    );
    assert_eq!(eval_expr(&Sexpr::sym("actor"), &state, None, None), nil());
    assert_eq!(eval_expr(&Sexpr::sym("input"), &state, None, None), nil());
}

#[test]
fn oracle_02j_expr_result_is_the_placeholder_symbol() {
    let state = sample_state();
    assert_eq!(
        eval_expr(&Sexpr::sym("result"), &state, None, None),
        Sexpr::sym("result-placeholder")
    );
    // Regardless of actor/input being present.
    let a = Sexpr::sym("a");
    let i = Sexpr::sym("i");
    assert_eq!(
        eval_expr(&Sexpr::sym("result"), &state, Some(&a), Some(&i)),
        Sexpr::sym("result-placeholder")
    );
}

#[test]
fn oracle_02k_expr_other_symbol_state_lookup_or_itself() {
    let state = sample_state();
    assert_eq!(
        eval_expr(&Sexpr::sym("foo"), &state, None, None),
        Sexpr::Int(42)
    );
    assert_eq!(
        eval_expr(&Sexpr::sym("unbound"), &state, None, None),
        Sexpr::sym("unbound")
    );
}

#[test]
fn oracle_02l_expr_list_is_verbatim_never_recursively_evaluated() {
    let state = sample_state();
    // Contains `foo`, which WOULD evaluate to 42 if recursed into --
    // but lists evaluate to themselves, verbatim.
    let list_expr = Sexpr::list(vec![
        Sexpr::sym("insert_task"),
        Sexpr::sym("pre"),
        Sexpr::sym("foo"),
    ]);
    assert_eq!(eval_expr(&list_expr, &state, None, None), list_expr);
}

// =======================================================================
// 3. apply_transition: failure-clause precedence over preconditions; a
//    holding `:when` yields `(failed <error>)` with state preserved;
//    passing preconditions appends input to every writes entry; failing
//    preconditions yields `(precondition-failed)` with state unchanged.
// =======================================================================

#[test]
fn oracle_03a_failure_precedes_preconditions_state_preserved() {
    let state: State = vec![("items".to_string(), nil())];
    // The `:when` predicate is a bare atom -> always holds.
    let failure = Sexpr::list(vec![
        Sexpr::sym("forbidden"),
        Sexpr::sym(":when"),
        Sexpr::sym("t"),
        Sexpr::sym(":preserves"),
        Sexpr::sym("all_state"),
    ]);
    // Preconditions would ALSO hold here (also a bare atom) -- proving
    // the failure clause takes precedence, not just that it's the only
    // path.
    let t = make_transition(
        "m/behavior/x",
        "svc/op",
        Some("actor"),
        Some("input"),
        vec!["items"],
        vec!["items"],
        vec![Sexpr::sym("some_precondition")],
        vec![failure],
    );
    let actor = Sexpr::sym("user1");
    let input = Sexpr::sym("payload");
    let step = apply_transition(&t, &state, Some(&actor), Some(&input));

    assert_eq!(
        step.outcome,
        Sexpr::list(vec![Sexpr::sym("failed"), Sexpr::sym("forbidden")])
    );
    assert_eq!(step.pre_state, state);
    assert_eq!(
        step.post_state, state,
        "state must be preserved on a matched failure"
    );
    assert_eq!(step.transition_id, "m/behavior/x");
    assert_eq!(step.result, None);
}

#[test]
fn oracle_03b_passing_preconditions_appends_input_to_every_write() {
    let state: State = vec![
        (
            "tasks".to_string(),
            Sexpr::list(vec![Sexpr::sym("existing")]),
        ),
        ("audit".to_string(), nil()),
    ];
    let t = make_transition(
        "m/behavior/y",
        "svc/op2",
        Some("actor"),
        Some("input"),
        vec![],
        vec!["tasks", "audit"],
        vec![Sexpr::sym("holds_trivially")],
        vec![], // no failure clauses at all -> the failure branch never matches
    );
    let input_val = Sexpr::sym("new-item");
    let step = apply_transition(&t, &state, None, Some(&input_val));

    assert_eq!(step.outcome, Sexpr::list(vec![Sexpr::sym("succeeded")]));
    assert_eq!(step.result, Some(input_val.clone()));

    let expected_post: State = vec![
        (
            "tasks".to_string(),
            Sexpr::list(vec![Sexpr::sym("existing"), input_val.clone()]),
        ),
        ("audit".to_string(), Sexpr::list(vec![input_val.clone()])),
    ];
    assert_eq!(step.post_state, expected_post);
}

#[test]
fn oracle_03c_failing_preconditions_precondition_failed_state_unchanged() {
    let state: State = vec![("tasks".to_string(), nil())];
    let t = make_transition(
        "m/behavior/z",
        "svc/op3",
        None,
        None,
        vec![],
        vec!["tasks"],
        vec![call("=", vec![Sexpr::Int(1), Sexpr::Int(2)])], // false
        vec![],
    );
    let step = apply_transition(&t, &state, None, None);

    assert_eq!(
        step.outcome,
        Sexpr::list(vec![Sexpr::sym("precondition-failed")])
    );
    assert_eq!(step.post_state, state, "state must be unchanged");
    assert_eq!(step.result, None);
}

// =======================================================================
// 4. Bounded trace: a steps list longer than TRACE_BOUND stops at the
//    bound; unmatched op records the violation AND the error step;
//    deterministic across two runs.
// =======================================================================

#[test]
fn oracle_04_bounded_trace_stops_at_bound_records_violation_and_error_step_deterministically() {
    assert_eq!(TRACE_BOUND, 1000);
    let ir = empty_ir("m"); // no behavior nodes -> nothing ever matches
    let one_step = Sexpr::list(vec![
        Sexpr::sym("bogus_op"),
        Sexpr::sym("a"),
        Sexpr::sym("b"),
    ]);
    let steps: Vec<Sexpr> = std::iter::repeat(one_step)
        .take(TRACE_BOUND + 500)
        .collect();

    let trace1 = execute_trace(&ir, &steps);
    assert_eq!(
        trace1.steps.len(),
        TRACE_BOUND,
        "trace must stop exactly at the bound even though more steps were supplied"
    );
    assert_eq!(
        trace1.violations.len(),
        TRACE_BOUND,
        "every unmatched step records exactly one violation"
    );
    for s in &trace1.steps {
        assert_eq!(s.transition_id, "unknown");
        assert_eq!(s.actor, Some(Sexpr::sym("a")));
        assert_eq!(s.input, Some(Sexpr::sym("b")));
        assert_eq!(s.result, None);
        assert_eq!(
            s.pre_state, s.post_state,
            "an error step never mutates state"
        );
        assert_eq!(
            s.outcome,
            Sexpr::list(vec![
                Sexpr::sym("no-matching-transition"),
                Sexpr::sym("bogus_op")
            ])
        );
    }
    for v in &trace1.violations {
        assert_eq!(
            field(v, "type").and_then(|s| s.as_sym()),
            Some("no-matching-transition")
        );
        assert_eq!(
            field(v, "operation").and_then(|s| s.as_sym()),
            Some("bogus_op")
        );
    }

    let trace2 = execute_trace(&ir, &steps);
    assert_eq!(
        trace1.steps, trace2.steps,
        "two runs over the same steps must be deterministic"
    );
    assert_eq!(trace1.violations, trace2.violations);
    assert_eq!(trace1.final_state, trace2.final_state);
}

// =======================================================================
// 5. check_invariants: a violated always-predicate produces the
//    violation shape; holding predicates produce none.
// =======================================================================

#[test]
fn oracle_05a_check_invariants_violated_predicate_produces_violation_shape() {
    // `missing` is not bound in the (empty) state, so it evaluates to
    // itself (a Sym), which is not `= 5`.
    let bad_pred = call("=", vec![Sexpr::sym("missing"), Sexpr::Int(5)]);
    let node = invariant_node("test/invariant/bad", "bad", bad_pred.clone());
    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        vec![],
        vec![],
        vec![node],
        vec![],
        vec![],
    );
    let state: State = vec![];

    let violations = check_invariants(&ir, &state);
    assert_eq!(violations.len(), 1);
    let v = &violations[0];
    assert_eq!(
        field(v, "invariant").and_then(|s| s.as_str()),
        Some("test/invariant/bad")
    );
    assert_eq!(field(v, "predicate"), Some(&bad_pred));
    assert_eq!(field(v, "state"), Some(&state_sexpr(&state)));
}

#[test]
fn oracle_05b_check_invariants_holding_predicate_produces_none() {
    // A bare atom always holds.
    let node = invariant_node(
        "test/invariant/good",
        "good",
        Sexpr::sym("always_true_atom"),
    );
    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        vec![],
        vec![],
        vec![node],
        vec![],
        vec![],
    );
    let state: State = vec![];
    let violations = check_invariants(&ir, &state);
    assert!(violations.is_empty());
}

#[test]
fn oracle_05c_check_invariants_multiple_only_the_violated_one_reported() {
    let good = invariant_node(
        "test/invariant/a_good",
        "a_good",
        Sexpr::sym("trivially_true"),
    );
    let bad = invariant_node(
        "test/invariant/b_bad",
        "b_bad",
        call("=", vec![Sexpr::Int(1), Sexpr::Int(2)]),
    );
    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        vec![],
        vec![],
        vec![good, bad],
        vec![],
        vec![],
    );
    let state: State = vec![];
    let violations = check_invariants(&ir, &state);
    assert_eq!(violations.len(), 1);
    assert_eq!(
        field(&violations[0], "invariant").and_then(|s| s.as_str()),
        Some("test/invariant/b_bad")
    );
}

// =======================================================================
// 6. Initial state: one entry per state node, `empty` -> nil.
// =======================================================================

#[test]
fn oracle_06a_initial_state_from_todo_gym_empty_maps_to_nil() {
    let ir = load_todo_ir();
    let state = make_initial_state(&ir);
    assert_eq!(state.len(), 1, "todo.gym has exactly one state node");
    assert_eq!(state[0].0, "todo_state");
    assert_eq!(state[0].1, nil(), "`:initial empty` must map to nil");
}

#[test]
fn oracle_06b_initial_state_non_empty_initial_carried_verbatim() {
    let carried = Sexpr::list(vec![Sexpr::sym("seed"), Sexpr::Int(1)]);
    let node = state_node("test/state/s", "s", carried.clone());
    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        vec![node],
        vec![],
        vec![],
        vec![],
        vec![],
    );
    let state = make_initial_state(&ir);
    assert_eq!(state, vec![("s".to_string(), carried)]);
}

#[test]
fn oracle_06c_initial_state_one_entry_per_state_node_multiple() {
    let n1 = state_node("test/state/a", "a", Sexpr::sym("empty"));
    let n2 = state_node("test/state/b", "b", Sexpr::sym("empty"));
    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        vec![n1, n2],
        vec![],
        vec![],
        vec![],
        vec![],
    );
    let state = make_initial_state(&ir);
    assert_eq!(state.len(), 2);
    assert_eq!(state[0], ("a".to_string(), nil()));
    assert_eq!(state[1], ("b".to_string(), nil()));
}
