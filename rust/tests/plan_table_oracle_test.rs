//! Tests-of-record for phase-4 scope item 1 (`docs/rust-port-plan-phase4.md`,
//! "Required work from the phase-3 gate"), authored from that document and
//! from `docs/rust-port-plan-phase3.md`'s section B table ALONE, before any
//! implementation of the new `PlanNode` methods or the W404 diagnostic
//! exists (the phase-4 process upgrade: Stage 1 commits these oracle files
//! to git before Stage 2 touches `plan.rs`).
//!
//! This file implements every part of scope item 1 that phase 4's own
//! "Oracle tests" section assigns to `plan_table_oracle_test.rs`:
//!   1a. `PlanNode::contract_sexpr` (public) and `PlanNode::verify_fingerprint`.
//!   1b. E403's corrected message text, and the new W404 diagnostic.
//!   1c. The full 8-node table, transcribed field-by-field from
//!       `docs/rust-port-plan-phase3.md`'s table (NOT from `plan.rs`).
//!
//! (Item 1d, prompt section-presence, is assigned by the phase-4 doc to
//! `recipe_oracle_test.rs` instead — see that file's oracle_09.)
//!
//! Implementation stages MUST NOT edit this file. If an assertion here
//! turns out to conflict with a considered implementation choice, the
//! implementer reports the conflict; only the integrator resolves it.
//!
//! This file will not compile until `PlanNode::contract_sexpr` and
//! `PlanNode::verify_fingerprint` exist — that is expected at this stage.
//!
//! NOTE (ambiguity, reported per Process Rule 1 / the phase-4 prompt's
//! instruction to note ambiguities): the phase-4 doc's own prose for the
//! W404 example ("a minimal .gym spec with one behavior and no acceptance
//! block yields a W404 for the behavior node") appears to conflict with
//! the FIXED 8-node table it also mandates. `acceptance-harness`'s input
//! kind set is `{behavior, invariant, constraint, acceptance, interface,
//! state}` — a strict superset of the `transitions ∪ obligations`
//! partition kinds `{behavior, invariant, constraint, acceptance}` (see
//! `rust/src/elaborate.rs`'s partition match arms). Because the fixed
//! table selects `acceptance-harness`'s inputs purely BY KIND across the
//! whole IR (`ids_for_kinds`), every node kind that can land in
//! transitions/obligations is a) always a member of acceptance-harness's
//! kind set and b) therefore always present in its `inputs`, regardless
//! of whether the spec has an `acceptance` declaration at all. Under this
//! architecture W404 can structurally never fire — a per-item "no
//! acceptance block" condition is not something the fixed, kind-based
//! table can express. I verified this by hand-elaborating a minimal
//! one-behavior/no-acceptance-block spec against the current crate's
//! `ir`/`check` commands (the sole behavior node's kind is unconditionally
//! a member of acceptance-harness's input-kind set). I write the test
//! below LITERALLY as the phase-4 doc specifies (a checkable assertion,
//! per Process Rule 3) rather than inventing a different semantics; if my
//! reading is correct this test will need the integrator's resolution
//! (either the doc's example is wrong and should be replaced with "W404
//! never fires under the current architecture", or W404's trigger
//! condition needs a per-item relational check the fixed table does not
//! currently have the machinery for). The zero-W404-on-todo.gym half of
//! the same item is NOT in question and is asserted unconditionally.

use gymnast_rs::elaborate;
use gymnast_rs::fingerprint;
use gymnast_rs::ir::{Ir, IrNode};
use gymnast_rs::parser;
use gymnast_rs::plan::plan;
use gymnast_rs::sexpr::Sexpr;
use std::fs;
use std::io::Write;

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

fn full_id(module: &str, local: &str) -> String {
    format!("{}/plan/{}", module, local)
}

/// Pulls the `code` string out of a lowered `(diagnostic (severity s)
/// (code "C") (span a b) (message "..."))` shape.
fn diag_code(d: &Sexpr) -> Option<String> {
    if let Sexpr::List(items) = d {
        for item in items {
            if let Sexpr::List(pair) = item {
                if pair.len() == 2 {
                    if let Sexpr::Sym(key) = &pair[0] {
                        if key == "code" {
                            if let Sexpr::Str(c) = &pair[1] {
                                return Some(c.clone());
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn diag_message(d: &Sexpr) -> Option<String> {
    if let Sexpr::List(items) = d {
        for item in items {
            if let Sexpr::List(pair) = item {
                if pair.len() == 2 {
                    if let Sexpr::Sym(key) = &pair[0] {
                        if key == "message" {
                            if let Sexpr::Str(m) = &pair[1] {
                                return Some(m.clone());
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// A path ending in `.lisp` rewritten to `.rb` for todo.gym's `(ruby
/// rails)` target — transcribed from `docs/rust-port-plan-phase3.md`,
/// section B's extension map and path-rewrite rule, not from `plan.rs`.
/// Paths that never carried a `.lisp` suffix (the two `.sexpr` paths in
/// the table) pass through unrewritten.
fn rewrite_for_todo(path: &str) -> String {
    match path.strip_suffix(".lisp") {
        Some(stem) => format!("{}.rb", stem),
        None => path.to_string(),
    }
}

/// Union of ids of every IR node (any partition) whose kind is in
/// `kinds`, sorted — computed generically from `ir.nodes_of_kind`, not
/// from `plan.rs`'s internal `ids_for_kinds`.
fn expected_inputs(ir: &Ir, kinds: &[&str]) -> Vec<String> {
    let mut ids: Vec<String> = kinds
        .iter()
        .flat_map(|k| ir.nodes_of_kind(k))
        .map(|n| n.id.clone())
        .collect();
    ids.sort();
    ids
}

fn sorted(mut v: Vec<&'static str>) -> Vec<String> {
    v.sort();
    v.into_iter().map(|s| s.to_string()).collect()
}

/// One row of the fixed 8-node table (`docs/rust-port-plan-phase3.md`,
/// section B), transcribed field-by-field.
struct Row {
    local: &'static str,
    class: &'static str,
    recipe: &'static str,
    input_kinds: &'static [&'static str],
    depends_on_locals: &'static [&'static str],
    may_write_pre_rewrite: &'static [&'static str],
    capabilities: &'static [&'static str],
    obligations: &'static [&'static str],
    prohibitions: &'static [&'static str],
}

const TABLE: &[Row] = &[
    Row {
        local: "design-contracts",
        class: "structural",
        recipe: "design-contracts-v1",
        input_kinds: &["actor", "type", "component", "flow"],
        depends_on_locals: &[],
        may_write_pre_rewrite: &["generated/design/contracts.lisp"],
        capabilities: &[],
        obligations: &["well-formed-types", "explicit-capability-edges"],
        prohibitions: &["invent-product-semantics", "add-dependencies"],
    },
    Row {
        local: "transition-kernel",
        class: "generative",
        recipe: "transition-kernel-v1",
        input_kinds: &["type", "state", "behavior", "invariant"],
        depends_on_locals: &["design-contracts"],
        may_write_pre_rewrite: &["generated/domain/transitions.lisp"],
        capabilities: &["clock", "id-source"],
        obligations: &[
            "implements-transition-system",
            "preserves-invariants",
            "deterministic-under-same-input",
        ],
        prohibitions: &["perform-io", "weaken-preconditions", "invent-errors"],
    },
    Row {
        local: "authorization-policy",
        class: "generative",
        recipe: "authorization-policy-v1",
        input_kinds: &["actor", "flow", "behavior", "invariant"],
        depends_on_locals: &["design-contracts", "transition-kernel"],
        may_write_pre_rewrite: &["generated/domain/authorization.lisp"],
        capabilities: &[],
        obligations: &["deny-by-default", "noninterference", "owner-isolation"],
        prohibitions: &["grant-undeclared-capabilities", "reveal-resource-existence"],
    },
    Row {
        local: "persistence",
        class: "generative",
        recipe: "persistence-v1",
        input_kinds: &["type", "state", "behavior", "constraint"],
        depends_on_locals: &["design-contracts", "transition-kernel"],
        may_write_pre_rewrite: &[
            "generated/adapters/persistence.lisp",
            "generated/adapters/schema.sexpr",
        ],
        capabilities: &["durable-store", "transactions"],
        obligations: &["durable-commit", "atomic-boundaries", "retry-safety"],
        prohibitions: &["perform-network-io", "choose-unpinned-dependencies"],
    },
    Row {
        local: "interface-contracts",
        class: "structural",
        recipe: "interface-contracts-v1",
        input_kinds: &["type", "interface"],
        depends_on_locals: &["design-contracts"],
        may_write_pre_rewrite: &["generated/interfaces/contracts.lisp"],
        capabilities: &[],
        obligations: &["complete-operation-surface", "declared-errors-only"],
        prohibitions: &["change-observable-contract"],
    },
    Row {
        local: "service-handlers",
        class: "generative",
        recipe: "service-handlers-v1",
        input_kinds: &["interface", "behavior", "state", "constraint"],
        depends_on_locals: &[
            "transition-kernel",
            "authorization-policy",
            "persistence",
            "interface-contracts",
        ],
        may_write_pre_rewrite: &["generated/service/handlers.lisp"],
        capabilities: &["repository", "identity", "clock", "id-source"],
        obligations: &[
            "contract-conformance",
            "authorization-before-observation",
            "idempotent-retries",
        ],
        prohibitions: &["access-filesystem", "access-network", "add-endpoints"],
    },
    Row {
        local: "acceptance-harness",
        class: "verification",
        recipe: "acceptance-harness-v1",
        input_kinds: &[
            "behavior",
            "invariant",
            "constraint",
            "acceptance",
            "interface",
            "state",
        ],
        depends_on_locals: &["service-handlers"],
        may_write_pre_rewrite: &["generated/verification/acceptance.lisp"],
        capabilities: &[],
        obligations: &[
            "independent-oracle",
            "trace-equivalence",
            "boundary-coverage",
            "deterministic-execution",
        ],
        prohibitions: &[
            "read-generated-rationale",
            "weaken-obligations",
            "skip-failures",
        ],
    },
    Row {
        local: "application-assembly",
        class: "assembly",
        recipe: "application-assembly-v1",
        input_kinds: &[
            "application",
            "import",
            "component",
            "synthesis",
            "constraint",
        ],
        depends_on_locals: &[
            "transition-kernel",
            "authorization-policy",
            "persistence",
            "interface-contracts",
            "service-handlers",
            "acceptance-harness",
        ],
        may_write_pre_rewrite: &["generated/application.lisp", "generated/manifest.sexpr"],
        capabilities: &[],
        obligations: &["all-artifacts-linked", "all-obligations-addressed"],
        prohibitions: &["untracked-artifacts", "undeclared-capabilities"],
    },
];

// ---------------------------------------------------------------------
// 1c. The full 8-node table, transcribed field-by-field from the
//     phase-3 doc, checked against todo.gym's plan.
// ---------------------------------------------------------------------

#[test]
fn oracle_1c_ids_classes_recipes_in_table_order() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    assert_eq!(p.nodes.len(), 8, "plan must contain exactly 8 nodes");
    for (i, row) in TABLE.iter().enumerate() {
        let node = &p.nodes[i];
        assert_eq!(
            node.id,
            full_id(&ir.module_name, row.local),
            "node at position {} has wrong id",
            i
        );
        assert_eq!(node.class, row.class, "{}: wrong class", node.id);
        assert_eq!(node.recipe, row.recipe, "{}: wrong recipe string", node.id);
    }
}

#[test]
fn oracle_1c_input_kind_sets_full_coverage() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    for row in TABLE {
        let node = p
            .nodes
            .iter()
            .find(|n| n.id == full_id(&ir.module_name, row.local))
            .unwrap_or_else(|| panic!("no plan node for local {}", row.local));
        let expected = expected_inputs(&ir, row.input_kinds);
        assert_eq!(
            node.inputs, expected,
            "{}: inputs must equal the union of ids for kind set {:?}",
            node.id, row.input_kinds
        );
    }
}

#[test]
fn oracle_1c_depends_on_full_table() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    for row in TABLE {
        let node = p
            .nodes
            .iter()
            .find(|n| n.id == full_id(&ir.module_name, row.local))
            .unwrap();
        let mut expected: Vec<String> = row
            .depends_on_locals
            .iter()
            .map(|l| full_id(&ir.module_name, l))
            .collect();
        expected.sort();
        assert_eq!(
            node.depends_on, expected,
            "{}: depends_on does not match the table",
            node.id
        );
    }
}

#[test]
fn oracle_1c_capabilities_obligations_prohibitions_full_table() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    for row in TABLE {
        let node = p
            .nodes
            .iter()
            .find(|n| n.id == full_id(&ir.module_name, row.local))
            .unwrap();
        assert_eq!(
            node.capabilities,
            sorted(row.capabilities.to_vec()),
            "{}: capabilities mismatch",
            node.id
        );
        assert_eq!(
            node.obligations,
            sorted(row.obligations.to_vec()),
            "{}: obligations mismatch",
            node.id
        );
        assert_eq!(
            node.prohibitions,
            sorted(row.prohibitions.to_vec()),
            "{}: prohibitions mismatch",
            node.id
        );
    }
}

#[test]
fn oracle_1c_may_write_paths_rewritten_for_todo_target() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    // todo.gym's synthesis node declares target (ruby rails); the table's
    // pre-rewrite paths are rewritten .lisp -> .rb accordingly (section B).
    for row in TABLE {
        let node = p
            .nodes
            .iter()
            .find(|n| n.id == full_id(&ir.module_name, row.local))
            .unwrap();
        let mut expected: Vec<String> = row
            .may_write_pre_rewrite
            .iter()
            .map(|p| rewrite_for_todo(p))
            .collect();
        expected.sort();
        assert_eq!(
            node.may_write, expected,
            "{}: may_write paths do not match the rewritten table paths",
            node.id
        );
    }
}

// ---------------------------------------------------------------------
// 1a. PlanNode::contract_sexpr (public) and PlanNode::verify_fingerprint.
// ---------------------------------------------------------------------

#[test]
fn oracle_1a_contract_sexpr_is_public_fingerprint_free_node_contract_form() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    for node in &p.nodes {
        let contract = node.contract_sexpr();
        let printed = contract.print();
        assert!(
            printed.starts_with("(node-contract ("),
            "{}: contract_sexpr must be headed by the bare tag `node-contract`, got {}",
            node.id,
            printed
        );
        assert!(
            !printed.contains("(fingerprint "),
            "{}: contract_sexpr must be the FINGERPRINT-FREE form (the firewall must never see \
             a self-referential fingerprint), got {}",
            node.id,
            printed
        );
        // All eleven contract fields present.
        for key in [
            "(id ",
            "(class ",
            "(recipe ",
            "(inputs ",
            "(depends-on ",
            "(target ",
            "(model ",
            "(may-write ",
            "(capabilities ",
            "(obligations ",
            "(prohibitions ",
        ] {
            assert!(
                printed.contains(key),
                "{}: contract_sexpr is missing field {} -- got {}",
                node.id,
                key,
                printed
            );
        }
        // The fingerprint stored on the node is exactly the fingerprint of
        // this contract form (the firewall must never hand-roll the head
        // rewrite; contract_sexpr is the single source of truth for what
        // gets fingerprinted).
        assert_eq!(
            node.fingerprint,
            fingerprint::fingerprint(&contract),
            "{}: node.fingerprint must equal fingerprint(contract_sexpr())",
            node.id
        );
    }
}

#[test]
fn oracle_1a_verify_fingerprint_true_for_every_todo_node() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    for node in &p.nodes {
        assert!(
            node.verify_fingerprint(),
            "{}: verify_fingerprint must be true for an untampered node",
            node.id
        );
    }
}

#[test]
fn oracle_1a_verify_fingerprint_false_after_tampering_may_write() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    let mut node = p.nodes[0].clone();
    assert!(
        node.verify_fingerprint(),
        "precondition: clone starts valid"
    );
    node.may_write.push("generated/tampered.rb".to_string());
    assert!(
        !node.verify_fingerprint(),
        "verify_fingerprint must be false after tampering a cloned node's may_write \
         (the stored fingerprint no longer matches the recomputed contract)"
    );
}

// ---------------------------------------------------------------------
// 1b. E403's corrected message ("no implementation path"); the new W404
//     missing-evidence-path warning.
// ---------------------------------------------------------------------

#[test]
fn oracle_1b_e403_message_says_no_implementation_path() {
    let gadget = IrNode::new(
        "m/gadget/G".to_string(),
        "gadget",
        "G".to_string(),
        vec![],
        vec![],
    );
    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "m".to_string(),
        vec![],
        vec![gadget],
        vec![],
        vec![],
        vec![],
        vec![],
    );
    let p = plan(&ir);
    let e403 = p
        .diagnostics
        .iter()
        .find(|d| diag_code(d).as_deref() == Some("E403"))
        .expect("an unconsumed kind must raise E403");
    let message = diag_message(e403).expect("E403 diagnostic must carry a message");
    assert!(
        message.contains("no implementation path"),
        "E403's message must be corrected to contain \"no implementation path\", got {:?}",
        message
    );
}

#[test]
fn oracle_1b_w404_zero_on_todo_gym() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    let w404s: Vec<&Sexpr> = p
        .diagnostics
        .iter()
        .filter(|d| diag_code(d).as_deref() == Some("W404"))
        .collect();
    assert!(
        w404s.is_empty(),
        "todo.gym must produce zero W404 diagnostics (the acceptance harness consumes all of \
         them, per docs/rust-port-plan-phase4.md scope item 1b), got {:?}",
        w404s
    );
    // Plan golden is byte-unchanged: item 1b explicitly requires the
    // existing golden fixture to still match once W404 exists.
    let serialized = gymnast_rs::sexpr::canonical_serialize(&p.to_sexpr());
    let golden = fs::read_to_string("tests/fixtures/todo-plan.sexpr")
        .or_else(|_| fs::read_to_string("fixtures/todo-plan.sexpr"))
        .expect("read tests/fixtures/todo-plan.sexpr golden");
    assert_eq!(
        serialized, golden,
        "adding W404 support must not change todo.gym's plan golden (zero W404s means zero new \
         diagnostics entries)"
    );
}

#[test]
fn oracle_1b_w404_never_fires_for_design_partition_nodes() {
    // "Design-partition nodes are definitional, not normative -- no
    // W404." An `actor` node lands in the design partition and is NOT a
    // member of acceptance-harness's (the only verification-class node's)
    // input-kind set, so a buggy implementation that checked W404 over
    // ALL partitions (instead of only transitions/obligations) would
    // wrongly flag it. A correct implementation must not.
    let actor = IrNode::new(
        "solo/actor/User".to_string(),
        "actor",
        "User".to_string(),
        vec![],
        vec![],
    );
    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "solo".to_string(),
        vec![],
        vec![actor],
        vec![],
        vec![],
        vec![],
        vec![],
    );
    let p = plan(&ir);
    let w404s: Vec<&Sexpr> = p
        .diagnostics
        .iter()
        .filter(|d| diag_code(d).as_deref() == Some("W404"))
        .collect();
    assert!(
        w404s.is_empty(),
        "a lone design-partition node (actor) must never raise W404, got {:?}",
        w404s
    );
}

/// See the file-level NOTE: per the phase-4 doc's own worked example, a
/// minimal spec with one behavior and no acceptance block should yield a
/// W404 for the behavior node, and the CLI should still exit 0 (a warning
/// does not fail the build). Written literally per the plan; flagged as a
/// probable plan/architecture conflict in the file header for the
/// integrator.
#[test]
fn oracle_1b_w404_fires_for_minimal_spec_missing_acceptance_block() {
    const MINIMAL_SPEC: &str = "spec w = v 0.1 owner o exports svc\n\nactor user = person\n\ninterface svc = for user (\n  cmd act = () text )\n\nbehavior beh = on svc.act (user) ( )\n";

    let (code, stdout, stderr) = run_plan_cli(MINIMAL_SPEC);
    assert_eq!(
        code, 0,
        "a spec whose only diagnostic is a W404 WARNING must still exit 0, got stderr: {}",
        stderr
    );
    assert!(
        stdout.contains("W404") || stderr.contains("W404"),
        "expected a W404 diagnostic for the behavior node with no acceptance block; \
         stdout: {}\nstderr: {}",
        stdout,
        stderr
    );
}

fn run_plan_cli(source: &str) -> (i32, String, String) {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir();
    let path = dir.join(format!(
        "gymnast-plan-table-oracle-{}-{}.gym",
        std::process::id(),
        unique
    ));
    let mut f = fs::File::create(&path).expect("write temp spec");
    f.write_all(source.as_bytes()).unwrap();
    drop(f);
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_gymnast-rs"))
        .args(["plan", path.to_str().unwrap()])
        .output()
        .expect("run gymnast-rs plan");
    fs::remove_file(&path).ok();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}
