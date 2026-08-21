use std::env;
use std::fs;
use std::path::Path;

use gymnast_rs::adequacy;
use gymnast_rs::assembly;
use gymnast_rs::candidate::{is_unsafe_output_path, Candidate};
use gymnast_rs::diag::{self, Severity};
use gymnast_rs::elaborate;
use gymnast_rs::parser;
use gymnast_rs::plan;
use gymnast_rs::prompt;
use gymnast_rs::recipe;
use gymnast_rs::runner;
use gymnast_rs::sexpr::{self, Sexpr};
use gymnast_rs::verify;

const USAGE: &str = "usage: gymnast-rs <parse|check|ir|plan|prompts|verify|adequacy> FILE.gym\n       gymnast-rs compile FILE.gym OUT_DIR\n       gymnast-rs synthesize FILE.gym OUT_DIR [MAX_ATTEMPTS]";

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

    // `synthesize` takes FILE.gym, OUT_DIR, and an optional MAX_ATTEMPTS —
    // dispatched before the shared `FILE.gym`-only arity check below for
    // the same reason `compile` is. NOT exercised by CI (it invokes a
    // live model via `ClaudeSubprocessProvider`); see
    // `docs/rust-port-plan-phase5.md`, section C.
    if command == "synthesize" {
        if !(4..=5).contains(&args.len()) {
            eprintln!("{}", USAGE);
            std::process::exit(2);
        }
        let file_path = &args[2];
        let out_dir = &args[3];
        let max_attempts: u32 = match args.get(4) {
            Some(raw) => match raw.parse() {
                Ok(n) => n,
                Err(_) => {
                    eprintln!("error: MAX_ATTEMPTS must be a non-negative integer");
                    std::process::exit(2);
                }
            },
            None => 3,
        };
        let src = read_src_or_exit(file_path);
        cmd_synthesize(&src, file_path, out_dir, max_attempts);
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
        "verify" => cmd_verify(&src, file_path, file_name),
        "adequacy" => cmd_adequacy(&src, file_path, file_name),
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
    let mut ir_for_coverage = None;
    if let Some(file) = ast {
        let parse_diags = std::mem::take(&mut diags);
        let (ir, all_diags) = elaborate::elaborate_with_parse_diags(&file, &parse_diags);
        diags = all_diags;
        ir_for_coverage = Some(ir);
    }

    // Render all diagnostics to stderr
    if !diags.is_empty() {
        let rendered = diag::render(&diags, src, file_path);
        eprint!("{}", rendered);
    }

    // Exit with error if any diagnostic is an error
    let has_errors = diags.iter().any(|d| d.severity == Severity::Error);

    // W408/W409 coverage warnings surface on `check` stderr too
    // (post-gate-10, finding 3) — but only over an error-free IR, since
    // coverage over a broken spec is noise. Warnings never affect exit.
    if !has_errors {
        if let Some(ir) = &ir_for_coverage {
            for msg in verify::coverage_warning_messages(ir) {
                eprintln!("warning: {}", msg);
            }
        }
    }

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

/// Handle the `verify` subcommand: parse, elaborate, compile the
/// verification bundle; stdout is the canonical serialization of
/// `(verification-bundle ...)`. Same pipeline/diagnostic/exit contract
/// as `ir` (plan-phase6 section C): exit reflects parse/IR errors only.
/// A FAILED verification obligation is results data carried inside the
/// bundle, not a process error — it never affects the exit code, since
/// promotion decisions belong to assembly (phase 7), not this stage.
fn cmd_verify(src: &str, file_path: &str, _file_name: &str) {
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

    let bundle = verify::compile_verification(&ir);
    print!("{}", sexpr::canonical_serialize(&bundle));

    // Phase-7 gate, finding 8: an error-severity diagnostic INSIDE the
    // bundle (e.g. E601 duplicate-obligation-id, which has no IR-level
    // error) must fail the command visibly — cache keys and assembly
    // evidence depend on what these errors protect, and an exit-0 with
    // empty stderr defeats them. Warnings (W406) and infos stay exit 0.
    let bundle_error_messages = verify::bundle_error_diagnostics(&bundle);
    for msg in &bundle_error_messages {
        eprintln!("error: {}", msg);
    }

    // W408/W409 coverage warnings surface on `verify` stderr as the docs
    // promise (post-gate-10, finding 3); they also live in the bundle's
    // `coverage-diagnostics` field. Warnings never affect the exit code.
    if !parse_errors && !ir.has_errors() {
        for msg in verify::coverage_warning_messages(&ir) {
            eprintln!("warning: {}", msg);
        }
    }

    std::process::exit(
        if parse_errors || ir.has_errors() || !bundle_error_messages.is_empty() {
            1
        } else {
            0
        },
    );
}

/// Handle the `adequacy` subcommand: parse, elaborate, run the standard
/// mutation campaign over the elaborated IR; stdout is the canonical
/// serialization of `(campaign-result ...)`. Same arity/diagnostic
/// contract as `verify` (plan-phase9 section E); exit 1 on parse/IR
/// errors OR any error-severity diagnostic in the verification bundle
/// (phase-9 gate, finding 2) — and in the refusal cases NO campaign is
/// emitted at all, because a campaign over an unsound baseline is
/// fabricated evidence (unlike `verify`/`ir`/`plan`, which always print
/// their artifact first; the campaign is a JUDGMENT over verification,
/// not a projection of the spec). A failing campaign (`pass nil` —
/// critical mutants survived or could not be applied) is evidence data
/// carried inside the result, not a process error — the same rationale
/// as `hold` in the phase-8 evidence bundle. The reference has no
/// adequacy subcommand; this is a documented delta
/// (`docs/ir-contract-deltas.md`).
fn cmd_adequacy(src: &str, file_path: &str, _file_name: &str) {
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

    // Phase-9 gate, finding 2: a spec whose verification bundle carries
    // error-severity diagnostics (e.g. E601 duplicate-obligation-id) has
    // an UNSOUND baseline — duplicate ids make the first-match baseline
    // lookup mask genuine kills. `verify` refuses such specs visibly
    // (phase-7 gate, finding 8); `adequacy` must not silently accept
    // what `verify` rejects, and a campaign over an unsound baseline is
    // fabricated evidence — refuse before running it.
    let bundle = verify::compile_verification(&ir);
    let bundle_errors = verify::bundle_error_diagnostics(&bundle);
    if !bundle_errors.is_empty() {
        for msg in &bundle_errors {
            eprintln!("error: {}", msg);
        }
        eprintln!("error: verification bundle has errors; adequacy baseline would be unsound");
        std::process::exit(1);
    }

    let campaign = adequacy::run_campaign(&ir, &adequacy::standard_todo_mutants());
    print!("{}", sexpr::canonical_serialize(&campaign));

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

    // Phase-5 fold-in scope item 1d: `compile_prompt`'s own return shape
    // never carries an unresolved-input warning (the goldens are pinned
    // byte-for-byte), so the CLI — the caller — surfaces them itself.
    report_prompt_ir_slice_warnings(&ir, &p);

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

/// Reports every plan node's `W405 unresolved-input` prompt-side warning
/// to stderr (phase-5 fold-in scope item 1d) in plan order. A warning
/// never affects the exit code (W405 is warning-severity); `todo.gym`
/// has none anywhere in its plan, so this prints nothing for it.
fn report_prompt_ir_slice_warnings(ir: &gymnast_rs::ir::Ir, p: &plan::Plan) {
    for node in &p.nodes {
        for d in prompt::prompt_ir_slice_warnings(ir, node) {
            let severity = d
                .assoc("severity")
                .and_then(|s| s.as_sym())
                .unwrap_or("warning");
            let code = d.assoc("code").and_then(|s| s.as_str()).unwrap_or("");
            let message = d.assoc("message").and_then(|s| s.as_str()).unwrap_or("");
            eprintln!("{}[{}]: {}", severity, code, message);
        }
    }
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

    report_prompt_ir_slice_warnings(&ir, &p);

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
    write_evidence_bundle(out_path, &ir, &p, &results);

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
        if write_candidate_files(out_path, &result.node_id, candidate_sexpr) {
            execution_errors = true;
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

/// Writes every `(path, content)` file claim of one candidate under
/// `out_dir`, applying the same E511 guard `compile` always has (now the
/// library's `is_unsafe_output_path`, phase-5 fold-in scope item 1e):
/// an unsafe path is reported and skipped rather than written. Shared by
/// `cmd_compile` (deterministic-recipe candidates) and `cmd_synthesize`
/// (both deterministic AND model-run candidates), so the two write paths
/// can never disagree about what counts as a safe destination. Returns
/// `true` iff at least one path was rejected as unsafe.
fn write_candidate_files(out_path: &Path, node_id: &str, candidate_sexpr: Sexpr) -> bool {
    let candidate = match Candidate::from_sexpr(candidate_sexpr) {
        Some(c) => c,
        // A succeeded/accepted result whose candidate does not even parse
        // back is unreachable in practice (the firewall already required
        // it to parse to reach that status), but this must never panic on
        // it either way.
        None => return false,
    };
    let mut had_unsafe_path = false;
    for (path, content) in candidate.files() {
        if is_unsafe_output_path(&path) {
            eprintln!(
                "error[E511]: unsafe-output-path: candidate for {} names an unsafe path, \
                 skipped: {}",
                node_id, path
            );
            had_unsafe_path = true;
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
    had_unsafe_path
}

/// Handle the `synthesize` subcommand: the same front-half pipeline as
/// `compile` (parse, elaborate, plan, prompts, deterministic recipe
/// execution — writing the same four artifacts and every succeeded
/// deterministic candidate's files), PLUS running every generative plan
/// node through `runner::run_generative_nodes` with a live
/// `ClaudeSubprocessProvider`, writing `run-results.sexpr` (`(run-results
/// ((run-result ...) ...))`) and the files of every SUCCEEDED run
/// candidate through the same `write_candidate_files` E511 guard. Exit 1
/// if any compile-stage error exists OR any run result is `Exhausted`.
///
/// NOT exercised by CI or any test in this crate: it is the one path in
/// this codebase that invokes a live model subprocess.
fn cmd_synthesize(src: &str, file_path: &str, out_dir: &str, max_attempts: u32) {
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

    report_prompt_ir_slice_warnings(&ir, &p);

    let packages = prompt::compile_prompts(&ir, &p);
    let prompts_wrapper = Sexpr::list(vec![
        Sexpr::sym("prompts"),
        Sexpr::list(packages.iter().map(|pk| pk.to_sexpr()).collect()),
    ]);

    let results = recipe::execute_deterministic(&ir, &p);
    let results_wrapper = Sexpr::list(vec![
        Sexpr::sym("results"),
        Sexpr::list(results.iter().map(|r| r.to_sexpr()).collect()),
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
        if write_candidate_files(out_path, &result.node_id, candidate_sexpr) {
            execution_errors = true;
        }
    }

    // Never invoke the live model over a broken pipeline: parse,
    // elaboration, or planning errors end the run here (phase-5 gate,
    // finding 9) — spending model tokens on a spec that already failed
    // deterministic stages helps no one. The evidence bundle for this
    // path is honestly deterministic-only: the generative nodes never
    // ran and stay deferred in it.
    if parse_errors || ir.has_errors() || plan_has_errors {
        write_evidence_bundle(out_path, &ir, &p, &results);
        eprintln!("error: upstream errors present; skipping model synthesis");
        std::process::exit(1);
    }

    // The generative half: every generative plan node through the live
    // Claude subprocess, bounded repair per node.
    let mut provider = runner::ClaudeSubprocessProvider::new();
    let run_results = runner::run_generative_nodes(&ir, &p, &mut provider, max_attempts);
    let run_results_wrapper = Sexpr::list(vec![
        Sexpr::sym("run-results"),
        Sexpr::list(run_results.iter().map(|r| r.to_sexpr()).collect()),
    ]);
    write_artifact(
        out_path,
        "run-results.sexpr",
        &sexpr::canonical_serialize(&run_results_wrapper),
    );

    let mut run_exhausted = false;
    for result in &run_results {
        if result.status == runner::RunStatus::Exhausted {
            run_exhausted = true;
            eprintln!(
                "error: generative node {} exhausted its synthesis attempts",
                result.node_id
            );
            continue;
        }
        if let Some(candidate_sexpr) = result.candidate.clone() {
            if write_candidate_files(out_path, &result.node_id, candidate_sexpr) {
                execution_errors = true;
            }
        }
    }

    // The evidence bundle is assembled AFTER the generative half, over
    // the merged results, so the promotion decision sees every model
    // outcome (phase-8 gate, finding 1): an accepted run candidate
    // enters the artifact ledger with its digest; an exhausted node
    // becomes a Failed result with an error diagnostic, so the bundle
    // can never say `promote` while the process exits 1 on exhaustion.
    let merged = runner::merge_run_results(&results, &run_results);
    write_evidence_bundle(out_path, &ir, &p, &merged);

    std::process::exit(
        if parse_errors || ir.has_errors() || plan_has_errors || execution_errors || run_exhausted {
            1
        } else {
            0
        },
    );
}

/// Assembles the promotion evidence bundle over the given execution
/// results — `compile` passes the deterministic results; `synthesize`
/// passes the results MERGED with the generative run outcomes
/// (`runner::merge_run_results`), assembled after the model half so
/// the bundle is never blind to model outcomes (phase-8 gate, finding
/// 1) — plus the (deterministic) verification bundle, and
/// writes it as `evidence-bundle.sexpr` (phase-8 plan, section C):
/// one canonically printed two-element form,
/// `(assembly ((bundle <evidence-bundle>) (promotion <promotion-result>)))`,
/// trailing newline, byte-stable across compiles. Shared by
/// `cmd_compile` and `cmd_synthesize` so the two can never disagree
/// about the artifact's shape. A `hold` decision is evidence, not a
/// gate: it never affects the exit code — `compile` already fails on
/// failed recipes, and promotion is a judgment over the bundle, not a
/// stage of compilation.
fn write_evidence_bundle(
    out_path: &Path,
    ir: &gymnast_rs::ir::Ir,
    p: &plan::Plan,
    results: &[recipe::ExecutionResult],
) {
    let verification = verify::compile_verification(ir);
    let bundle = assembly::assemble_bundle(ir, p, results, Some(&verification));
    let promotion = assembly::evaluate_promotion(&assembly::default_promotion_policy(), &bundle);
    let wrapper = Sexpr::list(vec![
        Sexpr::sym("assembly"),
        Sexpr::list(vec![
            Sexpr::pair("bundle", bundle),
            Sexpr::pair("promotion", promotion),
        ]),
    ]);
    write_artifact(
        out_path,
        "evidence-bundle.sexpr",
        &sexpr::canonical_serialize(&wrapper),
    );
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
