use crate::span::Span;

/// Diagnostic severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Error-level diagnostic.
    Error,
    /// Warning-level diagnostic.
    Warning,
}

/// A diagnostic message with source location.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// Severity level.
    pub severity: Severity,
    /// Error code (e.g., "E101", "W301").
    pub code: &'static str,
    /// Byte span in source.
    pub span: Span,
    /// Diagnostic message.
    pub message: String,
}

/// Render diagnostics with source context.
///
/// Formats each diagnostic with file, line, column, and a caret showing the error location.
/// Diagnostics are sorted by span start before rendering.
pub fn render(diags: &[Diagnostic], src: &str, path: &str) -> String {
    let mut sorted = diags.to_vec();
    sorted.sort_by_key(|d| d.span.start);

    let mut output = String::new();

    // Precompute the line index once: rendering must stay linear in
    // (source size + diagnostic count), not their product — a large
    // malformed file can carry tens of thousands of diagnostics.
    let lines: Vec<&str> = src.lines().collect();
    let mut line_starts: Vec<usize> = vec![0];
    for (i, b) in src.bytes().enumerate() {
        if b == b'\n' {
            line_starts.push(i + 1);
        }
    }

    for diag in sorted {
        let line_idx = match line_starts.binary_search(&diag.span.start) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let line = line_idx + 1;
        let col = diag.span.start - line_starts[line_idx] + 1;

        let severity_str = match diag.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };

        output.push_str(&format!(
            "{}[{}]: {}\n",
            severity_str, diag.code, diag.message
        ));
        output.push_str(&format!("  --> {}:{}:{}\n", path, line, col));
        output.push_str("   |\n");

        // Find the source line
        if line > 0 && line <= lines.len() {
            let source_line = lines[line - 1];
            output.push_str(&format!("{:>2} | {}\n", line, source_line));
        }

        output.push_str("   | ");
        output.push_str(&" ".repeat(col.saturating_sub(1)));
        output.push_str("^\n");
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;

    fn diag_at(start: usize) -> Diagnostic {
        Diagnostic {
            severity: Severity::Error,
            code: "E101",
            span: Span {
                start,
                end: start + 1,
            },
            message: "test".to_string(),
        }
    }

    #[test]
    fn test_render_line_col_start() {
        let out = render(&[diag_at(0)], "hello world", "f.gym");
        assert!(out.contains("f.gym:1:1"), "{}", out);
    }

    #[test]
    fn test_render_line_col_mid() {
        let out = render(&[diag_at(6)], "hello world", "f.gym");
        assert!(out.contains("f.gym:1:7"), "{}", out);
    }

    #[test]
    fn test_render_line_col_newline() {
        let out = render(&[diag_at(6)], "hello\nworld", "f.gym");
        assert!(out.contains("f.gym:2:1"), "{}", out);
    }
}
