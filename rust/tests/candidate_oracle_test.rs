//! Tests-of-record for `candidate.rs`, authored from
//! `docs/rust-port-plan-phase4.md` section B ALONE, before any
//! implementation of `crate::candidate` exists (the phase-4 process
//! upgrade: Stage 1 commits these oracle files to git before Stage 2
//! touches the crate). `src/candidate.lisp` was consulted only for
//! behavioral intent (which checks exist, in what order); the candidate
//! value's own SHAPE is not specified by section B's prose, so this file
//! resolves that per Process Rule 1 by using the nested
//! `(candidate ((key value) ...))` alist form -- the SAME shape
//! `prompt.rs`'s already-implemented `build_output_protocol` uses for the
//! `OUTPUT PROTOCOL` a candidate must conform to, and consistent with
//! `docs/ir-contract-deltas.md`'s general Rust convention that "every
//! alist is one nested list" (unlike Lamedh's flatter `(candidate (schema
//! ...) (node-id ...) ...)` form in `src/recipe.lisp`, which is NOT used
//! here).
//!
//! Implementation stages MUST NOT edit this file. If an assertion here
//! turns out to conflict with a considered implementation choice, the
//! implementer reports the conflict; only the integrator resolves it.
//!
//! This file will not compile until `crate::candidate` exists -- that is
//! expected at this stage.

use gymnast_rs::candidate::{candidate_diagnostics, candidate_valid, Candidate};
use gymnast_rs::elaborate;
use gymnast_rs::parser;
use gymnast_rs::plan::{plan, PlanNode};
use gymnast_rs::sexpr::Sexpr;
use std::fs;

// ---------------------------------------------------------------------
// Shared fixtures / helpers (not tests themselves).
// ---------------------------------------------------------------------

fn load_todo_ir() -> gymnast_rs::ir::Ir {
    let src = fs::read_to_string("../examples/todo.gym").expect("read ../examples/todo.gym");
    let (ast, parse_diags) = parser::parse(&src);
    let file = ast.expect("parse todo.gym");
    let (ir, _all_diags) = elaborate::elaborate_with_parse_diags(&file, &parse_diags);
    ir
}

fn diag_code(d: &Sexpr) -> Option<String> {
    d.assoc("code").and_then(|c| c.as_str()).map(String::from)
}

/// A hand-built plan node used as the fixed target for the E501-E508
/// dimension-by-dimension tests: one required output path, one declared
/// capability, and a non-lamedh/lisp/scheme target so E507 is reachable.
fn test_node() -> PlanNode {
    PlanNode::new(
        "m/plan/x".to_string(),
        "generative",
        "recipe-v1",
        vec![],
        vec![],
        Sexpr::list(vec![Sexpr::sym("ruby"), Sexpr::sym("rails")]),
        Sexpr::sym("none"),
        vec!["out/a.rb".to_string()],
        vec!["clock".to_string()],
        vec![],
        vec![],
    )
}

/// Builds a `(candidate ((...)))` sexpr in the nested alist convention
/// (see the file header note). `files` are `(path, content)` string
/// pairs; `implements`/`edge_uses` are printed as bare symbols (matching
/// how plan-node `capabilities`/`inputs`-derived vocabulary terms print
/// elsewhere); `assumptions`/`unresolved` are passed through verbatim so
/// callers can construct both the empty (`nil`) and populated cases.
#[allow(clippy::too_many_arguments)]
fn build_candidate(
    node_id: &str,
    files: &[(&str, &str)],
    implements: &[&str],
    edge_uses: &[&str],
    assumptions: Sexpr,
    unresolved: Sexpr,
) -> Sexpr {
    Sexpr::list(vec![
        Sexpr::sym("candidate"),
        Sexpr::list(vec![
            Sexpr::pair("schema", Sexpr::Str("gymnast.candidate/0.1".to_string())),
            Sexpr::pair("node-id", Sexpr::Str(node_id.to_string())),
            Sexpr::pair(
                "files",
                Sexpr::list(
                    files
                        .iter()
                        .map(|(p, c)| {
                            Sexpr::list(vec![Sexpr::Str(p.to_string()), Sexpr::Str(c.to_string())])
                        })
                        .collect(),
                ),
            ),
            Sexpr::pair(
                "implements",
                Sexpr::list(
                    implements
                        .iter()
                        .map(|s| Sexpr::Str(s.to_string()))
                        .collect(),
                ),
            ),
            Sexpr::pair(
                "edge-uses",
                Sexpr::list(edge_uses.iter().map(|s| Sexpr::sym(s)).collect()),
            ),
            Sexpr::pair("assumptions", assumptions),
            Sexpr::pair("unresolved", unresolved),
        ]),
    ])
}

fn nil() -> Sexpr {
    Sexpr::list(vec![])
}

/// The canonical fully-valid candidate for `test_node()`: matches its
/// node-id, writes exactly its one required path with non-Lisp content,
/// and carries no assumptions/unresolved/undeclared edges.
fn good_candidate() -> Sexpr {
    build_candidate(
        "m/plan/x",
        &[("out/a.rb", "puts 1")],
        &[],
        &[],
        nil(),
        nil(),
    )
}

fn codes_of(diags: &[Sexpr]) -> Vec<String> {
    diags.iter().filter_map(diag_code).collect()
}

// ---------------------------------------------------------------------
// Baseline: a fully-valid candidate yields zero diagnostics and is valid.
// ---------------------------------------------------------------------

#[test]
fn baseline_fully_valid_candidate_yields_zero_diagnostics() {
    let node = test_node();
    let candidate = good_candidate();
    let diags = candidate_diagnostics(&node, &candidate);
    assert!(
        diags.is_empty(),
        "a fully-valid candidate must yield zero diagnostics, got {:?}",
        diags
    );
    assert!(candidate_valid(&node, &candidate));
}

// ---------------------------------------------------------------------
// E501 invalid-candidate: not a `(candidate (...))` tagged alist.
// Exclusive: no further checks run.
// ---------------------------------------------------------------------

#[test]
fn e501_fires_when_value_is_not_a_tagged_candidate_alist() {
    let node = test_node();
    let not_a_candidate = Sexpr::list(vec![
        Sexpr::sym("not-candidate"),
        Sexpr::list(vec![Sexpr::sym("garbage")]),
    ]);
    let diags = candidate_diagnostics(&node, &not_a_candidate);
    assert!(codes_of(&diags).contains(&"E501".to_string()));
}

#[test]
fn e501_short_circuits_yielding_exactly_one_diagnostic() {
    let node = test_node();
    // Deliberately ALSO wrong on multiple other dimensions -- a wrong
    // node-id (would be E502) and non-nil assumptions (would be E505) --
    // to prove the wrong tag alone suppresses every other check.
    let not_a_candidate = Sexpr::list(vec![
        Sexpr::sym("not-candidate"),
        Sexpr::list(vec![
            Sexpr::pair("node-id", Sexpr::Str("totally-different-node".to_string())),
            Sexpr::pair(
                "assumptions",
                Sexpr::list(vec![Sexpr::sym("an-assumption")]),
            ),
        ]),
    ]);
    let diags = candidate_diagnostics(&node, &not_a_candidate);
    assert_eq!(
        diags.len(),
        1,
        "an invalid tag must short-circuit to exactly one diagnostic, got {:?}",
        diags
    );
    assert_eq!(diag_code(&diags[0]).as_deref(), Some("E501"));
}

#[test]
fn e501_passes_when_correctly_tagged_even_if_bad_on_other_dimensions() {
    let node = test_node();
    // Correctly tagged `candidate`, but wrong node-id: E501 must not
    // fire merely because SOME other check fails.
    let candidate = build_candidate(
        "some-other-node",
        &[("out/a.rb", "puts 1")],
        &[],
        &[],
        nil(),
        nil(),
    );
    let diags = candidate_diagnostics(&node, &candidate);
    assert!(!codes_of(&diags).contains(&"E501".to_string()));
    assert!(codes_of(&diags).contains(&"E502".to_string()));
}

// ---------------------------------------------------------------------
// E502 candidate-node-mismatch.
// ---------------------------------------------------------------------

#[test]
fn e502_fires_when_node_id_mismatches() {
    let node = test_node();
    let candidate = build_candidate(
        "m/plan/DIFFERENT",
        &[("out/a.rb", "puts 1")],
        &[],
        &[],
        nil(),
        nil(),
    );
    let diags = candidate_diagnostics(&node, &candidate);
    assert!(codes_of(&diags).contains(&"E502".to_string()));
}

#[test]
fn e502_passes_when_node_id_matches() {
    let node = test_node();
    let diags = candidate_diagnostics(&node, &good_candidate());
    assert!(!codes_of(&diags).contains(&"E502".to_string()));
}

// ---------------------------------------------------------------------
// E503 unauthorized-output-path (one per offending path).
// ---------------------------------------------------------------------

#[test]
fn e503_fires_for_a_file_path_outside_may_write() {
    let node = test_node();
    let candidate = build_candidate(
        "m/plan/x",
        &[("out/a.rb", "puts 1"), ("out/evil.rb", "puts 2")],
        &[],
        &[],
        nil(),
        nil(),
    );
    let diags = candidate_diagnostics(&node, &candidate);
    assert!(codes_of(&diags).contains(&"E503".to_string()));
    let e503s: Vec<&Sexpr> = diags
        .iter()
        .filter(|d| diag_code(d).as_deref() == Some("E503"))
        .collect();
    assert_eq!(
        e503s.len(),
        1,
        "exactly one E503 per offending path, got {:?}",
        e503s
    );
    assert!(
        e503s[0].print().contains("out/evil.rb"),
        "E503 diagnostic must name the offending path, got {}",
        e503s[0].print()
    );
}

#[test]
fn e503_passes_when_every_file_path_is_authorized() {
    let node = test_node();
    let diags = candidate_diagnostics(&node, &good_candidate());
    assert!(!codes_of(&diags).contains(&"E503".to_string()));
}

// ---------------------------------------------------------------------
// E504 missing-output-file (one per missing required path).
// ---------------------------------------------------------------------

#[test]
fn e504_fires_when_a_required_output_is_absent() {
    let node = test_node();
    let candidate = build_candidate("m/plan/x", &[], &[], &[], nil(), nil());
    let diags = candidate_diagnostics(&node, &candidate);
    let e504s: Vec<&Sexpr> = diags
        .iter()
        .filter(|d| diag_code(d).as_deref() == Some("E504"))
        .collect();
    assert_eq!(e504s.len(), 1);
    assert!(e504s[0].print().contains("out/a.rb"));
}

#[test]
fn e504_passes_when_every_required_output_is_present() {
    let node = test_node();
    let diags = candidate_diagnostics(&node, &good_candidate());
    assert!(!codes_of(&diags).contains(&"E504".to_string()));
}

// ---------------------------------------------------------------------
// E505 candidate-added-assumptions.
// ---------------------------------------------------------------------

#[test]
fn e505_fires_when_assumptions_present_and_non_nil() {
    let node = test_node();
    let candidate = build_candidate(
        "m/plan/x",
        &[("out/a.rb", "puts 1")],
        &[],
        &[],
        Sexpr::list(vec![Sexpr::sym("an-assumption")]),
        nil(),
    );
    let diags = candidate_diagnostics(&node, &candidate);
    assert!(codes_of(&diags).contains(&"E505".to_string()));
}

#[test]
fn e505_passes_when_assumptions_nil_or_absent() {
    let node = test_node();
    let diags = candidate_diagnostics(&node, &good_candidate());
    assert!(!codes_of(&diags).contains(&"E505".to_string()));

    // Field entirely absent must also count as empty.
    let candidate_absent = Sexpr::list(vec![
        Sexpr::sym("candidate"),
        Sexpr::list(vec![
            Sexpr::pair("schema", Sexpr::Str("gymnast.candidate/0.1".to_string())),
            Sexpr::pair("node-id", Sexpr::Str("m/plan/x".to_string())),
            Sexpr::pair(
                "files",
                Sexpr::list(vec![Sexpr::list(vec![
                    Sexpr::Str("out/a.rb".to_string()),
                    Sexpr::Str("puts 1".to_string()),
                ])]),
            ),
            Sexpr::pair("implements", Sexpr::list(vec![])),
            Sexpr::pair("edge-uses", Sexpr::list(vec![])),
            Sexpr::pair("unresolved", nil()),
        ]),
    ]);
    let diags = candidate_diagnostics(&node, &candidate_absent);
    assert!(!codes_of(&diags).contains(&"E505".to_string()));
}

// ---------------------------------------------------------------------
// E506 candidate-unresolved.
// ---------------------------------------------------------------------

#[test]
fn e506_fires_when_unresolved_present_and_non_nil() {
    let node = test_node();
    let candidate = build_candidate(
        "m/plan/x",
        &[("out/a.rb", "puts 1")],
        &[],
        &[],
        nil(),
        Sexpr::list(vec![Sexpr::sym("some-unresolved-entry")]),
    );
    let diags = candidate_diagnostics(&node, &candidate);
    assert!(codes_of(&diags).contains(&"E506".to_string()));
}

#[test]
fn e506_passes_when_unresolved_nil_or_absent() {
    let node = test_node();
    let diags = candidate_diagnostics(&node, &good_candidate());
    assert!(!codes_of(&diags).contains(&"E506".to_string()));
}

// ---------------------------------------------------------------------
// E507 target-language-violation: non-lisp target AND Lisp-looking
// content (one per offending file).
// ---------------------------------------------------------------------

#[test]
fn e507_fires_for_lisp_looking_content_under_a_non_lisp_target() {
    let node = test_node(); // target (ruby rails)
    let candidate = build_candidate(
        "m/plan/x",
        &[("out/a.rb", "(defun sneaky () 1)")],
        &[],
        &[],
        nil(),
        nil(),
    );
    let diags = candidate_diagnostics(&node, &candidate);
    let e507s: Vec<&Sexpr> = diags
        .iter()
        .filter(|d| diag_code(d).as_deref() == Some("E507"))
        .collect();
    assert_eq!(e507s.len(), 1);
    assert!(
        e507s[0].print().contains("out/a.rb"),
        "E507 message must name the offending path"
    );
}

#[test]
fn e507_passes_for_non_lisp_content_under_a_non_lisp_target() {
    let node = test_node();
    let diags = candidate_diagnostics(&node, &good_candidate());
    assert!(!codes_of(&diags).contains(&"E507".to_string()));
}

#[test]
fn e507_never_fires_when_target_is_lamedh_even_with_lisp_markers() {
    let lamedh_node = PlanNode::new(
        "m/plan/y".to_string(),
        "generative",
        "recipe-v1",
        vec![],
        vec![],
        Sexpr::sym("lamedh"),
        Sexpr::sym("none"),
        vec!["out/b.lisp".to_string()],
        vec![],
        vec![],
        vec![],
    );
    let candidate = build_candidate(
        "m/plan/y",
        &[("out/b.lisp", "(defun ok () 1)")],
        &[],
        &[],
        nil(),
        nil(),
    );
    let diags = candidate_diagnostics(&lamedh_node, &candidate);
    assert!(!codes_of(&diags).contains(&"E507".to_string()));
}

// ---------------------------------------------------------------------
// E508 undeclared-edge-use (one per undeclared edge).
// ---------------------------------------------------------------------

#[test]
fn e508_fires_for_an_edge_use_not_in_node_capabilities() {
    let node = test_node(); // capabilities = ["clock"]
    let candidate = build_candidate(
        "m/plan/x",
        &[("out/a.rb", "puts 1")],
        &[],
        &["network"],
        nil(),
        nil(),
    );
    let diags = candidate_diagnostics(&node, &candidate);
    let e508s: Vec<&Sexpr> = diags
        .iter()
        .filter(|d| diag_code(d).as_deref() == Some("E508"))
        .collect();
    assert_eq!(e508s.len(), 1);
    assert!(e508s[0].print().contains("network"));
}

#[test]
fn e508_passes_for_a_declared_edge_use() {
    let node = test_node(); // capabilities = ["clock"]
    let candidate = build_candidate(
        "m/plan/x",
        &[("out/a.rb", "puts 1")],
        &[],
        &["clock"],
        nil(),
        nil(),
    );
    let diags = candidate_diagnostics(&node, &candidate);
    assert!(!codes_of(&diags).contains(&"E508".to_string()));
}

// ---------------------------------------------------------------------
// A fully-valid candidate for a real todo.gym plan node yields zero
// diagnostics.
// ---------------------------------------------------------------------

#[test]
fn fully_valid_candidate_for_real_todo_node_yields_zero_diagnostics() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    let node = p
        .nodes
        .iter()
        .find(|n| n.id.ends_with("/plan/design-contracts"))
        .expect("todo.gym plan must have a design-contracts node");
    assert_eq!(node.may_write.len(), 1);
    let path = node.may_write[0].as_str();
    let candidate = build_candidate(
        &node.id,
        &[(path, "module Contracts\nend\n")],
        &[],
        &[],
        nil(),
        nil(),
    );
    let diags = candidate_diagnostics(node, &candidate);
    assert!(
        diags.is_empty(),
        "a fully-valid candidate for the real design-contracts node must yield zero \
         diagnostics, got {:?}",
        diags
    );
    assert!(candidate_valid(node, &candidate));
}

// ---------------------------------------------------------------------
// Candidate field accessors, checked independently of the firewall.
// ---------------------------------------------------------------------

#[test]
fn candidate_from_sexpr_some_iff_tagged_alist() {
    assert!(Candidate::from_sexpr(good_candidate()).is_some());
    assert!(Candidate::from_sexpr(Sexpr::sym("nope")).is_none());
    assert!(Candidate::from_sexpr(Sexpr::list(vec![Sexpr::sym("not-candidate")])).is_none());
}

#[test]
fn candidate_field_access_is_total_missing_fields_read_as_empty() {
    let bare = Sexpr::list(vec![Sexpr::sym("candidate"), Sexpr::list(vec![])]);
    let c = Candidate::from_sexpr(bare).expect("bare (candidate ()) must still parse as tagged");
    assert_eq!(c.node_id(), None);
    assert_eq!(c.files(), Vec::<(String, String)>::new());
    assert_eq!(c.implements(), Vec::<String>::new());
    assert_eq!(c.edge_uses(), Vec::<String>::new());
    assert!(c.assumptions_empty());
    assert!(c.unresolved_empty());
}

#[test]
fn candidate_accessors_read_populated_fields() {
    let candidate = build_candidate(
        "m/plan/x",
        &[("out/a.rb", "puts 1"), ("out/b.rb", "puts 2")],
        &["m/type/A", "m/type/B"],
        &["clock"],
        nil(),
        nil(),
    );
    let c = Candidate::from_sexpr(candidate).unwrap();
    assert_eq!(c.node_id(), Some("m/plan/x"));
    assert_eq!(
        c.files(),
        vec![
            ("out/a.rb".to_string(), "puts 1".to_string()),
            ("out/b.rb".to_string(), "puts 2".to_string()),
        ]
    );
    assert_eq!(
        c.implements(),
        vec!["m/type/A".to_string(), "m/type/B".to_string()]
    );
    assert_eq!(c.edge_uses(), vec!["clock".to_string()]);
    assert!(c.assumptions_empty());
    assert!(c.unresolved_empty());
}
