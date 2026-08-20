//! Semantics tests the phase-6 gate found missing (finding 2): the
//! invariant-verification path was previously unconstrained — four
//! independent gutting mutations survived the whole suite because the
//! flagship spec's predicates are vacuously true. These tests use a
//! hand-built IR whose predicates the closed evaluator actually GROUNDS,
//! so the initial-state check, the post-transition check, and the
//! basis-honesty marking all have teeth. (New file, not a frozen oracle.)

use gymnast_rs::ir::{Ir, IrNode};
use gymnast_rs::sexpr::Sexpr;
use gymnast_rs::verify::{compile_verification, lower_all_obligations, verify_obligation};

fn state_node(name: &str, initial: Sexpr) -> IrNode {
    IrNode::new(
        format!("m/state/{}", name),
        "state",
        name.to_string(),
        vec![(":initial".to_string(), initial)],
        vec![],
    )
}

fn behavior_writing(name: &str, writes: &str) -> IrNode {
    IrNode::new(
        format!("m/behavior/{}", name),
        "behavior",
        name.to_string(),
        vec![
            (
                ":on".to_string(),
                Sexpr::list(vec![
                    Sexpr::sym(&format!("svc/{}", name)),
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
    // Verification forms are flat: (tag (k v) (k v) ...).
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

/// Grounded predicate `(= count 0)` over a state entry with Int initial:
/// holds initially, broken by the transition's writes-append — the
/// post-transition check must fire, name the transition, and carry the
/// post-state divergence type.
#[test]
fn test_invariant_broken_by_post_transition_state() {
    let ir = build_ir(
        vec![state_node("count", Sexpr::Int(0))],
        vec![behavior_writing("bump", "count")],
        vec![invariant(
            "zero",
            Sexpr::list(vec![Sexpr::sym("="), Sexpr::sym("count"), Sexpr::Int(0)]),
        )],
    );
    let obs = lower_all_obligations(&ir);
    let inv_ob = obs
        .iter()
        .find(|o| field(o, "kind").and_then(|k| k.as_sym()) == Some("invariant"))
        .expect("invariant obligation");
    let result = verify_obligation(&ir, inv_ob);
    assert_eq!(
        field(&result, "status").and_then(|s| s.as_sym()),
        Some("failed"),
        "post-transition violation must fail: {}",
        result.print()
    );
    assert_eq!(
        field(&result, "basis").and_then(|s| s.as_sym()),
        Some("checked"),
        "a grounded verdict is CHECKED: {}",
        result.print()
    );
    let printed = result.print();
    assert!(
        printed.contains("invariant-violation-post-transition"),
        "divergence type must name the post-transition case: {}",
        printed
    );
    assert!(
        printed.contains("m/behavior/bump"),
        "counterexample must name the breaking transition: {}",
        printed
    );
}

/// The initial-state check fires on its own (no transitions needed).
#[test]
fn test_invariant_broken_in_initial_state() {
    let ir = build_ir(
        vec![state_node("count", Sexpr::Int(5))],
        vec![],
        vec![invariant(
            "zero",
            Sexpr::list(vec![Sexpr::sym("="), Sexpr::sym("count"), Sexpr::Int(0)]),
        )],
    );
    let obs = lower_all_obligations(&ir);
    let result = verify_obligation(&ir, &obs[0]);
    assert_eq!(
        field(&result, "status").and_then(|s| s.as_sym()),
        Some("failed")
    );
    assert!(result
        .print()
        .contains("(divergence-type invariant-violation)"));
}

/// A grounded, genuinely-holding invariant passes with basis CHECKED and
/// no I601 marker.
#[test]
fn test_grounded_pass_is_checked_not_symbolic() {
    let ir = build_ir(
        vec![state_node("count", Sexpr::Int(0))],
        vec![],
        vec![invariant(
            "zero",
            Sexpr::list(vec![Sexpr::sym("="), Sexpr::sym("count"), Sexpr::Int(0)]),
        )],
    );
    let obs = lower_all_obligations(&ir);
    let result = verify_obligation(&ir, &obs[0]);
    assert_eq!(
        field(&result, "status").and_then(|s| s.as_sym()),
        Some("passed")
    );
    assert_eq!(
        field(&result, "basis").and_then(|s| s.as_sym()),
        Some("checked")
    );
    assert!(!result.print().contains("I601"));
}

/// A vacuous pass (unknown-head predicate) is marked SYMBOLIC and carries
/// the I601 marker — the phase-6 gate's finding 1, now a contract.
#[test]
fn test_vacuous_pass_is_marked_symbolic() {
    let ir = build_ir(
        vec![state_node("s", Sexpr::list(vec![]))],
        vec![],
        vec![invariant(
            "abstract",
            Sexpr::sym("no_observation_without_active_membership"),
        )],
    );
    let obs = lower_all_obligations(&ir);
    let result = verify_obligation(&ir, &obs[0]);
    assert_eq!(
        field(&result, "status").and_then(|s| s.as_sym()),
        Some("passed")
    );
    assert_eq!(
        field(&result, "basis").and_then(|s| s.as_sym()),
        Some("symbolic"),
        "a defaulted verdict must be marked symbolic: {}",
        result.print()
    );
    assert!(result.print().contains("I601"));
}

/// A fabricated failure from the non-Int-comparison delta is also marked
/// symbolic — the mirror direction of finding 4.
#[test]
fn test_defaulted_failure_is_marked_symbolic() {
    let ir = build_ir(
        vec![state_node("store", Sexpr::list(vec![]))],
        vec![],
        vec![invariant(
            "bounded",
            Sexpr::list(vec![
                Sexpr::sym("<="),
                Sexpr::list(vec![Sexpr::sym("item_count"), Sexpr::sym("store")]),
                Sexpr::Int(10),
            ]),
        )],
    );
    let obs = lower_all_obligations(&ir);
    let result = verify_obligation(&ir, &obs[0]);
    assert_eq!(
        field(&result, "status").and_then(|s| s.as_sym()),
        Some("failed")
    );
    assert_eq!(
        field(&result, "basis").and_then(|s| s.as_sym()),
        Some("symbolic")
    );
}

/// A broken-source bundle is self-describing: the IR's error diagnostics
/// ride in `source-diagnostics` (finding 3).
#[test]
fn test_bundle_carries_source_diagnostics() {
    let bad = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "m".to_string(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![gymnast_rs::diag::diag_sexpr(
            "error",
            "E301",
            (0, 0),
            "duplicate-semantic-id: m/x/y".to_string(),
        )],
    );
    let bundle = compile_verification(&bad);
    let printed = bundle.print();
    assert!(
        printed.contains("(source-diagnostics ((diagnostic") && printed.contains("E301"),
        "bundle must carry source diagnostics: {}",
        printed
    );
}

/// `(sequence a b)` in an execute value unwraps to two steps (finding 6).
#[test]
fn test_property_sequence_unwraps() {
    let ir = build_ir(
        vec![state_node("s", Sexpr::list(vec![]))],
        vec![behavior_writing("add", "s")],
        vec![IrNode::new(
            "m/acceptance/a".to_string(),
            "acceptance",
            "a".to_string(),
            vec![(":subject".to_string(), Sexpr::sym("app"))],
            vec![Sexpr::list(vec![
                Sexpr::sym("property"),
                Sexpr::sym("p"),
                Sexpr::sym(":execute"),
                Sexpr::list(vec![
                    Sexpr::sym("sequence"),
                    Sexpr::list(vec![
                        Sexpr::sym("svc/add"),
                        Sexpr::sym("u"),
                        Sexpr::sym("r"),
                    ]),
                    Sexpr::list(vec![
                        Sexpr::sym("svc/add"),
                        Sexpr::sym("u"),
                        Sexpr::sym("r"),
                    ]),
                ]),
                Sexpr::sym(":must"),
                Sexpr::sym("ok"),
            ])],
        )],
    );
    let obs = lower_all_obligations(&ir);
    let prop = obs
        .iter()
        .find(|o| field(o, "kind").and_then(|k| k.as_sym()) == Some("property"))
        .expect("property obligation");
    let result = verify_obligation(&ir, prop);
    // Two steps against a matching qualified op: trace runs both.
    let printed = result.print();
    let step_count = printed.matches("(trace-step").count();
    assert_eq!(
        step_count, 2,
        "sequence must unwrap to two steps, got {} in: {}",
        step_count, printed
    );
}

/// Skip paths stay skips (gate findings 11–13): no execute → skipped;
/// scenario with no steps → skipped.
#[test]
fn test_missing_execute_and_steps_skip_not_fail() {
    let ir = build_ir(
        vec![state_node("s", Sexpr::list(vec![]))],
        vec![],
        vec![IrNode::new(
            "m/acceptance/a".to_string(),
            "acceptance",
            "a".to_string(),
            vec![(":subject".to_string(), Sexpr::sym("app"))],
            vec![
                Sexpr::list(vec![Sexpr::sym("property"), Sexpr::sym("p")]),
                Sexpr::list(vec![
                    Sexpr::sym("scenario"),
                    Sexpr::sym("sc"),
                    Sexpr::list(vec![Sexpr::sym("given"), Sexpr::sym("x")]),
                ]),
            ],
        )],
    );
    for ob in lower_all_obligations(&ir) {
        let kind = field(&ob, "kind").and_then(|k| k.as_sym()).unwrap_or("");
        if kind == "property" || kind == "scenario" {
            let r = verify_obligation(&ir, &ob);
            assert_eq!(
                field(&r, "status").and_then(|s| s.as_sym()),
                Some("skipped"),
                "{} with nothing to run must skip: {}",
                kind,
                r.print()
            );
        }
    }
}
