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
