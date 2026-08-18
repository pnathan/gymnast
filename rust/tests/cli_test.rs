//! End-to-end CLI exit-code contract for the `ir` subcommand.

use std::io::Write;
use std::process::Command;

fn run_ir(source: &str) -> (i32, String, String) {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir();
    let path = dir.join(format!(
        "gymnast-cli-test-{}-{}.gym",
        std::process::id(),
        unique
    ));
    let mut f = std::fs::File::create(&path).expect("write temp spec");
    f.write_all(source.as_bytes()).unwrap();
    drop(f);
    let out = Command::new(env!("CARGO_BIN_EXE_gymnast-rs"))
        .args(["ir", path.to_str().unwrap()])
        .output()
        .expect("run gymnast-rs");
    std::fs::remove_file(&path).ok();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn test_ir_exit_zero_on_clean_spec() {
    let (code, stdout, _) = run_ir("spec m = v 0.1 owner o exports A\n\nmode A = opaque text\n");
    assert_eq!(code, 0);
    assert!(stdout.starts_with("(ir "));
}

#[test]
fn test_ir_exit_one_when_parser_recovered_from_errors() {
    // The malformed declaration parses away via error recovery; the
    // surviving declarations elaborate cleanly — but the spec must still
    // be rejected, and the parse error must reach stderr.
    let (code, stdout, stderr) =
        run_ir("spec m = v 0.1 owner o exports A\n\nmode ! nonsense\n\nmode A = opaque text\n");
    assert_eq!(code, 1, "partial parse must not read as valid");
    assert!(stdout.starts_with("(ir "), "IR is still emitted");
    assert!(stderr.contains("error["), "parse error rendered to stderr");
}

#[test]
fn test_ir_stderr_carries_elaboration_diagnostics() {
    // Missing required profile decision: E302 must be visible on stderr,
    // not only buried in the IR.
    let (code, _, stderr) = run_ir(
        "spec m = v 0.1 owner o exports A\n\nuse oddities/profiles/todo_standard @ 1.0 (sharing_limit 1)\n\nmode A = opaque text\n",
    );
    assert_eq!(code, 1);
    assert!(
        stderr.contains("E302"),
        "E302 must render to stderr, got: {}",
        stderr
    );
}

#[test]
fn test_ir_artifact_carries_parse_diagnostics() {
    // The serialized IR must be self-describing: a spec that recovered
    // from a parse error carries that diagnostic in its own diagnostics
    // list, not only on stderr / in the exit code.
    let (code, stdout, _) =
        run_ir("spec m = v 0.1 owner o exports A\n\nmode ! nonsense\n\nmode A = opaque text\n");
    assert_eq!(code, 1);
    assert!(
        stdout.contains("(diagnostics ((diagnostic (severity error)"),
        "IR diagnostics list must include the parse error, got: {}",
        stdout
    );
}
