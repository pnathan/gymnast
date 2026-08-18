use crate::ast::{Decl, Field, Ident, ModeDecl, ModeExpr};
use crate::sexpr::Sexpr;
use crate::span::Span;

/// A parameter default value for a profile.
#[derive(Debug, Clone, PartialEq)]
pub enum ParamDefault {
    /// Parameter is required; no default.
    Required,
    /// Parameter has a default value.
    Value(Sexpr),
}

/// A parameter definition for a profile.
#[derive(Debug, Clone)]
pub struct Param {
    /// Parameter key name.
    pub key: &'static str,
    /// Default value or required.
    pub default: ParamDefault,
}

/// A resolved profile: name, version, parameters, and a generator.
pub struct Profile {
    /// Profile name (e.g., "oddities/profiles/todo_standard").
    pub name: &'static str,
    /// Profile version (e.g., "1.0").
    pub version: &'static str,
    /// Parameter definitions.
    pub params: Vec<Param>,
    /// Generator function that produces declarations from validated args.
    pub generate: fn(&[(String, Sexpr)]) -> Vec<Decl>,
}

/// Look up a built-in profile by (name, version). Returns None if not found.
pub fn lookup(name: &str, version: &str) -> Option<Profile> {
    if name == "oddities/profiles/todo_standard" && version == "1.0" {
        Some(Profile {
            name: "oddities/profiles/todo_standard",
            version: "1.0",
            params: vec![
                Param {
                    key: "sharing_limit",
                    default: ParamDefault::Required,
                },
                Param {
                    key: "identity_provider",
                    default: ParamDefault::Required,
                },
            ],
            generate: generate_todo_standard,
        })
    } else {
        None
    }
}

/// Generate declarations for the todo_standard profile.
/// Creates four mode declarations: Cursor, Page, Membership, Invitation.
fn generate_todo_standard(_args: &[(String, Sexpr)]) -> Vec<Decl> {
    // All generated modes have a zero span since they have no surface location.
    let zero_span = Span { start: 0, end: 0 };

    let mut decls = Vec::new();

    // Cursor = opaque text
    decls.push(Decl::Mode(ModeDecl {
        name: Ident {
            text: "Cursor".to_string(),
            span: zero_span,
        },
        expr: ModeExpr::Opaque(Box::new(ModeExpr::Named {
            name: Ident {
                text: "text".to_string(),
                span: zero_span,
            },
            args: vec![],
        })),
        span: zero_span,
    }));

    // Page = opaque text
    decls.push(Decl::Mode(ModeDecl {
        name: Ident {
            text: "Page".to_string(),
            span: zero_span,
        },
        expr: ModeExpr::Opaque(Box::new(ModeExpr::Named {
            name: Ident {
                text: "text".to_string(),
                span: zero_span,
            },
            args: vec![],
        })),
        span: zero_span,
    }));

    // Membership = struct (ListId list, UserId principal, Role role, Version version)
    decls.push(Decl::Mode(ModeDecl {
        name: Ident {
            text: "Membership".to_string(),
            span: zero_span,
        },
        expr: ModeExpr::Struct(vec![
            Field {
                mode: ModeExpr::Named {
                    name: Ident {
                        text: "ListId".to_string(),
                        span: zero_span,
                    },
                    args: vec![],
                },
                name: Ident {
                    text: "list".to_string(),
                    span: zero_span,
                },
            },
            Field {
                mode: ModeExpr::Named {
                    name: Ident {
                        text: "UserId".to_string(),
                        span: zero_span,
                    },
                    args: vec![],
                },
                name: Ident {
                    text: "principal".to_string(),
                    span: zero_span,
                },
            },
            Field {
                mode: ModeExpr::Named {
                    name: Ident {
                        text: "Role".to_string(),
                        span: zero_span,
                    },
                    args: vec![],
                },
                name: Ident {
                    text: "role".to_string(),
                    span: zero_span,
                },
            },
            Field {
                mode: ModeExpr::Named {
                    name: Ident {
                        text: "Version".to_string(),
                        span: zero_span,
                    },
                    args: vec![],
                },
                name: Ident {
                    text: "version".to_string(),
                    span: zero_span,
                },
            },
        ]),
        span: zero_span,
    }));

    // Invitation = struct (ListId list, UserId principal, Role role, Version version)
    decls.push(Decl::Mode(ModeDecl {
        name: Ident {
            text: "Invitation".to_string(),
            span: zero_span,
        },
        expr: ModeExpr::Struct(vec![
            Field {
                mode: ModeExpr::Named {
                    name: Ident {
                        text: "ListId".to_string(),
                        span: zero_span,
                    },
                    args: vec![],
                },
                name: Ident {
                    text: "list".to_string(),
                    span: zero_span,
                },
            },
            Field {
                mode: ModeExpr::Named {
                    name: Ident {
                        text: "UserId".to_string(),
                        span: zero_span,
                    },
                    args: vec![],
                },
                name: Ident {
                    text: "principal".to_string(),
                    span: zero_span,
                },
            },
            Field {
                mode: ModeExpr::Named {
                    name: Ident {
                        text: "Role".to_string(),
                        span: zero_span,
                    },
                    args: vec![],
                },
                name: Ident {
                    text: "role".to_string(),
                    span: zero_span,
                },
            },
            Field {
                mode: ModeExpr::Named {
                    name: Ident {
                        text: "Version".to_string(),
                        span: zero_span,
                    },
                    args: vec![],
                },
                name: Ident {
                    text: "version".to_string(),
                    span: zero_span,
                },
            },
        ]),
        span: zero_span,
    }));

    decls
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_hit() {
        let profile = lookup("oddities/profiles/todo_standard", "1.0");
        assert!(profile.is_some());
        let p = profile.unwrap();
        assert_eq!(p.name, "oddities/profiles/todo_standard");
        assert_eq!(p.version, "1.0");
    }

    #[test]
    fn test_lookup_miss() {
        let profile = lookup("unknown/profile", "1.0");
        assert!(profile.is_none());
    }

    #[test]
    fn test_lookup_wrong_version() {
        let profile = lookup("oddities/profiles/todo_standard", "2.0");
        assert!(profile.is_none());
    }

    #[test]
    fn test_params_structure() {
        let profile = lookup("oddities/profiles/todo_standard", "1.0").unwrap();
        assert_eq!(profile.params.len(), 2);
        assert_eq!(profile.params[0].key, "sharing_limit");
        assert_eq!(profile.params[1].key, "identity_provider");
        assert!(matches!(profile.params[0].default, ParamDefault::Required));
        assert!(matches!(profile.params[1].default, ParamDefault::Required));
    }

    #[test]
    fn test_generator_output() {
        let profile = lookup("oddities/profiles/todo_standard", "1.0").unwrap();
        let args = vec![];
        let decls = (profile.generate)(&args);

        assert_eq!(decls.len(), 4);

        // Check that all four are Mode declarations
        for decl in &decls {
            assert!(matches!(decl, Decl::Mode(_)));
        }

        // Check mode names
        if let Decl::Mode(mode_decl) = &decls[0] {
            assert_eq!(mode_decl.name.text, "Cursor");
        }
        if let Decl::Mode(mode_decl) = &decls[1] {
            assert_eq!(mode_decl.name.text, "Page");
        }
        if let Decl::Mode(mode_decl) = &decls[2] {
            assert_eq!(mode_decl.name.text, "Membership");
        }
        if let Decl::Mode(mode_decl) = &decls[3] {
            assert_eq!(mode_decl.name.text, "Invitation");
        }
    }

    #[test]
    fn test_membership_struct_fields() {
        let profile = lookup("oddities/profiles/todo_standard", "1.0").unwrap();
        let args = vec![];
        let decls = (profile.generate)(&args);

        if let Decl::Mode(mode_decl) = &decls[2] {
            if let ModeExpr::Struct(fields) = &mode_decl.expr {
                assert_eq!(fields.len(), 4);
                assert_eq!(fields[0].name.text, "list");
                assert_eq!(fields[1].name.text, "principal");
                assert_eq!(fields[2].name.text, "role");
                assert_eq!(fields[3].name.text, "version");
            } else {
                panic!("Membership should be a struct");
            }
        } else {
            panic!("Membership should be a Mode declaration");
        }
    }

    #[test]
    fn test_invitation_struct_fields() {
        let profile = lookup("oddities/profiles/todo_standard", "1.0").unwrap();
        let args = vec![];
        let decls = (profile.generate)(&args);

        if let Decl::Mode(mode_decl) = &decls[3] {
            if let ModeExpr::Struct(fields) = &mode_decl.expr {
                assert_eq!(fields.len(), 4);
                assert_eq!(fields[0].name.text, "list");
                assert_eq!(fields[1].name.text, "principal");
                assert_eq!(fields[2].name.text, "role");
                assert_eq!(fields[3].name.text, "version");
            } else {
                panic!("Invitation should be a struct");
            }
        } else {
            panic!("Invitation should be a Mode declaration");
        }
    }

    #[test]
    fn test_cursor_opaque() {
        let profile = lookup("oddities/profiles/todo_standard", "1.0").unwrap();
        let args = vec![];
        let decls = (profile.generate)(&args);

        if let Decl::Mode(mode_decl) = &decls[0] {
            if let ModeExpr::Opaque(inner) = &mode_decl.expr {
                if let ModeExpr::Named { name, args } = &**inner {
                    assert_eq!(name.text, "text");
                    assert!(args.is_empty());
                } else {
                    panic!("Cursor inner should be Named");
                }
            } else {
                panic!("Cursor should be Opaque");
            }
        } else {
            panic!("Cursor should be a Mode declaration");
        }
    }

    #[test]
    fn test_page_opaque() {
        let profile = lookup("oddities/profiles/todo_standard", "1.0").unwrap();
        let args = vec![];
        let decls = (profile.generate)(&args);

        if let Decl::Mode(mode_decl) = &decls[1] {
            if let ModeExpr::Opaque(inner) = &mode_decl.expr {
                if let ModeExpr::Named { name, args } = &**inner {
                    assert_eq!(name.text, "text");
                    assert!(args.is_empty());
                } else {
                    panic!("Page inner should be Named");
                }
            } else {
                panic!("Page should be Opaque");
            }
        } else {
            panic!("Page should be a Mode declaration");
        }
    }
}
