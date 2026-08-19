//! Content-level regression protection for the deterministic half of the
//! compiler (phase-4 gate Finding 4): the emitters' exact output bytes are
//! pinned by tests/fixtures/todo-results.sexpr, which embeds every
//! generated file's content inside the succeeded candidates.
//!
//! Regenerate (only with a stated reason) via:
//!   cargo run -- compile ../examples/todo.gym /tmp/out
//!   cp /tmp/out/results.sexpr tests/fixtures/todo-results.sexpr

use std::process::Command;

fn compile_todo(dir_tag: &str) -> std::path::PathBuf {
    let out = std::env::temp_dir().join(format!(
        "gymnast-golden-results-{}-{}",
        std::process::id(),
        dir_tag
    ));
    let _ = std::fs::remove_dir_all(&out);
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

#[test]
fn test_results_match_content_golden() {
    let out = compile_todo("golden");
    let produced = std::fs::read_to_string(out.join("results.sexpr")).unwrap();
    let golden = include_str!("fixtures/todo-results.sexpr");
    assert_eq!(
        produced, golden,
        "results.sexpr (which embeds all emitted file content) diverged \
         from the golden; regenerate only with a stated reason"
    );
}

#[test]
fn test_acceptance_harness_carries_constraint_evidence() {
    // Phase-4 gate Finding 1: constraints are normative and must appear
    // in the evidence artifact, not only in coverage bookkeeping.
    let out = compile_todo("constraints");
    let harness =
        std::fs::read_to_string(out.join("generated/verification/acceptance.rb")).unwrap();
    assert!(
        harness.contains("# constraint: todo/constraint/collaborative_capacity"),
        "harness must carry a per-constraint entry:\n{}",
        harness
    );
    assert!(
        harness.contains("constraints: 1"),
        "harness summary must count constraints:\n{}",
        harness
    );
}

#[test]
fn test_compile_exits_nonzero_when_all_recipes_fail() {
    // Phase-4 gate Finding 3: a compilation that produced nothing must
    // not exit 0. A go target makes every deterministic recipe fail
    // with E510 (only the Ruby emitter exists).
    let spec_src =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/todo.gym"))
            .unwrap()
            .replace("target ruby / rails", "target go / stdlib");
    let spec = std::env::temp_dir().join(format!("gymnast-go-{}.gym", std::process::id()));
    std::fs::write(&spec, spec_src).unwrap();
    let out_dir = std::env::temp_dir().join(format!("gymnast-go-out-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out_dir);
    let output = Command::new(env!("CARGO_BIN_EXE_gymnast-rs"))
        .args(["compile", spec.to_str().unwrap(), out_dir.to_str().unwrap()])
        .output()
        .expect("run compile");
    std::fs::remove_file(&spec).ok();
    assert_eq!(
        output.status.code(),
        Some(1),
        "all-recipes-failed compile must exit 1"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("E510"),
        "the failure must reach stderr, got: {}",
        stderr
    );
}
