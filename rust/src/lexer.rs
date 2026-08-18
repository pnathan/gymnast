use crate::diag::{Diagnostic, Severity};
use crate::span::Span;

/// Token kind enumeration.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    /// Identifier (lowercase_snake_case or PascalCase).
    Ident(String),
    /// Integer literal.
    Int(i64),
    /// String literal.
    Str(String),
    /// Left parenthesis.
    LParen,
    /// Right parenthesis.
    RParen,
    /// Comma.
    Comma,
    /// Semicolon.
    Semi,
    /// Colon.
    Colon,
    /// Equals sign.
    Eq,
    /// Exclamation mark.
    Bang,
    /// Dot.
    Dot,
    /// At sign.
    At,
    /// Slash.
    Slash,
    /// Less than.
    Lt,
    /// Less than or equal.
    Le,
    /// Arrow.
    Arrow,
    /// Double dot.
    DotDot,
    /// End of file.
    Eof,
}

/// A token with location information.
#[derive(Debug, Clone)]
pub struct Token {
    /// The token kind.
    pub kind: TokenKind,
    /// The source span.
    pub span: Span,
}

/// Lexer for the `.gym` surface language.
pub struct Lexer<'a> {
    #[allow(dead_code)]
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Lexer<'a> {
    /// Create a new lexer for the given source.
    pub fn new(src: &'a str) -> Self {
        Lexer {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            tokens: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    /// Tokenize source code into tokens and diagnostics.
    pub fn tokenize(src: &str) -> (Vec<Token>, Vec<Diagnostic>) {
        let mut lexer = Lexer::new(src);
        lexer.run();
        (lexer.tokens, lexer.diagnostics)
    }

    fn run(&mut self) {
        loop {
            self.skip_whitespace_and_comments();
            if self.pos >= self.bytes.len() {
                let span = Span {
                    start: self.pos,
                    end: self.pos,
                };
                self.tokens.push(Token {
                    kind: TokenKind::Eof,
                    span,
                });
                break;
            }

            let start = self.pos;
            let ch = self.bytes[self.pos] as char;

            match ch {
                '(' => {
                    self.pos += 1;
                    self.tokens.push(Token {
                        kind: TokenKind::LParen,
                        span: Span {
                            start,
                            end: self.pos,
                        },
                    });
                }
                ')' => {
                    self.pos += 1;
                    self.tokens.push(Token {
                        kind: TokenKind::RParen,
                        span: Span {
                            start,
                            end: self.pos,
                        },
                    });
                }
                ',' => {
                    self.pos += 1;
                    self.tokens.push(Token {
                        kind: TokenKind::Comma,
                        span: Span {
                            start,
                            end: self.pos,
                        },
                    });
                }
                ';' => {
                    self.pos += 1;
                    self.tokens.push(Token {
                        kind: TokenKind::Semi,
                        span: Span {
                            start,
                            end: self.pos,
                        },
                    });
                }
                ':' => {
                    self.pos += 1;
                    self.tokens.push(Token {
                        kind: TokenKind::Colon,
                        span: Span {
                            start,
                            end: self.pos,
                        },
                    });
                }
                '=' => {
                    self.pos += 1;
                    self.tokens.push(Token {
                        kind: TokenKind::Eq,
                        span: Span {
                            start,
                            end: self.pos,
                        },
                    });
                }
                '!' => {
                    self.pos += 1;
                    self.tokens.push(Token {
                        kind: TokenKind::Bang,
                        span: Span {
                            start,
                            end: self.pos,
                        },
                    });
                }
                '@' => {
                    self.pos += 1;
                    self.tokens.push(Token {
                        kind: TokenKind::At,
                        span: Span {
                            start,
                            end: self.pos,
                        },
                    });
                }
                '/' => {
                    self.pos += 1;
                    self.tokens.push(Token {
                        kind: TokenKind::Slash,
                        span: Span {
                            start,
                            end: self.pos,
                        },
                    });
                }
                '<' => {
                    self.pos += 1;
                    if self.pos < self.bytes.len() && self.bytes[self.pos] as char == '=' {
                        self.pos += 1;
                        self.tokens.push(Token {
                            kind: TokenKind::Le,
                            span: Span {
                                start,
                                end: self.pos,
                            },
                        });
                    } else {
                        self.tokens.push(Token {
                            kind: TokenKind::Lt,
                            span: Span {
                                start,
                                end: self.pos,
                            },
                        });
                    }
                }
                '-' => {
                    self.pos += 1;
                    if self.pos < self.bytes.len() && self.bytes[self.pos] as char == '>' {
                        self.pos += 1;
                        self.tokens.push(Token {
                            kind: TokenKind::Arrow,
                            span: Span {
                                start,
                                end: self.pos,
                            },
                        });
                    } else {
                        self.diagnostics.push(Diagnostic {
                            severity: Severity::Error,
                            code: "E001",
                            span: Span {
                                start,
                                end: self.pos,
                            },
                            message: "unexpected token `-`, expected `->`".to_string(),
                        });
                    }
                }
                '.' => {
                    self.pos += 1;
                    if self.pos < self.bytes.len() && self.bytes[self.pos] as char == '.' {
                        self.pos += 1;
                        self.tokens.push(Token {
                            kind: TokenKind::DotDot,
                            span: Span {
                                start,
                                end: self.pos,
                            },
                        });
                    } else {
                        self.tokens.push(Token {
                            kind: TokenKind::Dot,
                            span: Span {
                                start,
                                end: self.pos,
                            },
                        });
                    }
                }
                '"' => {
                    self.lex_string(start);
                }
                '0'..='9' => {
                    self.lex_number(start);
                }
                'a'..='z' | 'A'..='Z' | '_' => {
                    self.lex_ident(start);
                }
                _ => {
                    self.pos += 1;
                    // For a multi-byte UTF-8 character, consume its
                    // continuation bytes too: one diagnostic per character,
                    // with a span that stays on char boundaries.
                    while self.pos < self.bytes.len() && (self.bytes[self.pos] & 0xC0) == 0x80 {
                        self.pos += 1;
                    }
                    let display = String::from_utf8_lossy(&self.bytes[start..self.pos])
                        .chars()
                        .next()
                        .map(|c| c.escape_default().to_string())
                        .unwrap_or_default();
                    self.diagnostics.push(Diagnostic {
                        severity: Severity::Error,
                        code: "E001",
                        span: Span {
                            start,
                            end: self.pos,
                        },
                        message: format!("unexpected token `{}`", display),
                    });
                }
            }
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            if self.pos >= self.bytes.len() {
                break;
            }

            let ch = self.bytes[self.pos] as char;

            if ch == '#' {
                while self.pos < self.bytes.len() {
                    let c = self.bytes[self.pos] as char;
                    self.pos += 1;
                    if c == '\n' {
                        break;
                    }
                }
            } else if ch.is_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn lex_string(&mut self, start: usize) {
        self.pos += 1;
        // Collect raw bytes and decode once at the end: pushing each byte as
        // a char would corrupt multi-byte UTF-8 sequences into mojibake. The
        // delimiters (`"`, `\`) are ASCII, so slicing at them keeps the
        // collected bytes valid UTF-8 whenever the input is.
        let mut result: Vec<u8> = Vec::new();

        loop {
            if self.pos >= self.bytes.len() {
                self.diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    code: "E002",
                    span: Span {
                        start,
                        end: self.pos,
                    },
                    message: "unterminated string literal".to_string(),
                });
                break;
            }

            let byte = self.bytes[self.pos];

            if byte == b'"' {
                self.pos += 1;
                self.tokens.push(Token {
                    kind: TokenKind::Str(String::from_utf8_lossy(&result).into_owned()),
                    span: Span {
                        start,
                        end: self.pos,
                    },
                });
                break;
            } else if byte == b'\\' {
                self.pos += 1;
                if self.pos >= self.bytes.len() {
                    self.diagnostics.push(Diagnostic {
                        severity: Severity::Error,
                        code: "E002",
                        span: Span {
                            start,
                            end: self.pos,
                        },
                        message: "unterminated string literal".to_string(),
                    });
                    break;
                }

                let escaped = self.bytes[self.pos];
                match escaped {
                    b'"' => result.push(b'"'),
                    b'\\' => result.push(b'\\'),
                    _ => {
                        result.push(b'\\');
                        result.push(escaped);
                    }
                }
                self.pos += 1;
            } else {
                result.push(byte);
                self.pos += 1;
            }
        }
    }

    fn lex_number(&mut self, start: usize) {
        let mut num_str = String::new();

        while self.pos < self.bytes.len() {
            let ch = self.bytes[self.pos] as char;
            if ch.is_ascii_digit() {
                num_str.push(ch);
                self.pos += 1;
            } else {
                break;
            }
        }

        match num_str.parse::<i64>() {
            Ok(n) => {
                self.tokens.push(Token {
                    kind: TokenKind::Int(n),
                    span: Span {
                        start,
                        end: self.pos,
                    },
                });
            }
            Err(_) => {
                self.diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    code: "E003",
                    span: Span {
                        start,
                        end: self.pos,
                    },
                    message: "integer literal out of range".to_string(),
                });
            }
        }
    }

    fn lex_ident(&mut self, start: usize) {
        let mut ident = String::new();

        // Identifiers are ASCII-only by the grammar; a non-ASCII byte would
        // otherwise be misread as a Latin-1 char and corrupt the name.
        while self.pos < self.bytes.len() {
            let ch = self.bytes[self.pos] as char;
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ident.push(ch);
                self.pos += 1;
            } else {
                break;
            }
        }

        self.tokens.push(Token {
            kind: TokenKind::Ident(ident),
            span: Span {
                start,
                end: self.pos,
            },
        });
    }
}
