use crate::ast::*;
use crate::diag::{Diagnostic, Severity};
use crate::lexer::{Lexer, Token, TokenKind};
use crate::span::Span;

/// Parser for the `.gym` surface language.
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    diags: Vec<Diagnostic>,
}

impl Parser {
    /// Create a new parser.
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser {
            tokens,
            pos: 0,
            diags: Vec::new(),
        }
    }

    /// Get current token without advancing.
    fn peek(&self) -> &Token {
        if self.pos < self.tokens.len() {
            &self.tokens[self.pos]
        } else {
            // Return a synthetic EOF at the end
            &self.tokens[self.tokens.len() - 1]
        }
    }

    /// Get current token kind.
    fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    /// Advance to next token, returning the current one.
    fn advance(&mut self) -> Token {
        let token = self.peek().clone();
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        token
    }

    /// Compare two TokenKind patterns.
    fn match_token(&self, actual: &TokenKind, expected: &TokenKind) -> bool {
        match (actual, expected) {
            (TokenKind::Ident(a), TokenKind::Ident(b)) => a == b,
            (a, b) => std::mem::discriminant(a) == std::mem::discriminant(b),
        }
    }

    /// Skip to next top-level declaration keyword at paren depth 0 for error recovery.
    fn skip_to_next_decl(&mut self, paren_depth: i32) {
        while self.pos < self.tokens.len() && self.peek_kind() != &TokenKind::Eof {
            match self.peek_kind() {
                TokenKind::LParen => {
                    self.advance();
                    if paren_depth == 0 {
                        self.skip_to_next_decl(1);
                    }
                }
                TokenKind::RParen => {
                    self.advance();
                    if paren_depth == 1 {
                        return;
                    }
                }
                TokenKind::Ident(name) if paren_depth == 0 => {
                    if matches!(
                        name.as_str(),
                        "use"
                            | "application"
                            | "actor"
                            | "mode"
                            | "component"
                            | "interface"
                            | "state"
                            | "flow"
                            | "behavior"
                            | "inv"
                            | "constraint"
                            | "synthesis"
                            | "acceptance"
                    ) {
                        return;
                    }
                    self.advance();
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    /// Get paren depth of current position by looking backwards.
    fn paren_depth(&self) -> i32 {
        let mut depth = 0;
        for i in 0..self.pos {
            match &self.tokens[i].kind {
                TokenKind::LParen => depth += 1,
                TokenKind::RParen => depth -= 1,
                _ => {}
            }
        }
        depth
    }

    /// Expect a specific token or emit an error.
    fn expect(&mut self, expected_kind: TokenKind, msg: &str) -> Option<Token> {
        if self.match_token(self.peek_kind(), &expected_kind) {
            Some(self.advance())
        } else {
            let span = self.peek().span;
            self.diags.push(Diagnostic {
                severity: Severity::Error,
                code: "E101",
                span,
                message: format!(
                    "unexpected token `{}`, expected {}",
                    self.token_display(),
                    msg
                ),
            });
            None
        }
    }

    /// Get a display string for the current token.
    fn token_display(&self) -> String {
        match self.peek_kind() {
            TokenKind::Ident(s) => s.clone(),
            TokenKind::Int(i) => i.to_string(),
            TokenKind::Str(s) => format!("\"{}\"", s),
            TokenKind::LParen => "(".to_string(),
            TokenKind::RParen => ")".to_string(),
            TokenKind::Comma => ",".to_string(),
            TokenKind::Semi => ";".to_string(),
            TokenKind::Colon => ":".to_string(),
            TokenKind::Eq => "=".to_string(),
            TokenKind::Bang => "!".to_string(),
            TokenKind::Dot => ".".to_string(),
            TokenKind::At => "@".to_string(),
            TokenKind::Slash => "/".to_string(),
            TokenKind::Lt => "<".to_string(),
            TokenKind::Le => "<=".to_string(),
            TokenKind::Arrow => "->".to_string(),
            TokenKind::DotDot => "..".to_string(),
            TokenKind::Eof => "EOF".to_string(),
        }
    }

    /// Check if current token is an identifier with specific text.
    fn check_ident(&self, name: &str) -> bool {
        if let TokenKind::Ident(s) = self.peek_kind() {
            s == name
        } else {
            false
        }
    }

    /// Parse a file starting with spec declaration.
    pub fn parse_file(&mut self) -> Option<File> {
        if self.peek_kind() == &TokenKind::Eof {
            let span = Span { start: 0, end: 0 };
            self.diags.push(Diagnostic {
                severity: Severity::Error,
                code: "E101",
                span,
                message: "expected spec declaration".to_string(),
            });
            return None;
        }

        let spec = self.parse_spec()?;
        let mut decls = Vec::new();

        while self.peek_kind() != &TokenKind::Eof {
            match self.parse_decl() {
                Some(decl) => decls.push(decl),
                None => {
                    let paren_depth = self.paren_depth();
                    self.skip_to_next_decl(paren_depth);
                }
            }
        }

        Some(File { spec, decls })
    }

    /// Parse spec declaration: `spec name = v version owner exports name, name, ...`
    pub fn parse_spec(&mut self) -> Option<SpecDecl> {
        let start_span = self.peek().span;
        if !self.check_ident("spec") {
            self.diags.push(Diagnostic {
                severity: Severity::Error,
                code: "E101",
                span: start_span,
                message: "expected `spec` declaration".to_string(),
            });
            return None;
        }
        self.advance();

        let name = self.parse_ident()?;

        self.expect(TokenKind::Eq, "`=`")?;

        // Parse version: `v 0.1`
        if !self.check_ident("v") {
            self.diags.push(Diagnostic {
                severity: Severity::Error,
                code: "E101",
                span: self.peek().span,
                message: "expected `v` for version".to_string(),
            });
            return None;
        }
        self.advance();

        let version = self.parse_version()?;

        // Parse 'owner' keyword
        if !self.check_ident("owner") {
            self.diags.push(Diagnostic {
                severity: Severity::Error,
                code: "E101",
                span: self.peek().span,
                message: "expected `owner` keyword".to_string(),
            });
            return None;
        }
        self.advance();

        let owner = self.parse_ident()?;

        // Parse 'exports' keyword
        if !self.check_ident("exports") {
            self.diags.push(Diagnostic {
                severity: Severity::Error,
                code: "E101",
                span: self.peek().span,
                message: "expected `exports` keyword".to_string(),
            });
            return None;
        }
        self.advance();

        // Parse export names
        let mut exports = vec![self.parse_ident()?];
        while self.peek_kind() == &TokenKind::Comma {
            self.advance();
            if let Some(export) = self.parse_ident() {
                exports.push(export);
            } else {
                break;
            }
        }

        let end_span = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span)
            .unwrap_or(start_span);
        Some(SpecDecl {
            name,
            version,
            owner,
            exports,
            span: start_span.join(end_span),
        })
    }

    /// Parse version string from tokens: Int Dot Int
    fn parse_version(&mut self) -> Option<String> {
        if let TokenKind::Int(major) = self.peek_kind() {
            let major_val = *major;
            self.advance();

            if self.peek_kind() != &TokenKind::Dot {
                self.diags.push(Diagnostic {
                    severity: Severity::Error,
                    code: "E103",
                    span: self.peek().span,
                    message: "expected `.` in version".to_string(),
                });
                return None;
            }
            self.advance();

            if let TokenKind::Int(minor) = self.peek_kind() {
                let minor_val = *minor;
                self.advance();
                Some(format!("{}.{}", major_val, minor_val))
            } else {
                self.diags.push(Diagnostic {
                    severity: Severity::Error,
                    code: "E103",
                    span: self.peek().span,
                    message: "expected minor version number".to_string(),
                });
                None
            }
        } else {
            self.diags.push(Diagnostic {
                severity: Severity::Error,
                code: "E103",
                span: self.peek().span,
                message: "expected major version number".to_string(),
            });
            None
        }
    }

    /// Parse a declaration based on keyword.
    pub fn parse_decl(&mut self) -> Option<Decl> {
        match self.peek_kind() {
            TokenKind::Ident(name) => match name.as_str() {
                "use" => self.parse_use().map(Decl::Use),
                "application" => {
                    let start_span = self.peek().span;
                    self.advance();
                    let name = self.parse_ident()?;
                    self.expect(TokenKind::Eq, "`=`")?;
                    let attrs = self.parse_pack()?;
                    let end_span = self
                        .tokens
                        .get(self.pos.saturating_sub(1))
                        .map(|t| t.span)
                        .unwrap_or(start_span);
                    Some(Decl::Application(ApplicationDecl {
                        name,
                        attrs,
                        span: start_span.join(end_span),
                    }))
                }
                "component" => {
                    let start_span = self.peek().span;
                    self.advance();
                    let name = self.parse_ident()?;
                    self.expect(TokenKind::Eq, "`=`")?;
                    let attrs = self.parse_pack()?;
                    let end_span = self
                        .tokens
                        .get(self.pos.saturating_sub(1))
                        .map(|t| t.span)
                        .unwrap_or(start_span);
                    Some(Decl::Component(ComponentDecl {
                        name,
                        attrs,
                        span: start_span.join(end_span),
                    }))
                }
                "state" => {
                    let start_span = self.peek().span;
                    self.advance();
                    let name = self.parse_ident()?;
                    self.expect(TokenKind::Eq, "`=`")?;
                    let attrs = self.parse_pack()?;
                    let end_span = self
                        .tokens
                        .get(self.pos.saturating_sub(1))
                        .map(|t| t.span)
                        .unwrap_or(start_span);
                    Some(Decl::State(StateDecl {
                        name,
                        attrs,
                        span: start_span.join(end_span),
                    }))
                }
                "actor" => self.parse_actor().map(Decl::Actor),
                "mode" => self.parse_mode_decl().map(Decl::Mode),
                "interface" => self.parse_interface().map(Decl::Interface),
                "flow" => self.parse_flow().map(Decl::Flow),
                "behavior" => self.parse_behavior().map(Decl::Behavior),
                "inv" => self.parse_invariant().map(Decl::Invariant),
                "constraint" => self.parse_constraint().map(Decl::Constraint),
                "synthesis" => self.parse_synthesis().map(Decl::Synthesis),
                "acceptance" => self.parse_acceptance().map(Decl::Acceptance),
                _ => {
                    let span = self.peek().span;
                    self.diags.push(Diagnostic {
                        severity: Severity::Error,
                        code: "E104",
                        span,
                        message: format!("unknown declaration keyword `{}`", name),
                    });
                    None
                }
            },
            _ => {
                let span = self.peek().span;
                self.diags.push(Diagnostic {
                    severity: Severity::Error,
                    code: "E101",
                    span,
                    message: "expected declaration".to_string(),
                });
                None
            }
        }
    }

    /// Parse identifier from Ident token.
    fn parse_ident(&mut self) -> Option<Ident> {
        match self.peek_kind().clone() {
            TokenKind::Ident(text) => {
                let span = self.peek().span;
                self.advance();
                Some(Ident { text, span })
            }
            _ => {
                let span = self.peek().span;
                self.diags.push(Diagnostic {
                    severity: Severity::Error,
                    code: "E101",
                    span,
                    message: format!("expected identifier, got `{}`", self.token_display()),
                });
                None
            }
        }
    }

    /// Parse use declaration: `use path @ version (args...)`
    pub fn parse_use(&mut self) -> Option<UseDecl> {
        let start_span = self.peek().span;
        self.advance(); // consume 'use'

        // Parse path with slashes
        let mut path = vec![self.parse_ident()?];
        while self.peek_kind() == &TokenKind::Slash {
            self.advance();
            path.push(self.parse_ident()?);
        }

        self.expect(TokenKind::At, "`@`")?;

        let version = self.parse_version()?;

        let args = if self.peek_kind() == &TokenKind::LParen {
            self.parse_pack()?
        } else {
            Vec::new()
        };

        let end_span = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span)
            .unwrap_or(start_span);
        Some(UseDecl {
            path,
            version,
            args,
            span: start_span.join(end_span),
        })
    }

    /// Parse actor declaration: `actor name = kind (attrs...)`
    pub fn parse_actor(&mut self) -> Option<ActorDecl> {
        let start_span = self.peek().span;
        self.advance(); // consume 'actor'

        let name = self.parse_ident()?;

        self.expect(TokenKind::Eq, "`=`")?;

        let kind = self.parse_ident()?;

        let attrs = if self.peek_kind() == &TokenKind::LParen {
            self.parse_pack()?
        } else {
            Vec::new()
        };

        let end_span = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span)
            .unwrap_or(start_span);
        Some(ActorDecl {
            name,
            kind,
            attrs,
            span: start_span.join(end_span),
        })
    }

    /// Parse mode declaration: `mode name = expr`
    pub fn parse_mode_decl(&mut self) -> Option<ModeDecl> {
        let start_span = self.peek().span;
        self.advance(); // consume 'mode'

        let name = self.parse_ident()?;

        self.expect(TokenKind::Eq, "`=`")?;

        let expr = self.parse_mode_expr()?;

        let end_span = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span)
            .unwrap_or(start_span);
        Some(ModeDecl {
            name,
            expr,
            span: start_span.join(end_span),
        })
    }

    /// Parse mode expression.
    pub fn parse_mode_expr(&mut self) -> Option<ModeExpr> {
        if self.check_ident("opaque") {
            self.advance();
            let inner = self.parse_mode_expr()?;
            Some(ModeExpr::Opaque(Box::new(inner)))
        } else if self.check_ident("opt") {
            self.advance();
            let inner = self.parse_mode_expr()?;
            Some(ModeExpr::Opt(Box::new(inner)))
        } else if self.check_ident("enum") {
            self.advance();
            self.expect(TokenKind::LParen, "`(`")?;
            let mut variants = Vec::new();
            if self.peek_kind() != &TokenKind::RParen {
                loop {
                    variants.push(self.parse_ident()?);
                    if self.peek_kind() == &TokenKind::Comma {
                        self.advance();
                        if self.peek_kind() == &TokenKind::RParen {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
            self.expect(TokenKind::RParen, "`)`")?;
            Some(ModeExpr::Enum(variants))
        } else if self.check_ident("union") {
            self.advance();
            self.expect(TokenKind::LParen, "`(`")?;
            let mut variants = Vec::new();
            if self.peek_kind() != &TokenKind::RParen {
                loop {
                    let tag = self.parse_ident()?;
                    let mode = self.parse_mode_expr()?;
                    variants.push((tag, mode));

                    if self.peek_kind() == &TokenKind::Comma {
                        self.advance();
                        if self.peek_kind() == &TokenKind::RParen {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
            self.expect(TokenKind::RParen, "`)`")?;
            Some(ModeExpr::Union(variants))
        } else if self.check_ident("struct") {
            self.advance();
            self.expect(TokenKind::LParen, "`(`")?;

            if self.peek_kind() == &TokenKind::RParen {
                // Empty struct
                self.advance();
                return Some(ModeExpr::Struct(Vec::new()));
            }

            let mut fields = Vec::new();
            loop {
                let mode = self.parse_mode_expr()?;
                let name = self.parse_ident()?;
                fields.push(Field { mode, name });

                if self.peek_kind() == &TokenKind::Comma {
                    self.advance();
                    if self.peek_kind() == &TokenKind::RParen {
                        break;
                    }
                } else {
                    break;
                }
            }
            self.expect(TokenKind::RParen, "`)`")?;
            Some(ModeExpr::Struct(fields))
        } else if self.peek_kind() == &TokenKind::LParen {
            // Bare parentheses - should be a struct with type-first fields
            self.advance(); // consume '('

            if self.peek_kind() == &TokenKind::RParen {
                // Empty struct
                self.advance();
                return Some(ModeExpr::Struct(Vec::new()));
            }

            let mut fields = Vec::new();
            loop {
                let mode = self.parse_mode_expr()?;
                let name = self.parse_ident()?;
                fields.push(Field { mode, name });

                if self.peek_kind() == &TokenKind::Comma {
                    self.advance();
                    if self.peek_kind() == &TokenKind::RParen {
                        break;
                    }
                } else {
                    break;
                }
            }
            self.expect(TokenKind::RParen, "`)`")?;
            Some(ModeExpr::Struct(fields))
        } else {
            // Parse primitive or named type
            let name = self.parse_ident()?;

            // Check for parameterized type like Page(Task) or refined type like text(1..200)
            if self.peek_kind() == &TokenKind::LParen {
                self.advance();

                // Check if this is a refined type (integers with ranges or just ranges)
                if let TokenKind::Int(lo) = self.peek_kind() {
                    let lo_val = *lo;
                    self.advance();

                    if self.peek_kind() == &TokenKind::DotDot {
                        self.advance();

                        let hi = if let TokenKind::Int(h) = self.peek_kind() {
                            let h_val = *h;
                            self.advance();
                            Some(h_val)
                        } else {
                            None
                        };

                        self.expect(TokenKind::RParen, "`)`")?;
                        return Some(ModeExpr::Refined {
                            name,
                            lo: Some(lo_val),
                            hi,
                        });
                    }
                } else if self.peek_kind() == &TokenKind::DotDot {
                    // Handle refined type like text(..20000)
                    self.advance();
                    let hi = if let TokenKind::Int(h) = self.peek_kind() {
                        let h_val = *h;
                        self.advance();
                        Some(h_val)
                    } else {
                        None
                    };

                    self.expect(TokenKind::RParen, "`)`")?;
                    return Some(ModeExpr::Refined { name, lo: None, hi });
                }

                // Not a refined type, parse as type arguments
                let mut args = Vec::new();
                if self.peek_kind() != &TokenKind::RParen {
                    args.push(self.parse_mode_expr()?);
                    while self.peek_kind() == &TokenKind::Comma {
                        self.advance();
                        args.push(self.parse_mode_expr()?);
                    }
                }
                self.expect(TokenKind::RParen, "`)`")?;

                Some(ModeExpr::Named { name, args })
            } else if self.peek_kind() == &TokenKind::DotDot {
                // Handle refined type like text(..20000)
                self.advance();
                if let TokenKind::Int(hi) = self.peek_kind() {
                    let hi_val = *hi;
                    self.advance();
                    Some(ModeExpr::Refined {
                        name,
                        lo: None,
                        hi: Some(hi_val),
                    })
                } else {
                    self.diags.push(Diagnostic {
                        severity: Severity::Error,
                        code: "E101",
                        span: self.peek().span,
                        message: "expected number after `..`".to_string(),
                    });
                    None
                }
            } else {
                Some(ModeExpr::Named {
                    name,
                    args: Vec::new(),
                })
            }
        }
    }

    /// Parse field list (comma-separated Mode name pairs).
    pub fn parse_fields(&mut self) -> Option<Vec<Field>> {
        self.expect(TokenKind::LParen, "`(`")?;

        let mut fields = Vec::new();
        if self.peek_kind() != &TokenKind::RParen {
            loop {
                let mode = self.parse_mode_expr()?;
                let name = self.parse_ident()?;
                fields.push(Field { mode, name });

                if self.peek_kind() == &TokenKind::Comma {
                    self.advance();
                    if self.peek_kind() == &TokenKind::RParen {
                        break;
                    }
                } else {
                    break;
                }
            }
        }

        self.expect(TokenKind::RParen, "`)`")?;
        Some(fields)
    }

    /// Parse interface declaration.
    pub fn parse_interface(&mut self) -> Option<InterfaceDecl> {
        let start_span = self.peek().span;
        self.advance(); // consume 'interface'

        let name = self.parse_ident()?;

        self.expect(TokenKind::Eq, "`=`")?;

        if !self.check_ident("for") {
            self.diags.push(Diagnostic {
                severity: Severity::Error,
                code: "E101",
                span: self.peek().span,
                message: "expected `for` in interface".to_string(),
            });
            return None;
        }
        self.advance();

        let default_actor = self.parse_ident()?;

        self.expect(TokenKind::LParen, "`(`")?;

        let mut ops = Vec::new();
        while self.peek_kind() != &TokenKind::RParen && self.peek_kind() != &TokenKind::Eof {
            if let Some(op) = self.parse_op() {
                ops.push(op);
                if self.peek_kind() == &TokenKind::Comma {
                    self.advance();
                }
            }
        }

        self.expect(TokenKind::RParen, "`)`")?;

        let end_span = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span)
            .unwrap_or(start_span);
        Some(InterfaceDecl {
            name,
            default_actor,
            ops,
            span: start_span.join(end_span),
        })
    }

    /// Parse operation declaration.
    pub fn parse_op(&mut self) -> Option<OpDecl> {
        let start_span = self.peek().span;

        let kind = if self.check_ident("cmd") {
            self.advance();
            OpKind::Cmd
        } else if self.check_ident("qry") {
            self.advance();
            OpKind::Qry
        } else {
            self.diags.push(Diagnostic {
                severity: Severity::Error,
                code: "E101",
                span: self.peek().span,
                message: "expected `cmd` or `qry`".to_string(),
            });
            return None;
        };

        let name = self.parse_ident()?;

        self.expect(TokenKind::Eq, "`=`")?;

        let params = self.parse_fields()?;

        let output = self.parse_mode_expr()?;

        let mut errors = Vec::new();
        if self.peek_kind() == &TokenKind::Bang {
            self.advance();
            self.expect(TokenKind::LParen, "`(`")?;
            loop {
                errors.push(self.parse_ident()?);
                if self.peek_kind() == &TokenKind::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect(TokenKind::RParen, "`)`")?;
        }

        let end_span = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span)
            .unwrap_or(start_span);
        Some(OpDecl {
            kind,
            name,
            params,
            output,
            errors,
            span: start_span.join(end_span),
        })
    }

    /// Parse flow declaration.
    pub fn parse_flow(&mut self) -> Option<FlowDecl> {
        let start_span = self.peek().span;
        self.advance(); // consume 'flow'

        let name = self.parse_ident()?;

        self.expect(TokenKind::Eq, "`=`")?;

        let from = self.parse_ident()?;

        self.expect(TokenKind::Arrow, "`->`")?;

        let to = self.parse_ident()?;

        self.expect(TokenKind::Colon, "`:`")?;

        let kind = self.parse_ident()?;

        let attrs = self.parse_pack()?;

        let end_span = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span)
            .unwrap_or(start_span);
        Some(FlowDecl {
            name,
            from,
            to,
            kind,
            attrs,
            span: start_span.join(end_span),
        })
    }

    /// Parse behavior declaration.
    pub fn parse_behavior(&mut self) -> Option<BehaviorDecl> {
        let start_span = self.peek().span;
        self.advance(); // consume 'behavior'

        let name = self.parse_ident()?;

        self.expect(TokenKind::Eq, "`=`")?;

        if !self.check_ident("on") {
            self.diags.push(Diagnostic {
                severity: Severity::Error,
                code: "E101",
                span: self.peek().span,
                message: "expected `on` in behavior".to_string(),
            });
            return None;
        }
        self.advance();

        let on_interface = self.parse_ident()?;

        self.expect(TokenKind::Dot, "`.`")?;

        let on_op = self.parse_ident()?;

        self.expect(TokenKind::LParen, "`(`")?;

        let mut binders = Vec::new();
        loop {
            binders.push(self.parse_ident()?);
            if self.peek_kind() == &TokenKind::Comma {
                self.advance();
            } else {
                break;
            }
        }

        self.expect(TokenKind::RParen, "`)`")?;

        self.expect(TokenKind::LParen, "`(`")?;

        // Parse pack items until semicolon or clause keyword
        let mut attrs = Vec::new();
        let mut clauses = Vec::new();
        let mut in_clauses = false;

        while self.peek_kind() != &TokenKind::RParen && self.peek_kind() != &TokenKind::Eof {
            if !in_clauses {
                // Check if next is a clause keyword
                if self.check_ident("requires")
                    || self.check_ident("ensures")
                    || self.check_ident("returns")
                    || self.check_ident("fails")
                    || self.check_ident("emits")
                {
                    in_clauses = true;
                } else if self.peek_kind() == &TokenKind::Semi {
                    // Semicolon switches to clause mode
                    self.advance();
                    in_clauses = true;
                    continue;
                } else {
                    // Parse as pack item
                    if let Some(item) = self.parse_pack_item() {
                        attrs.push(item);
                        if self.peek_kind() == &TokenKind::Comma {
                            self.advance();
                        } else if self.peek_kind() == &TokenKind::Semi {
                            self.advance();
                            in_clauses = true;
                        }
                    } else {
                        // Failed to parse pack item - might be error or end of attrs
                        // If not a clause keyword and not semicolon, skip this token
                        if !self.check_ident("requires")
                            && !self.check_ident("ensures")
                            && !self.check_ident("returns")
                            && !self.check_ident("fails")
                            && !self.check_ident("emits")
                            && self.peek_kind() != &TokenKind::Semi
                        {
                            self.advance();
                        } else if self.peek_kind() == &TokenKind::Semi {
                            self.advance();
                            in_clauses = true;
                        }
                    }
                }
            }

            if in_clauses {
                if let Some(clause) = self.parse_clause() {
                    clauses.push(clause);
                    if self.peek_kind() == &TokenKind::Semi {
                        self.advance();
                    }
                } else {
                    // Failed to parse clause - skip to next semicolon or clause keyword
                    while self.peek_kind() != &TokenKind::Semi
                        && self.peek_kind() != &TokenKind::RParen
                        && self.peek_kind() != &TokenKind::Eof
                        && !self.check_ident("requires")
                        && !self.check_ident("ensures")
                        && !self.check_ident("returns")
                        && !self.check_ident("fails")
                        && !self.check_ident("emits")
                    {
                        self.advance();
                    }
                    if self.peek_kind() == &TokenKind::Semi {
                        self.advance();
                    }
                }
            }
        }

        self.expect(TokenKind::RParen, "`)`")?;

        let end_span = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span)
            .unwrap_or(start_span);
        Some(BehaviorDecl {
            name,
            on_interface,
            on_op,
            binders,
            attrs,
            clauses,
            span: start_span.join(end_span),
        })
    }

    /// Parse behavior clause.
    pub fn parse_clause(&mut self) -> Option<Clause> {
        if self.check_ident("requires") {
            self.advance();
            let pred = self.parse_pred()?;
            Some(Clause::Requires(pred))
        } else if self.check_ident("ensures") {
            self.advance();
            let pred = self.parse_pred()?;
            Some(Clause::Ensures(pred))
        } else if self.check_ident("returns") {
            self.advance();
            let expr = self.parse_expr()?;
            Some(Clause::Returns(expr))
        } else if self.check_ident("fails") {
            self.advance();
            let error = self.parse_ident()?;

            let when = if self.check_ident("when") {
                self.advance();
                self.parse_pred()?
            } else {
                // Default to true predicate if no when clause
                Pred::Word(Ident {
                    text: "true".to_string(),
                    span: error.span,
                })
            };

            let preserves = if self.check_ident("preserves") {
                self.advance();
                Some(self.parse_ident()?)
            } else {
                None
            };

            Some(Clause::Fails {
                error,
                when,
                preserves,
            })
        } else if self.check_ident("emits") {
            self.advance();
            let event = self.parse_ident()?;

            let mut qualifier = Vec::new();
            // Parse qualifiers until we hit EOF, ), ;, or another clause keyword
            while !self.peek_kind().eq(&TokenKind::RParen)
                && !self.peek_kind().eq(&TokenKind::Semi)
                && !self.peek_kind().eq(&TokenKind::Eof)
                && !self.check_ident("requires")
                && !self.check_ident("ensures")
                && !self.check_ident("returns")
                && !self.check_ident("fails")
                && !self.check_ident("emits")
            {
                if let Some(q) = self.parse_ident() {
                    qualifier.push(q);
                } else {
                    break;
                }
            }

            Some(Clause::Emits { event, qualifier })
        } else {
            self.diags.push(Diagnostic {
                severity: Severity::Error,
                code: "E101",
                span: self.peek().span,
                message: "expected clause keyword".to_string(),
            });
            None
        }
    }

    /// Parse invariant declaration.
    pub fn parse_invariant(&mut self) -> Option<InvariantDecl> {
        let start_span = self.peek().span;
        self.advance(); // consume 'inv'

        let name = self.parse_ident()?;

        self.expect(TokenKind::Eq, "`=`")?;

        if !self.check_ident("on") {
            self.diags.push(Diagnostic {
                severity: Severity::Error,
                code: "E101",
                span: self.peek().span,
                message: "expected `on` in invariant".to_string(),
            });
            return None;
        }
        self.advance();

        let scope = self.parse_ident()?;

        if !self.check_ident("always") {
            self.diags.push(Diagnostic {
                severity: Severity::Error,
                code: "E101",
                span: self.peek().span,
                message: "expected `always` in invariant".to_string(),
            });
            return None;
        }
        self.advance();

        let always = self.parse_pred()?;

        let end_span = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span)
            .unwrap_or(start_span);
        Some(InvariantDecl {
            name,
            scope,
            always,
            span: start_span.join(end_span),
        })
    }

    /// Parse constraint declaration.
    pub fn parse_constraint(&mut self) -> Option<ConstraintDecl> {
        let start_span = self.peek().span;
        self.advance(); // consume 'constraint'

        let name = self.parse_ident()?;

        self.expect(TokenKind::Eq, "`=`")?;

        let class = self.parse_ident()?;

        if !self.check_ident("on") {
            self.diags.push(Diagnostic {
                severity: Severity::Error,
                code: "E101",
                span: self.peek().span,
                message: "expected `on` in constraint".to_string(),
            });
            return None;
        }
        self.advance();

        let scope = self.parse_ident()?;

        if !self.check_ident("under") {
            self.diags.push(Diagnostic {
                severity: Severity::Error,
                code: "E101",
                span: self.peek().span,
                message: "expected `under` in constraint".to_string(),
            });
            return None;
        }
        self.advance();

        let under = self.parse_pack()?;

        if !self.check_ident("must") {
            self.diags.push(Diagnostic {
                severity: Severity::Error,
                code: "E101",
                span: self.peek().span,
                message: "expected `must` in constraint".to_string(),
            });
            return None;
        }
        self.advance();

        let must = self.parse_pred()?;

        let end_span = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span)
            .unwrap_or(start_span);
        Some(ConstraintDecl {
            name,
            class,
            scope,
            under,
            must,
            span: start_span.join(end_span),
        })
    }

    /// Parse synthesis declaration.
    pub fn parse_synthesis(&mut self) -> Option<SynthesisDecl> {
        let start_span = self.peek().span;
        self.advance(); // consume 'synthesis'

        let name = self.parse_ident()?;

        self.expect(TokenKind::Eq, "`=`")?;

        if !self.check_ident("target") {
            self.diags.push(Diagnostic {
                severity: Severity::Error,
                code: "E101",
                span: self.peek().span,
                message: "expected `target` in synthesis".to_string(),
            });
            return None;
        }
        self.advance();

        let target_lang = self.parse_ident()?;

        let target_framework = if self.peek_kind() == &TokenKind::Slash {
            self.advance();
            Some(self.parse_ident()?)
        } else {
            None
        };

        let attrs = self.parse_pack()?;

        let end_span = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span)
            .unwrap_or(start_span);
        Some(SynthesisDecl {
            name,
            target_lang,
            target_framework,
            attrs,
            span: start_span.join(end_span),
        })
    }

    /// Parse acceptance declaration.
    pub fn parse_acceptance(&mut self) -> Option<AcceptanceDecl> {
        let start_span = self.peek().span;
        self.advance(); // consume 'acceptance'

        let name = self.parse_ident()?;

        self.expect(TokenKind::Eq, "`=`")?;

        if !self.check_ident("of") {
            self.diags.push(Diagnostic {
                severity: Severity::Error,
                code: "E101",
                span: self.peek().span,
                message: "expected `of` in acceptance".to_string(),
            });
            return None;
        }
        self.advance();

        let subject = self.parse_ident()?;

        self.expect(TokenKind::LParen, "`(`")?;

        let mut blocks = Vec::new();
        while self.peek_kind() != &TokenKind::RParen && self.peek_kind() != &TokenKind::Eof {
            if let Some(block) = self.parse_acceptance_block() {
                blocks.push(block);
                if self.peek_kind() == &TokenKind::Comma {
                    self.advance();
                }
            } else {
                // Parsing failed - skip to next block keyword or closing paren
                while self.peek_kind() != &TokenKind::RParen
                    && self.peek_kind() != &TokenKind::Eof
                    && !self.check_ident("property")
                    && !self.check_ident("scenario")
                    && !self.check_ident("concurrency")
                    && !self.check_ident("fault")
                    && !self.check_ident("coverage")
                    && !self.check_ident("execution")
                {
                    self.advance();
                }
                // If we found a comma, skip it
                if self.peek_kind() == &TokenKind::Comma {
                    self.advance();
                }
            }
        }

        self.expect(TokenKind::RParen, "`)`")?;

        let end_span = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span)
            .unwrap_or(start_span);
        Some(AcceptanceDecl {
            name,
            subject,
            blocks,
            span: start_span.join(end_span),
        })
    }

    /// Parse acceptance block.
    pub fn parse_acceptance_block(&mut self) -> Option<AcceptanceBlock> {
        if self.check_ident("property") {
            self.advance();
            let name = self.parse_ident()?;
            self.expect(TokenKind::Eq, "`=`")?;
            // Property value can be either (pack) or implicit pack items
            let body = if self.peek_kind() == &TokenKind::LParen {
                self.parse_pack()?
            } else {
                // Parse as implicit pack (items without outer parens)
                let mut items = Vec::new();
                while self.peek_kind() != &TokenKind::Comma
                    && self.peek_kind() != &TokenKind::RParen
                    && self.peek_kind() != &TokenKind::Eof
                {
                    if let Some(item) = self.parse_pack_item() {
                        items.push(item);
                    } else {
                        break;
                    }
                }
                items
            };
            Some(AcceptanceBlock::Property { name, body })
        } else if self.check_ident("scenario") {
            self.advance();
            let name = self.parse_ident()?;
            self.expect(TokenKind::Eq, "`=`")?;
            // Scenario steps are complex; for now just parse the outer pack structure
            // The grammar for scenario steps is special (given/when/then keywords)
            let steps = if self.peek_kind() == &TokenKind::LParen {
                // Skip the scenario body for now - it has special syntax
                self.advance(); // skip '('
                let mut paren_depth = 1;
                while self.pos < self.tokens.len() && paren_depth > 0 {
                    match self.peek_kind() {
                        TokenKind::LParen => paren_depth += 1,
                        TokenKind::RParen => paren_depth -= 1,
                        _ => {}
                    }
                    if paren_depth > 0 {
                        self.advance();
                    }
                }
                if paren_depth == 0 {
                    self.advance(); // skip final ')'
                }
                // Return empty pack - scenario parsing is not implemented
                Vec::new()
            } else {
                Vec::new()
            };
            Some(AcceptanceBlock::Scenario { name, steps })
        } else if self.check_ident("concurrency") {
            self.advance();
            let name = self.parse_ident()?;
            self.expect(TokenKind::Eq, "`=`")?;
            // Parse the concurrency attributes (can be in parens or bare pack items)
            let attrs = if self.peek_kind() == &TokenKind::LParen {
                self.parse_pack()?
            } else {
                Vec::new()
            };
            if !self.check_ident("must") {
                self.diags.push(Diagnostic {
                    severity: Severity::Error,
                    code: "E101",
                    span: self.peek().span,
                    message: "expected `must` in concurrency block".to_string(),
                });
                return None;
            }
            self.advance();
            let must = self.parse_pred()?;
            Some(AcceptanceBlock::Concurrency { name, attrs, must })
        } else if self.check_ident("fault") {
            self.advance();
            let name = self.parse_ident()?;
            self.expect(TokenKind::Eq, "`=`")?;
            let body = if self.peek_kind() == &TokenKind::LParen {
                self.parse_pack()?
            } else {
                // Parse as implicit pack (items without outer parens) until 'must' keyword
                let mut items = Vec::new();
                while !self.check_ident("must")
                    && self.peek_kind() != &TokenKind::Comma
                    && self.peek_kind() != &TokenKind::RParen
                    && self.peek_kind() != &TokenKind::Eof
                {
                    if let Some(item) = self.parse_pack_item() {
                        items.push(item);
                    } else {
                        break;
                    }
                }
                items
            };
            Some(AcceptanceBlock::Fault { name, body })
        } else if self.check_ident("coverage") {
            self.advance();
            self.expect(TokenKind::LParen, "`(`")?;
            let mut names = Vec::new();
            loop {
                names.push(self.parse_ident()?);
                if self.peek_kind() == &TokenKind::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect(TokenKind::RParen, "`)`")?;
            Some(AcceptanceBlock::Coverage(names))
        } else if self.check_ident("execution") {
            self.advance();
            let pack = self.parse_pack()?;
            Some(AcceptanceBlock::Execution(pack))
        } else {
            self.diags.push(Diagnostic {
                severity: Severity::Error,
                code: "E101",
                span: self.peek().span,
                message: "expected acceptance block keyword".to_string(),
            });
            None
        }
    }

    /// Parse a pack: (item, item, ...) or (item; item; ...)
    pub fn parse_pack(&mut self) -> Option<Pack> {
        self.expect(TokenKind::LParen, "`(`")?;

        let mut items = Vec::new();
        if self.peek_kind() != &TokenKind::RParen {
            loop {
                if let Some(item) = self.parse_pack_item() {
                    items.push(item);
                }

                // Accept both comma and semicolon as separators
                if self.peek_kind() == &TokenKind::Comma || self.peek_kind() == &TokenKind::Semi {
                    self.advance();
                    if self.peek_kind() == &TokenKind::RParen {
                        break;
                    }
                } else {
                    break;
                }
            }
        }

        self.expect(TokenKind::RParen, "`)`")?;
        Some(items)
    }

    /// Parse a pack item: key value*
    pub fn parse_pack_item(&mut self) -> Option<PackItem> {
        let key = self.parse_ident()?;
        let key_span = key.span;

        let value = if self.peek_kind() == &TokenKind::Comma
            || self.peek_kind() == &TokenKind::RParen
            || self.peek_kind() == &TokenKind::Semi
        {
            // Bare key (Unit)
            PackValue::Unit
        } else {
            match self.parse_pack_value() {
                Some(val) => val,
                None => {
                    // parse_pack_value failed; if position didn't advance,
                    // we're stuck. Return anyway to let caller handle it.
                    PackValue::Unit
                }
            }
        };

        Some(PackItem {
            key,
            value,
            span: key_span,
        })
    }

    /// Parse a pack value.
    pub fn parse_pack_value(&mut self) -> Option<PackValue> {
        match self.peek_kind() {
            TokenKind::Int(val) => {
                let int_val = *val;
                self.advance();

                // Check for quantity: int followed by unit name
                if let TokenKind::Ident(unit_name) = self.peek_kind() {
                    if matches!(unit_name.as_str(), "min" | "ms" | "s" | "sec") {
                        let unit = self.parse_ident()?;
                        return Some(PackValue::Quantity {
                            value: int_val,
                            unit,
                        });
                    }
                }

                Some(PackValue::Int(int_val))
            }
            TokenKind::Str(s) => {
                let str_val = s.clone();
                self.advance();
                Some(PackValue::Str(str_val))
            }
            TokenKind::LParen => {
                self.advance();

                // Try to parse as nested pack (key-value pairs) or plain list
                let mut pack_items = Vec::new();
                let mut value_items = Vec::new();
                let mut is_pack = true;

                if self.peek_kind() != &TokenKind::RParen {
                    // Peek to determine if this looks like key-value pairs
                    if let TokenKind::Ident(_) = self.peek_kind() {
                        let saved_pos = self.pos;

                        // Try to parse first item as key-value
                        if let Some(item) = self.parse_pack_item() {
                            // Check if this has a non-Unit value
                            if !matches!(item.value, PackValue::Unit) {
                                // This looks like a pack
                                pack_items.push(item);

                                let mut pack_iterations = 0;
                                while (self.peek_kind() == &TokenKind::Comma
                                    || self.peek_kind() == &TokenKind::Semi)
                                    && pack_iterations < 1000
                                {
                                    pack_iterations += 1;
                                    self.advance();
                                    if self.peek_kind() == &TokenKind::RParen {
                                        break;
                                    }
                                    let item_start = self.pos;
                                    if let Some(item) = self.parse_pack_item() {
                                        pack_items.push(item);
                                    } else {
                                        // If position didn't advance, skip to prevent infinite loop
                                        if self.pos == item_start && self.pos < self.tokens.len() {
                                            self.advance();
                                        }
                                    }
                                }

                                self.expect(TokenKind::RParen, "`)`")?;
                                return Some(PackValue::Nested(pack_items));
                            } else {
                                // This is a plain list - reparse as values
                                self.pos = saved_pos;
                                is_pack = false;
                            }
                        } else {
                            self.pos = saved_pos;
                            is_pack = false;
                        }
                    } else {
                        is_pack = false;
                    }

                    if !is_pack {
                        // Parse as plain value list
                        let mut value_iterations = 0;
                        loop {
                            value_iterations += 1;
                            if value_iterations > 1000 {
                                break; // Safety limit
                            }

                            if let Some(val) = self.parse_pack_value() {
                                value_items.push(val);
                            } else {
                                break;
                            }

                            if self.peek_kind() == &TokenKind::Comma {
                                self.advance();
                                if self.peek_kind() == &TokenKind::RParen {
                                    break;
                                }
                            } else {
                                break;
                            }
                        }
                    }
                }

                self.expect(TokenKind::RParen, "`)`")?;

                if is_pack && !pack_items.is_empty() {
                    Some(PackValue::Nested(pack_items))
                } else {
                    Some(PackValue::List(value_items))
                }
            }
            TokenKind::Ident(_) => {
                let first_ident = self.parse_ident()?;

                if self.peek_kind() == &TokenKind::LParen {
                    // Call - parse arguments as a nested pack if they look like key-value pairs
                    self.advance();
                    let mut args = Vec::new();
                    if self.peek_kind() != &TokenKind::RParen {
                        // Try to parse as pack items (comma or semicolon-separated key-value pairs)
                        let mut iterations = 0;
                        loop {
                            iterations += 1;
                            if iterations > 1000 {
                                break; // Safety limit
                            }
                            let item_start = self.pos;
                            if let Some(item) = self.parse_pack_item() {
                                args.push(PackValue::Nested(vec![item]));
                            } else {
                                // If position didn't advance, skip to prevent infinite loop
                                if self.pos == item_start && self.pos < self.tokens.len() {
                                    self.advance();
                                }
                            }

                            if self.peek_kind() == &TokenKind::Comma
                                || self.peek_kind() == &TokenKind::Semi
                            {
                                self.advance();
                            } else {
                                break;
                            }
                        }
                    }
                    self.expect(TokenKind::RParen, "`)`")?;
                    Some(PackValue::Call {
                        name: first_ident,
                        args,
                    })
                } else if self.peek_kind() == &TokenKind::Dot {
                    // Path
                    let mut path = vec![first_ident];
                    while self.peek_kind() == &TokenKind::Dot {
                        self.advance();
                        path.push(self.parse_ident()?);
                    }

                    // Check if there are more idents after the path (multi-word)
                    if let TokenKind::Ident(_) = self.peek_kind() {
                        let mut values = vec![PackValue::Path(path)];
                        loop {
                            if let TokenKind::Ident(_) = self.peek_kind() {
                                let next_ident = self.parse_ident()?;
                                if self.peek_kind() == &TokenKind::Dot {
                                    // This is another path
                                    let mut path2 = vec![next_ident];
                                    while self.peek_kind() == &TokenKind::Dot {
                                        self.advance();
                                        path2.push(self.parse_ident()?);
                                    }
                                    values.push(PackValue::Path(path2));
                                } else {
                                    // Just a word
                                    values.push(PackValue::Word(next_ident));
                                }
                            } else {
                                break;
                            }
                        }
                        Some(PackValue::List(values))
                    } else {
                        Some(PackValue::Path(path))
                    }
                } else if let TokenKind::Ident(_) = self.peek_kind() {
                    // Multi-word value: first ident is Word, rest as list
                    let mut values = vec![PackValue::Word(first_ident)];
                    loop {
                        if let TokenKind::Ident(_) = self.peek_kind() {
                            let next_ident = self.parse_ident()?;
                            if self.peek_kind() == &TokenKind::Dot {
                                // This is a path
                                let mut path = vec![next_ident];
                                while self.peek_kind() == &TokenKind::Dot {
                                    self.advance();
                                    path.push(self.parse_ident()?);
                                }
                                values.push(PackValue::Path(path));
                            } else {
                                values.push(PackValue::Word(next_ident));
                            }
                        } else {
                            break;
                        }
                    }
                    Some(PackValue::List(values))
                } else {
                    // Single word
                    Some(PackValue::Word(first_ident))
                }
            }
            _ => {
                self.diags.push(Diagnostic {
                    severity: Severity::Error,
                    code: "E101",
                    span: self.peek().span,
                    message: format!("unexpected token in pack: `{}`", self.token_display()),
                });
                // Advance to prevent infinite loops
                self.advance();
                None
            }
        }
    }

    /// Parse predicate with or precedence.
    pub fn parse_pred(&mut self) -> Option<Pred> {
        self.parse_pred_or()
    }

    /// Parse predicate or (lowest precedence).
    pub fn parse_pred_or(&mut self) -> Option<Pred> {
        let mut left = self.parse_pred_and()?;

        while self.check_ident("or") {
            self.advance();
            let right = self.parse_pred_and()?;
            left = Pred::Or(Box::new(left), Box::new(right));
        }

        Some(left)
    }

    /// Parse predicate and.
    pub fn parse_pred_and(&mut self) -> Option<Pred> {
        let mut left = self.parse_pred_not()?;

        while self.check_ident("and") {
            self.advance();
            let right = self.parse_pred_not()?;
            left = Pred::And(Box::new(left), Box::new(right));
        }

        Some(left)
    }

    /// Parse predicate not.
    pub fn parse_pred_not(&mut self) -> Option<Pred> {
        if self.check_ident("not") {
            self.advance();
            let pred = self.parse_pred_not()?;
            Some(Pred::Not(Box::new(pred)))
        } else {
            self.parse_pred_atom()
        }
    }

    /// Parse predicate atom (quantifiers, comparisons, calls, bare names).
    pub fn parse_pred_atom(&mut self) -> Option<Pred> {
        if self.check_ident("for") {
            self.advance();
            if !self.check_ident("all") {
                self.diags.push(Diagnostic {
                    severity: Severity::Error,
                    code: "E101",
                    span: self.peek().span,
                    message: "expected `all` in quantifier".to_string(),
                });
                return None;
            }
            self.advance();

            let mode = self.parse_ident()?;
            let var = self.parse_ident()?;

            if self.peek_kind() != &TokenKind::Colon {
                self.diags.push(Diagnostic {
                    severity: Severity::Error,
                    code: "E101",
                    span: self.peek().span,
                    message: "expected `:` in quantifier".to_string(),
                });
                return None;
            }
            self.advance();

            let body = self.parse_pred_or()?;
            Some(Pred::ForAll {
                mode,
                var,
                body: Box::new(body),
            })
        } else if self.check_ident("exists") {
            self.advance();

            let mode = self.parse_ident()?;
            let var = self.parse_ident()?;

            if self.peek_kind() == &TokenKind::Colon {
                self.advance();
            } else if !self.check_ident(":") {
                self.diags.push(Diagnostic {
                    severity: Severity::Error,
                    code: "E101",
                    span: self.peek().span,
                    message: "expected `:` in quantifier".to_string(),
                });
                return None;
            }

            let body = self.parse_pred_or()?;
            Some(Pred::Exists {
                mode,
                var,
                body: Box::new(body),
            })
        } else {
            // Parse expression for comparison or predicate call
            let lhs = self.parse_expr()?;

            if let TokenKind::Eq = self.peek_kind() {
                self.advance();
                let rhs = self.parse_expr()?;
                Some(Pred::Cmp {
                    op: CmpOp::Eq,
                    lhs,
                    rhs,
                })
            } else if let TokenKind::Lt = self.peek_kind() {
                self.advance();
                let rhs = self.parse_expr()?;
                Some(Pred::Cmp {
                    op: CmpOp::Lt,
                    lhs,
                    rhs,
                })
            } else if let TokenKind::Le = self.peek_kind() {
                self.advance();
                let rhs = self.parse_expr()?;
                Some(Pred::Cmp {
                    op: CmpOp::Le,
                    lhs,
                    rhs,
                })
            } else if let Expr::Call { name, args } = lhs {
                Some(Pred::Call { name, args })
            } else if let Expr::Path(mut path) = lhs {
                if path.len() == 1 {
                    Some(Pred::Word(path.pop().unwrap()))
                } else {
                    // Multi-segment path in predicate position is an error
                    self.diags.push(Diagnostic {
                        severity: Severity::Error,
                        code: "E101",
                        span: self.peek().span,
                        message: "invalid predicate".to_string(),
                    });
                    None
                }
            } else {
                self.diags.push(Diagnostic {
                    severity: Severity::Error,
                    code: "E101",
                    span: self.peek().span,
                    message: "invalid predicate".to_string(),
                });
                None
            }
        }
    }

    /// Parse expression.
    pub fn parse_expr(&mut self) -> Option<Expr> {
        match self.peek_kind() {
            TokenKind::Int(val) => {
                let int_val = *val;
                self.advance();
                Some(Expr::Int(int_val))
            }
            TokenKind::Str(s) => {
                let str_val = s.clone();
                self.advance();
                Some(Expr::Str(str_val))
            }
            TokenKind::Ident(_) => {
                let first = self.parse_ident()?;

                if self.peek_kind() == &TokenKind::Dot {
                    // Path
                    let mut path = vec![first];
                    while self.peek_kind() == &TokenKind::Dot {
                        self.advance();
                        path.push(self.parse_ident()?);
                    }
                    Some(Expr::Path(path))
                } else if self.peek_kind() == &TokenKind::LParen {
                    // Call
                    self.advance();
                    let mut args = Vec::new();
                    if self.peek_kind() != &TokenKind::RParen {
                        loop {
                            args.push(self.parse_expr()?);
                            if self.peek_kind() == &TokenKind::Comma {
                                self.advance();
                            } else {
                                break;
                            }
                        }
                    }
                    self.expect(TokenKind::RParen, "`)`")?;
                    Some(Expr::Call { name: first, args })
                } else {
                    // Single ident as path
                    Some(Expr::Path(vec![first]))
                }
            }
            _ => {
                self.diags.push(Diagnostic {
                    severity: Severity::Error,
                    code: "E101",
                    span: self.peek().span,
                    message: format!("expected expression, got `{}`", self.token_display()),
                });
                None
            }
        }
    }
}

/// Parse source code into an AST.
pub fn parse(src: &str) -> (Option<File>, Vec<Diagnostic>) {
    let (tokens, lex_diags) = Lexer::tokenize(src);
    let mut parser = Parser::new(tokens);
    let file = parser.parse_file();
    let mut all_diags = lex_diags;
    all_diags.extend(parser.diags);
    (file, all_diags)
}
