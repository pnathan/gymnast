use std::env;
use std::fs;
use std::path::Path;

use gymnast_rs::diag::{self, Severity};
use gymnast_rs::elaborate;
use gymnast_rs::parser;
use gymnast_rs::sexpr;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 3 {
        eprintln!("usage: gymnast-rs <parse|check|ir> FILE.gym");
        std::process::exit(2);
    }

    let command = &args[1];
    let file_path = &args[2];

    let src = match fs::read_to_string(file_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("error: cannot read file: {}", e);
            std::process::exit(2);
        }
    };

    let file_name = Path::new(file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file.gym");

    match command.as_str() {
        "parse" => cmd_parse(&src, file_path, file_name),
        "check" => cmd_check(&src, file_path, file_name),
        "ir" => cmd_ir(&src, file_path, file_name),
        _ => {
            eprintln!("usage: gymnast-rs <parse|check|ir> FILE.gym");
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
