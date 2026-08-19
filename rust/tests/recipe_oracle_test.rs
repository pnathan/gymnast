//! Tests-of-record for `recipe.rs`, the section-A `sexpr::parse` reader,
//! and the `compile` CLI subcommand, authored from
//! `docs/rust-port-plan-phase4.md` (sections A, C, D and the "Oracle
//! tests" section's `recipe_oracle_test.rs` list) ALONE, before any of
//! `crate::recipe`, `sexpr::parse`, or `gymnast-rs compile` exist (the
//! phase-4 process upgrade: Stage 1 commits these oracle files to git
//! before any implementation stage runs). `src/recipe.lisp` was consulted
//! only for behavioral intent; shapes come from the phase-4 doc and the
//! Rust IR/plan/prompt contracts already committed in `rust/src/`.
//!
//! Implementation stages MUST NOT edit this file. If an assertion here
//! turns out to conflict with a considered implementation choice, the
//! implementer reports the conflict; only the integrator resolves it.
//!
//! This file will not compile until `crate::recipe` and `sexpr::parse`
//! exist and `main.rs` gains a `compile` subcommand -- that is expected
//! at this stage.
//!
//! Numbering below follows the phase-4 doc's `recipe_oracle_test.rs` list
//! items 1-10 exactly; each gets one or more `#[test]`s, none merged or
//! dropped.

use gymnast_rs::candidate::{candidate_diagnostics, Candidate};
use gymnast_rs::elaborate;
use gymnast_rs::ir::Ir;
use gymnast_rs::parser;
use gymnast_rs::plan::{plan, Plan, PlanNode};
use gymnast_rs::prompt::compile_prompts;
use gymnast_rs::recipe::{
    execute_deterministic, execute_recipe, lookup, ExecutionStatus, RecipeClass,
};
use gymnast_rs::sexpr::{self, canonical_serialize, Sexpr};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

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

fn diag_code(d: &Sexpr) -> Option<String> {
    d.assoc("code").and_then(|c| c.as_str()).map(String::from)
}

fn hand_node(id: &str, recipe: &str, target: Sexpr) -> PlanNode {
    PlanNode::new(
        id.to_string(),
        "structural",
        recipe,
        vec![],
        vec![],
        target,
        Sexpr::sym("none"),
        vec!["out/a.rb".to_string()],
        vec![],
        vec![],
        vec![],
    )
}

// ---------------------------------------------------------------------
// 1. Registry totality: all 8 recipe names from the plan table resolve;
//    the four generative ones have no executor; classes match.
// ---------------------------------------------------------------------

const RECIPE_TABLE: &[(&str, RecipeClass, bool)] = &[
    ("design-contracts-v1", RecipeClass::Structural, true),
    ("transition-kernel-v1", RecipeClass::Generative, false),
    ("authorization-policy-v1", RecipeClass::Generative, false),
    ("persistence-v1", RecipeClass::Generative, false),
    ("interface-contracts-v1", RecipeClass::Structural, true),
    ("service-handlers-v1", RecipeClass::Generative, false),
    ("acceptance-harness-v1", RecipeClass::Verification, true),
    ("application-assembly-v1", RecipeClass::Assembly, true),
];

#[test]
fn oracle_01_registry_totality_all_eight_recipes_resolve() {
    for (name, class, has_executor) in RECIPE_TABLE {
        let r = lookup(name).unwrap_or_else(|| panic!("recipe {} must resolve", name));
        assert_eq!(r.name, *name, "recipe name mismatch for {}", name);
        assert_eq!(r.class, *class, "{}: wrong class", name);
        assert_eq!(
            r.execute.is_some(),
            *has_executor,
            "{}: executor presence mismatch (generative recipes have none)",
            name
        );
    }
}

#[test]
fn oracle_01_unknown_recipe_name_does_not_resolve() {
    assert!(lookup("not-a-real-recipe-v1").is_none());
}

// ---------------------------------------------------------------------
// 2. Deterministic execution over todo.gym: statuses are exactly
//    {design-contracts, interface-contracts, acceptance-harness,
//    application-assembly: Succeeded; transition-kernel,
//    authorization-policy, persistence, service-handlers: Deferred}.
// ---------------------------------------------------------------------

fn local_of(node_id: &str) -> &str {
    node_id.rsplit('/').next().unwrap()
}

#[test]
fn oracle_02_deterministic_execution_statuses_over_todo() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    let results = execute_deterministic(&ir, &p);
    assert_eq!(results.len(), 8, "one execution result per plan node");

    let mut by_local: HashMap<&str, &ExecutionStatus> = HashMap::new();
    for r in &results {
        by_local.insert(local_of(&r.node_id), &r.status);
    }

    let expected_succeeded = [
        "design-contracts",
        "interface-contracts",
        "acceptance-harness",
        "application-assembly",
    ];
    let expected_deferred = [
        "transition-kernel",
        "authorization-policy",
        "persistence",
        "service-handlers",
    ];

    for local in expected_succeeded {
        assert_eq!(
            by_local.get(local).copied(),
            Some(&ExecutionStatus::Succeeded),
            "{} must succeed",
            local
        );
    }
    for local in expected_deferred {
        assert_eq!(
            by_local.get(local).copied(),
            Some(&ExecutionStatus::Deferred),
            "{} must be deferred",
            local
        );
    }
}

// ---------------------------------------------------------------------
// 3. Every succeeded candidate passes the firewall with zero
//    diagnostics, writes exactly its node's may_write paths, and its
//    `implements` list equals the node's inputs (slice order).
// ---------------------------------------------------------------------

#[test]
fn oracle_03_succeeded_candidates_pass_firewall_and_match_contract() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    let results = execute_deterministic(&ir, &p);

    let mut checked_any = false;
    for r in &results {
        if r.status != ExecutionStatus::Succeeded {
            continue;
        }
        checked_any = true;
        let node = p
            .nodes
            .iter()
            .find(|n| n.id == r.node_id)
            .unwrap_or_else(|| panic!("no plan node for result {}", r.node_id));
        let candidate_sexpr = r
            .candidate
            .as_ref()
            .expect("a succeeded result must carry a candidate");

        let diags = candidate_diagnostics(node, candidate_sexpr);
        assert!(
            diags.is_empty(),
            "{}: succeeded candidate must pass the firewall with zero diagnostics, got {:?}",
            node.id,
            diags
        );

        let cand = Candidate::from_sexpr(candidate_sexpr.clone())
            .unwrap_or_else(|| panic!("{}: succeeded candidate must itself parse", node.id));

        let mut written: Vec<String> = cand
            .files()
            .iter()
            .map(|(path, _): &(String, String)| path.clone())
            .collect();
        written.sort();
        let mut expected = node.may_write.clone();
        expected.sort();
        assert_eq!(
            written, expected,
            "{}: candidate must write exactly its node's may_write paths",
            node.id
        );

        assert_eq!(
            cand.implements(),
            node.inputs,
            "{}: implements must equal node.inputs in slice order",
            node.id
        );
    }
    assert!(
        checked_any,
        "todo.gym must have at least one Succeeded result to check"
    );
}

// ---------------------------------------------------------------------
// 4. Determinism: two independent execute_deterministic runs serialize
//    byte-identically.
// ---------------------------------------------------------------------

#[test]
fn oracle_04_determinism_two_runs_byte_identical() {
    let ir1 = load_todo_ir();
    let ir2 = load_todo_ir();
    let p1 = plan(&ir1);
    let p2 = plan(&ir2);
    let r1 = execute_deterministic(&ir1, &p1);
    let r2 = execute_deterministic(&ir2, &p2);

    assert_eq!(r1.len(), r2.len());
    let s1: Vec<String> = r1
        .iter()
        .map(|r| canonical_serialize(&r.to_sexpr()))
        .collect();
    let s2: Vec<String> = r2
        .iter()
        .map(|r| canonical_serialize(&r.to_sexpr()))
        .collect();
    assert_eq!(
        s1, s2,
        "two independent execute_deterministic runs must serialize identically"
    );
}

// ---------------------------------------------------------------------
// 5. Unknown recipe -> Failed + E509 (construct a PlanNode by hand).
// ---------------------------------------------------------------------

#[test]
fn oracle_05_unknown_recipe_fails_with_e509() {
    let ir = load_todo_ir();
    let node = hand_node(
        "m/plan/bogus",
        "totally-unregistered-recipe-v99",
        Sexpr::list(vec![Sexpr::sym("ruby"), Sexpr::sym("rails")]),
    );
    let r = execute_recipe(&ir, &node);
    assert_eq!(r.status, ExecutionStatus::Failed);
    let codes: Vec<String> = r.diagnostics.iter().filter_map(diag_code).collect();
    assert!(
        codes.contains(&"E509".to_string()),
        "expected E509 unknown-recipe, got {:?}",
        codes
    );
}

// ---------------------------------------------------------------------
// 6. Non-ruby target -> Failed + E510 (hand-built node with go target).
// ---------------------------------------------------------------------

#[test]
fn oracle_06_non_ruby_target_fails_with_e510() {
    let ir = load_todo_ir();
    let node = hand_node(
        "m/plan/design-contracts-ish",
        "design-contracts-v1",
        Sexpr::list(vec![Sexpr::sym("go"), Sexpr::sym("stdlib")]),
    );
    let r = execute_recipe(&ir, &node);
    assert_eq!(r.status, ExecutionStatus::Failed);
    let codes: Vec<String> = r.diagnostics.iter().filter_map(diag_code).collect();
    assert!(
        codes.contains(&"E510".to_string()),
        "expected E510 unsupported-target-emitter, got {:?}",
        codes
    );
    let joined = r
        .diagnostics
        .iter()
        .map(|d| d.print())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        joined.contains("go"),
        "E510 message must name the unsupported language, got {}",
        joined
    );
}

// ---------------------------------------------------------------------
// 7. sexpr reader round-trip law over a structured corpus (every
//    constructor, nesting, escapes, negative ints, nil) + parse rejects:
//    unbalanced parens, trailing garbage, depth > 256, unterminated
//    string -- each with Err, never a panic.
// ---------------------------------------------------------------------

#[test]
fn oracle_07a_round_trip_law_over_structured_corpus() {
    // NOTE: Sexpr::Sym("nil") is deliberately excluded from this corpus.
    // Per section A, "nil" is the printed form of the empty list, so a
    // constructed Sym("nil") does not round-trip
    // (parse(&Sym("nil").print()) == Ok(List(vec![])) != Sym("nil")) --
    // this is documented, not tested, exactly as section A specifies.
    let corpus: Vec<Sexpr> = vec![
        Sexpr::sym("foo"),
        Sexpr::sym("foo-bar_baz123"),
        Sexpr::sym("a/b/c"),
        Sexpr::sym("todo/plan/design-contracts"),
        Sexpr::Str("hello world".to_string()),
        Sexpr::Str(String::new()),
        Sexpr::Str("with \"quotes\" and \\backslash\\ inside".to_string()),
        Sexpr::Int(0),
        Sexpr::Int(42),
        Sexpr::Int(-17),
        Sexpr::Int(i64::MAX),
        Sexpr::Int(i64::MIN),
        Sexpr::list(vec![
            Sexpr::sym("a"),
            Sexpr::Int(1),
            Sexpr::Str("s".to_string()),
        ]),
        Sexpr::list(vec![
            Sexpr::list(vec![Sexpr::sym("nested")]),
            Sexpr::list(vec![]), // nested empty list, still round-trips
            Sexpr::list(vec![Sexpr::list(vec![Sexpr::list(vec![Sexpr::sym(
                "deep",
            )])])]),
        ]),
        Sexpr::list(vec![
            Sexpr::sym("diagnostic"),
            Sexpr::list(vec![Sexpr::sym("severity"), Sexpr::sym("error")]),
            Sexpr::list(vec![Sexpr::sym("code"), Sexpr::Str("E501".to_string())]),
            Sexpr::list(vec![Sexpr::sym("span"), Sexpr::Int(0), Sexpr::Int(0)]),
            Sexpr::list(vec![
                Sexpr::sym("message"),
                Sexpr::Str("candidate writes outside its node contract: \"a/b\\c\"".to_string()),
            ]),
        ]),
    ];

    for v in &corpus {
        let printed = v.print();
        let parsed = sexpr::parse(&printed);
        assert_eq!(
            parsed,
            Ok(v.clone()),
            "round trip failed for {:?} (printed {:?})",
            v,
            printed
        );
    }
}

#[test]
fn oracle_07b_empty_list_prints_as_nil_and_reads_back_as_empty_list() {
    let v = Sexpr::list(vec![]);
    assert_eq!(v.print(), "nil");
    assert_eq!(sexpr::parse("nil"), Ok(v));
}

#[test]
fn oracle_07c_unknown_escape_keeps_the_backslash() {
    // The printer only ever escapes \" and \\; an escape sequence the
    // printer would never produce (e.g. \n) must still be accepted by
    // the reader for untrusted input, keeping the backslash literally.
    let parsed = sexpr::parse("\"a\\nb\"").expect("parse must accept an unknown escape");
    assert_eq!(parsed, Sexpr::Str("a\\nb".to_string()));
}

#[test]
fn oracle_07d_parse_rejects_malformed_input_without_panicking() {
    let bad_inputs: Vec<String> = vec![
        "(a b".to_string(),                                 // unbalanced: unclosed paren
        ")".to_string(),                                    // unbalanced: stray close paren
        "(a) trailing garbage".to_string(),                 // trailing non-whitespace
        "\"unterminated".to_string(),                       // unterminated string
        format!("{}x{}", "(".repeat(300), ")".repeat(300)), // depth > 256, but balanced
    ];
    for input in &bad_inputs {
        let outcome = std::panic::catch_unwind(|| sexpr::parse(input));
        let result =
            outcome.unwrap_or_else(|_| panic!("parse must never panic on input {:?}", input));
        assert!(
            result.is_err(),
            "expected Err for malformed input {:?}, got {:?}",
            input,
            result
        );
    }
}

#[test]
fn oracle_07e_parse_accepts_a_depth_at_the_boundary() {
    // 256 levels deep must still be accepted; only > 256 is rejected.
    let input = format!("{}x{}", "(".repeat(256), ")".repeat(256));
    let outcome = std::panic::catch_unwind(|| sexpr::parse(&input));
    let result = outcome.unwrap_or_else(|_| panic!("parse must never panic on depth 256"));
    assert!(
        result.is_ok(),
        "depth exactly 256 must be accepted, got {:?}",
        result
    );
}

// ---------------------------------------------------------------------
// 8. Firewall-on-recipes: tamper one emitter output path (via a
//    hand-built candidate) and assert E503/E504 fire -- the firewall
//    applies to recipe output, not only model output.
// ---------------------------------------------------------------------

#[test]
fn oracle_08_firewall_applies_to_recipe_output_not_only_model_output() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    let node = p
        .nodes
        .iter()
        .find(|n| n.id.ends_with("/plan/design-contracts"))
        .expect("todo.gym must have a design-contracts plan node");
    assert_eq!(
        node.may_write.len(),
        1,
        "precondition: exactly one required path"
    );
    let real_path = node.may_write[0].clone();

    // A hand-built candidate simulating a recipe emission whose output
    // path was tampered with: correct node-id and candidate shape, but a
    // file written to the WRONG path -- as if a recipe (not a model) had
    // silently redirected its own output.
    let tampered = Sexpr::list(vec![
        Sexpr::sym("candidate"),
        Sexpr::list(vec![
            Sexpr::pair("schema", Sexpr::Str("gymnast.candidate/0.1".to_string())),
            Sexpr::pair("node-id", Sexpr::Str(node.id.clone())),
            Sexpr::pair(
                "files",
                Sexpr::list(vec![Sexpr::list(vec![
                    Sexpr::Str(format!("{}.tampered", real_path)),
                    Sexpr::Str("# tampered output".to_string()),
                ])]),
            ),
            Sexpr::pair("implements", Sexpr::list(vec![])),
            Sexpr::pair("edge-uses", Sexpr::list(vec![])),
            Sexpr::pair("assumptions", Sexpr::list(vec![])),
            Sexpr::pair("unresolved", Sexpr::list(vec![])),
        ]),
    ]);

    let diags = candidate_diagnostics(node, &tampered);
    let codes: Vec<String> = diags.iter().filter_map(diag_code).collect();
    assert!(
        codes.contains(&"E503".to_string()),
        "a tampered recipe output path must trip E503 unauthorized-output-path, got {:?}",
        codes
    );
    assert!(
        codes.contains(&"E504".to_string()),
        "the real contract path is then missing, tripping E504 missing-output-file, got {:?}",
        codes
    );
}

// ---------------------------------------------------------------------
// 9. Section-presence (item 1d): every todo.gym prompt package text
//    contains each of the 8 unconditional headers exactly once at line
//    start.
// ---------------------------------------------------------------------

const UNCONDITIONAL_HEADERS: &[&str] = &[
    "GYMNAST NODE CONTRACT",
    "TARGET",
    "OBLIGATIONS",
    "PROHIBITIONS",
    "AUTHORIZED FILES",
    "DEPENDENCIES",
    "OUTPUT PROTOCOL",
    "AUTHORITATIVE INPUT (reference)",
];

#[test]
fn oracle_09_section_presence_eight_unconditional_headers() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    let packages = compile_prompts(&ir, &p);
    assert_eq!(packages.len(), 8);
    for pkg in &packages {
        for header in UNCONDITIONAL_HEADERS {
            let count = pkg.text.lines().filter(|line| line == header).count();
            assert_eq!(
                count, 1,
                "{}: header {:?} must appear exactly once at line start, got {} occurrences",
                pkg.node_id, header, count
            );
        }
    }
}

// ---------------------------------------------------------------------
// 10. compile: run via CLI into two temp dirs, assert identical file
//     sets and identical bytes per file, results.sexpr contains 8
//     execution-results, and generated/ contains exactly the succeeded
//     candidates' files.
// ---------------------------------------------------------------------

fn run_compile_cli(spec_path: &str, out_dir: &Path) -> (i32, String, String) {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_gymnast-rs"))
        .args(["compile", spec_path, out_dir.to_str().unwrap()])
        .output()
        .expect("run gymnast-rs compile");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn walk_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !dir.exists() {
        return out;
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let entries = match fs::read_dir(&d) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries {
            let entry = entry.expect("read_dir entry");
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

#[test]
fn oracle_10_compile_reproducible_results_sexpr_and_generated_files() {
    let unique = std::process::id();
    let dir1 = std::env::temp_dir().join(format!("gymnast-compile-oracle-1-{}", unique));
    let dir2 = std::env::temp_dir().join(format!("gymnast-compile-oracle-2-{}", unique));
    let _ = fs::remove_dir_all(&dir1);
    let _ = fs::remove_dir_all(&dir2);

    let (code1, _out1, err1) = run_compile_cli("../examples/todo.gym", &dir1);
    let (code2, _out2, err2) = run_compile_cli("../examples/todo.gym", &dir2);
    assert_eq!(
        code1, 0,
        "compile of a clean spec must exit 0, stderr: {}",
        err1
    );
    assert_eq!(
        code2, 0,
        "compile of a clean spec must exit 0, stderr: {}",
        err2
    );

    let files1 = walk_files(&dir1);
    let files2 = walk_files(&dir2);
    let rel1: Vec<String> = files1
        .iter()
        .map(|p| {
            p.strip_prefix(&dir1)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    let rel2: Vec<String> = files2
        .iter()
        .map(|p| {
            p.strip_prefix(&dir2)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    assert_eq!(rel1, rel2, "two compiles must produce identical file sets");

    for (f1, f2) in files1.iter().zip(files2.iter()) {
        let b1 = fs::read(f1).unwrap();
        let b2 = fs::read(f2).unwrap();
        assert_eq!(
            b1, b2,
            "{:?} vs {:?}: two compiles must be byte-identical",
            f1, f2
        );
    }

    // results.sexpr: exactly 8 execution-result entries.
    let results_text = fs::read_to_string(dir1.join("results.sexpr")).expect("read results.sexpr");
    let results_sexpr = sexpr::parse(results_text.trim_end()).expect("results.sexpr must parse");
    let outer = results_sexpr
        .as_list()
        .expect("results.sexpr top level must be a list");
    assert_eq!(outer.first().and_then(|s| s.as_sym()), Some("results"));
    let entries = outer
        .get(1)
        .and_then(|v| v.as_list())
        .expect("(results (...)) second element must be a list");
    assert_eq!(
        entries.len(),
        8,
        "results.sexpr must contain exactly 8 execution-results"
    );
    for e in entries {
        let head = e.as_list().and_then(|l| l.first()).and_then(|s| s.as_sym());
        assert_eq!(head, Some("execution-result"));
    }

    // generated/ contains exactly the succeeded candidates' files.
    let ir = load_todo_ir();
    let p: Plan = plan(&ir);
    let results = execute_deterministic(&ir, &p);
    let mut expected_generated: Vec<String> = Vec::new();
    for r in &results {
        if r.status == ExecutionStatus::Succeeded {
            let node = p.nodes.iter().find(|n| n.id == r.node_id).unwrap();
            expected_generated.extend(node.may_write.iter().cloned());
        }
    }
    expected_generated.sort();

    let mut actual_generated: Vec<String> = rel1
        .iter()
        .filter(|p| p.starts_with("generated/"))
        .cloned()
        .collect();
    actual_generated.sort();

    assert_eq!(
        actual_generated, expected_generated,
        "generated/ must contain exactly the succeeded candidates' files"
    );

    let _ = fs::remove_dir_all(&dir1);
    let _ = fs::remove_dir_all(&dir2);
}

#[test]
fn oracle_10_compile_stderr_exit_contract_matches_plan_on_bad_spec() {
    // "stderr/exit contract identical to ir/plan" (section D). A
    // known-invalid spec (duplicate semantic id, same fixture family as
    // plan_oracle_test.rs's KNOWN_BAD_SPEC) must exit 1.
    const KNOWN_BAD_SPEC: &str = "spec t = v 0.1 owner o exports A\n\nmode A = opaque text\ninv a = on s always p\ninv a = on s always q\n";

    let unique = std::process::id();
    let bad_path = std::env::temp_dir().join(format!("gymnast-compile-bad-{}.gym", unique));
    fs::write(&bad_path, KNOWN_BAD_SPEC).unwrap();
    let out_dir = std::env::temp_dir().join(format!("gymnast-compile-bad-out-{}", unique));
    let _ = fs::remove_dir_all(&out_dir);

    let (code, _stdout, stderr) = run_compile_cli(bad_path.to_str().unwrap(), &out_dir);
    assert_eq!(
        code, 1,
        "compile must exit 1 on invalid IR, stderr: {}",
        stderr
    );

    fs::remove_file(&bad_path).ok();
    let _ = fs::remove_dir_all(&out_dir);
}
