//! Regression tests for the phase-7 Opus gate's findings (new file, not
//! a frozen oracle). Each test names the finding it pins:
//!
//!  1. BLOCKER — in-trace invariant checks must use the tri-state
//!     evaluator; a property whose trace evaluated any `Unknown`
//!     invariant must never claim `(basis checked)`.
//!  2. `=` over a failed symbol lookup compared against a non-symbol is
//!     `Unknown`, not a fabricated grounded `Fails`.
//!  3. The suffix rule's `/` boundary: a bare-suffix op must NOT match.
//!  4. Cache change-detection with a genuinely modified node: non-empty
//!     `modified` and the exact affected closure.
//!  5. The post-transition `Unknown → indeterminate` dispatch arm.
//!  6. Strict runner readback rejects duplicate keys.
//!  7. Counterexamples pair each violation with the step that produced
//!     it (via `step-index`), not the trace's first step.
//!  8. `verify` exits nonzero when the bundle carries an error-severity
//!     diagnostic (E601).
//!  9. An empty step op / empty transition operation never matches.
//! 10. Ambiguous-operation violations carry actionable transition ids.
//! 11. I601 reads "symbolically-undecided", not "-satisfied".

use gymnast_rs::cache::{diff_plans, plan_node_changed};
use gymnast_rs::elaborate;
use gymnast_rs::ir::{Ir, IrNode};
use gymnast_rs::parser;
use gymnast_rs::plan::plan;
use gymnast_rs::runner::{Attempt, RunResult};
use gymnast_rs::sexpr::Sexpr;
use gymnast_rs::transition::{eval_predicate3, execute_trace, make_initial_state, Verdict};
use gymnast_rs::verify::{lower_all_obligations, verify_obligation};
use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

// ---------------------------------------------------------------------
// Shared builders (same conventions as verify_semantics_test.rs).
// ---------------------------------------------------------------------

fn state_node(name: &str, initial: Sexpr) -> IrNode {
    IrNode::new(
        format!("m/state/{}", name),
        "state",
        name.to_string(),
        vec![(":initial".to_string(), initial)],
        vec![],
    )
}

fn behavior_on(name: &str, operation: &str, writes: &str) -> IrNode {
    IrNode::new(
        format!("m/behavior/{}", name),
        "behavior",
        name.to_string(),
        vec![
            (
                ":on".to_string(),
                Sexpr::list(vec![
                    Sexpr::sym(operation),
                    Sexpr::sym("user"),
                    Sexpr::sym("request"),
                ]),
            ),
            (":writes".to_string(), Sexpr::list(vec![Sexpr::sym(writes)])),
        ],
        vec![],
    )
}

fn invariant(name: &str, pred: Sexpr) -> IrNode {
    IrNode::new(
        format!("m/invariant/{}", name),
        "invariant",
        name.to_string(),
        vec![
            (":scope".to_string(), Sexpr::sym("s")),
            (":always".to_string(), pred),
        ],
        vec![],
    )
}

fn build_ir(design: Vec<IrNode>, transitions: Vec<IrNode>, obligations: Vec<IrNode>) -> Ir {
    Ir::new(
        "gymnast.ir/0.1".to_string(),
        "m".to_string(),
        vec![],
        design,
        transitions,
        obligations,
        vec![],
        vec![],
    )
}

fn field<'a>(form: &'a Sexpr, key: &str) -> Option<&'a Sexpr> {
    let items = form.as_list()?;
    items.iter().skip(1).find_map(|e| {
        let pair = e.as_list()?;
        if pair.len() >= 2 && pair[0].as_sym() == Some(key) {
            Some(&pair[1])
        } else {
            None
        }
    })
}

fn load_todo_ir() -> Ir {
    let src = fs::read_to_string("../examples/todo.gym").expect("read ../examples/todo.gym");
    let (ast, parse_diags) = parser::parse(&src);
    let file = ast.expect("parse todo.gym");
    let (ir, _all_diags) = elaborate::elaborate_with_parse_diags(&file, &parse_diags);
    ir
}

/// Hand-built property obligation executing one step.
fn property_obligation(id: &str, execute: Sexpr) -> Sexpr {
    Sexpr::list(vec![
        Sexpr::sym("verification-obligation"),
        Sexpr::pair("id", Sexpr::Str(id.to_string())),
        Sexpr::pair("kind", Sexpr::sym("property")),
        Sexpr::pair("execute", execute),
    ])
}

// ---------------------------------------------------------------------
// Finding 1 (BLOCKER): grounded steps + undecided invariants must not
// yield `(basis checked)`.
// ---------------------------------------------------------------------

#[test]
fn gate1_property_with_grounded_steps_but_undecided_invariants_is_not_basis_checked() {
    // The behavior has NO preconditions and NO failure clauses, so its
    // one step is fully grounded (symbolic would be false under the
    // phase-6 rules). The invariant's predicate is an unrecognized
    // head, so its in-trace verdict is `Unknown` at every check point.
    let ir = build_ir(
        vec![state_node("count", Sexpr::Int(0))],
        vec![behavior_on("bump", "svc/bump", "count")],
        vec![invariant("mystery", Sexpr::sym("no_lost_updates_ever"))],
    );
    let ob = property_obligation(
        "m/acceptance/p/property/grounded_step",
        Sexpr::list(vec![
            Sexpr::sym("bump"),
            Sexpr::sym("user"),
            Sexpr::sym("request"),
        ]),
    );
    let result = verify_obligation(&ir, &ob);
    assert_eq!(
        field(&result, "status").and_then(|s| s.as_sym()),
        Some("passed"),
        "no violation exists: the unknown invariant is undecided, not violated"
    );
    assert_eq!(
        field(&result, "basis").and_then(|s| s.as_sym()),
        Some("symbolic"),
        "a trace that evaluated an Unknown invariant must never claim basis checked: {}",
        result.print()
    );
}

#[test]
fn gate1_control_property_with_grounded_invariants_stays_basis_checked() {
    // Control for the test above: with a GROUNDED invariant that holds
    // after the write, the same trace is honestly `checked`.
    let ir = build_ir(
        vec![state_node("count", Sexpr::Int(0))],
        vec![behavior_on("bump", "svc/bump", "other")],
        vec![invariant(
            "count_zero",
            Sexpr::list(vec![Sexpr::sym("="), Sexpr::sym("count"), Sexpr::Int(0)]),
        )],
    );
    let ob = property_obligation(
        "m/acceptance/p/property/grounded_all",
        Sexpr::list(vec![
            Sexpr::sym("bump"),
            Sexpr::sym("user"),
            Sexpr::sym("request"),
        ]),
    );
    let result = verify_obligation(&ir, &ob);
    assert_eq!(
        field(&result, "status").and_then(|s| s.as_sym()),
        Some("passed")
    );
    assert_eq!(
        field(&result, "basis").and_then(|s| s.as_sym()),
        Some("checked")
    );
}

#[test]
fn gate1_grounded_invariant_violation_in_trace_still_fails() {
    // The tri-state in-trace check must still produce violations for a
    // grounded Fails: `(= count 0)` breaks when the write appends.
    let ir = build_ir(
        vec![state_node("count", Sexpr::Int(0))],
        vec![behavior_on("bump", "svc/bump", "count")],
        vec![invariant(
            "count_zero",
            Sexpr::list(vec![Sexpr::sym("="), Sexpr::sym("count"), Sexpr::Int(0)]),
        )],
    );
    let trace = execute_trace(
        &ir,
        &[Sexpr::list(vec![
            Sexpr::sym("bump"),
            Sexpr::sym("user"),
            Sexpr::sym("request"),
        ])],
    );
    assert_eq!(trace.violations.len(), 1);
    assert_eq!(
        field(&trace.violations[0], "invariant").and_then(|s| s.as_str()),
        Some("m/invariant/count_zero")
    );
}

// ---------------------------------------------------------------------
// Finding 2: `=` groundedness qualification.
// ---------------------------------------------------------------------

#[test]
fn gate2_equality_with_failed_lookup_vs_int_is_unknown() {
    let state = vec![("count".to_string(), Sexpr::Int(0))];
    // `lost_updates` resolves through no binding; compared against a
    // grounded Int the verdict is undecidable, not a fabricated Fails.
    let pred = Sexpr::list(vec![
        Sexpr::sym("="),
        Sexpr::sym("lost_updates"),
        Sexpr::Int(0),
    ]);
    assert_eq!(eval_predicate3(&pred, &state, None, None), Verdict::Unknown);
}

#[test]
fn gate2_equality_sym_vs_sym_stays_grounded_enum_semantics() {
    let state = vec![("status".to_string(), Sexpr::sym("active"))];
    // Resolved sym vs literal sym: grounded enum comparison.
    let holds = Sexpr::list(vec![
        Sexpr::sym("="),
        Sexpr::sym("status"),
        Sexpr::sym("active"),
    ]);
    assert_eq!(eval_predicate3(&holds, &state, None, None), Verdict::Holds);
    let fails = Sexpr::list(vec![
        Sexpr::sym("="),
        Sexpr::sym("status"),
        Sexpr::sym("trashed"),
    ]);
    assert_eq!(eval_predicate3(&fails, &state, None, None), Verdict::Fails);
    // Two floating syms have NO grounded reading (gate re-review
    // residual): neither side is bound to anything, so even
    // structurally equal literals are `Unknown` — the legitimate enum
    // case is resolved-vs-literal, pinned above. `(= current_status
    // open)` over a state with neither entry was the re-review's
    // fabricated `(status failed) (basis checked)` reproduction.
    let both_floating_eq = Sexpr::list(vec![
        Sexpr::sym("="),
        Sexpr::sym("pending"),
        Sexpr::sym("pending"),
    ]);
    assert_eq!(
        eval_predicate3(&both_floating_eq, &state, None, None),
        Verdict::Unknown
    );
    let both_floating_ne = Sexpr::list(vec![
        Sexpr::sym("="),
        Sexpr::sym("current_status"),
        Sexpr::sym("open"),
    ]);
    assert_eq!(
        eval_predicate3(&both_floating_ne, &state, None, None),
        Verdict::Unknown
    );
}

#[test]
fn gate2_invariant_on_missing_state_entry_is_indeterminate_not_failed() {
    // The gate's reproduction: `(= lost_updates 0)` as an invariant over
    // a state with no such entry must be indeterminate — previously a
    // grounded-looking `failed` with a counterexample and no I601.
    let ir = build_ir(
        vec![state_node("count", Sexpr::Int(0))],
        vec![],
        vec![invariant(
            "fabricated_check",
            Sexpr::list(vec![
                Sexpr::sym("="),
                Sexpr::sym("lost_updates"),
                Sexpr::Int(0),
            ]),
        )],
    );
    let obs = lower_all_obligations(&ir);
    let ob = obs
        .iter()
        .find(|o| field(o, "kind").and_then(|k| k.as_sym()) == Some("invariant"))
        .expect("invariant obligation");
    let result = verify_obligation(&ir, ob);
    assert_eq!(
        field(&result, "status").and_then(|s| s.as_sym()),
        Some("indeterminate"),
        "{}",
        result.print()
    );
    assert_eq!(
        field(&result, "basis").and_then(|s| s.as_sym()),
        Some("symbolic")
    );
    assert!(result.print().contains("I601"));
}

// ---------------------------------------------------------------------
// Finding 11: the I601 wording.
// ---------------------------------------------------------------------

#[test]
fn gate11_i601_message_says_undecided_not_satisfied() {
    let ir = build_ir(
        vec![state_node("count", Sexpr::Int(0))],
        vec![],
        vec![invariant("mystery", Sexpr::sym("unknowable"))],
    );
    let obs = lower_all_obligations(&ir);
    let ob = obs
        .iter()
        .find(|o| field(o, "kind").and_then(|k| k.as_sym()) == Some("invariant"))
        .expect("invariant obligation");
    let result = verify_obligation(&ir, ob);
    let printed = result.print();
    assert!(printed.contains("symbolically-undecided"), "{}", printed);
    assert!(!printed.contains("symbolically-satisfied"), "{}", printed);
}

// ---------------------------------------------------------------------
// Finding 3: the suffix rule's `/` boundary.
// ---------------------------------------------------------------------

#[test]
fn gate3_bare_suffix_without_slash_boundary_does_not_match() {
    let ir = build_ir(
        vec![state_node("tasks", Sexpr::list(vec![]))],
        vec![behavior_on("create_task", "svc/create_task", "tasks")],
        vec![],
    );
    // `task` is a bare suffix of `svc/create_task` but not
    // `/`-delimited: it must NOT match (a plain ends_with would).
    let trace = execute_trace(
        &ir,
        &[Sexpr::list(vec![
            Sexpr::sym("task"),
            Sexpr::sym("user"),
            Sexpr::sym("request"),
        ])],
    );
    assert_eq!(trace.violations.len(), 1);
    assert_eq!(
        field(&trace.violations[0], "type").and_then(|s| s.as_sym()),
        Some("no-matching-transition")
    );
    // Control: the full `/`-delimited helper name DOES match.
    let trace_ok = execute_trace(
        &ir,
        &[Sexpr::list(vec![
            Sexpr::sym("create_task"),
            Sexpr::sym("user"),
            Sexpr::sym("request"),
        ])],
    );
    assert!(trace_ok.violations.is_empty());
    assert_eq!(
        trace_ok.steps[0].transition_id,
        "m/behavior/create_task".to_string()
    );
}

// ---------------------------------------------------------------------
// Finding 9: empty ops never participate in matching.
// ---------------------------------------------------------------------

#[test]
fn gate9_empty_step_op_and_empty_operation_never_match() {
    // One behavior with NO :on (operation "") and one whose operation
    // ends in a bare slash. An empty-list step (op "") must match
    // NEITHER — under the unguarded rules it would match both.
    let no_on = IrNode::new(
        "m/behavior/secret".to_string(),
        "behavior",
        "secret".to_string(),
        vec![(
            ":writes".to_string(),
            Sexpr::list(vec![Sexpr::sym("tasks")]),
        )],
        vec![],
    );
    let ir = build_ir(
        vec![state_node("tasks", Sexpr::list(vec![]))],
        vec![no_on, behavior_on("slash", "svc/", "tasks")],
        vec![],
    );
    let initial = make_initial_state(&ir);
    let trace = execute_trace(&ir, &[Sexpr::list(vec![])]);
    assert_eq!(trace.violations.len(), 1);
    assert_eq!(
        field(&trace.violations[0], "type").and_then(|s| s.as_sym()),
        Some("no-matching-transition")
    );
    assert_eq!(
        trace.final_state, initial,
        "an empty op must never mutate state"
    );
}

// ---------------------------------------------------------------------
// Finding 10: ambiguity violations carry transition ids.
// ---------------------------------------------------------------------

#[test]
fn gate10_ambiguous_violation_names_candidate_transition_ids() {
    // Two behaviors declaring the SAME operation: the ops in
    // `candidates` are indistinguishable; `candidate-transitions`
    // carries the actionable ids.
    let ir = build_ir(
        vec![state_node("tasks", Sexpr::list(vec![]))],
        vec![
            behavior_on("first", "svc/op", "tasks"),
            behavior_on("second", "svc/op", "tasks"),
        ],
        vec![],
    );
    let trace = execute_trace(
        &ir,
        &[Sexpr::list(vec![
            Sexpr::sym("op"),
            Sexpr::sym("user"),
            Sexpr::sym("request"),
        ])],
    );
    assert_eq!(trace.violations.len(), 1);
    let v = &trace.violations[0];
    assert_eq!(
        field(v, "type").and_then(|s| s.as_sym()),
        Some("ambiguous-operation")
    );
    let ids: Vec<&str> = field(v, "candidate-transitions")
        .and_then(|c| c.as_list())
        .expect("candidate-transitions must be present")
        .iter()
        .filter_map(|s| s.as_str())
        .collect();
    assert_eq!(ids, vec!["m/behavior/first", "m/behavior/second"]);
}

// ---------------------------------------------------------------------
// Finding 5: the post-transition Unknown arm decides `indeterminate`.
// ---------------------------------------------------------------------

#[test]
fn gate5_post_transition_unknown_decides_indeterminate() {
    // Initially `(< count 10)` is grounded Holds (0 < 10). The write
    // turns `count` into a list, so the post-transition comparison is
    // over a non-Int: `Unknown` must decide `indeterminate` — reverting
    // that arm to `continue` would launder it back into `passed`.
    let ir = build_ir(
        vec![state_node("count", Sexpr::Int(0))],
        vec![behavior_on("bump", "svc/bump", "count")],
        vec![invariant(
            "bounded",
            Sexpr::list(vec![Sexpr::sym("<"), Sexpr::sym("count"), Sexpr::Int(10)]),
        )],
    );
    let obs = lower_all_obligations(&ir);
    let ob = obs
        .iter()
        .find(|o| field(o, "kind").and_then(|k| k.as_sym()) == Some("invariant"))
        .expect("invariant obligation");
    let result = verify_obligation(&ir, ob);
    assert_eq!(
        field(&result, "status").and_then(|s| s.as_sym()),
        Some("indeterminate"),
        "{}",
        result.print()
    );
    assert_eq!(
        field(&result, "basis").and_then(|s| s.as_sym()),
        Some("symbolic")
    );
}

// ---------------------------------------------------------------------
// Finding 7: counterexamples pair violations with the CORRECT step.
// ---------------------------------------------------------------------

#[test]
fn gate7_counterexample_pairs_violation_with_its_own_step() {
    let ir = load_todo_ir();
    let obs = lower_all_obligations(&ir);
    let ob = obs
        .iter()
        .find(|o| {
            field(o, "id").and_then(|i| i.as_str())
                == Some("todo/acceptance/production/property/create_then_read")
        })
        .expect("create_then_read obligation");
    let result = verify_obligation(&ir, ob);
    let ces = field(&result, "counterexamples")
        .and_then(|c| c.as_list())
        .expect("counterexamples list");
    assert_eq!(ces.len(), 1);
    // The violation is query_tasks's no-match at step index 1; the
    // embedded trace-step must be THAT step (outcome
    // no-matching-transition), not the applied create_task step the
    // first-step pairing used to glue on.
    let embedded = field(&ces[0], "trace-step").expect("embedded trace-step");
    let printed = embedded.print();
    assert!(
        printed.contains("no-matching-transition"),
        "counterexample must embed the violating step, got: {}",
        printed
    );
    assert!(
        !printed.contains("(outcome (succeeded))"),
        "counterexample must not embed the unrelated first step: {}",
        printed
    );
}

// ---------------------------------------------------------------------
// Finding 4: cache change-detection with a genuinely modified node.
// ---------------------------------------------------------------------

#[test]
fn gate4_modified_node_produces_nonempty_diff_and_exact_closure() {
    let ir = load_todo_ir();
    let old_plan = plan(&ir);
    let mut new_plan = plan(&ir);
    let target = "todo/plan/transition-kernel";
    let node = new_plan
        .nodes
        .iter_mut()
        .find(|n| n.id == target)
        .expect("transition-kernel node");
    node.fingerprint = "fnv1a64:12345".to_string();

    assert!(
        plan_node_changed(&old_plan, &new_plan, target),
        "a moved contract fingerprint must register as changed"
    );
    for other in old_plan.nodes.iter().filter(|n| n.id != target) {
        assert!(
            !plan_node_changed(&old_plan, &new_plan, &other.id),
            "{} did not change",
            other.id
        );
    }

    let diff = diff_plans(&old_plan, &new_plan);
    let modified: Vec<&str> = diff
        .assoc("modified")
        .and_then(|m| m.as_list())
        .expect("modified list")
        .iter()
        .filter_map(|s| s.as_str())
        .collect();
    assert_eq!(modified, vec![target]);

    // The affected closure must be the modified node plus its
    // transitive dependents — the same six-node kernel closure the
    // cache oracle pins for seed transition-kernel. Losing the
    // `modified → changed` extension collapses this to empty and
    // breaks incremental regeneration.
    let closure: Vec<&str> = diff
        .assoc("affected-closure")
        .and_then(|c| c.as_list())
        .expect("affected-closure list")
        .iter()
        .filter_map(|s| s.as_str())
        .collect();
    assert!(closure.contains(&target), "closure includes the seed");
    assert_eq!(closure.len(), 6, "closure: {:?}", closure);
    assert!(closure.contains(&"todo/plan/acceptance-harness"));
    assert!(!closure.contains(&"todo/plan/design-contracts"));
}

// ---------------------------------------------------------------------
// Finding 6: duplicate keys are rejected by the strict readback.
// ---------------------------------------------------------------------

fn attempt_fields() -> Vec<Sexpr> {
    vec![
        Sexpr::pair("number", Sexpr::Int(1)),
        Sexpr::pair("prompt-fingerprint", Sexpr::Str("fnv1a64:1".into())),
        Sexpr::pair("response-length", Sexpr::Int(2)),
        Sexpr::pair("response-fingerprint", Sexpr::Str("fnv1a64:2".into())),
        Sexpr::pair("diagnostics", Sexpr::list(vec![])),
        Sexpr::pair("status", Sexpr::sym("accepted")),
    ]
}

#[test]
fn gate6_attempt_readback_rejects_duplicate_keys() {
    let valid = Sexpr::list(vec![Sexpr::sym("attempt"), Sexpr::List(attempt_fields())]);
    assert!(Attempt::from_sexpr(&valid).is_some(), "control must parse");

    let mut dup_fields = attempt_fields();
    dup_fields.push(Sexpr::pair("number", Sexpr::Int(99)));
    let dup = Sexpr::list(vec![Sexpr::sym("attempt"), Sexpr::List(dup_fields)]);
    assert!(
        Attempt::from_sexpr(&dup).is_none(),
        "a repeated key must be rejected, not first-wins-silently-kept"
    );
}

#[test]
fn gate6_run_result_readback_rejects_duplicate_keys() {
    let attempt = Sexpr::list(vec![Sexpr::sym("attempt"), Sexpr::List(attempt_fields())]);
    let base = vec![
        Sexpr::pair("node-id", Sexpr::Str("m/plan/transition-kernel".into())),
        Sexpr::pair("node-fingerprint", Sexpr::Str("fnv1a64:3".into())),
        Sexpr::pair("status", Sexpr::sym("succeeded")),
        Sexpr::pair("attempts", Sexpr::list(vec![attempt])),
        Sexpr::pair("candidate", Sexpr::sym("benign-candidate")),
    ];
    let valid = Sexpr::list(vec![Sexpr::sym("run-result"), Sexpr::List(base.clone())]);
    assert!(
        RunResult::from_sexpr(&valid).is_some(),
        "control must parse"
    );

    // The gate's reproduction: a second `candidate` (and a second
    // `node-id`) smuggled after the benign ones. First-wins acceptance
    // is a parser differential; strict readback must reject.
    let mut tampered_fields = base;
    tampered_fields.push(Sexpr::pair("candidate", Sexpr::sym("MALICIOUS-candidate")));
    tampered_fields.push(Sexpr::pair("node-id", Sexpr::Str("m/plan/OTHER".into())));
    let tampered = Sexpr::list(vec![Sexpr::sym("run-result"), Sexpr::List(tampered_fields)]);
    assert!(RunResult::from_sexpr(&tampered).is_none());
}

// ---------------------------------------------------------------------
// Finding 8: `verify` fails visibly on bundle-level error diagnostics.
// ---------------------------------------------------------------------

static TEMP_SEQ: AtomicU32 = AtomicU32::new(0);

#[test]
fn gate8_verify_exits_nonzero_on_e601_duplicate_obligation_ids() {
    // Duplicate the viewer_cannot_mutate property verbatim: two
    // obligations lower to the same id, E601 lands in the bundle's
    // diagnostics, and the CLI must exit nonzero with the error on
    // stderr instead of a silent exit 0.
    let src = fs::read_to_string("../examples/todo.gym").expect("read todo.gym");
    let block = "  property viewer_cannot_mutate =\n    generate (actor authenticated_viewer of user, task valid_task)\n    execute create_task (actor, task)\n    must fails_with forbidden,\n";
    assert!(
        src.contains(block),
        "todo.gym's viewer_cannot_mutate block moved; update this test"
    );
    let doubled = src.replace(block, &format!("{}\n{}", block, block));
    let path = std::env::temp_dir().join(format!(
        "gate8-dup-property-{}-{}.gym",
        std::process::id(),
        TEMP_SEQ.fetch_add(1, Ordering::SeqCst)
    ));
    fs::write(&path, doubled).expect("write temp spec");

    let out = Command::new(env!("CARGO_BIN_EXE_gymnast-rs"))
        .args(["verify", path.to_str().expect("utf-8 temp path")])
        .output()
        .expect("run verify");
    let _ = fs::remove_file(&path);

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.contains("E601"),
        "bundle must carry E601; stdout: {}",
        &stdout[..stdout.len().min(400)]
    );
    assert!(
        !out.status.success(),
        "verify must exit nonzero on an error-severity bundle diagnostic"
    );
    assert!(
        stderr.contains("E601"),
        "the error must be visible on stderr, got: {}",
        stderr
    );
}
