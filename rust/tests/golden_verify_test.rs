//! Content-level regression protection for `verify.rs`'s compiled
//! verification bundle (plan-phase6 section C): the exact output bytes
//! are pinned by `tests/fixtures/todo-verify.sexpr`.
//!
//! Regenerate (only with a stated reason) via:
//!   cargo run -- verify ../examples/todo.gym > tests/fixtures/todo-verify.sexpr

use gymnast_rs::elaborate;
use gymnast_rs::parser;
use gymnast_rs::sexpr;
use gymnast_rs::verify;
use std::fs;
use std::process::Command;

#[test]
fn test_todo_verify_matches_golden_fixture() {
    let src = fs::read_to_string("../examples/todo.gym").expect("Cannot read ../examples/todo.gym");
    let (ast, parse_diags) = parser::parse(&src);
    let file = ast.expect("Failed to parse todo.gym");
    let (ir, _) = elaborate::elaborate_with_parse_diags(&file, &parse_diags);

    let bundle = verify::compile_verification(&ir);
    let serialized = sexpr::canonical_serialize(&bundle);

    let golden = fs::read_to_string("tests/fixtures/todo-verify.sexpr")
        .or_else(|_| fs::read_to_string("fixtures/todo-verify.sexpr"))
        .expect("Cannot read tests/fixtures/todo-verify.sexpr");

    assert_eq!(
        serialized, golden,
        "compile_verification(todo.gym) does not match golden fixture.\n\
         To regenerate the fixture, run:\n\
         cargo run -- verify ../examples/todo.gym > tests/fixtures/todo-verify.sexpr"
    );
}

#[test]
fn test_todo_verify_deterministic_two_compiles() {
    let src = fs::read_to_string("../examples/todo.gym").expect("Cannot read ../examples/todo.gym");
    let (ast1, parse_diags1) = parser::parse(&src);
    let (ast2, parse_diags2) = parser::parse(&src);
    let (ir1, _) = elaborate::elaborate_with_parse_diags(&ast1.unwrap(), &parse_diags1);
    let (ir2, _) = elaborate::elaborate_with_parse_diags(&ast2.unwrap(), &parse_diags2);

    let b1 = sexpr::canonical_serialize(&verify::compile_verification(&ir1));
    let b2 = sexpr::canonical_serialize(&verify::compile_verification(&ir2));
    assert_eq!(
        b1, b2,
        "verification bundle must be byte-identical across runs"
    );
}

#[test]
fn test_cli_verify_stdout_matches_golden_fixture() {
    // Exercises the full CLI path (`gymnast-rs verify FILE.gym`), not
    // just the library function directly, so a divergence between the
    // CLI's own serialization and the library's would show up here too.
    let out = Command::new(env!("CARGO_BIN_EXE_gymnast-rs"))
        .args(["verify", "../examples/todo.gym"])
        .output()
        .expect("run gymnast-rs verify");
    assert!(out.status.success(), "verify on todo.gym must exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();

    let golden = fs::read_to_string("tests/fixtures/todo-verify.sexpr")
        .or_else(|_| fs::read_to_string("fixtures/todo-verify.sexpr"))
        .expect("Cannot read tests/fixtures/todo-verify.sexpr");

    assert_eq!(
        stdout, golden,
        "`gymnast-rs verify` stdout does not match the golden fixture"
    );
}
