//! Tests-of-record for `assembly.rs` (`docs/rust-port-plan-phase8.md`,
//! sections A-C). Authored from the plan ALONE, BEFORE `crate::assembly`
//! exists (the committed-oracle discipline: Stage 1 commits this file to
//! git red, before any implementation stage runs). `src/assembly.lisp`
//! (read in full) was consulted only for BEHAVIORAL INTENT; every Rust
//! shape adaptation comes from the phase-8 plan's explicit signatures.
//!
//! Implementation stages MUST NOT edit this file. If an assertion here
//! turns out to conflict with a considered implementation choice, the
//! implementer reports the conflict; only the integrator resolves it.
//!
//! This file will not compile until `crate::assembly` exists -- the
//! standard pattern from phases 4-7. Gating the `gymnast_rs::assembly`
//! imports so the file "fails tests, not compilation" is not achievable
//! with std-only stable Rust (there is no cfg predicate for "module
//! exists", and a feature gate would let the implementation stages run
//! with the oracle silently skipped), so the import is unconditional and
//! the red state is a compile failure until Stage 2 lands. That is
//! expected at this stage.
//!
//! RESOLVED AMBIGUITIES (plan text under-specifies these; the
//! contract-consistent reading taken here is noted at each site too):
//!
//!  1. Diagnostic shape: the plan writes
//!     `(diagnostic ((severity error) (code untracked-artifact) ...))`
//!     -- the NESTED house convention (one body alist), unlike
//!     `diag::diag_sexpr`'s flat span-carrying shape. The Lamedh
//!     constructor is `(make-gymnast-diagnostic severity code subject
//!     message details)`; the plan names `subject` and `message`
//!     explicitly and elides the rest as `...`. Pinned here: two-element
//!     `(diagnostic (<body>))` form whose body's FIRST two pairs are
//!     `(severity <sym>)` then `(code <sym>)` in that order, and whose
//!     body carries `(subject "<string>")` and `(message "<string>")`
//!     pairs (severity and code as bare symbols exactly as the plan
//!     prints them; subject/message as strings -- the Lamedh subject is
//!     `princ-to-string`ed). Field order after `code`, and any trailing
//!     reference-parity field (`details`), are NOT pinned here; test 08's
//!     byte-golden freezes whatever Stage 2 chose.
//!  2. `dependency_lock`'s `(recipe "...")` in the plan's shape sketch is
//!     read as a PLACEHOLDER for the node's recipe value, printed the way
//!     the crate prints recipe everywhere else: a bare symbol (the
//!     id-vs-vocabulary-term convention `PlanNode::field_pairs`
//!     establishes -- `(recipe design-contracts-v1)` in the committed
//!     plan fixture). `node-id`/`fingerprint`/`plan-fingerprint` are
//!     genuine strings there, exactly as on `plan-node` itself.
//!  3. The plan gives serialization shapes for `Artifact` and
//!     `TraceabilityEntry` but no method names. This oracle asserts those
//!     shapes ONLY through `assemble_bundle`'s output (whose `artifacts`
//!     / `traceability` fields must contain them), and reads the typed
//!     values through their pub fields -- no `to_sexpr` method is
//!     assumed.
//!  4. `TraceabilityEntry.kind` serializes as a STRING, `(kind "...")`,
//!     exactly as the plan's section-A sketch quotes it (semantic-id and
//!     kind both quoted there).
//!  5. The bundle's `(dependency-lock (...))` field holds
//!     `dependency_lock(plan)`'s form VERBATIM as its value -- Lamedh
//!     parity with `(list 'dependency-lock lock)`, the same
//!     value-verbatim convention the results file uses for
//!     `(candidate (candidate (...)))` and the plan spells out for
//!     `(verification <bundle>|nil)`.
//!  6. `has_evidence` is STATUS-BLIND: the Lamedh filters results by
//!     node-id membership in the entry's plan-nodes alone, so a
//!     `deferred` result is evidence-presence too. Pinned in
//!     oracle_04c.
//!
//! ---------------------------------------------------------------------
//! Derived todo.gym pins (all re-derived from the committed fixtures
//! tests/fixtures/todo-{ir,plan,verify,results}.sexpr and from running
//! `cargo run -q -- compile ../examples/todo.gym <tmp>`):
//!
//! Plan nodes (todo-plan.sexpr, in order), status from todo-results.sexpr:
//!   1 todo/plan/design-contracts     structural   succeeded  1 file
//!   2 todo/plan/transition-kernel    generative   deferred   0 files
//!   3 todo/plan/authorization-policy generative   deferred   0 files
//!   4 todo/plan/persistence          generative   deferred   0 files
//!   5 todo/plan/interface-contracts  structural   succeeded  1 file
//!   6 todo/plan/service-handlers     generative   deferred   0 files
//!   7 todo/plan/acceptance-harness   verification succeeded  1 file
//!   8 todo/plan/application-assembly assembly     succeeded  2 files
//!   -> total-nodes 8; succeeded-nodes 4; failed-nodes 0; deferred 4
//!      (counting toward neither); artifacts 1+0+0+0+1+0+1+2 = 5, in
//!      result order:
//!        generated/design/contracts.rb        (design-contracts)
//!        generated/interfaces/contracts.rb    (interface-contracts)
//!        generated/verification/acceptance.rb (acceptance-harness)
//!        generated/application.rb             (application-assembly)
//!        generated/manifest.sexpr             (application-assembly)
//!
//! Declared may-write paths (concatenated in plan-node order; each
//! node's own list is sorted by `PlanNode::new`), 1+1+1+2+1+1+1+2 = 10:
//!    1 generated/design/contracts.rb        (node 1)
//!    2 generated/domain/transitions.rb      (node 2)
//!    3 generated/domain/authorization.rb    (node 3)
//!    4 generated/adapters/persistence.rb    (node 4)
//!    5 generated/adapters/schema.sexpr      (node 4)
//!    6 generated/interfaces/contracts.rb    (node 5)
//!    7 generated/service/handlers.rb        (node 6)
//!    8 generated/verification/acceptance.rb (node 7)
//!    9 generated/application.rb             (node 8)
//!   10 generated/manifest.sexpr             (node 8)
//!   All 5 produced paths are declared -> untracked = 0.
//!   Declared-but-not-produced (declared order) = rows 2,3,4,5,7 ->
//!   exactly 5 missing-artifact warnings.
//!
//! Capability edges (todo-plan.sexpr): concatenated capabilities =
//!   node2 (clock id-source) ++ node4 (durable-store transactions) ++
//!   node6 (clock id-source identity repository), 2+2+4 = 8 entries;
//!   concatenated prohibitions = 2+3+2+2+1+3+3+2 = 18 entries
//!   {add-dependencies invent-product-semantics invent-errors perform-io
//!    weaken-preconditions grant-undeclared-capabilities
//!    reveal-resource-existence choose-unpinned-dependencies
//!    perform-network-io change-observable-contract access-filesystem
//!    access-network add-endpoints read-generated-rationale skip-failures
//!    weaken-obligations undeclared-capabilities untracked-artifacts}.
//!   Intersection with {clock id-source durable-store transactions
//!   identity repository} = empty -> 0 prohibited-capability errors.
//!
//! Traceability (todo-ir.sexpr all-nodes order: design, transitions,
//! obligations, synthesis; id-sorted within each partition):
//!   design = 7 singletons (actor/user application/todo
//!   component/todo_app flow/authenticated_user_to_service
//!   import/oddities/profiles/todo_standard interface/todo_service
//!   state/todo_state) + 14 types = 21; transitions = 2 behaviors;
//!   obligations = 4 (acceptance/production
//!   constraint/collaborative_capacity invariant/owner_isolation
//!   invariant/sharing_limit); synthesis = 1 (synthesis/prototype).
//!   21 + 2 + 4 + 1 = 28 entries. The plan fixture's coverage table has
//!   one entry per IR node, every one with a NON-EMPTY plan-node list ->
//!   has-implementation t for all 28; the set with has-implementation
//!   nil is EMPTY, so unimplemented-semantic-node warnings = 0. Every
//!   one of the 8 plan-node ids appears in the results (has_evidence is
//!   status-blind, resolved ambiguity 6) -> has-evidence t for all 28 ->
//!   traceability-complete evaluates to t.
//!
//! Verification (todo-verify.sexpr summary): (total 9) (passed 1)
//!   (failed 2) (skipped 4) (indeterminate 2); 1+2+4+2 = 9. failed 2 > 0
//!   -> verification-passed nil; indeterminate 2 > 0 ->
//!   no-indeterminate-verification nil.
//!
//! Promotion over the todo bundle, all five checks derived:
//!   no-error-diagnostics          t   (diagnostics = 5 warnings + 0 + 0)
//!   all-nodes-succeeded           t   (failed-nodes 0; deferred does
//!                                      not block, Lamedh parity)
//!   verification-passed           nil (failed 2)
//!   no-indeterminate-verification nil (indeterminate 2)
//!   traceability-complete         t   (28/28 impl AND evidence)
//!   -> decision hold.
//!
//! Fingerprints pinned from the fixtures:
//!   ir-fingerprint   "fnv1a64:2580289592425819482"  (todo-ir.sexpr)
//!   plan-fingerprint "fnv1a64:2556822733247637826"  (todo-plan.sexpr)
//!   per-node contract fingerprints as listed in oracle_05.

use gymnast_rs::assembly::{
    assemble_bundle, build_traceability_map, collect_artifacts, default_promotion_policy,
    dependency_lock, evaluate_promotion, traceability_diagnostics, traceability_entry,
    validate_artifacts, validate_capability_edges, Artifact, TraceabilityEntry, BUNDLE_SCHEMA,
};
use gymnast_rs::elaborate;
use gymnast_rs::fingerprint;
use gymnast_rs::ir::{Ir, IrNode};
use gymnast_rs::parser;
use gymnast_rs::plan::{plan, Plan, PlanNode};
use gymnast_rs::recipe::{execute_deterministic, ExecutionResult, ExecutionStatus};
use gymnast_rs::sexpr::{parse, Sexpr};
use gymnast_rs::verify::{bundle_summary, compile_verification};
use std::fs;
use std::process::Command;

// ---------------------------------------------------------------------
// Shared fixtures / helpers (not tests themselves).
// ---------------------------------------------------------------------

const TODO_IR_FINGERPRINT: &str = "fnv1a64:2580289592425819482";
const TODO_PLAN_FINGERPRINT: &str = "fnv1a64:2556822733247637826";

fn todo_ir() -> Ir {
    let src = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/todo.gym"))
        .expect("read ../examples/todo.gym");
    let (ast, parse_diags) = parser::parse(&src);
    let file = ast.expect("parse todo.gym");
    let (ir, _all_diags) = elaborate::elaborate_with_parse_diags(&file, &parse_diags);
    ir
}

/// (ir, plan, deterministic execution results) for todo.gym -- the same
/// pipeline `compile` runs before assembly.
fn todo_pipeline() -> (Ir, Plan, Vec<ExecutionResult>) {
    let ir = todo_ir();
    let p = plan(&ir);
    let results = execute_deterministic(&ir, &p);
    (ir, p, results)
}

/// Runs the real `compile` binary into a per-test temp dir (the pattern
/// golden_results_test.rs already uses) so artifact digests/sizes can be
/// cross-checked against the bytes actually written to disk.
fn compile_todo(dir_tag: &str) -> std::path::PathBuf {
    let out = std::env::temp_dir().join(format!(
        "gymnast-assembly-oracle-{}-{}",
        std::process::id(),
        dir_tag
    ));
    let _ = fs::remove_dir_all(&out);
    let status = Command::new(env!("CARGO_BIN_EXE_gymnast-rs"))
        .args([
            "compile",
            concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/todo.gym"),
            out.to_str().unwrap(),
        ])
        .status()
        .expect("run compile");
    assert!(status.success(), "compile of todo.gym must exit 0");
    out
}

/// The five artifacts todo.gym's deterministic recipes produce, in
/// result order (see the header derivation): (path, producing node id).
const TODO_ARTIFACTS: [(&str, &str); 5] = [
    (
        "generated/design/contracts.rb",
        "todo/plan/design-contracts",
    ),
    (
        "generated/interfaces/contracts.rb",
        "todo/plan/interface-contracts",
    ),
    (
        "generated/verification/acceptance.rb",
        "todo/plan/acceptance-harness",
    ),
    ("generated/application.rb", "todo/plan/application-assembly"),
    ("generated/manifest.sexpr", "todo/plan/application-assembly"),
];

/// The five declared-but-not-produced paths, in declared (plan-node
/// concatenation) order (rows 2,3,4,5,7 of the header's declared table).
const TODO_MISSING: [&str; 5] = [
    "generated/domain/transitions.rb",
    "generated/domain/authorization.rb",
    "generated/adapters/persistence.rb",
    "generated/adapters/schema.sexpr",
    "generated/service/handlers.rb",
];

/// Looks up the content of `path` inside `results`' candidate files
/// (the same data `collect_artifacts` consumes).
fn candidate_file_content(results: &[ExecutionResult], node_id: &str, path: &str) -> String {
    let result = results
        .iter()
        .find(|r| r.node_id == node_id)
        .unwrap_or_else(|| panic!("no result for {}", node_id));
    let candidate = result
        .candidate
        .as_ref()
        .unwrap_or_else(|| panic!("no candidate on {}", node_id));
    let body = &candidate.as_list().expect("candidate is a list")[1];
    let files = body.assoc("files").expect("candidate has files");
    for entry in files.as_list().expect("files is a list") {
        let pair = entry.as_list().expect("files entry is a pair");
        if pair[0].as_str() == Some(path) {
            return pair[1].as_str().expect("content is a string").to_string();
        }
    }
    panic!("no files entry for {} in {}", path, node_id);
}

/// Asserts the pinned diagnostic contract (resolved ambiguity 1): a
/// `(diagnostic (<body>))` two-element form, body's first two pairs are
/// `(severity <sym>)` and `(code <sym>)` in that order, and the body
/// carries `(subject "<string>")` and `(message "<string>")`.
fn assert_diag(d: &Sexpr, severity: &str, code: &str, subject: &str, message: &str) {
    let items = d
        .as_list()
        .unwrap_or_else(|| panic!("diagnostic must be a list: {}", d.print()));
    assert_eq!(
        items.len(),
        2,
        "diagnostic must be (diagnostic (<body>)): {}",
        d.print()
    );
    assert_eq!(items[0].as_sym(), Some("diagnostic"), "{}", d.print());
    let body = items[1]
        .as_list()
        .unwrap_or_else(|| panic!("diagnostic body must be a list: {}", d.print()));
    assert!(body.len() >= 2, "{}", d.print());
    assert_eq!(
        body[0].print(),
        format!("(severity {})", severity),
        "first body pair must be severity: {}",
        d.print()
    );
    assert_eq!(
        body[1].print(),
        format!("(code {})", code),
        "second body pair must be code: {}",
        d.print()
    );
    assert_eq!(
        items[1].assoc("subject").and_then(|s| s.as_str()),
        Some(subject),
        "subject: {}",
        d.print()
    );
    assert_eq!(
        items[1].assoc("message").and_then(|s| s.as_str()),
        Some(message),
        "message: {}",
        d.print()
    );
}

/// A hand-built plan node for the synthetic cases. NOTE:
/// `PlanNode::new` sorts every list field, so callers pass each node's
/// lists pre-sorted to keep the intended order readable at the call
/// site.
fn hand_node(
    id: &str,
    inputs: Vec<String>,
    may_write: Vec<String>,
    capabilities: Vec<String>,
    prohibitions: Vec<String>,
) -> PlanNode {
    PlanNode::new(
        id.to_string(),
        "structural",
        "design-contracts-v1",
        inputs,
        vec![],
        Sexpr::list(vec![Sexpr::sym("ruby"), Sexpr::sym("rails")]),
        Sexpr::sym("none"),
        may_write,
        capabilities,
        vec![],
        prohibitions,
    )
}

/// A hand-built plan around the given nodes (all `Plan` fields are pub;
/// the fingerprints are inert placeholders -- nothing in sections A-B
/// recomputes them).
fn hand_plan(nodes: Vec<PlanNode>) -> Plan {
    Plan {
        schema: "gymnast.plan/0.1".to_string(),
        ir_fingerprint: "fnv1a64:0".to_string(),
        target: Sexpr::list(vec![Sexpr::sym("ruby"), Sexpr::sym("rails")]),
        nodes,
        coverage: vec![],
        diagnostics: vec![],
        fingerprint: "fnv1a64:0".to_string(),
    }
}

fn hand_result(
    node_id: &str,
    status: ExecutionStatus,
    candidate: Option<Sexpr>,
) -> ExecutionResult {
    ExecutionResult {
        node_id: node_id.to_string(),
        status,
        candidate,
        recipe_identity: None,
        diagnostics: vec![],
    }
}

fn candidate_with_files(node_id: &str, files: &[(&str, &str)]) -> Sexpr {
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
                            Sexpr::list(vec![
                                Sexpr::Str((*p).to_string()),
                                Sexpr::Str((*c).to_string()),
                            ])
                        })
                        .collect(),
                ),
            ),
        ]),
    ])
}

/// The bundle's body alist (panics with the print on shape mismatch).
fn bundle_body(bundle: &Sexpr) -> &Sexpr {
    let items = bundle.as_list().expect("bundle is a list");
    assert_eq!(items.len(), 2, "bundle: {}", bundle.print());
    assert_eq!(items[0].as_sym(), Some("evidence-bundle"));
    &items[1]
}

/// The body's field keys, in order.
fn field_keys(body: &Sexpr) -> Vec<String> {
    body.as_list()
        .expect("body is a list")
        .iter()
        .map(|pair| {
            pair.as_list().expect("field is a pair")[0]
                .as_sym()
                .expect("field key is a symbol")
                .to_string()
        })
        .collect()
}

/// A synthetic all-green evidence bundle: no error diagnostics, zero
/// failed nodes, a verification section with failed 0 / indeterminate 0,
/// and complete traceability -- every promotion check must come out t.
fn green_bundle() -> Sexpr {
    parse(
        r#"(evidence-bundle ((schema "gymnast.bundle/0.1")
             (ir-fingerprint "fnv1a64:1") (plan-fingerprint "fnv1a64:2")
             (artifacts ((artifact ((path "out/a.rb") (node-id "m/plan/a")
                                    (digest "fnv1a64:3") (size 4)))))
             (traceability ((traceability-entry ((semantic-id "m/type/A")
                              (kind "type") (plan-nodes ("m/plan/a"))
                              (has-implementation t) (has-evidence t)))))
             (dependency-lock (dependency-lock ((plan-fingerprint "fnv1a64:2")
                                                (node-locks nil))))
             (verification (verification-bundle ((schema "gymnast.verify/0.1")
                              (summary ((total 1) (passed 1) (failed 0)
                                        (skipped 0) (indeterminate 0))))))
             (summary ((total-nodes 1) (artifacts-produced 1)
                       (succeeded-nodes 1) (failed-nodes 0)
                       (has-verification t)))
             (diagnostics nil)
             (fingerprint "fnv1a64:5")))"#,
    )
    .expect("green bundle parses")
}

// ---------------------------------------------------------------------
// 01: collect_artifacts over the todo compile results.
// ---------------------------------------------------------------------

#[test]
fn oracle_01_collect_artifacts_todo() {
    let (_ir, _plan, results) = todo_pipeline();

    // Sanity precondition documenting the derivation: statuses in plan
    // order are S D D D S D S S (4 succeeded, 4 deferred, 0 failed).
    let statuses: Vec<&ExecutionStatus> = results.iter().map(|r| &r.status).collect();
    assert_eq!(results.len(), 8);
    for (i, expect_succeeded) in [true, false, false, false, true, false, true, true]
        .iter()
        .enumerate()
    {
        if *expect_succeeded {
            assert_eq!(*statuses[i], ExecutionStatus::Succeeded, "result {}", i);
        } else {
            assert_eq!(*statuses[i], ExecutionStatus::Deferred, "result {}", i);
        }
    }

    let artifacts: Vec<Artifact> = collect_artifacts(&results);

    // 1+0+0+0+1+0+1+2 = 5 artifacts, in result order (header derivation).
    assert_eq!(artifacts.len(), 5);
    for (i, (path, node_id)) in TODO_ARTIFACTS.iter().enumerate() {
        assert_eq!(artifacts[i].path, *path, "artifact {} path", i);
        assert_eq!(artifacts[i].node_id, *node_id, "artifact {} node", i);
        assert!(
            artifacts[i].digest.starts_with("fnv1a64:"),
            "artifact {} digest shape: {}",
            i,
            artifacts[i].digest
        );
        assert!(artifacts[i].size > 0, "artifact {} size", i);

        // Digest is over the file CONTENT string (not the (path content)
        // pair); size is the content length in BYTES.
        let content = candidate_file_content(&results, node_id, path);
        assert_eq!(
            artifacts[i].digest,
            fingerprint::fingerprint_string(&content),
            "artifact {} digest must equal fingerprint_string(content)",
            i
        );
        assert_eq!(
            artifacts[i].size,
            content.len() as i64,
            "artifact {} size must be content bytes",
            i
        );
    }

    // Cross-check against the bytes the real compile wrote to disk.
    let out = compile_todo("01-disk");
    for (i, (path, _)) in TODO_ARTIFACTS.iter().enumerate() {
        let on_disk = fs::read_to_string(out.join(path))
            .unwrap_or_else(|e| panic!("read {} from compile output: {}", path, e));
        assert_eq!(
            artifacts[i].digest,
            fingerprint::fingerprint_string(&on_disk),
            "artifact {} digest must match the file on disk",
            i
        );
        assert_eq!(artifacts[i].size, on_disk.len() as i64);
    }
}

#[test]
fn oracle_01b_candidateless_and_malformed_contribute_nothing() {
    // Never an error here -- the firewall already ruled on candidates;
    // assembly only collects.
    let results = vec![
        // candidate: None (a deferred generative node's shape).
        hand_result("m/plan/none", ExecutionStatus::Deferred, None),
        // Malformed: not a list at all.
        hand_result(
            "m/plan/garbage",
            ExecutionStatus::Succeeded,
            Some(Sexpr::sym("garbage")),
        ),
        // Malformed: wrong tag.
        hand_result(
            "m/plan/wrong-tag",
            ExecutionStatus::Succeeded,
            Some(
                parse(r#"(not-a-candidate ((files (("p.rb" "content")))))"#)
                    .expect("wrong-tag parses"),
            ),
        ),
        // Well-formed tag but no files key.
        hand_result(
            "m/plan/no-files",
            ExecutionStatus::Succeeded,
            Some(parse(r#"(candidate ((node-id "m/plan/no-files")))"#).expect("no-files parses")),
        ),
        // Well-formed tag, empty files list.
        hand_result(
            "m/plan/empty-files",
            ExecutionStatus::Succeeded,
            Some(parse(r#"(candidate ((files nil)))"#).expect("empty-files parses")),
        ),
        // One good candidate proving the function is not just returning
        // nothing. Its candidate's own node-id names a DIFFERENT node:
        // the artifact must carry the RESULT's node-id (the Lamedh reads
        // the result's node-id, never the candidate's claim).
        hand_result(
            "m/plan/good",
            ExecutionStatus::Succeeded,
            Some(candidate_with_files(
                "m/plan/claimed-other",
                &[("out/only.rb", "content-x")],
            )),
        ),
    ];

    let artifacts = collect_artifacts(&results);
    assert_eq!(artifacts.len(), 1, "only the good candidate contributes");
    assert_eq!(artifacts[0].path, "out/only.rb");
    assert_eq!(artifacts[0].node_id, "m/plan/good");
    assert_eq!(
        artifacts[0].digest,
        fingerprint::fingerprint_string("content-x")
    );
    assert_eq!(artifacts[0].size, "content-x".len() as i64);
}

// ---------------------------------------------------------------------
// 02: validate_artifacts.
// ---------------------------------------------------------------------

#[test]
fn oracle_02_validate_artifacts_todo() {
    let (_ir, p, results) = todo_pipeline();
    let artifacts = collect_artifacts(&results);
    let diags = validate_artifacts(&p, &artifacts);

    // 10 declared - 5 produced (all produced are declared, so untracked
    // = 0) = 5 missing-artifact warnings, in declared order (header
    // derivation rows 2,3,4,5,7).
    assert_eq!(diags.len(), 5, "exactly the 5 missing paths");
    for (i, path) in TODO_MISSING.iter().enumerate() {
        assert_diag(
            &diags[i],
            "warning",
            "missing-artifact",
            path,
            "declared artifact not produced",
        );
    }
}

#[test]
fn oracle_02b_untracked_first_ordering() {
    // Declared = [out/a.rb, out/b.rb]; actual = [out/rogue.rb, out/a.rb]
    // -> untracked [out/rogue.rb] (error) then missing [out/b.rb]
    // (warning): ALL untracked first (artifact order), then all missing
    // (declared order).
    let p = hand_plan(vec![hand_node(
        "m/plan/a",
        vec![],
        vec!["out/a.rb".to_string(), "out/b.rb".to_string()],
        vec![],
        vec![],
    )]);
    let artifacts = vec![
        Artifact {
            path: "out/rogue.rb".to_string(),
            node_id: "m/plan/a".to_string(),
            digest: fingerprint::fingerprint_string("r"),
            size: 1,
        },
        Artifact {
            path: "out/a.rb".to_string(),
            node_id: "m/plan/a".to_string(),
            digest: fingerprint::fingerprint_string("a"),
            size: 1,
        },
    ];
    let diags = validate_artifacts(&p, &artifacts);
    assert_eq!(diags.len(), 2);
    assert_diag(
        &diags[0],
        "error",
        "untracked-artifact",
        "out/rogue.rb",
        "artifact not declared in any plan node",
    );
    assert_diag(
        &diags[1],
        "warning",
        "missing-artifact",
        "out/b.rb",
        "declared artifact not produced",
    );
}

#[test]
fn oracle_02c_no_dedup() {
    // Duplicates are NOT deduplicated (Lamedh parity: `filter` over the
    // raw lists).
    // (a) Two artifacts with the same undeclared path -> 2 untracked
    //     errors.
    let p = hand_plan(vec![hand_node("m/plan/a", vec![], vec![], vec![], vec![])]);
    let dup = |node: &str| Artifact {
        path: "out/dup.rb".to_string(),
        node_id: node.to_string(),
        digest: fingerprint::fingerprint_string("d"),
        size: 1,
    };
    let diags = validate_artifacts(&p, &[dup("m/plan/a"), dup("m/plan/b")]);
    assert_eq!(diags.len(), 2, "duplicate untracked path is not deduped");
    for d in &diags {
        assert_diag(
            d,
            "error",
            "untracked-artifact",
            "out/dup.rb",
            "artifact not declared in any plan node",
        );
    }

    // (b) Two nodes both declaring the same never-produced path -> 2
    //     missing warnings.
    let p2 = hand_plan(vec![
        hand_node(
            "m/plan/a",
            vec![],
            vec!["out/x.rb".to_string()],
            vec![],
            vec![],
        ),
        hand_node(
            "m/plan/b",
            vec![],
            vec!["out/x.rb".to_string()],
            vec![],
            vec![],
        ),
    ]);
    let diags2 = validate_artifacts(&p2, &[]);
    assert_eq!(diags2.len(), 2, "duplicate declared path is not deduped");
    for d in &diags2 {
        assert_diag(
            d,
            "warning",
            "missing-artifact",
            "out/x.rb",
            "declared artifact not produced",
        );
    }

    // (c) Edge-table row "duplicate produced path across two nodes": the
    //     path IS declared -> two artifacts, both validated, no
    //     diagnostics at all.
    let p3 = hand_plan(vec![hand_node(
        "m/plan/a",
        vec![],
        vec!["out/x.rb".to_string()],
        vec![],
        vec![],
    )]);
    let ok = |node: &str| Artifact {
        path: "out/x.rb".to_string(),
        node_id: node.to_string(),
        digest: fingerprint::fingerprint_string("x"),
        size: 1,
    };
    let diags3 = validate_artifacts(&p3, &[ok("m/plan/a"), ok("m/plan/b")]);
    assert_eq!(diags3.len(), 0, "declared duplicates are clean");
}

// ---------------------------------------------------------------------
// 03: validate_capability_edges.
// ---------------------------------------------------------------------

#[test]
fn oracle_03_capability_edges_todo() {
    // Header derivation: 8 concatenated capabilities, 18 concatenated
    // prohibitions, intersection empty -> 0 diagnostics.
    let ir = todo_ir();
    let p = plan(&ir);
    let all_capabilities: Vec<&String> = p.nodes.iter().flat_map(|n| &n.capabilities).collect();
    let all_prohibitions: Vec<&String> = p.nodes.iter().flat_map(|n| &n.prohibitions).collect();
    assert_eq!(all_capabilities.len(), 8, "2+2+4 = 8 capability uses");
    assert_eq!(
        all_prohibitions.len(),
        18,
        "2+3+2+2+1+3+3+2 = 18 prohibitions"
    );
    assert!(
        all_capabilities
            .iter()
            .all(|c| !all_prohibitions.contains(c)),
        "the two sets are disjoint in todo.gym"
    );

    assert_eq!(validate_capability_edges(&p).len(), 0);
}

#[test]
fn oracle_03b_capability_overlap() {
    // Node a USES clock; node b PROHIBITS clock -> exactly one error.
    let p = hand_plan(vec![
        hand_node(
            "m/plan/a",
            vec![],
            vec![],
            vec!["clock".to_string()],
            vec![],
        ),
        hand_node(
            "m/plan/b",
            vec![],
            vec![],
            vec![],
            vec!["clock".to_string()],
        ),
    ]);
    let diags = validate_capability_edges(&p);
    assert_eq!(diags.len(), 1);
    assert_diag(
        &diags[0],
        "error",
        "prohibited-capability",
        "clock",
        "capability is both used and prohibited",
    );
}

#[test]
fn oracle_03c_capability_overlap_no_dedup() {
    // clock is USED twice (nodes a and b) and prohibited once (node c):
    // one diagnostic per capability OCCURRENCE, capability order, no
    // dedup -> 2 diagnostics.
    let p = hand_plan(vec![
        hand_node(
            "m/plan/a",
            vec![],
            vec![],
            vec!["clock".to_string()],
            vec![],
        ),
        hand_node(
            "m/plan/b",
            vec![],
            vec![],
            vec!["clock".to_string()],
            vec![],
        ),
        hand_node(
            "m/plan/c",
            vec![],
            vec![],
            vec![],
            vec!["clock".to_string()],
        ),
    ]);
    let diags = validate_capability_edges(&p);
    assert_eq!(diags.len(), 2, "one per use occurrence, not deduped");
    for d in &diags {
        assert_diag(
            d,
            "error",
            "prohibited-capability",
            "clock",
            "capability is both used and prohibited",
        );
    }
}

// ---------------------------------------------------------------------
// 04: traceability.
// ---------------------------------------------------------------------

#[test]
fn oracle_04_traceability_todo() {
    let (ir, p, results) = todo_pipeline();
    let map: Vec<TraceabilityEntry> = build_traceability_map(&ir, &p, &results);

    // 21 design + 2 transitions + 4 obligations + 1 synthesis = 28
    // entries, in IR all-nodes order.
    assert_eq!(map.len(), 28);
    assert_eq!(map[0].semantic_id, "todo/actor/user");
    assert_eq!(map[0].kind, "actor");
    assert_eq!(map[27].semantic_id, "todo/synthesis/prototype");

    // The has-implementation-nil set is EMPTY (every coverage row in the
    // plan fixture is non-empty), and every plan node has a result, so
    // has-evidence holds everywhere too.
    for entry in &map {
        assert!(entry.has_implementation, "{}", entry.semantic_id);
        assert!(entry.has_evidence, "{}", entry.semantic_id);
    }

    // Two named entries, plan-nodes pinned in PLAN order (the filter
    // walks plan.nodes): todo_state is an input of nodes 2,4,6,7;
    // prototype only of node 8.
    let state = map
        .iter()
        .find(|e| e.semantic_id == "todo/state/todo_state")
        .expect("todo_state entry");
    assert_eq!(state.kind, "state");
    assert_eq!(
        state.plan_nodes,
        vec![
            "todo/plan/transition-kernel".to_string(),
            "todo/plan/persistence".to_string(),
            "todo/plan/service-handlers".to_string(),
            "todo/plan/acceptance-harness".to_string(),
        ]
    );
    let prototype = map
        .iter()
        .find(|e| e.semantic_id == "todo/synthesis/prototype")
        .expect("prototype entry");
    assert_eq!(prototype.kind, "synthesis");
    assert_eq!(
        prototype.plan_nodes,
        vec!["todo/plan/application-assembly".to_string()]
    );

    // 0 entries without implementation -> 0 warnings.
    assert_eq!(traceability_diagnostics(&map).len(), 0);
}

#[test]
fn oracle_04b_unimplemented_semantic_node_warning() {
    // An IR node no plan node takes as input: plan-nodes empty,
    // has-implementation false, has-evidence false, and exactly one
    // warning from traceability_diagnostics.
    let orphan = IrNode::new(
        "m/type/Orphan".to_string(),
        "type",
        "Orphan".to_string(),
        vec![],
        vec![],
    );
    let p = hand_plan(vec![hand_node(
        "m/plan/a",
        vec!["m/type/Used".to_string()],
        vec![],
        vec![],
        vec![],
    )]);
    let entry = traceability_entry(&orphan, &p, &[]);
    assert_eq!(entry.semantic_id, "m/type/Orphan");
    assert_eq!(entry.kind, "type");
    assert!(entry.plan_nodes.is_empty());
    assert!(!entry.has_implementation);
    assert!(!entry.has_evidence);

    let diags = traceability_diagnostics(&[entry]);
    assert_eq!(diags.len(), 1);
    assert_diag(
        &diags[0],
        "warning",
        "unimplemented-semantic-node",
        "m/type/Orphan",
        "semantic node has no implementation path",
    );
}

#[test]
fn oracle_04c_has_evidence_derivation() {
    // has_implementation comes from plan inputs alone; has_evidence
    // additionally needs SOME execution result whose node-id is in the
    // entry's plan-nodes -- status-blind (resolved ambiguity 6): a
    // deferred result is evidence-presence too.
    let node = IrNode::new(
        "m/type/A".to_string(),
        "type",
        "A".to_string(),
        vec![],
        vec![],
    );
    let p = hand_plan(vec![hand_node(
        "m/plan/a",
        vec!["m/type/A".to_string()],
        vec![],
        vec![],
        vec![],
    )]);

    // No results: implemented but no evidence.
    let entry = traceability_entry(&node, &p, &[]);
    assert_eq!(entry.plan_nodes, vec!["m/plan/a".to_string()]);
    assert!(entry.has_implementation);
    assert!(!entry.has_evidence);
    // has_implementation == !plan_nodes.is_empty() by contract.
    assert_eq!(entry.has_implementation, !entry.plan_nodes.is_empty());

    // A result for an UNRELATED node is not evidence for this entry.
    let unrelated = vec![hand_result(
        "m/plan/other",
        ExecutionStatus::Succeeded,
        None,
    )];
    assert!(!traceability_entry(&node, &p, &unrelated).has_evidence);

    // A DEFERRED result for the covering node IS evidence-presence.
    let deferred = vec![hand_result("m/plan/a", ExecutionStatus::Deferred, None)];
    assert!(traceability_entry(&node, &p, &deferred).has_evidence);
}

// ---------------------------------------------------------------------
// 05: dependency_lock.
// ---------------------------------------------------------------------

#[test]
fn oracle_05_dependency_lock_todo() {
    let ir = todo_ir();
    let p = plan(&ir);
    // The plan fixture's own fingerprint (tests/fixtures/todo-plan.sexpr,
    // final field) -- ties the in-process plan to the committed fixture.
    assert_eq!(p.fingerprint, TODO_PLAN_FINGERPRINT);

    let lock = dependency_lock(&p);

    // One node-lock per plan node, plan order; recipe prints as a bare
    // symbol and model rides along verbatim (resolved ambiguity 2). The
    // generative model sexpr and every contract fingerprint are pinned
    // from tests/fixtures/todo-plan.sexpr.
    let gen_model = "(small_code_model ((class nano) (temperature 0) (max_attempts 3)))";
    let locks: [(&str, &str, &str, &str); 8] = [
        (
            "todo/plan/design-contracts",
            "design-contracts-v1",
            "none",
            "fnv1a64:-3371151352788807458",
        ),
        (
            "todo/plan/transition-kernel",
            "transition-kernel-v1",
            gen_model,
            "fnv1a64:5415764140198673342",
        ),
        (
            "todo/plan/authorization-policy",
            "authorization-policy-v1",
            gen_model,
            "fnv1a64:7242446598303957171",
        ),
        (
            "todo/plan/persistence",
            "persistence-v1",
            gen_model,
            "fnv1a64:-5296410691327471254",
        ),
        (
            "todo/plan/interface-contracts",
            "interface-contracts-v1",
            "none",
            "fnv1a64:8511166407102391424",
        ),
        (
            "todo/plan/service-handlers",
            "service-handlers-v1",
            gen_model,
            "fnv1a64:4886959663632728905",
        ),
        (
            "todo/plan/acceptance-harness",
            "acceptance-harness-v1",
            "none",
            "fnv1a64:-8185184007917156936",
        ),
        (
            "todo/plan/application-assembly",
            "application-assembly-v1",
            "none",
            "fnv1a64:3477656653550226031",
        ),
    ];
    let expected_locks: Vec<String> = locks
        .iter()
        .map(|(id, recipe, model, fp)| {
            format!(
                "(node-lock ((node-id \"{}\") (recipe {}) (model {}) (fingerprint \"{}\")))",
                id, recipe, model, fp
            )
        })
        .collect();
    let expected = format!(
        "(dependency-lock ((plan-fingerprint \"{}\") (node-locks ({}))))",
        TODO_PLAN_FINGERPRINT,
        expected_locks.join(" ")
    );
    assert_eq!(lock.print(), expected);
}

// ---------------------------------------------------------------------
// 06: promotion policy and evaluation.
// ---------------------------------------------------------------------

#[test]
fn oracle_06_default_policy_shape() {
    // The requires list is descriptive policy metadata (Lamedh parity);
    // its exact printed form is the contract.
    assert_eq!(
        default_promotion_policy().print(),
        "(promotion-policy ((name default) (requires ((all-artifacts-present t) \
         (no-untracked-artifacts t) (no-capability-violations t) (all-nodes-succeeded t) \
         (verification-passed t) (traceability-complete t)))))"
    );
}

#[test]
fn oracle_06b_promotion_todo_hold() {
    let (ir, p, results) = todo_pipeline();
    let verification = compile_verification(&ir);

    // Sanity precondition documenting the derivation: the verification
    // summary is the fixture's (total 9) (passed 1) (failed 2)
    // (skipped 4) (indeterminate 2); 1+2+4+2 = 9.
    let vs = bundle_summary(&verification).expect("verification summary");
    assert_eq!(
        (vs.total, vs.passed, vs.failed, vs.skipped, vs.indeterminate),
        (9, 1, 2, 4, 2)
    );

    let bundle = assemble_bundle(&ir, &p, &results, Some(&verification));
    let result = evaluate_promotion(&default_promotion_policy(), &bundle);

    // Header derivation: hold. verification-passed nil alone suffices,
    // but every check's value is pinned.
    // INTEGRATOR RESOLUTION (phase-8 gate, finding 4): checks list
    // gained `all-artifacts-present` (nil: todo has 5 missing-artifact
    // warnings). Decision stays hold.
    assert_eq!(
        result.print(),
        "(promotion-result ((policy default) (decision hold) (checks \
         ((no-error-diagnostics t) (all-artifacts-present nil) (all-nodes-succeeded t) \
         (verification-passed nil) \
         (no-indeterminate-verification nil) (traceability-complete t)))))"
    );
}

#[test]
fn oracle_06c_promotion_all_green_promotes() {
    // INTEGRATOR RESOLUTION (phase-8 gate, finding 4): checks list
    // gained `all-artifacts-present` (t: the green bundle's
    // diagnostics are empty). Still promote — the green bundle's
    // verification summary has total 1 > 0, so the new
    // zero-obligation guard on verification-passed does not fire.
    let result = evaluate_promotion(&default_promotion_policy(), &green_bundle());
    assert_eq!(
        result.print(),
        "(promotion-result ((policy default) (decision promote) (checks \
         ((no-error-diagnostics t) (all-artifacts-present t) (all-nodes-succeeded t) \
         (verification-passed t) \
         (no-indeterminate-verification t) (traceability-complete t)))))"
    );
}

#[test]
fn oracle_06d_fail_closed_on_missing_summary() {
    // The green bundle with its summary field REMOVED: the
    // all-nodes-succeeded check can no longer read failed-nodes and must
    // evaluate to nil (fail-closed), never panic.
    let bundle = green_bundle();
    let items = bundle.as_list().expect("bundle list").to_vec();
    let body: Vec<Sexpr> = items[1]
        .as_list()
        .expect("body list")
        .iter()
        .filter(|pair| pair.as_list().and_then(|p| p[0].as_sym()) != Some("summary"))
        .cloned()
        .collect();
    assert_eq!(body.len(), 9, "one field removed from the 10");
    let no_summary = Sexpr::list(vec![Sexpr::sym("evidence-bundle"), Sexpr::list(body)]);

    // INTEGRATOR RESOLUTION (phase-8 gate, finding 4): checks list
    // gained `all-artifacts-present` (t: empty diagnostics). Decision
    // stays hold via all-nodes-succeeded nil.
    let result = evaluate_promotion(&default_promotion_policy(), &no_summary);
    assert_eq!(
        result.print(),
        "(promotion-result ((policy default) (decision hold) (checks \
         ((no-error-diagnostics t) (all-artifacts-present t) (all-nodes-succeeded nil) \
         (verification-passed t) \
         (no-indeterminate-verification t) (traceability-complete t)))))"
    );
}

#[test]
fn oracle_06e_indeterminate_check_has_teeth() {
    // Otherwise-green bundle whose verification summary has
    // indeterminate 1 (failed still 0): verification-passed stays t;
    // ONLY no-indeterminate-verification goes nil -- the DELTA check
    // must block promotion on its own.
    // INTEGRATOR RESOLUTION (phase-8 gate re-review, residual 1): the
    // mutated summary keeps one PASSED obligation so verification-passed
    // stays t under the new executed-count rule (passed + failed > 0) —
    // preserving this test's isolation intent: ONLY the indeterminate
    // check blocks. (The original all-indeterminate summary now fails
    // verification-passed too, which is the honest reading but tests a
    // different thing.)
    let text = green_bundle().print().replace(
        "(summary ((total 1) (passed 1) (failed 0) (skipped 0) (indeterminate 0)))",
        "(summary ((total 2) (passed 1) (failed 0) (skipped 0) (indeterminate 1)))",
    );
    let bundle = parse(&text).expect("mutated green bundle parses");
    assert_ne!(bundle.print(), green_bundle().print(), "mutation applied");

    // INTEGRATOR RESOLUTION (phase-8 gate, finding 4): checks list
    // gained `all-artifacts-present` (t). Decision stays hold via
    // no-indeterminate-verification nil alone, as this test pins.
    let result = evaluate_promotion(&default_promotion_policy(), &bundle);
    assert_eq!(
        result.print(),
        "(promotion-result ((policy default) (decision hold) (checks \
         ((no-error-diagnostics t) (all-artifacts-present t) (all-nodes-succeeded t) \
         (verification-passed t) \
         (no-indeterminate-verification nil) (traceability-complete t)))))"
    );
}

#[test]
fn oracle_06f_policy_with_missing_requires() {
    // Edge table: requires is metadata; a policy without it still gets
    // the computed checks, and the result carries ITS name.
    // INTEGRATOR RESOLUTION (phase-8 gate, finding 4): six checks now.
    let policy = parse("(promotion-policy ((name custom)))").expect("custom policy parses");
    let result = evaluate_promotion(&policy, &green_bundle());
    assert_eq!(
        result.print(),
        "(promotion-result ((policy custom) (decision promote) (checks \
         ((no-error-diagnostics t) (all-artifacts-present t) (all-nodes-succeeded t) \
         (verification-passed t) \
         (no-indeterminate-verification t) (traceability-complete t)))))"
    );
}

// ---------------------------------------------------------------------
// 07: assemble_bundle.
// ---------------------------------------------------------------------

#[test]
fn oracle_07_bundle_field_order_and_schema() {
    let (ir, p, results) = todo_pipeline();
    let verification = compile_verification(&ir);
    let bundle = assemble_bundle(&ir, &p, &results, Some(&verification));
    let body = bundle_body(&bundle);

    assert_eq!(BUNDLE_SCHEMA, "gymnast.bundle/0.1");
    assert_eq!(
        field_keys(body),
        vec![
            "schema",
            "ir-fingerprint",
            "plan-fingerprint",
            "artifacts",
            "traceability",
            "dependency-lock",
            "verification",
            "summary",
            "diagnostics",
            "fingerprint",
        ]
    );
    assert_eq!(
        body.assoc("schema").and_then(|s| s.as_str()),
        Some(BUNDLE_SCHEMA)
    );
    assert_eq!(
        body.assoc("ir-fingerprint").and_then(|s| s.as_str()),
        Some(TODO_IR_FINGERPRINT),
        "the todo-ir.sexpr fixture's fingerprint"
    );
    assert_eq!(
        body.assoc("plan-fingerprint").and_then(|s| s.as_str()),
        Some(TODO_PLAN_FINGERPRINT),
        "the todo-plan.sexpr fixture's fingerprint"
    );

    // Summary pin (header derivation: 8 nodes, 5 artifacts, 4 succeeded,
    // 0 failed -- 4 deferred count toward neither -- verification
    // present).
    assert_eq!(
        body.assoc("summary").expect("summary").print(),
        "((total-nodes 8) (artifacts-produced 5) (succeeded-nodes 4) (failed-nodes 0) \
         (has-verification t))"
    );

    // Artifacts field: 5 serialized (artifact (...)) forms in artifact
    // order; the first is fully pinned (digest/size derived from the
    // emitted content per the plan's instruction, not hardcoded).
    let artifacts_field = body.assoc("artifacts").expect("artifacts");
    let artifact_forms = artifacts_field.as_list().expect("artifacts list");
    assert_eq!(artifact_forms.len(), 5);
    let first_content = candidate_file_content(
        &results,
        "todo/plan/design-contracts",
        "generated/design/contracts.rb",
    );
    assert_eq!(
        artifact_forms[0].print(),
        format!(
            "(artifact ((path \"generated/design/contracts.rb\") \
             (node-id \"todo/plan/design-contracts\") (digest \"{}\") (size {})))",
            fingerprint::fingerprint_string(&first_content),
            first_content.len()
        )
    );

    // Traceability field: 28 serialized entries; the first is fully
    // pinned (actor/user is an input of plan nodes 1 and 3, in plan
    // order).
    let trace_field = body.assoc("traceability").expect("traceability");
    let entry_forms = trace_field.as_list().expect("traceability list");
    assert_eq!(entry_forms.len(), 28);
    assert_eq!(
        entry_forms[0].print(),
        "(traceability-entry ((semantic-id \"todo/actor/user\") (kind \"actor\") \
         (plan-nodes (\"todo/plan/design-contracts\" \"todo/plan/authorization-policy\")) \
         (has-implementation t) (has-evidence t)))"
    );

    // dependency-lock rides along verbatim (resolved ambiguity 5), and
    // verification is the bundle passed in, verbatim.
    assert_eq!(
        body.assoc("dependency-lock").expect("lock").print(),
        dependency_lock(&p).print()
    );
    assert_eq!(
        body.assoc("verification").expect("verification").print(),
        verification.print()
    );

    // Diagnostics: artifact diags (5 missing warnings) ++ capability
    // diags (0) ++ traceability diags (0), in that order.
    let diags = body
        .assoc("diagnostics")
        .expect("diagnostics")
        .as_list()
        .expect("diagnostics list")
        .to_vec();
    assert_eq!(diags.len(), 5);
    for (i, path) in TODO_MISSING.iter().enumerate() {
        assert_diag(
            &diags[i],
            "warning",
            "missing-artifact",
            path,
            "declared artifact not produced",
        );
    }
}

#[test]
fn oracle_07b_bundle_fingerprint_self_consistency() {
    let (ir, p, results) = todo_pipeline();
    let verification = compile_verification(&ir);
    let bundle = assemble_bundle(&ir, &p, &results, Some(&verification));

    // The fingerprint is the LAST field, computed over the
    // fingerprint-free form (phase-7 pattern verbatim).
    let items = bundle.as_list().expect("bundle list");
    let body = items[1].as_list().expect("body list");
    let (last, rest) = body.split_last().expect("non-empty body");
    let last_pair = last.as_list().expect("last field is a pair");
    assert_eq!(last_pair[0].as_sym(), Some("fingerprint"));
    let recorded = last_pair[1].as_str().expect("fingerprint is a string");
    assert!(recorded.starts_with("fnv1a64:"));

    let stripped = Sexpr::list(vec![
        Sexpr::sym("evidence-bundle"),
        Sexpr::list(rest.to_vec()),
    ]);
    assert_eq!(
        fingerprint::fingerprint(&stripped),
        recorded,
        "recomputing over the fingerprint-free form must reproduce it"
    );

    // Mutating one artifact digest inside the fingerprint-free form must
    // change the fingerprint: the fingerprint covers the artifacts.
    let artifacts = collect_artifacts(&results);
    let text = stripped.print();
    assert!(text.contains(&artifacts[0].digest));
    let mutated = text.replacen(&artifacts[0].digest, "fnv1a64:0", 1);
    assert_ne!(mutated, text);
    assert_ne!(fingerprint::fingerprint_string(&mutated), recorded);
}

#[test]
fn oracle_07c_bundle_byte_stability() {
    // Two independent assemblies from scratch print byte-identically.
    let (ir1, p1, results1) = todo_pipeline();
    let v1 = compile_verification(&ir1);
    let b1 = assemble_bundle(&ir1, &p1, &results1, Some(&v1));

    let (ir2, p2, results2) = todo_pipeline();
    let v2 = compile_verification(&ir2);
    let b2 = assemble_bundle(&ir2, &p2, &results2, Some(&v2));

    assert_eq!(b1.print(), b2.print());
}

// ---------------------------------------------------------------------
// 08: the golden. RED until Stage 3 generates and commits
// tests/fixtures/todo-bundle.sexpr (then frozen).
// ---------------------------------------------------------------------

#[test]
fn oracle_08_evidence_bundle_matches_golden() {
    let out = compile_todo("08-golden");
    let produced = fs::read_to_string(out.join("evidence-bundle.sexpr"))
        .expect("compile must write evidence-bundle.sexpr (Stage 3)");

    // One canonical printed form: the two-element assembly file with a
    // trailing newline.
    assert!(
        produced.starts_with("(assembly ((bundle (evidence-bundle "),
        "file head: {}",
        &produced[..produced.len().min(60)]
    );
    assert!(produced.contains("(promotion (promotion-result "));
    assert!(produced.ends_with('\n'));

    // Byte-for-byte against the committed fixture (generated ONCE by
    // Stage 3, frozen afterward; the fs::read keeps this file compiling
    // while the fixture does not exist yet).
    let golden = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/todo-bundle.sexpr"
    ))
    .expect("tests/fixtures/todo-bundle.sexpr committed by Stage 3");
    assert_eq!(
        produced, golden,
        "evidence-bundle.sexpr diverged from the golden; regenerate only with a stated reason"
    );
}

// ---------------------------------------------------------------------
// Edge semantics (plan's edge table).
// ---------------------------------------------------------------------

#[test]
fn edge_01_empty_results() {
    // Empty results: artifacts [], summary zeros, missing = every
    // declared path (all 10), decision from the checks as computed --
    // never a panic.
    assert_eq!(collect_artifacts(&[]).len(), 0);

    let ir = todo_ir();
    let p = plan(&ir);
    let bundle = assemble_bundle(&ir, &p, &[], None);
    let body = bundle_body(&bundle);

    assert_eq!(
        body.assoc("summary").expect("summary").print(),
        "((total-nodes 8) (artifacts-produced 0) (succeeded-nodes 0) (failed-nodes 0) \
         (has-verification nil))"
    );
    assert_eq!(body.assoc("artifacts").expect("artifacts").print(), "nil");

    // All 10 declared paths missing, declared order (header table rows
    // 1-10); still warnings, so no-error-diagnostics stays t.
    let declared = [
        "generated/design/contracts.rb",
        "generated/domain/transitions.rb",
        "generated/domain/authorization.rb",
        "generated/adapters/persistence.rb",
        "generated/adapters/schema.sexpr",
        "generated/interfaces/contracts.rb",
        "generated/service/handlers.rb",
        "generated/verification/acceptance.rb",
        "generated/application.rb",
        "generated/manifest.sexpr",
    ];
    let diags = body
        .assoc("diagnostics")
        .expect("diagnostics")
        .as_list()
        .expect("diagnostics list")
        .to_vec();
    assert_eq!(diags.len(), 10);
    for (i, path) in declared.iter().enumerate() {
        assert_diag(
            &diags[i],
            "warning",
            "missing-artifact",
            path,
            "declared artifact not produced",
        );
    }

    // Promotion: no results -> no evidence anywhere ->
    // traceability-complete nil; nil verification section keeps both
    // verification checks vacuously t; failed-nodes is 0.
    // INTEGRATOR RESOLUTION (phase-8 gate, findings 2+4): the checks
    // list gained `all-artifacts-present` (nil here -- all 10 declared
    // paths missing). Decision stays hold.
    let result = evaluate_promotion(&default_promotion_policy(), &bundle);
    assert_eq!(
        result.print(),
        "(promotion-result ((policy default) (decision hold) (checks \
         ((no-error-diagnostics t) (all-artifacts-present nil) (all-nodes-succeeded t) \
         (verification-passed t) \
         (no-indeterminate-verification t) (traceability-complete nil)))))"
    );
}

#[test]
fn edge_02_verification_none() {
    // verification: None -> bundle (verification nil), has-verification
    // nil, and the two verification checks vacuously t. With todo's
    // real results everything else is green too -> promote.
    let (ir, p, results) = todo_pipeline();
    let bundle = assemble_bundle(&ir, &p, &results, None);
    let body = bundle_body(&bundle);

    assert_eq!(
        body.assoc("verification").expect("verification").print(),
        "nil"
    );
    assert_eq!(
        body.assoc("summary").expect("summary").print(),
        "((total-nodes 8) (artifacts-produced 5) (succeeded-nodes 4) (failed-nodes 0) \
         (has-verification nil))"
    );

    // INTEGRATOR RESOLUTION (phase-8 gate, finding 4): this test
    // originally pinned `promote` here — a bundle with 4 never-executed
    // nodes and 5 declared-but-unproduced artifacts. That composition
    // was the gate's vacuous-promote MAJOR: each ingredient was
    // individually sanctioned, but the composed consequence (promote
    // over evidence that mostly does not exist) is exactly what the
    // bundle exists to prevent. With the new computed
    // `all-artifacts-present` check (nil: 5 missing-artifact warnings),
    // the decision is now honestly `hold`.
    let result = evaluate_promotion(&default_promotion_policy(), &bundle);
    assert_eq!(
        result.print(),
        "(promotion-result ((policy default) (decision hold) (checks \
         ((no-error-diagnostics t) (all-artifacts-present nil) (all-nodes-succeeded t) \
         (verification-passed t) \
         (no-indeterminate-verification t) (traceability-complete t)))))"
    );
}

#[test]
fn edge_03_status_tallies_with_candidateless_results() {
    // A result with candidate: None contributes no artifacts but still
    // counts in the succeeded/failed tallies by status; deferred counts
    // toward neither.
    let p = hand_plan(vec![hand_node("m/plan/a", vec![], vec![], vec![], vec![])]);
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
    let results = vec![
        hand_result("m/plan/a", ExecutionStatus::Succeeded, None),
        hand_result("m/plan/a", ExecutionStatus::Failed, None),
        hand_result("m/plan/a", ExecutionStatus::Deferred, None),
    ];
    assert_eq!(collect_artifacts(&results).len(), 0);

    let bundle = assemble_bundle(&ir, &p, &results, None);
    let body = bundle_body(&bundle);
    assert_eq!(
        body.assoc("summary").expect("summary").print(),
        "((total-nodes 1) (artifacts-produced 0) (succeeded-nodes 1) (failed-nodes 1) \
         (has-verification nil))"
    );
}
