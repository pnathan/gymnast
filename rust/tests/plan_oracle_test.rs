//! Tests-of-record for `plan.rs`, authored from
//! `docs/rust-port-plan-phase3.md` alone, BEFORE any implementation of
//! `crate::plan` exists (Process Rule 2). This file implements every
//! numbered item in the plan's "Oracle tests" section for
//! `plan_oracle_test.rs`, one `#[test]` per numbered item (some items are
//! split into several tests where the item bundles distinct assertions;
//! none are merged or dropped).
//!
//! Implementation stages MUST NOT edit this file. If an assertion here
//! turns out to conflict with a considered implementation choice, the
//! implementer reports the conflict; only the integrator resolves it.
//!
//! This file will not compile until `crate::plan` exists — that is
//! expected at this stage.

use gymnast_rs::elaborate;
use gymnast_rs::ir::{Ir, IrNode};
use gymnast_rs::parser;
use gymnast_rs::plan::{plan, PlanNode};
use gymnast_rs::sexpr::{canonical_serialize, Sexpr};
use std::fs;
use std::io::Write;

// ---------------------------------------------------------------------
// Shared fixtures / helpers (not tests themselves).
// ---------------------------------------------------------------------

/// Parses and elaborates `examples/todo.gym` fresh. Two separate calls
/// give two independently-built `Ir` values for determinism checks.
fn load_todo_ir() -> Ir {
    let src = fs::read_to_string("../examples/todo.gym").expect("read ../examples/todo.gym");
    let (ast, parse_diags) = parser::parse(&src);
    let file = ast.expect("parse todo.gym");
    let (ir, _all_diags) = elaborate::elaborate_with_parse_diags(&file, &parse_diags);
    ir
}

/// Builds the `<module>/plan/<local>` id for a local plan-node name.
fn full_id(module: &str, local: &str) -> String {
    format!("{}/plan/{}", module, local)
}

/// Pulls the `code` string out of a lowered `(diagnostic (severity s)
/// (code "C") (span a b) (message "...")) ` shape. Manual walk (not
/// `Sexpr::assoc`) since the outer list's head is a bare tag symbol, not
/// itself a (key value) pair, and `assoc`'s behavior over a mixed list is
/// unspecified by the plan.
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

/// One row of the fixed 8-node table (plan section B), trimmed to the
/// fields the numbered oracle items actually pin down: local name,
/// class, and the `depends_on` locals.
struct ExpectedNode {
    local: &'static str,
    class: &'static str,
    depends_on_locals: &'static [&'static str],
}

const TABLE: &[ExpectedNode] = &[
    ExpectedNode {
        local: "design-contracts",
        class: "structural",
        depends_on_locals: &[],
    },
    ExpectedNode {
        local: "transition-kernel",
        class: "generative",
        depends_on_locals: &["design-contracts"],
    },
    ExpectedNode {
        local: "authorization-policy",
        class: "generative",
        depends_on_locals: &["design-contracts", "transition-kernel"],
    },
    ExpectedNode {
        local: "persistence",
        class: "generative",
        depends_on_locals: &["design-contracts", "transition-kernel"],
    },
    ExpectedNode {
        local: "interface-contracts",
        class: "structural",
        depends_on_locals: &["design-contracts"],
    },
    ExpectedNode {
        local: "service-handlers",
        class: "generative",
        depends_on_locals: &[
            "transition-kernel",
            "authorization-policy",
            "persistence",
            "interface-contracts",
        ],
    },
    ExpectedNode {
        local: "acceptance-harness",
        class: "verification",
        depends_on_locals: &["service-handlers"],
    },
    ExpectedNode {
        local: "application-assembly",
        class: "assembly",
        depends_on_locals: &[
            "transition-kernel",
            "authorization-policy",
            "persistence",
            "interface-contracts",
            "service-handlers",
            "acceptance-harness",
        ],
    },
];

/// Kahn's-algorithm cycle check over `depends_on`, generic (not assuming
/// the fixed table). Bounded: the frontier queue grows by at most one
/// entry per edge and the loop consumes one queue entry per iteration,
/// so it terminates in at most `nodes.len()` iterations regardless of
/// whether the graph is in fact acyclic.
fn assert_acyclic(nodes: &[PlanNode]) {
    let mut indegree: std::collections::HashMap<&str, usize> =
        nodes.iter().map(|n| (n.id.as_str(), 0)).collect();
    let mut adj: std::collections::HashMap<&str, Vec<&str>> = std::collections::HashMap::new();
    for n in nodes {
        adj.entry(n.id.as_str()).or_default();
    }
    for n in nodes {
        for dep in &n.depends_on {
            adj.entry(dep.as_str()).or_default().push(n.id.as_str());
            *indegree.entry(n.id.as_str()).or_insert(0) += 1;
        }
    }

    let mut queue: Vec<&str> = indegree
        .iter()
        .filter(|(_, &d)| d == 0)
        .map(|(k, _)| *k)
        .collect();
    let mut visited = 0usize;
    let mut i = 0usize;
    while i < queue.len() {
        let cur = queue[i];
        i += 1;
        visited += 1;
        if let Some(neighbors) = adj.get(cur) {
            for &nb in neighbors {
                if let Some(d) = indegree.get_mut(nb) {
                    *d -= 1;
                    if *d == 0 {
                        queue.push(nb);
                    }
                }
            }
        }
    }
    assert_eq!(
        visited,
        nodes.len(),
        "plan dependency graph has a cycle (topological visit reached {} of {} nodes)",
        visited,
        nodes.len()
    );
}

fn assert_sorted(v: &[String], node_id: &str, field: &str) {
    let mut sorted = v.to_vec();
    sorted.sort();
    assert_eq!(
        v.to_vec(),
        sorted,
        "{}: field `{}` is not byte-sorted: {:?}",
        node_id,
        field,
        v
    );
}

fn run_plan_cli(source: &str) -> (i32, String, String) {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir();
    let path = dir.join(format!(
        "gymnast-plan-oracle-{}-{}.gym",
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

// A known-bad spec: `a` is declared as an invariant name twice, which
// elaborates to an E301 duplicate-semantic-id error (see
// tests/cli_test.rs::test_check_and_ir_agree_on_validity for the same
// fixture used against `ir`/`check`).
const KNOWN_BAD_SPEC: &str = "spec t = v 0.1 owner o exports A\n\nmode A = opaque text\ninv a = on s always p\ninv a = on s always q\n";

// ---------------------------------------------------------------------
// 1. Determinism: two independent parse -> elaborate -> plan runs over
//    todo.gym serialize byte-identically.
// ---------------------------------------------------------------------

#[test]
fn oracle_01_determinism_two_runs_byte_identical() {
    let ir1 = load_todo_ir();
    let ir2 = load_todo_ir();
    let p1 = plan(&ir1);
    let p2 = plan(&ir2);
    let s1 = canonical_serialize(&p1.to_sexpr());
    let s2 = canonical_serialize(&p2.to_sexpr());
    assert_eq!(
        s1, s2,
        "two independent parse->elaborate->plan runs must serialize identically"
    );
}

// ---------------------------------------------------------------------
// 2. Plan/IR binding: plan.ir_fingerprint == ir.fingerprint.
// ---------------------------------------------------------------------

#[test]
fn oracle_02_plan_ir_fingerprint_binding() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    assert_eq!(p.ir_fingerprint, ir.fingerprint);
}

// ---------------------------------------------------------------------
// 3. Exactly 8 nodes, ids and classes exactly per the table, in table
//    order.
// ---------------------------------------------------------------------

#[test]
fn oracle_03_eight_nodes_ids_and_classes_in_table_order() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    assert_eq!(p.nodes.len(), 8, "plan must contain exactly 8 nodes");
    for (i, expected) in TABLE.iter().enumerate() {
        let node = &p.nodes[i];
        assert_eq!(
            node.id,
            full_id(&ir.module_name, expected.local),
            "node at position {} has wrong id",
            i
        );
        assert_eq!(
            node.class, expected.class,
            "node {} has wrong class",
            node.id
        );
    }
}

// ---------------------------------------------------------------------
// 4. Dependency closure: every depends_on entry is a plan node id, and
//    the full table is asserted (not just the acceptance-harness case).
// ---------------------------------------------------------------------

#[test]
fn oracle_04_dependency_closure_full_table() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    let ids: std::collections::HashSet<&str> = p.nodes.iter().map(|n| n.id.as_str()).collect();

    for (i, expected) in TABLE.iter().enumerate() {
        let node = &p.nodes[i];
        for dep in &node.depends_on {
            assert!(
                ids.contains(dep.as_str()),
                "{}: depends_on entry {} names no plan node",
                node.id,
                dep
            );
        }
        let mut expected_deps: Vec<String> = expected
            .depends_on_locals
            .iter()
            .map(|l| full_id(&ir.module_name, l))
            .collect();
        expected_deps.sort();
        assert_eq!(
            node.depends_on, expected_deps,
            "{}: depends_on does not match the plan table",
            node.id
        );
    }
}

// ---------------------------------------------------------------------
// 5. DAG acyclicity, checked generically over depends_on (not assumed
//    from the table).
// ---------------------------------------------------------------------

#[test]
fn oracle_05_dag_acyclic_generic() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    assert_acyclic(&p.nodes);
}

// ---------------------------------------------------------------------
// 6. Coverage totality on todo.gym: zero E403 diagnostics AND every IR
//    node id appears in >= 1 coverage entry with a non-empty plan list.
// ---------------------------------------------------------------------

#[test]
fn oracle_06_coverage_totality_on_todo() {
    let ir = load_todo_ir();
    let p = plan(&ir);

    for d in &p.diagnostics {
        if let Some(code) = diag_code(d) {
            assert_ne!(code, "E403", "unexpected E403 on todo.gym: {:?}", d);
        }
    }

    let all_ids: Vec<String> = ir.all_nodes().iter().map(|n| n.id.clone()).collect();
    for id in &all_ids {
        let entry = p
            .coverage
            .iter()
            .find(|(cid, _)| cid == id)
            .unwrap_or_else(|| panic!("no coverage entry for IR node {}", id));
        assert!(
            !entry.1.is_empty(),
            "coverage entry for {} has an empty plan-node list",
            id
        );
    }
}

// ---------------------------------------------------------------------
// 7. Coverage failure path: the fixed table covers all 13 kinds, so no
//    real spec can trigger E403 by omission. Instead: a synthetic Ir
//    with an extra node of a covered kind ("type") and a fabricated id
//    IS covered, and the coverage list length equals all_nodes().len().
// ---------------------------------------------------------------------

#[test]
fn oracle_07_coverage_synthetic_extra_type_node_is_covered() {
    let extra = IrNode::new(
        "synth/type/Extra".to_string(),
        "type",
        "Extra".to_string(),
        vec![],
        vec![],
    );
    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "synth".to_string(),
        vec![],
        vec![extra],
        vec![],
        vec![],
        vec![],
        vec![],
    );

    let p = plan(&ir);

    for d in &p.diagnostics {
        if let Some(code) = diag_code(d) {
            assert_ne!(code, "E403", "unexpected E403 on synthetic Ir: {:?}", d);
        }
    }

    assert_eq!(
        p.coverage.len(),
        ir.all_nodes().len(),
        "coverage list length must equal all_nodes().len()"
    );

    let entry = p
        .coverage
        .iter()
        .find(|(id, _)| id == "synth/type/Extra")
        .expect("fabricated type node must appear in coverage");
    assert!(
        !entry.1.is_empty(),
        "fabricated type node must be covered by at least one plan node"
    );
}

// ---------------------------------------------------------------------
// 8. Sorted-lists canonicality: for every node, inputs/depends_on/
//    may_write/capabilities/obligations/prohibitions are byte-sorted.
// ---------------------------------------------------------------------

#[test]
fn oracle_08_sorted_lists_canonicality() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    for node in &p.nodes {
        assert_sorted(&node.inputs, &node.id, "inputs");
        assert_sorted(&node.depends_on, &node.id, "depends_on");
        assert_sorted(&node.may_write, &node.id, "may_write");
        assert_sorted(&node.capabilities, &node.id, "capabilities");
        assert_sorted(&node.obligations, &node.id, "obligations");
        assert_sorted(&node.prohibitions, &node.id, "prohibitions");
    }
}

// ---------------------------------------------------------------------
// 9. Node fingerprint stability: rebuilding a PlanNode from the same
//    arguments yields the same fingerprint; permuting input order
//    yields the same fingerprint (sorting erases it).
//
//    ASSUMPTION (noted in the stage report): the plan document describes
//    `PlanNode::new(...)` in prose ("mirrors the Lamedh constructor
//    exactly") without giving a Rust signature. This test assumes the
//    natural signature: positional parameters in struct-field order,
//    `class`/`recipe` as `&str` (matching `IrNode::new`'s existing
//    `kind: &str` convention), everything else owned (`String`/`Vec<
//    String>`/`Sexpr`) as in the struct.
// ---------------------------------------------------------------------

#[test]
fn oracle_09_node_fingerprint_stability_and_permutation_invariance() {
    let a = PlanNode::new(
        "m/plan/x".to_string(),
        "generative",
        "recipe-v1",
        vec!["m/type/A".to_string(), "m/type/B".to_string()],
        vec!["m/plan/dep1".to_string(), "m/plan/dep2".to_string()],
        Sexpr::list(vec![Sexpr::sym("ruby"), Sexpr::sym("rails")]),
        Sexpr::sym("none"),
        vec!["out/a.rb".to_string(), "out/b.rb".to_string()],
        vec!["clock".to_string(), "id-source".to_string()],
        vec!["ob1".to_string(), "ob2".to_string()],
        vec!["pr1".to_string(), "pr2".to_string()],
    );

    let b = PlanNode::new(
        "m/plan/x".to_string(),
        "generative",
        "recipe-v1",
        vec!["m/type/A".to_string(), "m/type/B".to_string()],
        vec!["m/plan/dep1".to_string(), "m/plan/dep2".to_string()],
        Sexpr::list(vec![Sexpr::sym("ruby"), Sexpr::sym("rails")]),
        Sexpr::sym("none"),
        vec!["out/a.rb".to_string(), "out/b.rb".to_string()],
        vec!["clock".to_string(), "id-source".to_string()],
        vec!["ob1".to_string(), "ob2".to_string()],
        vec!["pr1".to_string(), "pr2".to_string()],
    );

    assert_eq!(
        a.fingerprint, b.fingerprint,
        "identical constructor arguments must yield identical fingerprints"
    );

    // Every list argument permuted relative to `a`/`b`.
    let c = PlanNode::new(
        "m/plan/x".to_string(),
        "generative",
        "recipe-v1",
        vec!["m/type/B".to_string(), "m/type/A".to_string()],
        vec!["m/plan/dep2".to_string(), "m/plan/dep1".to_string()],
        Sexpr::list(vec![Sexpr::sym("ruby"), Sexpr::sym("rails")]),
        Sexpr::sym("none"),
        vec!["out/b.rb".to_string(), "out/a.rb".to_string()],
        vec!["id-source".to_string(), "clock".to_string()],
        vec!["ob2".to_string(), "ob1".to_string()],
        vec!["pr2".to_string(), "pr1".to_string()],
    );

    assert_eq!(
        a.fingerprint, c.fingerprint,
        "permuting list-argument order must not change the fingerprint"
    );
}

// ---------------------------------------------------------------------
// 10. Invalid-IR refusal: planning an Ir carrying an error diagnostic
//     yields E401 and zero nodes, and the CLI exits 1.
//     Split into a unit-level check (10a) and a CLI-level check (10b).
// ---------------------------------------------------------------------

#[test]
fn oracle_10a_invalid_ir_refusal_yields_e401_zero_nodes() {
    let bad_diag = Sexpr::list(vec![
        Sexpr::sym("diagnostic"),
        Sexpr::list(vec![Sexpr::sym("severity"), Sexpr::sym("error")]),
        Sexpr::list(vec![Sexpr::sym("code"), Sexpr::Str("E301".to_string())]),
        Sexpr::list(vec![Sexpr::sym("span"), Sexpr::Int(0), Sexpr::Int(0)]),
        Sexpr::list(vec![
            Sexpr::sym("message"),
            Sexpr::Str("duplicate id".to_string()),
        ]),
    ]);
    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "bad".to_string(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![bad_diag],
    );
    assert!(ir.has_errors());

    let p = plan(&ir);

    assert!(
        p.nodes.is_empty(),
        "planning invalid IR must yield zero plan nodes"
    );
    assert!(
        p.coverage.is_empty(),
        "planning invalid IR must yield zero coverage entries"
    );
    assert_eq!(
        p.target,
        Sexpr::list(vec![Sexpr::sym("lamedh")]),
        "planning invalid IR must still fall back to the default target"
    );

    let codes: Vec<String> = p.diagnostics.iter().filter_map(diag_code).collect();
    assert_eq!(
        codes,
        vec!["E401".to_string()],
        "planning invalid IR must yield exactly one E401 diagnostic"
    );
}

#[test]
fn oracle_10b_cli_plan_exits_1_on_invalid_ir() {
    let (code, stdout, stderr) = run_plan_cli(KNOWN_BAD_SPEC);
    assert_eq!(code, 1, "gymnast-rs plan must exit 1 on invalid IR");
    assert!(
        stdout.starts_with("(plan "),
        "plan output is still emitted on failure, got: {}",
        stdout
    );
    assert!(
        stderr.contains("E301") || stderr.contains("E401"),
        "stderr should surface the causing or refusal diagnostic, got: {}",
        stderr
    );
}

// ---------------------------------------------------------------------
// 11. Target/model selection: todo.gym's plan target is (ruby rails)
//     and generative nodes carry the todo model; a spec with no
//     synthesis node yields target (lamedh) and the default model;
//     paths in may_write end in .rb for todo.gym.
//     Split into three tests for the three distinct scenarios.
// ---------------------------------------------------------------------

#[test]
fn oracle_11a_todo_target_and_generative_model_match_synthesis_node() {
    let ir = load_todo_ir();
    let synth = ir.nodes_of_kind("synthesis");
    assert_eq!(
        synth.len(),
        1,
        "todo.gym must have exactly one synthesis node"
    );

    let expected_target = synth[0]
        .field(":target")
        .cloned()
        .expect("todo.gym synthesis node must carry :target");
    let expected_model = synth[0]
        .field(":model")
        .cloned()
        .expect("todo.gym synthesis node must carry :model");

    assert_eq!(
        expected_target,
        Sexpr::list(vec![Sexpr::sym("ruby"), Sexpr::sym("rails")]),
        "todo.gym's synthesis :target must be (ruby rails)"
    );

    let p = plan(&ir);
    assert_eq!(p.target, expected_target);

    for node in &p.nodes {
        if node.class == "generative" {
            assert_eq!(
                node.model, expected_model,
                "{}: generative node must carry the todo model verbatim",
                node.id
            );
        } else {
            assert_eq!(
                node.model,
                Sexpr::sym("none"),
                "{}: non-generative node (class {}) must carry model none",
                node.id,
                node.class
            );
        }
    }
}

#[test]
fn oracle_11b_default_target_and_model_when_no_synthesis_node() {
    let type_node = IrNode::new(
        "nosyn/type/Foo".to_string(),
        "type",
        "Foo".to_string(),
        vec![],
        vec![],
    );
    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "nosyn".to_string(),
        vec![],
        vec![type_node],
        vec![],
        vec![],
        vec![], // no synthesis nodes
        vec![],
    );

    let p = plan(&ir);

    assert_eq!(
        p.target,
        Sexpr::list(vec![Sexpr::sym("lamedh")]),
        "with no synthesis node, target must default to (lamedh)"
    );

    let default_model = Sexpr::list(vec![
        Sexpr::sym("small_code_model"),
        Sexpr::list(vec![Sexpr::list(vec![
            Sexpr::sym("class"),
            Sexpr::sym("nano"),
        ])]),
    ]);

    for node in &p.nodes {
        if node.class == "generative" {
            assert_eq!(
                node.model, default_model,
                "{}: with no synthesis node, generative model must default to (small_code_model ((class nano)))",
                node.id
            );
        }
    }
}

#[test]
fn oracle_11c_todo_may_write_paths_end_in_rb() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    for node in &p.nodes {
        for path in &node.may_write {
            // The two paths that never carried a .lisp suffix
            // (generated/adapters/schema.sexpr, generated/manifest.sexpr)
            // pass through unrewritten; every other path in the table
            // originates as .lisp and must be rewritten to .rb for the
            // (ruby rails) target.
            if path.ends_with(".sexpr") {
                continue;
            }
            assert!(
                path.ends_with(".rb"),
                "{}: expected a .rb path, got {}",
                node.id,
                path
            );
        }
    }
}

// ---------------------------------------------------------------------
// Golden fixture comparison (Section D). Not itself a numbered oracle
// item; ignored until the integrator generates
// tests/fixtures/todo-plan.sexpr via the CLI, per Process Rule 5.
// ---------------------------------------------------------------------

#[test]
fn golden_plan_matches_fixture() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    let serialized = canonical_serialize(&p.to_sexpr());

    let golden = fs::read_to_string("tests/fixtures/todo-plan.sexpr")
        .or_else(|_| fs::read_to_string("fixtures/todo-plan.sexpr"))
        .expect("read tests/fixtures/todo-plan.sexpr golden");

    assert_eq!(
        serialized, golden,
        "Elaborated plan does not match golden fixture.\n\
         To regenerate the fixture, run:\n\
         cargo run -- plan ../examples/todo.gym > tests/fixtures/todo-plan.sexpr"
    );
}
