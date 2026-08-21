//! Regression tests for the phase-8 Opus gate's findings (new file, not
//! a frozen oracle). Findings pinned:
//!
//!  1. BLOCKER — the synthesize evidence bundle must see model
//!     outcomes: `merge_run_results` bridges run results into the
//!     execution results the bundle is assembled over.
//!  2. Shadow-proof promotion reads: a duplicated bundle key fails the
//!     affected checks closed instead of first-wins shadowing; the
//!     bundle fingerprint is independently verifiable.
//!  3. Firewall-rejected candidates never enter the artifact ledger.
//!  4. No vacuous promote: a zero-obligation verification section
//!     fails `verification-passed`; missing artifacts fail the new
//!     `all-artifacts-present` check.
//!  5. `no-error-diagnostics` sees errors inside the nested
//!     verification section (E601 / source diagnostics).
//!  6. Artifact `size` is BYTES, not chars (multi-byte content).
//!  7. Four fail-closed behaviors, re-pinned outside the
//!     implementer-authored in-module unit tests: missing
//!     `diagnostics` field, missing `traceability` field, unreadable
//!     severity, present-but-unreadable verification summary.

use gymnast_rs::assembly::{
    assemble_bundle, collect_artifacts, default_promotion_policy, evaluate_promotion,
    verify_bundle_fingerprint,
};
use gymnast_rs::ir::Ir;
use gymnast_rs::plan::plan;
use gymnast_rs::recipe::{ExecutionResult, ExecutionStatus};
use gymnast_rs::runner::{merge_run_results, Attempt, AttemptStatus, RunResult, RunStatus};
use gymnast_rs::sexpr::{parse, Sexpr};
use gymnast_rs::{elaborate, parser, verify};
use std::fs;

fn todo_ir() -> Ir {
    let src = fs::read_to_string("../examples/todo.gym").expect("read ../examples/todo.gym");
    let (ast, parse_diags) = parser::parse(&src);
    let file = ast.expect("parse todo.gym");
    let (ir, _all_diags) = elaborate::elaborate_with_parse_diags(&file, &parse_diags);
    ir
}

fn result(node_id: &str, status: ExecutionStatus, candidate: Option<Sexpr>) -> ExecutionResult {
    ExecutionResult {
        node_id: node_id.to_string(),
        status,
        candidate,
        recipe_identity: None,
        diagnostics: vec![],
    }
}

fn candidate_with_file(node_id: &str, path: &str, content: &str) -> Sexpr {
    Sexpr::list(vec![
        Sexpr::sym("candidate"),
        Sexpr::list(vec![
            Sexpr::pair("schema", Sexpr::Str("gymnast.candidate/0.1".into())),
            Sexpr::pair("node-id", Sexpr::Str(node_id.into())),
            Sexpr::pair(
                "files",
                Sexpr::list(vec![Sexpr::list(vec![
                    Sexpr::Str(path.into()),
                    Sexpr::Str(content.into()),
                ])]),
            ),
        ]),
    ])
}

fn checks_of(promotion: &Sexpr) -> Vec<(String, String)> {
    promotion
        .as_list()
        .and_then(|items| items.get(1))
        .and_then(|body| body.assoc("checks"))
        .and_then(|c| c.as_list())
        .expect("checks list")
        .iter()
        .filter_map(|pair| {
            let p = pair.as_list()?;
            Some((p[0].as_sym()?.to_string(), p[1].print()))
        })
        .collect()
}

fn check_value(promotion: &Sexpr, name: &str) -> String {
    checks_of(promotion)
        .into_iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v)
        .unwrap_or_else(|| panic!("check {} missing", name))
}

fn decision_of(promotion: &Sexpr) -> String {
    promotion
        .as_list()
        .and_then(|items| items.get(1))
        .and_then(|body| body.assoc("decision"))
        .map(|d| d.print())
        .expect("decision")
}

// ---------------------------------------------------------------------
// Finding 1: the run-result bridge.
// ---------------------------------------------------------------------

fn attempt() -> Attempt {
    Attempt {
        number: 1,
        prompt_fingerprint: "fnv1a64:1".into(),
        response_length: 2,
        response_fingerprint: "fnv1a64:2".into(),
        diagnostics: vec![],
        status: AttemptStatus::Accepted,
    }
}

#[test]
fn gate1_merge_folds_succeeded_and_exhausted_runs_into_results() {
    let accepted = candidate_with_file("m/plan/gen-a", "out/a.rb", "body-a");
    let results = vec![
        result("m/plan/structural", ExecutionStatus::Succeeded, None),
        result("m/plan/gen-a", ExecutionStatus::Deferred, None),
        result("m/plan/gen-b", ExecutionStatus::Deferred, None),
    ];
    let runs = vec![
        RunResult {
            node_id: "m/plan/gen-a".into(),
            node_fingerprint: "fnv1a64:3".into(),
            status: RunStatus::Succeeded,
            attempts: vec![attempt()],
            candidate: Some(accepted.clone()),
        },
        RunResult {
            node_id: "m/plan/gen-b".into(),
            node_fingerprint: "fnv1a64:4".into(),
            status: RunStatus::Exhausted,
            attempts: vec![],
            // Deliberately Some: run_node never sets a candidate on
            // Exhausted, but the bridge must strip one anyway
            // (belt-and-braces; phase-8 gate re-review, note 4 — with
            // None here the candidate-stripping assertion below could
            // never fail).
            candidate: Some(candidate_with_file("m/plan/gen-b", "out/b.rb", "junk")),
        },
    ];

    let merged = merge_run_results(&results, &runs);
    assert_eq!(merged.len(), 3);
    assert_eq!(merged[0].status, ExecutionStatus::Succeeded);
    assert_eq!(merged[1].status, ExecutionStatus::Succeeded);
    assert_eq!(merged[1].candidate, Some(accepted));
    assert_eq!(merged[2].status, ExecutionStatus::Failed);
    assert_eq!(
        merged[2].candidate, None,
        "an exhausted run must never carry a candidate into the ledger"
    );
    assert!(
        merged[2].diagnostics[0]
            .print()
            .contains("synthesis-exhausted"),
        "the failure must be visible to the bundle's error census"
    );

    // The accepted run candidate now reaches the artifact ledger.
    let artifacts = collect_artifacts(&merged);
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].path, "out/a.rb");
    assert_eq!(artifacts[0].node_id, "m/plan/gen-a");
}

#[test]
fn gate1_exhausted_run_blocks_promotion_via_error_diagnostic_and_failed_count() {
    // With an exhausted generative node merged in, the bundle's
    // failed-nodes count is nonzero: all-nodes-succeeded nil -> hold.
    let ir = todo_ir();
    let p = plan(&ir);
    let results = vec![result(
        "todo/plan/transition-kernel",
        ExecutionStatus::Deferred,
        None,
    )];
    let runs = vec![RunResult {
        node_id: "todo/plan/transition-kernel".into(),
        node_fingerprint: "fnv1a64:9".into(),
        status: RunStatus::Exhausted,
        attempts: vec![],
        candidate: None,
    }];
    let merged = merge_run_results(&results, &runs);
    let bundle = assemble_bundle(&ir, &p, &merged, None);
    // The WHY reaches the bundle, not only the THAT (gate re-review,
    // residual 2): execution-result diagnostics fold into the bundle's
    // diagnostics, so the synthesis-exhausted error is evidence.
    assert!(
        bundle.print().contains("synthesis-exhausted"),
        "the bundle must record why the node failed"
    );
    let promotion = evaluate_promotion(&default_promotion_policy(), &bundle);
    assert_eq!(check_value(&promotion, "all-nodes-succeeded"), "nil");
    assert_eq!(
        check_value(&promotion, "no-error-diagnostics"),
        "nil",
        "the folded synthesis-exhausted error must fail the error census"
    );
    assert_eq!(decision_of(&promotion), "hold");
}

#[test]
fn gate_residual1_all_skipped_verification_is_not_evidence() {
    // (total 4, all skipped): nothing executed — the same vacuity
    // class as total 0. verification-passed must be nil.
    let bundle_text = r#"(evidence-bundle ((schema "gymnast.bundle/0.1")
        (ir-fingerprint "fnv1a64:1") (plan-fingerprint "fnv1a64:2")
        (artifacts nil) (traceability nil)
        (dependency-lock (dependency-lock ((plan-fingerprint "fnv1a64:2") (node-locks nil))))
        (verification (verification-bundle ((schema "gymnast.verify/0.1")
          (summary ((total 4) (passed 0) (failed 0) (skipped 4) (indeterminate 0))))))
        (summary ((total-nodes 0) (artifacts-produced 0) (succeeded-nodes 0)
                  (failed-nodes 0) (has-verification t)))
        (diagnostics nil) (fingerprint "fnv1a64:5")))"#;
    let bundle = parse(bundle_text).expect("bundle parses");
    let promotion = evaluate_promotion(&default_promotion_policy(), &bundle);
    assert_eq!(check_value(&promotion, "verification-passed"), "nil");
    assert_eq!(decision_of(&promotion), "hold");
}

// ---------------------------------------------------------------------
// Finding 2: shadow-proof reads + fingerprint verification.
// ---------------------------------------------------------------------

#[test]
fn gate2_prepended_verification_nil_cannot_shadow_the_real_section() {
    // The gate's reproduction against the committed golden: prepend one
    // (verification nil) pair to the body. Under first-wins assoc that
    // flipped hold to promote; under assoc_unique the duplicate fails
    // the verification checks closed and the decision stays hold.
    let golden = fs::read_to_string("tests/fixtures/todo-bundle.sexpr").expect("golden");
    let wrapper = parse(&golden).expect("golden parses");
    let bundle = wrapper
        .as_list()
        .and_then(|items| items.get(1))
        .and_then(|body| body.assoc("bundle"))
        .expect("bundle in wrapper")
        .clone();

    let items = bundle.as_list().expect("bundle list");
    let mut body = items[1].as_list().expect("body").to_vec();
    body.insert(
        0,
        Sexpr::list(vec![Sexpr::sym("verification"), Sexpr::list(vec![])]),
    );
    let tampered = Sexpr::list(vec![Sexpr::sym("evidence-bundle"), Sexpr::List(body)]);

    let promotion = evaluate_promotion(&default_promotion_policy(), &tampered);
    assert_eq!(decision_of(&promotion), "hold");
    assert_eq!(check_value(&promotion, "verification-passed"), "nil");
    assert_eq!(
        check_value(&promotion, "no-indeterminate-verification"),
        "nil"
    );
}

#[test]
fn gate2_bundle_fingerprint_verifies_and_detects_tampering() {
    let ir = todo_ir();
    let p = plan(&ir);
    let verification = verify::compile_verification(&ir);
    let bundle = assemble_bundle(&ir, &p, &[], Some(&verification));
    assert!(verify_bundle_fingerprint(&bundle));

    // Any content change breaks verification.
    let tampered_text = bundle
        .print()
        .replace("(failed-nodes 0)", "(failed-nodes 1)");
    let tampered = parse(&tampered_text).expect("tampered parses");
    assert_ne!(tampered.print(), bundle.print());
    assert!(!verify_bundle_fingerprint(&tampered));

    // A second fingerprint pair (shadow) is rejected outright.
    let items = bundle.as_list().unwrap();
    let mut body = items[1].as_list().unwrap().to_vec();
    body.insert(
        0,
        Sexpr::list(vec![
            Sexpr::sym("fingerprint"),
            Sexpr::Str("fnv1a64:0".into()),
        ]),
    );
    let doubled = Sexpr::list(vec![Sexpr::sym("evidence-bundle"), Sexpr::List(body)]);
    assert!(!verify_bundle_fingerprint(&doubled));
}

// ---------------------------------------------------------------------
// Finding 3: rejected candidates stay out of the ledger.
// ---------------------------------------------------------------------

#[test]
fn gate3_failed_result_with_rejected_candidate_contributes_no_artifacts() {
    // The gate's reproduction: a Failed (firewall-rejected) result
    // still carrying its candidate for provenance. It must not appear
    // in the ledger, and the missing-artifact warning for its declared
    // path must NOT be suppressed by content that was never written.
    let rejected = candidate_with_file("m/plan/a", "out/a.rb", "rejected body");
    let results = vec![result("m/plan/a", ExecutionStatus::Failed, Some(rejected))];
    assert_eq!(collect_artifacts(&results).len(), 0);

    let deferred = candidate_with_file("m/plan/b", "out/b.rb", "deferred body");
    let results = vec![result(
        "m/plan/b",
        ExecutionStatus::Deferred,
        Some(deferred),
    )];
    assert_eq!(collect_artifacts(&results).len(), 0);
}

// ---------------------------------------------------------------------
// Finding 4: no vacuous promote.
// ---------------------------------------------------------------------

#[test]
fn gate4_zero_obligation_verification_fails_verification_passed() {
    // A present verification section with total 0 is no evidence:
    // verification-passed must be nil (it laundered into promote
    // before). Absent/nil section stays vacuously t (pinned by the
    // frozen oracle's edge tests).
    let bundle_text = r#"(evidence-bundle ((schema "gymnast.bundle/0.1")
        (ir-fingerprint "fnv1a64:1") (plan-fingerprint "fnv1a64:2")
        (artifacts nil) (traceability nil)
        (dependency-lock (dependency-lock ((plan-fingerprint "fnv1a64:2") (node-locks nil))))
        (verification (verification-bundle ((schema "gymnast.verify/0.1")
          (summary ((total 0) (passed 0) (failed 0) (skipped 0) (indeterminate 0))))))
        (summary ((total-nodes 0) (artifacts-produced 0) (succeeded-nodes 0)
                  (failed-nodes 0) (has-verification t)))
        (diagnostics nil) (fingerprint "fnv1a64:5")))"#;
    let bundle = parse(bundle_text).expect("bundle parses");
    let promotion = evaluate_promotion(&default_promotion_policy(), &bundle);
    assert_eq!(check_value(&promotion, "verification-passed"), "nil");
    assert_eq!(decision_of(&promotion), "hold");
}

#[test]
fn gate4_missing_artifacts_fail_all_artifacts_present() {
    // The real CLI-shaped repro: todo results minus every candidate ->
    // 10 missing-artifact warnings -> all-artifacts-present nil.
    let ir = todo_ir();
    let p = plan(&ir);
    let bundle = assemble_bundle(&ir, &p, &[], None);
    let promotion = evaluate_promotion(&default_promotion_policy(), &bundle);
    assert_eq!(check_value(&promotion, "all-artifacts-present"), "nil");
    assert_eq!(decision_of(&promotion), "hold");
}

// ---------------------------------------------------------------------
// Finding 5: nested verification errors reach no-error-diagnostics.
// ---------------------------------------------------------------------

#[test]
fn gate5_nested_verification_error_fails_no_error_diagnostics() {
    let bundle_text = r#"(evidence-bundle ((schema "gymnast.bundle/0.1")
        (ir-fingerprint "fnv1a64:1") (plan-fingerprint "fnv1a64:2")
        (artifacts nil)
        (traceability ((traceability-entry ((semantic-id "m/type/A") (kind "type")
          (plan-nodes ("m/plan/a")) (has-implementation t) (has-evidence t)))))
        (dependency-lock (dependency-lock ((plan-fingerprint "fnv1a64:2") (node-locks nil))))
        (verification (verification-bundle ((schema "gymnast.verify/0.1")
          (summary ((total 1) (passed 1) (failed 0) (skipped 0) (indeterminate 0)))
          (diagnostics ((diagnostic (severity error) (code "E601") (span 0 0)
            (message "duplicate-obligation-id: m/x")))))))
        (summary ((total-nodes 1) (artifacts-produced 0) (succeeded-nodes 1)
                  (failed-nodes 0) (has-verification t)))
        (diagnostics nil) (fingerprint "fnv1a64:5")))"#;
    let bundle = parse(bundle_text).expect("bundle parses");
    let promotion = evaluate_promotion(&default_promotion_policy(), &bundle);
    assert_eq!(check_value(&promotion, "no-error-diagnostics"), "nil");
    assert_eq!(decision_of(&promotion), "hold");
}

// ---------------------------------------------------------------------
// Finding 6: sizes are BYTES.
// ---------------------------------------------------------------------

#[test]
fn gate6_artifact_size_is_bytes_not_chars() {
    // "héllo→" is 6 chars but 9 bytes (é = 2, → = 3). A chars() count
    // survives an all-ASCII corpus; this pins the byte contract.
    let content = "héllo→";
    assert_eq!(content.chars().count(), 6);
    assert_eq!(content.len(), 9);
    let cand = candidate_with_file("m/plan/a", "out/a.txt", content);
    let results = vec![result("m/plan/a", ExecutionStatus::Succeeded, Some(cand))];
    let artifacts = collect_artifacts(&results);
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].size, 9);
}

// ---------------------------------------------------------------------
// Finding 7: fail-closed pins outside the implementer's module tests.
// ---------------------------------------------------------------------

fn minimal_bundle(drop_field: &str) -> Sexpr {
    let full = r#"(evidence-bundle ((schema "gymnast.bundle/0.1")
        (ir-fingerprint "fnv1a64:1") (plan-fingerprint "fnv1a64:2")
        (artifacts nil)
        (traceability ((traceability-entry ((semantic-id "m/type/A") (kind "type")
          (plan-nodes ("m/plan/a")) (has-implementation t) (has-evidence t)))))
        (dependency-lock (dependency-lock ((plan-fingerprint "fnv1a64:2") (node-locks nil))))
        (verification (verification-bundle ((schema "gymnast.verify/0.1")
          (summary ((total 1) (passed 1) (failed 0) (skipped 0) (indeterminate 0))))))
        (summary ((total-nodes 1) (artifacts-produced 0) (succeeded-nodes 1)
                  (failed-nodes 0) (has-verification t)))
        (diagnostics nil) (fingerprint "fnv1a64:5")))"#;
    let bundle = parse(full).expect("bundle parses");
    let items = bundle.as_list().unwrap();
    let body: Vec<Sexpr> = items[1]
        .as_list()
        .unwrap()
        .iter()
        .filter(|pair| {
            pair.as_list()
                .and_then(|p| p.first())
                .and_then(|s| s.as_sym())
                != Some(drop_field)
        })
        .cloned()
        .collect();
    Sexpr::list(vec![Sexpr::sym("evidence-bundle"), Sexpr::List(body)])
}

#[test]
fn gate7_missing_diagnostics_field_fails_closed() {
    let promotion = evaluate_promotion(&default_promotion_policy(), &minimal_bundle("diagnostics"));
    assert_eq!(check_value(&promotion, "no-error-diagnostics"), "nil");
    assert_eq!(check_value(&promotion, "all-artifacts-present"), "nil");
    assert_eq!(decision_of(&promotion), "hold");
}

#[test]
fn gate7_missing_traceability_field_fails_closed() {
    let promotion =
        evaluate_promotion(&default_promotion_policy(), &minimal_bundle("traceability"));
    assert_eq!(check_value(&promotion, "traceability-complete"), "nil");
    assert_eq!(decision_of(&promotion), "hold");
}

#[test]
fn gate7_unreadable_severity_counts_as_error() {
    let mut bundle = minimal_bundle("diagnostics");
    // Reinsert a diagnostics field holding one severity-less entry.
    if let Sexpr::List(ref mut items) = bundle {
        if let Some(Sexpr::List(ref mut body)) = items.get_mut(1) {
            body.push(Sexpr::list(vec![
                Sexpr::sym("diagnostics"),
                Sexpr::list(vec![parse("(diagnostic ((code mystery)))").unwrap()]),
            ]));
        }
    }
    let promotion = evaluate_promotion(&default_promotion_policy(), &bundle);
    assert_eq!(check_value(&promotion, "no-error-diagnostics"), "nil");
}

#[test]
fn gate7_unreadable_verification_summary_fails_both_checks() {
    let text = minimal_bundle("verification")
        .print()
        .replace("(schema \"gymnast.bundle/0.1\")",
                 "(schema \"gymnast.bundle/0.1\") (verification (verification-bundle ((schema \"gymnast.verify/0.1\") (summary garbled))))");
    let bundle = parse(&text).expect("bundle parses");
    let promotion = evaluate_promotion(&default_promotion_policy(), &bundle);
    assert_eq!(check_value(&promotion, "verification-passed"), "nil");
    assert_eq!(
        check_value(&promotion, "no-indeterminate-verification"),
        "nil"
    );
    assert_eq!(decision_of(&promotion), "hold");
}
