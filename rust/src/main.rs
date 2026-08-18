use std::env;
use std::fs;
use std::path::Path;

use gymnast_rs::check;
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

    // If parsing succeeded, run the checker
    if let Some(file) = ast {
        let check_diags = check::check(&file);
        diags.extend(check_diags);
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
    let (ast, mut diags) = parser::parse(src);

    // If parsing succeeded, elaborate
    if let Some(file) = ast {
        let ir = elaborate::elaborate(&file);

        // Render diagnostics to stderr
        if !ir.diagnostics.is_empty() {
            let check_diags = check::check(&file);
            diags.extend(check_diags);
            if !diags.is_empty() {
                let rendered = diag::render(&diags, src, file_path);
                eprint!("{}", rendered);
            }
        }

        // Print IR to stdout
        let serialized = sexpr::canonical_serialize(&ir.to_sexpr());
        print!("{}", serialized);

        // Exit with error if any diagnostic is an error
        let has_errors = ir.has_errors();
        std::process::exit(if has_errors { 1 } else { 0 });
    } else {
        // Render parse diagnostics
        if !diags.is_empty() {
            let rendered = diag::render(&diags, src, file_path);
            eprint!("{}", rendered);
        }
        std::process::exit(1);
    }
}
