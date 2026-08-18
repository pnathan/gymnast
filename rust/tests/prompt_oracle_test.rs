//! Tests-of-record for `prompt.rs`, authored from
//! `docs/rust-port-plan-phase3.md` alone, BEFORE any implementation of
//! `crate::prompt` exists (Process Rule 2). This file implements every
//! numbered item in the plan's "Oracle tests" section for
//! `prompt_oracle_test.rs`, one `#[test]` per numbered item.
//!
//! Implementation stages MUST NOT edit this file. If an assertion here
//! turns out to conflict with a considered implementation choice, the
//! implementer reports the conflict; only the integrator resolves it.
//!
//! This file will not compile until `crate::plan` and `crate::prompt`
//! exist — that is expected at this stage.

use gymnast_rs::elaborate;
use gymnast_rs::fingerprint;
use gymnast_rs::ir::Ir;
use gymnast_rs::parser;
use gymnast_rs::plan::{plan, Plan, PlanNode};
use gymnast_rs::prompt::{compile_prompt, compile_prompts, PromptPackage};
use gymnast_rs::sexpr::{canonical_serialize, Sexpr};
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

/// Parses, elaborates, and plans todo.gym fresh. Two separate calls give
/// two independently-built (Ir, Plan) pairs for determinism checks.
fn load_todo_ir_and_plan() -> (Ir, Plan) {
    let ir = load_todo_ir();
    let p = plan(&ir);
    (ir, p)
}

/// The CLI's `(prompts ((prompt-package ...) ...))` wrapper (Section D):
/// a plain list wrapper carrying no fingerprint of its own.
fn wrap_packages(pkgs: &[PromptPackage]) -> Sexpr {
    Sexpr::list(vec![
        Sexpr::sym("prompts"),
        Sexpr::list(pkgs.iter().map(|pk| pk.to_sexpr()).collect()),
    ])
}

fn serialize_packages(pkgs: &[PromptPackage]) -> String {
    canonical_serialize(&wrap_packages(pkgs))
}

/// Drops the trailing `(fingerprint "...")` entry that `to_sexpr()`
/// appends to the inner field list, mirroring the same
/// build-then-strip relationship `Ir`/`Plan` use between their
/// `to_sexpr_without_fingerprint` form and their stored fingerprint
/// (see ir.rs's `test_fingerprint_excludes_fingerprint_field`).
fn strip_last_field(sexpr: &Sexpr) -> Sexpr {
    match sexpr {
        Sexpr::List(outer) => {
            let mut outer = outer.clone();
            if let Some(Sexpr::List(inner)) = outer.last_mut() {
                inner.pop();
            }
            Sexpr::List(outer)
        }
        other => other.clone(),
    }
}

const SECTION_HEADERS: &[&str] = &[
    "GYMNAST NODE CONTRACT",
    "TARGET",
    "CAPABILITY CONTRACTS",
    "STATE MODEL",
    "TYPE REFERENCE",
    "BEHAVIORAL REFERENCE",
    "OBLIGATIONS",
    "PROHIBITIONS",
    "AUTHORIZED FILES",
    "DEPENDENCIES",
    "OUTPUT PROTOCOL",
    "AUTHORITATIVE INPUT (reference)",
];

const CLOSING_INSTRUCTION: &str = "Return only the candidate S-expression. Report no confidence score. If the contract is not locally closed, return an unresolved entry and no files.";

// ---------------------------------------------------------------------
// 1. Determinism: compile_prompts over todo.gym twice, byte-identical.
// ---------------------------------------------------------------------

#[test]
fn oracle_01_determinism_compile_prompts_byte_identical() {
    let (ir1, plan1) = load_todo_ir_and_plan();
    let (ir2, plan2) = load_todo_ir_and_plan();
    let pkgs1 = compile_prompts(&ir1, &plan1);
    let pkgs2 = compile_prompts(&ir2, &plan2);
    assert_eq!(
        serialize_packages(&pkgs1),
        serialize_packages(&pkgs2),
        "compile_prompts over two independent runs must serialize identically"
    );
}

// ---------------------------------------------------------------------
// 2. One package per plan node, node_id/node_fingerprint/executor match
//    the plan node.
// ---------------------------------------------------------------------

#[test]
fn oracle_02_one_package_per_plan_node_fields_match() {
    let (ir, p) = load_todo_ir_and_plan();
    let pkgs = compile_prompts(&ir, &p);

    assert_eq!(
        pkgs.len(),
        p.nodes.len(),
        "compile_prompts must produce exactly one package per plan node"
    );

    for (pkg, node) in pkgs.iter().zip(p.nodes.iter()) {
        assert_eq!(pkg.node_id, node.id);
        assert_eq!(pkg.node_fingerprint, node.fingerprint);
        assert_eq!(pkg.executor, node.class);
    }
}

// ---------------------------------------------------------------------
// 3. Projection totality (the anti-silent-drop invariant): for every
//    package, every obligation name, every prohibition name, and every
//    may_write path of its node appears verbatim in `text`; every id in
//    node.inputs appears in the serialized ir_slice; every dependency id
//    appears in the dependency slice with the depended node's actual
//    fingerprint.
// ---------------------------------------------------------------------

#[test]
fn oracle_03_projection_totality_anti_silent_drop() {
    let (ir, p) = load_todo_ir_and_plan();
    let pkgs = compile_prompts(&ir, &p);
    let node_by_id: std::collections::HashMap<&str, &PlanNode> =
        p.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    for (pkg, node) in pkgs.iter().zip(p.nodes.iter()) {
        for ob in &node.obligations {
            assert!(
                pkg.text.contains(ob.as_str()),
                "{}: obligation `{}` missing from text",
                node.id,
                ob
            );
        }
        for pr in &node.prohibitions {
            assert!(
                pkg.text.contains(pr.as_str()),
                "{}: prohibition `{}` missing from text",
                node.id,
                pr
            );
        }
        for path in &node.may_write {
            assert!(
                pkg.text.contains(path.as_str()),
                "{}: may_write path `{}` missing from text",
                node.id,
                path
            );
        }

        let slice_text = Sexpr::list(pkg.ir_slice.iter().map(|n| n.to_sexpr()).collect()).print();
        for input_id in &node.inputs {
            assert!(
                slice_text.contains(input_id.as_str()),
                "{}: input `{}` missing from serialized ir_slice",
                node.id,
                input_id
            );
        }

        for dep_id in &node.depends_on {
            let (found_id, found_fp) = pkg
                .dependency_slice
                .iter()
                .find(|(id, _)| id == dep_id)
                .unwrap_or_else(|| {
                    panic!(
                        "{}: dependency `{}` missing from dependency_slice",
                        node.id, dep_id
                    )
                });
            assert_eq!(found_id, dep_id);
            match node_by_id.get(dep_id.as_str()) {
                Some(dep_node) => assert_eq!(
                    found_fp, &dep_node.fingerprint,
                    "{}: dependency `{}` fingerprint does not match the depended node's actual fingerprint",
                    node.id, dep_id
                ),
                None => assert_eq!(
                    found_fp, "missing",
                    "{}: dependency `{}` resolves to no plan node, must be recorded as missing",
                    node.id, dep_id
                ),
            }
        }
    }
}

// ---------------------------------------------------------------------
// 4. Section order: the section headers present in each package's text
//    appear in the specified order. (Projection sections may be omitted
//    entirely when their backing node set is empty, so this checks the
//    relative order of whichever headers ARE present, not that all are
//    present.)
// ---------------------------------------------------------------------

#[test]
fn oracle_04_section_order() {
    let (ir, p) = load_todo_ir_and_plan();
    let pkgs = compile_prompts(&ir, &p);

    for pkg in &pkgs {
        let mut last_pos: Option<usize> = None;
        let mut seen_any = false;
        for header in SECTION_HEADERS {
            if let Some(pos) = pkg.text.find(header) {
                seen_any = true;
                if let Some(last) = last_pos {
                    assert!(
                        pos > last,
                        "{}: section header `{}` appears out of order",
                        pkg.node_id,
                        header
                    );
                }
                last_pos = Some(pos);
            }
        }
        assert!(
            seen_any,
            "{}: no known section header found in prompt text",
            pkg.node_id
        );
    }
}

// ---------------------------------------------------------------------
// 5. Behavioral reference fidelity on todo.gym's transition-kernel
//    package: contains create_task, its Failures: line contains
//    "forbidden when" and "preserves all_state" — the fault-class
//    regression guard at the prompt level.
// ---------------------------------------------------------------------

#[test]
fn oracle_05_behavioral_reference_fidelity_transition_kernel_create_task() {
    let (ir, p) = load_todo_ir_and_plan();
    let node = p
        .nodes
        .iter()
        .find(|n| n.id.ends_with("/plan/transition-kernel"))
        .expect("transition-kernel plan node must exist");

    let pkg = compile_prompt(&ir, &p, node);

    assert!(
        pkg.text.contains("create_task"),
        "transition-kernel package must reference create_task: {}",
        pkg.text
    );
    assert!(
        pkg.text.contains("Failures:"),
        "transition-kernel package must have a Failures: line"
    );
    assert!(
        pkg.text.contains("forbidden when"),
        "transition-kernel package's Failures: line must contain `forbidden when`: {}",
        pkg.text
    );
    assert!(
        pkg.text.contains("preserves all_state"),
        "transition-kernel package's Failures: line must contain `preserves all_state`: {}",
        pkg.text
    );
}

// ---------------------------------------------------------------------
// 6. The closing instruction is the last line of every text.
// ---------------------------------------------------------------------

#[test]
fn oracle_06_closing_instruction_is_last_line() {
    let (ir, p) = load_todo_ir_and_plan();
    let pkgs = compile_prompts(&ir, &p);

    for pkg in &pkgs {
        let trimmed = pkg.text.trim_end_matches('\n');
        let last_line = trimmed.lines().last().unwrap_or("");
        assert_eq!(
            last_line, CLOSING_INSTRUCTION,
            "{}: closing instruction is not the last line",
            pkg.node_id
        );
    }
}

// ---------------------------------------------------------------------
// 7. Prompt fingerprint recomputes over the fingerprint-free package
//    form.
// ---------------------------------------------------------------------

#[test]
fn oracle_07_fingerprint_recomputes_over_fingerprint_free_form() {
    let (ir, p) = load_todo_ir_and_plan();
    let pkgs = compile_prompts(&ir, &p);

    for pkg in &pkgs {
        let full = pkg.to_sexpr();
        let stripped = strip_last_field(&full);
        let recomputed = fingerprint::fingerprint(&stripped);
        assert_eq!(
            pkg.fingerprint, recomputed,
            "{}: fingerprint does not recompute over the fingerprint-free form",
            pkg.node_id
        );
    }
}

// ---------------------------------------------------------------------
// 8. ir_slice resolution drops nothing: slice length equals
//    node.inputs.len() for todo.gym (all ids resolve).
// ---------------------------------------------------------------------

#[test]
fn oracle_08_ir_slice_resolves_all_inputs_todo() {
    let (ir, p) = load_todo_ir_and_plan();
    let pkgs = compile_prompts(&ir, &p);

    for (pkg, node) in pkgs.iter().zip(p.nodes.iter()) {
        assert_eq!(
            pkg.ir_slice.len(),
            node.inputs.len(),
            "{}: ir_slice length does not equal node.inputs.len() (some input failed to resolve)",
            node.id
        );
    }
}

// ---------------------------------------------------------------------
// Golden fixture comparison (Section D). Not itself a numbered oracle
// item; ignored until the integrator generates
// tests/fixtures/todo-prompts.sexpr via the CLI, per Process Rule 5.
// ---------------------------------------------------------------------

#[test]
fn golden_prompts_matches_fixture() {
    let (ir, p) = load_todo_ir_and_plan();
    let pkgs = compile_prompts(&ir, &p);
    let serialized = serialize_packages(&pkgs);

    let golden = fs::read_to_string("tests/fixtures/todo-prompts.sexpr")
        .or_else(|_| fs::read_to_string("fixtures/todo-prompts.sexpr"))
        .expect("read tests/fixtures/todo-prompts.sexpr golden");

    assert_eq!(
        serialized, golden,
        "Compiled prompts do not match golden fixture.\n\
         To regenerate the fixture, run:\n\
         cargo run -- prompts ../examples/todo.gym > tests/fixtures/todo-prompts.sexpr"
    );
}
