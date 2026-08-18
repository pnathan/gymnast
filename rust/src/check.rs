use crate::ast::*;
use crate::diag::{Diagnostic, Severity};
use crate::span::Span;
use std::collections::HashMap;

/// Built-in mode names available in every spec.
const BUILTIN_MODES: &[&str] = &[
    "text",
    "int",
    "bool",
    "local_date",
    "zoned_datetime",
    "void",
];

/// A symbol table organizing declarations by namespace.
struct SymbolTable {
    modes: HashMap<String, Ident>,
    actors: HashMap<String, Ident>,
    interfaces: HashMap<String, Ident>,
    interface_ops: HashMap<String, Vec<String>>, // interface_name -> list of op names
    states: HashMap<String, Ident>,
    components: HashMap<String, Ident>,
    behaviors: HashMap<String, Ident>,
    flows: HashMap<String, Ident>,
    has_use_decl: bool,
}

impl SymbolTable {
    fn new() -> Self {
        let mut modes = HashMap::new();
        for &builtin_mode in BUILTIN_MODES {
            modes.insert(
                builtin_mode.to_string(),
                Ident {
                    text: builtin_mode.to_string(),
                    span: Span { start: 0, end: 0 },
                },
            );
        }
        SymbolTable {
            modes,
            actors: HashMap::new(),
            interfaces: HashMap::new(),
            interface_ops: HashMap::new(),
            states: HashMap::new(),
            components: HashMap::new(),
            behaviors: HashMap::new(),
            flows: HashMap::new(),
            has_use_decl: false,
        }
    }
}

/// Check a parsed file for semantic errors.
pub fn check(file: &File) -> Vec<Diagnostic> {
    let mut checker = Checker::new();
    checker.check_file(file)
}

struct Checker {
    symtab: SymbolTable,
    diags: Vec<Diagnostic>,
}

impl Checker {
    fn new() -> Self {
        Checker {
            symtab: SymbolTable::new(),
            diags: Vec::new(),
        }
    }

    fn check_file(&mut self, file: &File) -> Vec<Diagnostic> {
        // First pass: collect all declarations and check for duplicates (E201)
        // Also check capitalization (W302)
        for decl in &file.decls {
            match decl {
                Decl::Use(_) => {
                    self.symtab.has_use_decl = true;
                }
                Decl::Mode(m) => {
                    self.check_capitalization_mode(&m.name);
                    self.add_mode(&m.name);
                }
                Decl::Actor(a) => {
                    self.check_capitalization_non_mode(&a.name);
                    self.add_actor(&a.name);
                }
                Decl::Interface(i) => {
                    self.check_capitalization_non_mode(&i.name);
                    self.add_interface(&i.name);
                    for op in &i.ops {
                        self.add_interface_op(&i.name, &op.name);
                    }
                }
                Decl::State(s) => {
                    self.check_capitalization_non_mode(&s.name);
                    self.add_state(&s.name);
                }
                Decl::Component(c) => {
                    self.check_capitalization_non_mode(&c.name);
                    self.add_component(&c.name);
                }
                Decl::Behavior(b) => {
                    self.check_capitalization_non_mode(&b.name);
                    self.add_behavior(&b.name);
                }
                Decl::Flow(f) => {
                    self.check_capitalization_non_mode(&f.name);
                    self.add_flow(&f.name);
                }
                Decl::Application(_)
                | Decl::Invariant(_)
                | Decl::Constraint(_)
                | Decl::Synthesis(_)
                | Decl::Acceptance(_) => {}
            }
        }

        // Second pass: check references (E202-E206)
        for decl in &file.decls {
            match decl {
                Decl::Mode(m) => {
                    self.check_mode_expr(&m.expr);
                }
                Decl::Interface(i) => {
                    self.check_interface_decl(i);
                }
                Decl::Behavior(b) => {
                    self.check_behavior_decl(b);
                }
                Decl::Invariant(inv) => {
                    self.check_invariant_decl(inv);
                }
                Decl::Constraint(con) => {
                    self.check_constraint_decl(con);
                }
                _ => {}
            }
        }

        // Third pass: check exports (E206)
        self.check_exports(&file.spec.exports);

        self.diags.clone()
    }

    fn add_mode(&mut self, name: &Ident) {
        if let Some(existing) = self.symtab.modes.get(&name.text) {
            self.diag(
                Severity::Error,
                "E201",
                name.span,
                format!(
                    "duplicate mode name '{}' (previously declared at {}:{})",
                    name.text, existing.span.start, existing.span.end
                ),
            );
        } else {
            self.symtab.modes.insert(name.text.clone(), name.clone());
        }
    }

    fn add_actor(&mut self, name: &Ident) {
        if let Some(existing) = self.symtab.actors.get(&name.text) {
            self.diag(
                Severity::Error,
                "E201",
                name.span,
                format!(
                    "duplicate actor name '{}' (previously declared at {}:{})",
                    name.text, existing.span.start, existing.span.end
                ),
            );
        } else {
            self.symtab.actors.insert(name.text.clone(), name.clone());
        }
    }

    fn add_interface(&mut self, name: &Ident) {
        if let Some(existing) = self.symtab.interfaces.get(&name.text) {
            self.diag(
                Severity::Error,
                "E201",
                name.span,
                format!(
                    "duplicate interface name '{}' (previously declared at {}:{})",
                    name.text, existing.span.start, existing.span.end
                ),
            );
        } else {
            self.symtab
                .interfaces
                .insert(name.text.clone(), name.clone());
        }
    }

    fn add_interface_op(&mut self, iface_name: &Ident, op_name: &Ident) {
        let ops = self
            .symtab
            .interface_ops
            .entry(iface_name.text.clone())
            .or_insert_with(Vec::new);

        // Check for duplicates
        if ops.contains(&op_name.text) {
            // Find the existing op to report its location
            // For E201, we need the previous declaration location
            self.diag(
                Severity::Error,
                "E201",
                op_name.span,
                format!(
                    "duplicate operation name '{}' in interface '{}'",
                    op_name.text, iface_name.text
                ),
            );
        } else {
            ops.push(op_name.text.clone());
        }
    }

    fn add_state(&mut self, name: &Ident) {
        if let Some(existing) = self.symtab.states.get(&name.text) {
            self.diag(
                Severity::Error,
                "E201",
                name.span,
                format!(
                    "duplicate state name '{}' (previously declared at {}:{})",
                    name.text, existing.span.start, existing.span.end
                ),
            );
        } else {
            self.symtab.states.insert(name.text.clone(), name.clone());
        }
    }

    fn add_component(&mut self, name: &Ident) {
        if let Some(existing) = self.symtab.components.get(&name.text) {
            self.diag(
                Severity::Error,
                "E201",
                name.span,
                format!(
                    "duplicate component name '{}' (previously declared at {}:{})",
                    name.text, existing.span.start, existing.span.end
                ),
            );
        } else {
            self.symtab
                .components
                .insert(name.text.clone(), name.clone());
        }
    }

    fn add_behavior(&mut self, name: &Ident) {
        if let Some(existing) = self.symtab.behaviors.get(&name.text) {
            self.diag(
                Severity::Error,
                "E201",
                name.span,
                format!(
                    "duplicate behavior name '{}' (previously declared at {}:{})",
                    name.text, existing.span.start, existing.span.end
                ),
            );
        } else {
            self.symtab
                .behaviors
                .insert(name.text.clone(), name.clone());
        }
    }

    fn add_flow(&mut self, name: &Ident) {
        if let Some(existing) = self.symtab.flows.get(&name.text) {
            self.diag(
                Severity::Error,
                "E201",
                name.span,
                format!(
                    "duplicate flow name '{}' (previously declared at {}:{})",
                    name.text, existing.span.start, existing.span.end
                ),
            );
        } else {
            self.symtab.flows.insert(name.text.clone(), name.clone());
        }
    }

    fn check_mode_expr(&mut self, expr: &ModeExpr) {
        match expr {
            ModeExpr::Opaque(inner) => self.check_mode_expr(inner),
            ModeExpr::Enum(_) => {}
            ModeExpr::Union(variants) => {
                for (_, mode) in variants {
                    self.check_mode_expr(mode);
                }
            }
            ModeExpr::Struct(fields) => {
                for field in fields {
                    self.check_mode_expr(&field.mode);
                }
            }
            ModeExpr::Opt(inner) => self.check_mode_expr(inner),
            ModeExpr::Row(inner) => self.check_mode_expr(inner),
            ModeExpr::Named { name, args } => {
                self.check_mode_ref(name);
                for arg in args {
                    self.check_mode_expr(arg);
                }
            }
            ModeExpr::Refined { name, .. } => {
                self.check_mode_ref(name);
            }
        }
    }

    fn check_mode_ref(&mut self, name: &Ident) {
        if !self.symtab.modes.contains_key(&name.text) {
            self.check_unknown_mode(name);
        }
    }

    fn check_unknown_mode(&mut self, name: &Ident) {
        let severity = if self.symtab.has_use_decl {
            Severity::Warning
        } else {
            Severity::Error
        };

        let code = if self.symtab.has_use_decl {
            "W301"
        } else {
            "E202"
        };

        let mut message = format!("unknown mode '{}'", name.text);

        // Find nearest declared mode by edit distance <= 2
        if let Some(suggestion) = self.find_nearest_mode(&name.text) {
            message.push_str(&format!("; did you mean '{}'?", suggestion));
        }

        self.diag(severity, code, name.span, message);
    }

    fn find_nearest_mode(&self, name: &str) -> Option<String> {
        let mut candidates: Vec<_> = self
            .symtab
            .modes
            .keys()
            .filter_map(|mode| {
                let dist = edit_distance(name, mode);
                if dist <= 2 {
                    Some((dist, mode.clone()))
                } else {
                    None
                }
            })
            .collect();

        if candidates.is_empty() {
            return None;
        }

        candidates.sort_by_key(|(dist, _)| *dist);
        Some(candidates[0].1.clone())
    }

    fn check_interface_decl(&mut self, iface: &InterfaceDecl) {
        // E203: default_actor must be declared
        if !self.symtab.actors.contains_key(&iface.default_actor.text) {
            self.diag(
                Severity::Error,
                "E203",
                iface.default_actor.span,
                format!(
                    "interface '{}' references unknown actor '{}'",
                    iface.name.text, iface.default_actor.text
                ),
            );
        }

        // Check op params and outputs
        for op in &iface.ops {
            for param in &op.params {
                self.check_mode_expr(&param.mode);
            }
            self.check_mode_expr(&op.output);
        }
    }

    fn check_behavior_decl(&mut self, beh: &BehaviorDecl) {
        // E204: on_interface must be declared
        if !self.symtab.interfaces.contains_key(&beh.on_interface.text) {
            self.diag(
                Severity::Error,
                "E204",
                beh.on_interface.span,
                format!(
                    "behavior '{}' references unknown interface '{}'",
                    beh.name.text, beh.on_interface.text
                ),
            );
            return; // Can't check op if interface doesn't exist
        }

        // E204: on_op must be declared in the interface
        let has_op = self
            .symtab
            .interface_ops
            .get(&beh.on_interface.text)
            .map(|ops| ops.contains(&beh.on_op.text))
            .unwrap_or(false);

        if !has_op {
            self.diag(
                Severity::Error,
                "E204",
                beh.on_op.span,
                format!(
                    "interface '{}' has no operation '{}'",
                    beh.on_interface.text, beh.on_op.text
                ),
            );
        }
    }

    fn check_invariant_decl(&mut self, inv: &InvariantDecl) {
        // E205: scope must be a declared state, interface, or component
        if !self.symtab.states.contains_key(&inv.scope.text)
            && !self.symtab.interfaces.contains_key(&inv.scope.text)
            && !self.symtab.components.contains_key(&inv.scope.text)
        {
            self.diag(
                Severity::Error,
                "E205",
                inv.scope.span,
                format!(
                    "invariant '{}' references unknown scope '{}' (must be a state, interface, or component)",
                    inv.name.text, inv.scope.text
                ),
            );
        }
    }

    fn check_constraint_decl(&mut self, con: &ConstraintDecl) {
        // E205: scope must be a declared state, interface, or component
        if !self.symtab.states.contains_key(&con.scope.text)
            && !self.symtab.interfaces.contains_key(&con.scope.text)
            && !self.symtab.components.contains_key(&con.scope.text)
        {
            self.diag(
                Severity::Error,
                "E205",
                con.scope.span,
                format!(
                    "constraint '{}' references unknown scope '{}' (must be a state, interface, or component)",
                    con.name.text, con.scope.text
                ),
            );
        }
    }

    fn check_exports(&mut self, exports: &[Ident]) {
        for export in exports {
            // Check if the exported name is declared anywhere
            let is_declared = self.symtab.modes.contains_key(&export.text)
                || self.symtab.actors.contains_key(&export.text)
                || self.symtab.interfaces.contains_key(&export.text)
                || self.symtab.states.contains_key(&export.text)
                || self.symtab.components.contains_key(&export.text)
                || self.symtab.behaviors.contains_key(&export.text)
                || self.symtab.flows.contains_key(&export.text);

            if !is_declared {
                let severity = if self.symtab.has_use_decl {
                    Severity::Warning
                } else {
                    Severity::Error
                };

                let code = if self.symtab.has_use_decl {
                    "W301"
                } else {
                    "E206"
                };

                let mut message = format!("exported name '{}' is not declared", export.text);

                // Find nearest declared name by edit distance <= 2
                if let Some(suggestion) = self.find_nearest_decl(&export.text) {
                    message.push_str(&format!("; did you mean '{}'?", suggestion));
                }

                self.diag(severity, code, export.span, message);
            }
        }
    }

    fn find_nearest_decl(&self, name: &str) -> Option<String> {
        let all_decls = std::iter::empty()
            .chain(self.symtab.modes.keys())
            .chain(self.symtab.actors.keys())
            .chain(self.symtab.interfaces.keys())
            .chain(self.symtab.states.keys())
            .chain(self.symtab.components.keys())
            .chain(self.symtab.behaviors.keys())
            .chain(self.symtab.flows.keys());

        let mut candidates: Vec<_> = all_decls
            .filter_map(|decl_name| {
                let dist = edit_distance(name, decl_name);
                if dist <= 2 {
                    Some((dist, decl_name.clone()))
                } else {
                    None
                }
            })
            .collect();

        if candidates.is_empty() {
            return None;
        }

        candidates.sort_by_key(|(dist, _)| *dist);
        Some(candidates[0].1.clone())
    }

    fn check_capitalization_mode(&mut self, name: &Ident) {
        // W302: mode names should be capitalized
        if !name.text.is_empty() && !name.text[0..1].chars().next().unwrap().is_uppercase() {
            self.diag(
                Severity::Warning,
                "W302",
                name.span,
                format!(
                    "mode name '{}' should be capitalized (like 'Task')",
                    name.text
                ),
            );
        }
    }

    fn check_capitalization_non_mode(&mut self, name: &Ident) {
        // W302: non-mode declaration names should not be capitalized
        if !name.text.is_empty() && name.text[0..1].chars().next().unwrap().is_uppercase() {
            self.diag(
                Severity::Warning,
                "W302",
                name.span,
                format!(
                    "non-mode declaration '{}' should not be capitalized (like '{}')",
                    name.text,
                    name.text[0..1].to_lowercase() + &name.text[1..]
                ),
            );
        }
    }

    fn diag(&mut self, severity: Severity, code: &'static str, span: Span, message: String) {
        self.diags.push(Diagnostic {
            severity,
            code,
            span,
            message,
        });
    }
}

/// Compute Levenshtein edit distance between two strings.
fn edit_distance(s1: &str, s2: &str) -> usize {
    let s1_chars: Vec<char> = s1.chars().collect();
    let s2_chars: Vec<char> = s2.chars().collect();
    let (len1, len2) = (s1_chars.len(), s2_chars.len());

    if len1 == 0 {
        return len2;
    }
    if len2 == 0 {
        return len1;
    }

    let mut dp = vec![vec![0; len2 + 1]; len1 + 1];

    for i in 0..=len1 {
        dp[i][0] = i;
    }
    for j in 0..=len2 {
        dp[0][j] = j;
    }

    for i in 1..=len1 {
        for j in 1..=len2 {
            let cost = if s1_chars[i - 1] == s2_chars[j - 1] {
                0
            } else {
                1
            };
            dp[i][j] = std::cmp::min(
                std::cmp::min(dp[i - 1][j] + 1, dp[i][j - 1] + 1),
                dp[i - 1][j - 1] + cost,
            );
        }
    }

    dp[len1][len2]
}
