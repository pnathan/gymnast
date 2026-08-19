use std::env;
use std::fs;
use std::path::Path;

use gymnast_rs::candidate::Candidate;
use gymnast_rs::diag::{self, Severity};
use gymnast_rs::elaborate;
use gymnast_rs::parser;
use gymnast_rs::plan;
use gymnast_rs::prompt;
use gymnast_rs::recipe;
use gymnast_rs::sexpr;

const USAGE: &str = "usage: gymnast-rs <parse|check|ir|plan|prompts> FILE.gym\n       gymnast-rs compile FILE.gym OUT_DIR";

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("{}", USAGE);
        std::process::exit(2);
    }

    let command = &args[1];

    // `compile` takes an extra positional (OUT_DIR) the other subcommands
    // don't, so it is dispatched before the shared `FILE.gym`-only arity
    // check below.
    if command == "compile" {
        if args.len() != 4 {
            eprintln!("{}", USAGE);
            std::process::exit(2);
        }
        let file_path = &args[2];
        let out_dir = &args[3];
        let src = read_src_or_exit(file_path);
        cmd_compile(&src, file_path, out_dir);
        return;
    }

    if args.len() != 3 {
        eprintln!("{}", USAGE);
        std::process::exit(2);
    }

    let file_path = &args[2];
    let src = read_src_or_exit(file_path);

    let file_name = Path::new(file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file.gym");

    match command.as_str() {
        "parse" => cmd_parse(&src, file_path, file_name),
        "check" => cmd_check(&src, file_path, file_name),
        "ir" => cmd_ir(&src, file_path, file_name),
        "plan" => cmd_plan(&src, file_path, file_name),
        "prompts" => cmd_prompts(&src, file_path, file_name),
        _ => {
            eprintln!("{}", USAGE);
            std::process::exit(2);
        }
    }
}

fn read_src_or_exit(file_path: &str) -> String {
    match fs::read_to_string(file_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("error: cannot read file: {}", e);
            std::process::exit(2);
        }
    }
}

/// Handle the `parse` subcommand.
fn cmd_parse(src: &str, file_path: &str, _file_name: &str) {
    let (ast, diags) = parser::parse(src);

    // Render diagnostics to stderr
    if !diags.is_empty() {
        let rendered = diag::render(&diags, src, file_path);
        eprint!("{}", rendered);
    }

    // Print AST to stdout
    if let Some(file) = ast {
        println!("{:#?}", file);
    }

    // Exit with error if any diagnostic is an error
    let has_errors = diags.iter().any(|d| d.severity == Severity::Error);
    std::process::exit(if has_errors { 1 } else { 0 });
}

/// Handle the `check` subcommand.
fn cmd_check(src: &str, file_path: &str, _file_name: &str) {
    let (ast, mut diags) = parser::parse(src);

    // Run the FULL elaboration diagnostic pipeline (profile expansion,
    // checking over expanded declarations, duplicate semantic IDs) and
    // discard the IR: `check` and `ir` report from one code path, so
    // they can never disagree about whether a spec is valid.
    if let Some(file) = ast {
        let parse_diags = std::mem::take(&mut diags);
        let (_ir, all_diags) = elaborate::elaborate_with_parse_diags(&file, &parse_diags);
        diags = all_diags;
    }

    // Render all diagnostics to stderr
    if !diags.is_empty() {
        let rendered = diag::render(&diags, src, file_path);
        eprint!("{}", rendered);
    }

    // Exit with error if any diagnostic is an error
    let has_errors = diags.iter().any(|d| d.severity == Severity::Error);
    std::process::exit(if has_errors { 1 } else { 0 });
}

/// Handle the `ir` subcommand.
fn cmd_ir(src: &str, file_path: &str, _file_name: &str) {
    let (ast, parse_diags) = parser::parse(src);

    // Parse diagnostics always render to stderr with source context.
    if !parse_diags.is_empty() {
        eprint!("{}", diag::render(&parse_diags, src, file_path));
    }
    let parse_errors = parse_diags.iter().any(|d| d.severity == Severity::Error);

    let file = match ast {
        Some(file) => file,
        None => std::process::exit(1),
    };

    // Parse diagnostics are folded into the IR so the serialized artifact
    // is self-describing; the returned typed list drives stderr rendering.
    let (ir, all_diags) = elaborate::elaborate_with_parse_diags(&file, &parse_diags);

    // Render check/elaboration diagnostics with source context (the parse
    // slice was already rendered above).
    let later_diags = &all_diags[parse_diags.len()..];
    if !later_diags.is_empty() {
        eprint!("{}", diag::render(later_diags, src, file_path));
    }

    print!("{}", sexpr::canonical_serialize(&ir.to_sexpr()));

    // Exit 1 on any error-severity diagnostic, parse or IR: a spec that
    // only partially parsed must not read as valid even when the
    // surviving declarations elaborate cleanly.
    std::process::exit(if parse_errors || ir.has_errors() {
        1
    } else {
        0
    });
}

/// Handle the `plan` subcommand: parse, elaborate, plan; stdout is the
/// canonical serialization of the plan (emitted even on failure, so a
/// caller inspecting stdout can always see the E401 refusal shape);
/// diagnostics (parse + IR + plan) render to stderr; exit 1 on any
/// error-severity diagnostic anywhere in the pipeline.
fn cmd_plan(src: &str, file_path: &str, _file_name: &str) {
    let (ast, parse_diags) = parser::parse(src);

    if !parse_diags.is_empty() {
        eprint!("{}", diag::render(&parse_diags, src, file_path));
    }
    let parse_errors = parse_diags.iter().any(|d| d.severity == Severity::Error);

    let file = match ast {
        Some(file) => file,
        None => std::process::exit(1),
    };

    let (ir, all_diags) = elaborate::elaborate_with_parse_diags(&file, &parse_diags);
    let later_diags = &all_diags[parse_diags.len()..];
    if !later_diags.is_empty() {
        eprint!("{}", diag::render(later_diags, src, file_path));
    }

    let p = plan::plan(&ir);

    // Plan-level diagnostics (E401/E402/E403) are lowered `Sexpr`
    // shapes, not `diag::Diagnostic` values with a source span worth
    // rendering (their span is always 0 0) — report them as plain
    // lines rather than forcing them through source-context rendering.
    let plan_has_errors = p.diagnostics.iter().any(|d| {
        d.assoc("severity")
            .and_then(|s| s.as_sym())
            .map(|s| s == "error")
            .unwrap_or(false)
    });
    for d in &p.diagnostics {
        let severity = d
            .assoc("severity")
            .and_then(|s| s.as_sym())
            .unwrap_or("error");
        let code = d.assoc("code").and_then(|s| s.as_str()).unwrap_or("");
        let message = d.assoc("message").and_then(|s| s.as_str()).unwrap_or("");
        eprintln!("{}[{}]: {}", severity, code, message);
    }

    print!("{}", sexpr::canonical_serialize(&p.to_sexpr()));

    std::process::exit(if parse_errors || ir.has_errors() || plan_has_errors {
        1
    } else {
        0
    });
}

/// Handle the `prompts` subcommand: parse, elaborate, plan, compile
/// prompts; stdout is the canonical serialization of `(prompts
/// ((prompt-package ...) ...))` — a plain list wrapper carrying no
/// fingerprint of its own (plan section D). Diagnostics (parse + IR +
/// plan) render to stderr; same exit-code contract as `plan`, since
/// prompt compilation is a pure projection of the plan and cannot itself
/// fail or add diagnostics.
fn cmd_prompts(src: &str, file_path: &str, _file_name: &str) {
    let (ast, parse_diags) = parser::parse(src);

    if !parse_diags.is_empty() {
        eprint!("{}", diag::render(&parse_diags, src, file_path));
    }
    let parse_errors = parse_diags.iter().any(|d| d.severity == Severity::Error);

    let file = match ast {
        Some(file) => file,
        None => std::process::exit(1),
    };

    let (ir, all_diags) = elaborate::elaborate_with_parse_diags(&file, &parse_diags);
    let later_diags = &all_diags[parse_diags.len()..];
    if !later_diags.is_empty() {
        eprint!("{}", diag::render(later_diags, src, file_path));
    }

    let p = plan::plan(&ir);

    let plan_has_errors = p.diagnostics.iter().any(|d| {
        d.assoc("severity")
            .and_then(|s| s.as_sym())
            .map(|s| s == "error")
            .unwrap_or(false)
    });
    for d in &p.diagnostics {
        let severity = d
            .assoc("severity")
            .and_then(|s| s.as_sym())
            .unwrap_or("error");
        let code = d.assoc("code").and_then(|s| s.as_str()).unwrap_or("");
        let message = d.assoc("message").and_then(|s| s.as_str()).unwrap_or("");
        eprintln!("{}[{}]: {}", severity, code, message);
    }

    let packages = prompt::compile_prompts(&ir, &p);
    let wrapper = sexpr::Sexpr::list(vec![
        sexpr::Sexpr::sym("prompts"),
        sexpr::Sexpr::list(packages.iter().map(|pk| pk.to_sexpr()).collect()),
    ]);
    print!("{}", sexpr::canonical_serialize(&wrapper));

    std::process::exit(if parse_errors || ir.has_errors() || plan_has_errors {
        1
    } else {
        0
    });
}

/// Handle the `compile` subcommand: parse, elaborate, plan, compile
/// prompts, execute the deterministic recipes; then write the full
/// front-half compilation into `out_dir` (plan section D):
/// `ir.sexpr`/`plan.sexpr`/`prompts.sexpr` (byte-identical to the
/// `ir`/`plan`/`prompts` subcommands' stdout), `results.sexpr` — `(results
/// ((execution-result ...) ...))` for all 8 nodes, deferred ones included
/// — and every file of every SUCCEEDED candidate, materialized at
/// `out_dir/<may-write path>` (parent directories created; any path
/// containing `..` or starting with `/` is rejected with `E511
/// unsafe-output-path` and skipped rather than written). stderr/exit
/// contract identical to `ir`/`plan`: two compiles of the same spec into
/// two directories must be byte-identical trees.
fn cmd_compile(src: &str, file_path: &str, out_dir: &str) {
    let (ast, parse_diags) = parser::parse(src);

    if !parse_diags.is_empty() {
        eprint!("{}", diag::render(&parse_diags, src, file_path));
    }
    let parse_errors = parse_diags.iter().any(|d| d.severity == Severity::Error);

    let file = match ast {
        Some(file) => file,
        None => std::process::exit(1),
    };

    let (ir, all_diags) = elaborate::elaborate_with_parse_diags(&file, &parse_diags);
    let later_diags = &all_diags[parse_diags.len()..];
    if !later_diags.is_empty() {
        eprint!("{}", diag::render(later_diags, src, file_path));
    }

    let p = plan::plan(&ir);

    let plan_has_errors = p.diagnostics.iter().any(|d| {
        d.assoc("severity")
            .and_then(|s| s.as_sym())
            .map(|s| s == "error")
            .unwrap_or(false)
    });
    for d in &p.diagnostics {
        let severity = d
            .assoc("severity")
            .and_then(|s| s.as_sym())
            .unwrap_or("error");
        let code = d.assoc("code").and_then(|s| s.as_str()).unwrap_or("");
        let message = d.assoc("message").and_then(|s| s.as_str()).unwrap_or("");
        eprintln!("{}[{}]: {}", severity, code, message);
    }

    let packages = prompt::compile_prompts(&ir, &p);
    let prompts_wrapper = sexpr::Sexpr::list(vec![
        sexpr::Sexpr::sym("prompts"),
        sexpr::Sexpr::list(packages.iter().map(|pk| pk.to_sexpr()).collect()),
    ]);

    let results = recipe::execute_deterministic(&ir, &p);
    let results_wrapper = sexpr::Sexpr::list(vec![
        sexpr::Sexpr::sym("results"),
        sexpr::Sexpr::list(results.iter().map(|r| r.to_sexpr()).collect()),
    ]);

    let out_path = Path::new(out_dir);
    if let Err(e) = fs::create_dir_all(out_path) {
        eprintln!("error: cannot create output directory {}: {}", out_dir, e);
        std::process::exit(2);
    }

    write_artifact(
        out_path,
        "ir.sexpr",
        &sexpr::canonical_serialize(&ir.to_sexpr()),
    );
    write_artifact(
        out_path,
        "plan.sexpr",
        &sexpr::canonical_serialize(&p.to_sexpr()),
    );
    write_artifact(
        out_path,
        "prompts.sexpr",
        &sexpr::canonical_serialize(&prompts_wrapper),
    );
    write_artifact(
        out_path,
        "results.sexpr",
        &sexpr::canonical_serialize(&results_wrapper),
    );

    // A failed deterministic recipe is a failed compilation: render its
    // diagnostics like plan diagnostics and fold them into the exit code
    // — `compile` must never exit 0 having produced nothing.
    let mut execution_errors = false;
    for result in &results {
        for d in &result.diagnostics {
            let severity = d
                .assoc("severity")
                .and_then(|s| s.as_sym())
                .unwrap_or("error");
            if severity == "error" {
                execution_errors = true;
            }
            let code = d.assoc("code").and_then(|s| s.as_str()).unwrap_or("");
            let message = d.assoc("message").and_then(|s| s.as_str()).unwrap_or("");
            eprintln!("{}[{}]: {}", severity, code, message);
        }
    }

    for result in &results {
        if result.status != recipe::ExecutionStatus::Succeeded {
            continue;
        }
        let candidate_sexpr = match &result.candidate {
            Some(c) => c.clone(),
            None => continue,
        };
        let candidate = match Candidate::from_sexpr(candidate_sexpr) {
            Some(c) => c,
            // A succeeded result whose candidate does not even parse back
            // is unreachable in practice (the firewall already required
            // it to parse to reach Succeeded), but `compile` must never
            // panic on it either way.
            None => continue,
        };
        for (path, content) in candidate.files() {
            if is_unsafe_output_path(&path) {
                eprintln!(
                    "error[E511]: unsafe-output-path: candidate for {} names an unsafe path, \
                     skipped: {}",
                    result.node_id, path
                );
                execution_errors = true;
                continue;
            }
            let dest = out_path.join(&path);
            if let Some(parent) = dest.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    eprintln!(
                        "error: cannot create directory for {}: {}",
                        dest.display(),
                        e
                    );
                    continue;
                }
            }
            if let Err(e) = fs::write(&dest, content) {
                eprintln!("error: cannot write {}: {}", dest.display(), e);
            }
        }
    }

    std::process::exit(
        if parse_errors || ir.has_errors() || plan_has_errors || execution_errors {
            1
        } else {
            0
        },
    );
}

/// Rejects any path containing `..` or starting with `/` (plan section D,
/// `E511 unsafe-output-path`): the candidate firewall already constrains
/// `files` paths to the node's `may_write` contract, but the filesystem
/// write is the last line of defense against an escaping path reaching
/// outside `out_dir`.
fn is_unsafe_output_path(path: &str) -> bool {
    path.starts_with('/') || path.contains("..")
}

/// Writes one top-level compilation artifact (`ir.sexpr`, `plan.sexpr`,
/// ...) directly under `out_dir`. A write failure here (permissions, a
/// full disk) is an operational error, not a diagnosable-input one, so it
/// exits 2 rather than folding into the diagnostic exit-code contract.
fn write_artifact(out_dir: &Path, name: &str, content: &str) {
    let dest = out_dir.join(name);
    if let Err(e) = fs::write(&dest, content) {
        eprintln!("error: cannot write {}: {}", dest.display(), e);
        std::process::exit(2);
    }
}
