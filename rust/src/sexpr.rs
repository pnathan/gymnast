/// A Lisp-shaped value: the canonical serialization boundary.
#[derive(Debug, Clone, PartialEq)]
pub enum Sexpr {
    Sym(String),
    Str(String),
    Int(i64),
    List(Vec<Sexpr>),
}

impl Sexpr {
    pub fn sym(s: &str) -> Sexpr {
        Sexpr::Sym(s.to_string())
    }

    pub fn list(items: Vec<Sexpr>) -> Sexpr {
        Sexpr::List(items)
    }

    /// (key value) pair used in field alists.
    pub fn pair(key: &str, value: Sexpr) -> Sexpr {
        Sexpr::List(vec![Sexpr::sym(key), value])
    }

    /// Canonical single-line printing:
    ///   Sym  -> bare text
    ///   Str  -> "..." with only \" and \\ escaped
    ///   Int  -> decimal
    ///   List -> ( item item ... ) with single spaces, no trailing space;
    ///           the empty list prints as nil
    pub fn print(&self) -> String {
        match self {
            Sexpr::Sym(s) => s.clone(),
            Sexpr::Str(s) => {
                let mut result = String::from("\"");
                for ch in s.chars() {
                    match ch {
                        '\\' => result.push_str("\\\\"),
                        '"' => result.push_str("\\\""),
                        _ => result.push(ch),
                    }
                }
                result.push('"');
                result
            }
            Sexpr::Int(i) => i.to_string(),
            Sexpr::List(items) => {
                if items.is_empty() {
                    "nil".to_string()
                } else {
                    let mut result = String::from("(");
                    for (i, item) in items.iter().enumerate() {
                        if i > 0 {
                            result.push(' ');
                        }
                        result.push_str(&item.print());
                    }
                    result.push(')');
                    result
                }
            }
        }
    }

    /// `Sym(s) => Some(s)`, `None` for any other variant.
    pub fn as_sym(&self) -> Option<&str> {
        match self {
            Sexpr::Sym(s) => Some(s),
            _ => None,
        }
    }

    /// `Str(s) => Some(s)`, `None` for any other variant.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Sexpr::Str(s) => Some(s),
            _ => None,
        }
    }

    /// `Int(i) => Some(i)`, `None` for any other variant.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Sexpr::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// `List(items) => Some(items)`, `None` for any other variant.
    pub fn as_list(&self) -> Option<&[Sexpr]> {
        match self {
            Sexpr::List(items) => Some(items),
            _ => None,
        }
    }

    /// Alist lookup over a `List` of `(key value)` pairs: returns the
    /// value of the first pair whose head symbol equals `key`. `None`
    /// when `self` is not a list, or no pair matches. Entries that are
    /// not two-element `(Sym, Sexpr)` pairs (e.g. a leading bare tag
    /// symbol) are skipped rather than treated as an error, so `assoc`
    /// is safe to call on a mixed list like `(diagnostic (severity s) ...)`.
    pub fn assoc(&self, key: &str) -> Option<&Sexpr> {
        let items = self.as_list()?;
        for item in items {
            if let Sexpr::List(pair) = item {
                if pair.len() == 2 {
                    if let Some(k) = pair[0].as_sym() {
                        if k == key {
                            return Some(&pair[1]);
                        }
                    }
                }
            }
        }
        None
    }
}

/// Canonical serialization: print + one trailing newline (LF).
pub fn canonical_serialize(value: &Sexpr) -> String {
    format!("{}\n", value.print())
}

/// Recursion/nesting-depth bound for `parse`: untrusted text (a model
/// candidate, or a corrupted cache/fixture file) must never be able to
/// blow the stack via deeply nested parens.
const MAX_PARSE_DEPTH: usize = 256;

/// The exact set of bytes that end a bare symbol/int token or open/close
/// a list: the ASCII parens, the ASCII string-quote, and ASCII
/// whitespace. Checked byte-wise (never via `char`/`is_whitespace` on a
/// casted byte) so a UTF-8 continuation byte (0x80..=0xBF) can never be
/// misread as a delimiter — none of these delimiter bytes fall in that
/// range, so slicing only ever lands on a valid `char` boundary of the
/// original `&str`.
fn is_delimiter_byte(b: u8) -> bool {
    matches!(
        b,
        b'(' | b')' | b'"' | b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C
    )
}

fn skip_ws(bytes: &[u8], pos: &mut usize) {
    while *pos < bytes.len() && matches!(bytes[*pos], b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C) {
        *pos += 1;
    }
}

/// Parse ONE S-expression from untrusted text. Total: never panics,
/// bounded recursion (nesting past `MAX_PARSE_DEPTH` errors out before
/// recursing further), rejects trailing non-whitespace after the one
/// value. Accepts a mild SUPERSET of the canonical printer's language
/// (leading zeros and `+` signs read as ints; adjacent tokens like
/// `(abc"def")` tokenize at the delimiter): symbols (any
/// run of bytes other than whitespace, parens, and `"`), `"..."` strings
/// with `\"` and `\\` escapes (an unrecognized escape keeps its
/// backslash, mirroring `lexer.rs`'s `lex_string`), decimal integers
/// (optional leading `-`), lists, and `nil` reading as the empty list.
///
/// Round-trip law: for every `Sexpr` value the COMPILER PIPELINE
/// produces, `parse(&v.print())` == `Ok(v)`. The public constructors can
/// build values outside that set with more exceptions: `Sym` text that
/// tokenizes as an int, contains whitespace/parens/quotes, or is empty
/// does not round-trip (unreachable from specs — the lexer's identifier
/// charset excludes all of these) — and `Sexpr::List(vec![])`, which
/// prints as `nil` and reads back as the
/// empty list (the two are equal as values, so this holds trivially: it
/// is called out here only because the printed bytes differ from a
/// literal `()`), and `Sexpr::Sym("nil")`, which is unrepresentable
/// round-trip (its print output `nil` always reads back as the empty
/// list instead) — do not construct that value.
pub fn parse(text: &str) -> Result<Sexpr, String> {
    let bytes = text.as_bytes();
    let mut pos = 0usize;
    skip_ws(bytes, &mut pos);
    if pos >= bytes.len() {
        return Err("unexpected end of input: expected an S-expression".to_string());
    }
    let value = parse_value(bytes, &mut pos, 0)?;
    skip_ws(bytes, &mut pos);
    if pos != bytes.len() {
        return Err(format!(
            "trailing non-whitespace content at byte offset {}",
            pos
        ));
    }
    Ok(value)
}

fn parse_value(bytes: &[u8], pos: &mut usize, depth: usize) -> Result<Sexpr, String> {
    // Boundary: the limit counts value-recursion frames, so 256 levels
    // of non-empty nesting parse and the 257th is rejected; an EMPTY
    // innermost list adds no child frame and buys exactly one more
    // level. Bounded (and panic-free) either way.
    if depth > MAX_PARSE_DEPTH {
        return Err(format!(
            "nesting depth exceeds limit of {}",
            MAX_PARSE_DEPTH
        ));
    }
    skip_ws(bytes, pos);
    if *pos >= bytes.len() {
        return Err("unexpected end of input: expected a value".to_string());
    }
    match bytes[*pos] {
        b'(' => parse_list(bytes, pos, depth),
        b')' => Err(format!("unexpected `)` at byte offset {}", *pos)),
        b'"' => parse_string(bytes, pos),
        _ => parse_atom(bytes, pos),
    }
}

fn parse_list(bytes: &[u8], pos: &mut usize, depth: usize) -> Result<Sexpr, String> {
    let start = *pos;
    *pos += 1; // consume '('
    let mut items = Vec::new();
    loop {
        skip_ws(bytes, pos);
        if *pos >= bytes.len() {
            return Err(format!(
                "unterminated list starting at byte offset {}: missing `)`",
                start
            ));
        }
        if bytes[*pos] == b')' {
            *pos += 1;
            return Ok(Sexpr::List(items));
        }
        // Every iteration consumes at least one byte via parse_value
        // (an atom/string/list all advance `pos`), so this loop always
        // makes progress toward either `)` or end-of-input.
        let value = parse_value(bytes, pos, depth + 1)?;
        items.push(value);
    }
}

fn parse_string(bytes: &[u8], pos: &mut usize) -> Result<Sexpr, String> {
    let start = *pos;
    *pos += 1; // consume opening '"'
               // Collect raw bytes and decode once at the end, mirroring
               // `lexer.rs::lex_string`: the delimiters ('"', '\\') are ASCII, so
               // slicing at them keeps the collected bytes valid UTF-8 whenever the
               // input is.
    let mut result: Vec<u8> = Vec::new();
    loop {
        if *pos >= bytes.len() {
            return Err(format!(
                "unterminated string literal starting at byte offset {}",
                start
            ));
        }
        let byte = bytes[*pos];
        if byte == b'"' {
            *pos += 1;
            return Ok(Sexpr::Str(String::from_utf8_lossy(&result).into_owned()));
        } else if byte == b'\\' {
            *pos += 1;
            if *pos >= bytes.len() {
                return Err(format!(
                    "unterminated string literal starting at byte offset {}",
                    start
                ));
            }
            let escaped = bytes[*pos];
            match escaped {
                b'"' => result.push(b'"'),
                b'\\' => result.push(b'\\'),
                // Unknown escape: keep the backslash, mirror the lexer.
                _ => {
                    result.push(b'\\');
                    result.push(escaped);
                }
            }
            *pos += 1;
        } else {
            result.push(byte);
            *pos += 1;
        }
    }
}

fn parse_atom(bytes: &[u8], pos: &mut usize) -> Result<Sexpr, String> {
    let start = *pos;
    while *pos < bytes.len() && !is_delimiter_byte(bytes[*pos]) {
        *pos += 1;
    }
    if *pos == start {
        // Defensive only: `parse_value` already dispatched away from
        // '(', ')', '"', and whitespace before calling here, so an empty
        // token cannot actually occur. No panic either way.
        return Err(format!("unexpected character at byte offset {}", start));
    }
    // Safe: `start` and `*pos` only ever land immediately after
    // whitespace/parens/quote or at the original string bounds, all of
    // which are ASCII byte positions and therefore valid `char`
    // boundaries of the source `&str`.
    let text = match std::str::from_utf8(&bytes[start..*pos]) {
        Ok(t) => t,
        Err(_) => return Err(format!("invalid UTF-8 in token at byte offset {}", start)),
    };
    if text == "nil" {
        return Ok(Sexpr::List(vec![]));
    }
    if let Ok(i) = text.parse::<i64>() {
        return Ok(Sexpr::Int(i));
    }
    Ok(Sexpr::Sym(text.to_string()))
}
