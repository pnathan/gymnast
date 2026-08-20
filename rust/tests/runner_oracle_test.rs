//! Tests-of-record for `runner.rs` (the bounded model-run loop), the
//! section-B `claude_model_flag` pure mapping, and the phase-5 fold-in
//! APIs from `docs/rust-port-plan-phase5.md` (scope item 1), authored
//! from that plan document ALONE, before `crate::runner` exists (the
//! phase-4/5 process upgrade: Stage 1 commits this oracle file to git
//! before any implementation stage runs). `src/runner.lisp` was
//! consulted only for behavioral intent; shapes come from the phase-5
//! doc (sections A, B) and the Rust IR/plan/prompt/candidate/recipe
//! contracts already committed in `rust/src/`.
//!
//! Implementation stages MUST NOT edit this file. If an assertion here
//! turns out to conflict with a considered implementation choice, the
//! implementer reports the conflict; only the integrator resolves it.
//!
//! This file will not compile until `crate::runner` exists (with
//! `lib.rs` gaining `pub mod runner;`), `Plan::node` and
//! `ExecutionResult::from_sexpr` exist, deferred `ExecutionResult`
//! values carry `recipe_identity`, and `candidate::is_unsafe_output_path`
//! exists — that is expected at this stage.
//!
//! Numbering below follows the phase-5 doc's "Oracle tests" list items
//! 1-11 exactly; each gets one or more `#[test]`s, none merged or
//! dropped.
//!
//! NOTE (ambiguities, reported per Process Rule 1 — resolved with the
//! contract-consistent reading since section A gives `ScriptedProvider`'s
//! fields only as a non-`pub` placeholder comment, not a full API):
//!
//! 1. `ScriptedProvider` is used via `ScriptedProvider::new(responses:
//!    Vec<Option<String>>) -> ScriptedProvider` (matching the `IrNode::new`
//!    / `PlanNode::new` constructor convention already used throughout
//!    this crate) plus `ScriptedProvider::call_count(&self) -> usize`
//!    (oracle item 10 explicitly requires reading back "provider never
//!    called" via "ScriptedProvider records call count", which requires
//!    a public accessor since the doc's field list is not `pub`).
//! 2. Oracle items 3 and 8 require inspecting the exact `ModelRequest`
//!    sent on a specific call (its `prompt_text` / `node_id`), which
//!    `ScriptedProvider` (a response script only) has no reason to
//!    expose. This file defines its own `RecordingProvider` test helper
//!    implementing the PUBLIC `Provider` trait to capture every request
//!    — infrastructure local to this test file, not a pinned library
//!    API.
//! 3. `claude_model_flag(&Sexpr) -> String` (section B) is assumed to
//!    live in `crate::runner` alongside `ClaudeSubprocessProvider` (the
//!    only module section B introduces). This file does not construct
//!    `ClaudeSubprocessProvider` itself: its "max run config" shape is
//!    given only in prose, is not part of the numbered oracle list
//!    above, and the doc explicitly limits its guard to "unit tests
//!    only construct it and check the model-flag mapping" — the mapping
//!    table (item 11) is pinned here; the constructor shape is left for
//!    the Stage 3 implementer to choose freely.

use gymnast_rs::candidate::is_unsafe_output_path;
use gymnast_rs::elaborate;
use gymnast_rs::ir::{Ir, IrNode};
use gymnast_rs::parser;
use gymnast_rs::plan::{plan, Plan, PlanNode};
use gymnast_rs::prompt::compile_prompt;
use gymnast_rs::recipe::{execute_recipe, ExecutionResult, ExecutionStatus};
use gymnast_rs::runner::{
    claude_model_flag, extract_sexpr, run_generative_nodes, run_node, Attempt, AttemptStatus,
    ModelRequest, Provider, RunResult, RunStatus, ScriptedProvider,
};
use gymnast_rs::sexpr::{self, Sexpr};
use std::fs;

// ---------------------------------------------------------------------
// Shared fixtures / helpers (not tests themselves).
// ---------------------------------------------------------------------

const TRANSITION_NODE_ID: &str = "m/plan/transition-kernel";

/// A minimal `Ir` with one type node, module `m` — enough for `plan::plan`
/// to build the fixed 8-node table with a default (lamedh) target, so
/// candidates built against it never trip E507 by accident.
fn minimal_ir() -> Ir {
    let type_node = IrNode::new(
        "m/type/Foo".to_string(),
        "type",
        "Foo".to_string(),
        vec![],
        vec![],
    );
    Ir::new(
        "gymnast.ir/0.1".to_string(),
        "m".to_string(),
        vec![],
        vec![type_node],
        vec![],
        vec![],
        vec![],
        vec![],
    )
}

fn minimal_plan() -> Plan {
    plan(&minimal_ir())
}

/// The `transition-kernel` generative plan node from `minimal_plan()`,
/// fetched through the fold-in `Plan::node` accessor (oracle item 11a) —
/// used as the shared subject node for the run-loop oracle tests (items
/// 2-7, 10).
fn transition_node(p: &Plan) -> PlanNode {
    p.node(TRANSITION_NODE_ID)
        .expect("minimal_plan must contain the transition-kernel node")
        .clone()
}

fn nil() -> Sexpr {
    Sexpr::list(vec![])
}

/// Builds a `(candidate ((node-id "...") (files (...)) (assumptions nil)
/// (unresolved nil)))` value, the same untrusted-candidate shape used
/// throughout `candidate.rs`'s own oracle tests.
fn candidate_sexpr(node_id: &str, files: &[(&str, &str)]) -> Sexpr {
    Sexpr::list(vec![
        Sexpr::sym("candidate"),
        Sexpr::list(vec![
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
            Sexpr::pair("assumptions", nil()),
            Sexpr::pair("unresolved", nil()),
        ]),
    ])
}

fn diag_codes(diags: &[Sexpr]) -> Vec<String> {
    diags
        .iter()
        .filter_map(|d| d.assoc("code").and_then(|c| c.as_str().map(String::from)))
        .collect()
}

fn load_todo_ir() -> Ir {
    let src = fs::read_to_string("../examples/todo.gym").expect("read ../examples/todo.gym");
    let (ast, parse_diags) = parser::parse(&src);
    let file = ast.expect("parse todo.gym");
    let (ir, _all_diags) = elaborate::elaborate_with_parse_diags(&file, &parse_diags);
    ir
}

/// Test-local `Provider` that records every `ModelRequest` it receives
/// (see ambiguity note 2 above) in addition to replaying a response
/// script like `ScriptedProvider`.
struct RecordingProvider {
    responses: Vec<Option<String>>,
    cursor: usize,
    requests: Vec<ModelRequest>,
}

impl RecordingProvider {
    fn new(responses: Vec<Option<String>>) -> RecordingProvider {
        RecordingProvider {
            responses,
            cursor: 0,
            requests: Vec::new(),
        }
    }
}

impl Provider for RecordingProvider {
    fn synthesize(&mut self, request: &ModelRequest) -> Option<String> {
        self.requests.push(request.clone());
        let response = self.responses.get(self.cursor).cloned().unwrap_or(None);
        self.cursor += 1;
        response
    }
}

fn generative_recipe_node(id: &str, recipe: &str) -> PlanNode {
    PlanNode::new(
        id.to_string(),
        "generative",
        recipe,
        vec![],
        vec![],
        Sexpr::list(vec![Sexpr::sym("ruby"), Sexpr::sym("rails")]),
        Sexpr::sym("none"),
        vec![],
        vec![],
        vec![],
        vec![],
    )
}

// ---------------------------------------------------------------------
// 1. extract_sexpr
// ---------------------------------------------------------------------

#[test]
fn oracle_01_extract_sexpr_markdown_fenced_candidate() {
    let input = "```lisp\n(candidate (foo))\n```";
    assert_eq!(extract_sexpr(input), "(candidate (foo))");
}

#[test]
fn oracle_01_extract_sexpr_no_parens_unchanged() {
    let input = "no parens here at all";
    assert_eq!(extract_sexpr(input), input);
}

#[test]
fn oracle_01_extract_sexpr_close_before_open_unchanged() {
    // A lone ')' precedes the only '(': the two do not exist "in that
    // order", so the text must pass through unchanged.
    let input = ") closed first ( opened later";
    assert_eq!(extract_sexpr(input), input);
}

#[test]
fn oracle_01_extract_sexpr_nested_content_first_to_last() {
    let input = "junk (a (b (c)) d) trailing junk";
    assert_eq!(extract_sexpr(input), "(a (b (c)) d)");
}

// ---------------------------------------------------------------------
// 2. First-attempt success
// ---------------------------------------------------------------------

#[test]
fn oracle_02_first_attempt_success() {
    let ir = minimal_ir();
    let p = minimal_plan();
    let node = transition_node(&p);
    let path = node.may_write[0].clone();

    let good = candidate_sexpr(TRANSITION_NODE_ID, &[(path.as_str(), "; ok")]);
    let mut provider = ScriptedProvider::new(vec![Some(good.print())]);

    let result = run_node(&ir, &p, &node, &mut provider, 3);

    assert_eq!(result.status, RunStatus::Succeeded);
    assert_eq!(result.attempts.len(), 1);
    assert_eq!(result.attempts[0].status, AttemptStatus::Accepted);
    assert!(result.candidate.is_some());

    let package = compile_prompt(&ir, &p, &node);
    assert_eq!(result.attempts[0].prompt_fingerprint, package.fingerprint);
}

// ---------------------------------------------------------------------
// 3. Reject-then-accept
// ---------------------------------------------------------------------

#[test]
fn oracle_03_reject_then_accept() {
    let ir = minimal_ir();
    let p = minimal_plan();
    let node = transition_node(&p);
    let path = node.may_write[0].clone();

    // Wrong node-id (E502) with a >2000-byte file content, so the
    // rejected-output truncation rule is exercised too.
    let long_content = "A".repeat(2500);
    let bad = candidate_sexpr("not-the-right-node-id", &[(path.as_str(), &long_content)]);
    let good = candidate_sexpr(TRANSITION_NODE_ID, &[(path.as_str(), "; ok")]);

    let mut provider = RecordingProvider::new(vec![Some(bad.print()), Some(good.print())]);
    let result = run_node(&ir, &p, &node, &mut provider, 3);

    assert_eq!(result.status, RunStatus::Succeeded);
    assert_eq!(result.attempts.len(), 2);
    assert_eq!(result.attempts[0].status, AttemptStatus::Rejected);
    assert_eq!(result.attempts[1].status, AttemptStatus::Accepted);
    assert!(diag_codes(&result.attempts[0].diagnostics).contains(&"E502".to_string()));

    assert_eq!(provider.requests.len(), 2);
    let second_request_text = &provider.requests[1].prompt_text;
    assert!(
        second_request_text.contains("REPAIR ATTEMPT 2"),
        "{}",
        second_request_text
    );
    assert!(
        second_request_text.contains("E502"),
        "{}",
        second_request_text
    );
    assert!(
        second_request_text.contains("... [truncated]"),
        "{}",
        second_request_text
    );

    assert_ne!(
        result.attempts[0].prompt_fingerprint,
        result.attempts[1].prompt_fingerprint,
        "the repaired attempt's prompt fingerprint must be recomputed, not carried over"
    );
}

// ---------------------------------------------------------------------
// 4. Exhaustion
// ---------------------------------------------------------------------

#[test]
fn oracle_04_exhaustion_always_invalid_script() {
    let ir = minimal_ir();
    let p = minimal_plan();
    let node = transition_node(&p);
    let path = node.may_write[0].clone();

    let bad = candidate_sexpr("still-not-the-right-node-id", &[(path.as_str(), "; nope")]);
    let mut provider =
        ScriptedProvider::new(vec![Some(bad.print()), Some(bad.print()), Some(bad.print())]);

    let result = run_node(&ir, &p, &node, &mut provider, 3);

    assert_eq!(result.status, RunStatus::Exhausted);
    assert_eq!(result.attempts.len(), 3);
    for attempt in &result.attempts {
        assert_eq!(attempt.status, AttemptStatus::Rejected);
    }
    assert!(result.candidate.is_none());
}

// ---------------------------------------------------------------------
// 5. None-response and unparseable-response attempts
// ---------------------------------------------------------------------

#[test]
fn oracle_05_none_and_unparseable_responses_record_e514_and_continue() {
    let ir = minimal_ir();
    let p = minimal_plan();
    let node = transition_node(&p);
    let path = node.may_write[0].clone();

    let good = candidate_sexpr(TRANSITION_NODE_ID, &[(path.as_str(), "; ok")]);
    let mut provider = ScriptedProvider::new(vec![
        None,                                  // provider itself failed
        Some("((( not a closed sexpr".to_string()), // unparseable
        Some(good.print()),                    // recovers on the third attempt
    ]);

    let result = run_node(&ir, &p, &node, &mut provider, 3);

    assert_eq!(result.attempts.len(), 3);

    let none_diags = diag_codes(&result.attempts[0].diagnostics);
    assert_eq!(none_diags, vec!["E514".to_string()]);
    assert_eq!(result.attempts[0].status, AttemptStatus::Rejected);

    let unparseable_diags = diag_codes(&result.attempts[1].diagnostics);
    assert_eq!(unparseable_diags, vec!["E514".to_string()]);
    assert_eq!(result.attempts[1].status, AttemptStatus::Rejected);

    assert_eq!(result.attempts[2].status, AttemptStatus::Accepted);
    assert_eq!(result.status, RunStatus::Succeeded);
}

// ---------------------------------------------------------------------
// 6. Determinism
// ---------------------------------------------------------------------

#[test]
fn oracle_06_determinism_byte_identical_serializations() {
    let ir = minimal_ir();
    let p = minimal_plan();
    let node = transition_node(&p);
    let path = node.may_write[0].clone();

    let long_content = "B".repeat(2200);
    let bad = candidate_sexpr("nope", &[(path.as_str(), &long_content)]);
    let good = candidate_sexpr(TRANSITION_NODE_ID, &[(path.as_str(), "; ok")]);

    let script = || vec![Some(bad.print()), Some(good.print())];

    let mut provider_a = ScriptedProvider::new(script());
    let result_a = run_node(&ir, &p, &node, &mut provider_a, 3);

    let mut provider_b = ScriptedProvider::new(script());
    let result_b = run_node(&ir, &p, &node, &mut provider_b, 3);

    assert_eq!(result_a.to_sexpr().print(), result_b.to_sexpr().print());
}

// ---------------------------------------------------------------------
// 7. Firewall supremacy
// ---------------------------------------------------------------------

#[test]
fn oracle_07_firewall_supremacy_tampered_node_never_succeeds() {
    let ir = minimal_ir();
    let p = minimal_plan();
    let mut node = transition_node(&p);
    let path = node.may_write[0].clone();
    // Tamper the node's contract after construction without re-deriving
    // its fingerprint (mirrors candidate.rs's own gate-regression test).
    node.may_write.push("generated/domain/injected.lisp".to_string());
    assert!(!node.verify_fingerprint());

    // A candidate perfectly shaped for the ORIGINAL (untampered) contract.
    let good = candidate_sexpr(TRANSITION_NODE_ID, &[(path.as_str(), "; ok")]);
    let mut provider =
        ScriptedProvider::new(vec![Some(good.print()), Some(good.print()), Some(good.print())]);

    let result = run_node(&ir, &p, &node, &mut provider, 3);

    assert_eq!(result.status, RunStatus::Exhausted);
    assert!(result.candidate.is_none());
    for attempt in &result.attempts {
        assert_eq!(attempt.status, AttemptStatus::Rejected);
        assert!(
            diag_codes(&attempt.diagnostics).contains(&"E513".to_string()),
            "every attempt against a tampered node must carry E513, got {:?}",
            diag_codes(&attempt.diagnostics)
        );
    }
}

// ---------------------------------------------------------------------
// 8. run_generative_nodes over todo.gym
// ---------------------------------------------------------------------

#[test]
fn oracle_08_run_generative_nodes_over_todo_gym() {
    let ir = load_todo_ir();
    let p = plan(&ir);

    let expected_ids = [
        "todo/plan/transition-kernel",
        "todo/plan/authorization-policy",
        "todo/plan/persistence",
        "todo/plan/service-handlers",
    ];

    let mut provider = RecordingProvider::new(vec![None, None, None, None]);
    let results = run_generative_nodes(&ir, &p, &mut provider, 1);

    let result_ids: Vec<&str> = results.iter().map(|r| r.node_id.as_str()).collect();
    assert_eq!(result_ids, expected_ids);

    assert_eq!(provider.requests.len(), 4);
    let request_ids: Vec<&str> = provider
        .requests
        .iter()
        .map(|r| r.node_id.as_str())
        .collect();
    assert_eq!(request_ids, expected_ids);
}

// ---------------------------------------------------------------------
// 9. No-evaluation invariant
// ---------------------------------------------------------------------

#[test]
fn oracle_09_no_evaluation_invariant_defun_lands_as_data_in_e507() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    let node = p
        .node("todo/plan/transition-kernel")
        .expect("todo.gym plan must contain transition-kernel")
        .clone();
    let path = node.may_write[0].clone();

    // A hostile candidate whose file content is Lisp-shaped code, for a
    // ruby-target node. The runner must never evaluate it -- it can only
    // ever land as DATA inside a candidate-firewall diagnostic.
    let hostile = candidate_sexpr(
        "todo/plan/transition-kernel",
        &[(path.as_str(), "def evil\n  (defun evil ())\nend\n")],
    );
    let mut provider = ScriptedProvider::new(vec![Some(hostile.print())]);

    let result = run_node(&ir, &p, &node, &mut provider, 1);

    assert_eq!(result.attempts.len(), 1);
    assert!(
        diag_codes(&result.attempts[0].diagnostics).contains(&"E507".to_string()),
        "hostile Lisp-shaped content targeting Ruby must trip E507, got {:?}",
        diag_codes(&result.attempts[0].diagnostics)
    );
    assert_eq!(result.attempts[0].status, AttemptStatus::Rejected);
}

// ---------------------------------------------------------------------
// 10. max_attempts = 0
// ---------------------------------------------------------------------

#[test]
fn oracle_10_zero_max_attempts_never_calls_provider() {
    let ir = minimal_ir();
    let p = minimal_plan();
    let node = transition_node(&p);
    let path = node.may_write[0].clone();
    let good = candidate_sexpr(TRANSITION_NODE_ID, &[(path.as_str(), "; ok")]);

    let mut provider = ScriptedProvider::new(vec![Some(good.print())]);
    let result = run_node(&ir, &p, &node, &mut provider, 0);

    assert_eq!(result.status, RunStatus::Exhausted);
    assert!(result.attempts.is_empty());
    assert!(result.candidate.is_none());
    assert_eq!(
        provider.call_count(),
        0,
        "max_attempts = 0 must never call the provider"
    );
}

// ---------------------------------------------------------------------
// Supplementary: the runner never mutates the plan node passed to it
// (section A, "Runner invariants" — type-enforced via `&PlanNode`/`&Plan`,
// pinned here as a value-equality regression guard too).
// ---------------------------------------------------------------------

#[test]
fn invariant_run_node_never_mutates_its_node_argument() {
    let ir = minimal_ir();
    let p = minimal_plan();
    let node = transition_node(&p);
    let before = node.clone();
    let path = node.may_write[0].clone();

    let good = candidate_sexpr(TRANSITION_NODE_ID, &[(path.as_str(), "; ok")]);
    let mut provider = ScriptedProvider::new(vec![Some(good.print())]);
    let _ = run_node(&ir, &p, &node, &mut provider, 3);

    assert_eq!(node, before);
}

// ---------------------------------------------------------------------
// 11. Fold-ins
// ---------------------------------------------------------------------

#[test]
fn oracle_11a_plan_node_hit_and_miss() {
    let p = minimal_plan();
    let hit = p.node(TRANSITION_NODE_ID);
    assert!(hit.is_some());
    assert_eq!(hit.unwrap().id, TRANSITION_NODE_ID);

    let miss = p.node("m/plan/does-not-exist");
    assert!(miss.is_none());
}

#[test]
fn oracle_11b_execution_result_from_sexpr_round_trips_todo_results_golden() {
    let golden = include_str!("fixtures/todo-results.sexpr");
    let parsed = sexpr::parse(golden).expect("todo-results.sexpr must parse");
    let entries = parsed
        .as_list()
        .and_then(|items| items.get(1))
        .and_then(|body| body.as_list())
        .expect("(results (...)) shape");

    assert!(!entries.is_empty(), "golden must carry at least one entry");

    for entry in entries {
        let result = ExecutionResult::from_sexpr(entry)
            .unwrap_or_else(|| panic!("from_sexpr must parse every golden entry: {}", entry.print()));
        assert_eq!(
            result.to_sexpr().print(),
            entry.print(),
            "round-trip through ExecutionResult must reprint byte-identically"
        );
    }
}

#[test]
fn oracle_11c_deferred_execution_result_carries_recipe_identity() {
    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "m".to_string(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    );
    let node = generative_recipe_node("m/plan/x", "transition-kernel-v1");
    let result = execute_recipe(&ir, &node);

    assert_eq!(result.status, ExecutionStatus::Deferred);
    assert_eq!(
        result.recipe_identity.as_deref(),
        Some("transition-kernel-v1"),
        "phase 5 fold-in 1c: deferred results must carry recipe_identity for the trust boundary"
    );

    let printed = result.to_sexpr().print();
    assert!(
        printed.contains("(recipe-identity \"transition-kernel-v1\")"),
        "{}",
        printed
    );
}

#[test]
fn oracle_11d_is_unsafe_output_path_table() {
    assert!(is_unsafe_output_path("../x"));
    assert!(is_unsafe_output_path("/abs"));
    assert!(is_unsafe_output_path("a/../b"));
    // Documented over-rejection (phase-5 scope item 1e permits this): the
    // contains-based check flags any path whose bytes merely contain the
    // two-character run "..", even where it is not a real ".." component.
    assert!(is_unsafe_output_path("a..b.rb"));
    // New in phase 5: backslash rejection.
    assert!(is_unsafe_output_path("back\\slash"));
    assert!(!is_unsafe_output_path("generated/design/contracts.rb"));
}

#[test]
fn oracle_11e_claude_model_flag_mapping_table() {
    // small_code_model-headed list -> "haiku".
    assert_eq!(
        claude_model_flag(&Sexpr::list(vec![
            Sexpr::sym("small_code_model"),
            Sexpr::list(vec![]),
        ])),
        "haiku"
    );
    // A list headed by another symbol -> that symbol's text.
    assert_eq!(
        claude_model_flag(&Sexpr::list(vec![
            Sexpr::sym("claude_opus"),
            Sexpr::sym("extra"),
        ])),
        "claude_opus"
    );
    // A bare symbol -> itself.
    assert_eq!(
        claude_model_flag(&Sexpr::sym("bare_symbol_model")),
        "bare_symbol_model"
    );
    // A bare string -> itself.
    assert_eq!(
        claude_model_flag(&Sexpr::Str("string_model".to_string())),
        "string_model"
    );
    // Anything else (an int, or a list with no head symbol) -> "haiku".
    assert_eq!(claude_model_flag(&Sexpr::Int(42)), "haiku");
    assert_eq!(claude_model_flag(&Sexpr::list(vec![])), "haiku");
}

// Keeps `Attempt`/`RunResult` field-shape referenced directly (not just
// through helper return values), so a future accidental field rename is
// caught here too.
#[test]
fn oracle_11_supplementary_public_field_shapes_are_referenced() {
    let ir = minimal_ir();
    let p = minimal_plan();
    let node = transition_node(&p);
    let path = node.may_write[0].clone();
    let good = candidate_sexpr(TRANSITION_NODE_ID, &[(path.as_str(), "; ok")]);
    let mut provider = ScriptedProvider::new(vec![Some(good.print())]);
    let result: RunResult = run_node(&ir, &p, &node, &mut provider, 1);
    let attempt: &Attempt = &result.attempts[0];
    assert_eq!(attempt.number, 1);
    assert!(attempt.response_length >= 0);
    assert!(!attempt.response_fingerprint.is_empty());
}
