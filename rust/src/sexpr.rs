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
}

/// Canonical serialization: print + one trailing newline (LF).
pub fn canonical_serialize(value: &Sexpr) -> String {
    format!("{}\n", value.print())
}
