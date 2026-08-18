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

    for diag in sorted {
        let (line, col) = byte_offset_to_line_col(src, diag.span.start);

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
        let lines: Vec<&str> = src.lines().collect();
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

/// Convert a byte offset to 1-indexed line and column.
///
/// Line numbers start at 1. Column numbers start at 1.
fn byte_offset_to_line_col(src: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    let mut byte_pos = 0;

    for c in src.chars() {
        if byte_pos >= offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
        byte_pos += c.len_utf8();
    }

    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_col_start() {
        let src = "hello world";
        let (line, col) = byte_offset_to_line_col(src, 0);
        assert_eq!((line, col), (1, 1));
    }

    #[test]
    fn test_line_col_mid() {
        let src = "hello world";
        let (line, col) = byte_offset_to_line_col(src, 6);
        assert_eq!((line, col), (1, 7));
    }

    #[test]
    fn test_line_col_newline() {
        let src = "hello\nworld";
        let (line, col) = byte_offset_to_line_col(src, 6);
        assert_eq!((line, col), (2, 1));
    }
}
