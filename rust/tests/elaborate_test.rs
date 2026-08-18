use gymnast_rs::ast::*;
use gymnast_rs::elaborate;
use gymnast_rs::ir::IrNode;
use gymnast_rs::sexpr::Sexpr;
use gymnast_rs::span::Span;

fn spec_todo() -> SpecDecl {
    SpecDecl {
        name: Ident {
            text: "todo".to_string(),
            span: Span { start: 0, end: 0 },
        },
        version: "0.1".to_string(),
        owner: Ident {
            text: "owner".to_string(),
            span: Span { start: 0, end: 0 },
        },
        exports: vec![],
        span: Span { start: 0, end: 0 },
    }
}

#[test]
fn test_mode_semantic_id_format() {
    let mode = ModeDecl {
        name: Ident {
            text: "TaskId".to_string(),
            span: Span { start: 0, end: 0 },
        },
        expr: ModeExpr::Opaque(Box::new(ModeExpr::Named {
            name: Ident {
                text: "text".to_string(),
                span: Span { start: 0, end: 0 },
            },
            args: vec![],
        })),
        span: Span { start: 0, end: 0 },
    };

    let file = File {
        spec: spec_todo(),
        decls: vec![Decl::Mode(mode)],
    };

    let ir = elaborate::elaborate(&file);
    assert_eq!(ir.design.len(), 1);
    assert_eq!(ir.design[0].id, "todo/type/TaskId");
    assert_eq!(ir.design[0].kind, "type");
    assert_eq!(ir.design[0].name, "TaskId");
}

#[test]
fn test_duplicate_mode_yields_errors() {
    let mode1 = ModeDecl {
        name: Ident {
            text: "TaskId".to_string(),
            span: Span { start: 0, end: 0 },
        },
        expr: ModeExpr::Opaque(Box::new(ModeExpr::Named {
            name: Ident {
                text: "text".to_string(),
                span: Span { start: 0, end: 0 },
            },
            args: vec![],
        })),
        span: Span { start: 0, end: 0 },
    };

    let mode2 = ModeDecl {
        name: Ident {
            text: "TaskId".to_string(),
            span: Span { start: 0, end: 0 },
        },
        expr: ModeExpr::Named {
            name: Ident {
                text: "int".to_string(),
                span: Span { start: 0, end: 0 },
            },
            args: vec![],
        },
        span: Span { start: 0, end: 0 },
    };

    let file = File {
        spec: spec_todo(),
        decls: vec![Decl::Mode(mode1), Decl::Mode(mode2)],
    };

    let ir = elaborate::elaborate(&file);
    // Should have E201 from checker
    let error_codes: Vec<String> = ir
        .diagnostics
        .iter()
        .filter_map(|d| {
            if let Sexpr::List(items) = d {
                let mut severity_is_error = false;
                let mut code = String::new();
                for item in items {
                    if let Sexpr::List(pair) = item {
                        if pair.len() == 2 {
                            if let Sexpr::Sym(key) = &pair[0] {
                                if key == "severity" {
                                    if let Sexpr::Sym(severity) = &pair[1] {
                                        if severity == "error" {
                                            severity_is_error = true;
                                        }
                                    }
                                }
                                if key == "code" {
                                    if let Sexpr::Str(c) = &pair[1] {
                                        code = c.clone();
                                    }
                                }
                            }
                        }
                    }
                }
                if severity_is_error && !code.is_empty() {
                    return Some(code);
                }
            }
            None
        })
        .collect();

    // Should have E201 from checker for duplicate
    assert!(
        error_codes.contains(&"E201".to_string()),
        "Expected E201, got: {:?}",
        error_codes
    );
}

#[test]
fn test_interface_lowers_with_clauses() {
    let iface = InterfaceDecl {
        name: Ident {
            text: "TodoService".to_string(),
            span: Span { start: 0, end: 0 },
        },
        default_actor: Ident {
            text: "user".to_string(),
            span: Span { start: 0, end: 0 },
        },
        ops: vec![
            OpDecl {
                kind: OpKind::Cmd,
                name: Ident {
                    text: "create_task".to_string(),
                    span: Span { start: 0, end: 0 },
                },
                params: vec![],
                output: ModeExpr::Named {
                    name: Ident {
                        text: "Task".to_string(),
                        span: Span { start: 0, end: 0 },
                    },
                    args: vec![],
                },
                errors: vec![],
                span: Span { start: 0, end: 0 },
            },
            OpDecl {
                kind: OpKind::Qry,
                name: Ident {
                    text: "query_tasks".to_string(),
                    span: Span { start: 0, end: 0 },
                },
                params: vec![],
                output: ModeExpr::Named {
                    name: Ident {
                        text: "Page".to_string(),
                        span: Span { start: 0, end: 0 },
                    },
                    args: vec![],
                },
                errors: vec![],
                span: Span { start: 0, end: 0 },
            },
        ],
        span: Span { start: 0, end: 0 },
    };

    let file = File {
        spec: spec_todo(),
        decls: vec![Decl::Interface(iface)],
    };

    let ir = elaborate::elaborate(&file);
    assert_eq!(ir.design.len(), 1);
    assert_eq!(ir.design[0].kind, "interface");
    assert_eq!(ir.design[0].name, "TodoService");
    assert_eq!(ir.design[0].clauses.len(), 2);

    // Check :for field
    let has_for_field = ir.design[0].fields.iter().any(|(key, _val)| key == ":for");
    assert!(has_for_field);
}

#[test]
fn test_behavior_on_field_format() {
    let behavior = BehaviorDecl {
        name: Ident {
            text: "create_task_impl".to_string(),
            span: Span { start: 0, end: 0 },
        },
        on_interface: Ident {
            text: "TodoService".to_string(),
            span: Span { start: 0, end: 0 },
        },
        on_op: Ident {
            text: "create_task".to_string(),
            span: Span { start: 0, end: 0 },
        },
        binders: vec![
            Ident {
                text: "user".to_string(),
                span: Span { start: 0, end: 0 },
            },
            Ident {
                text: "request".to_string(),
                span: Span { start: 0, end: 0 },
            },
        ],
        attrs: vec![],
        clauses: vec![],
        span: Span { start: 0, end: 0 },
    };

    let file = File {
        spec: spec_todo(),
        decls: vec![Decl::Behavior(behavior)],
    };

    let ir = elaborate::elaborate(&file);
    assert_eq!(ir.transitions.len(), 1);
    assert_eq!(ir.transitions[0].kind, "behavior");

    // Check :on field
    let on_field = ir.transitions[0]
        .fields
        .iter()
        .find(|(key, _)| key == ":on")
        .map(|(_, val)| val);

    assert!(on_field.is_some());
    if let Some(Sexpr::List(items)) = on_field {
        assert!(items.len() >= 3);
        if let Sexpr::Sym(iface_op) = &items[0] {
            assert!(iface_op.contains("TodoService"));
            assert!(iface_op.contains("create_task"));
        }
    }
}

#[test]
fn test_fails_clause_with_preserves() {
    let behavior = BehaviorDecl {
        name: Ident {
            text: "create_task_impl".to_string(),
            span: Span { start: 0, end: 0 },
        },
        on_interface: Ident {
            text: "TodoService".to_string(),
            span: Span { start: 0, end: 0 },
        },
        on_op: Ident {
            text: "create_task".to_string(),
            span: Span { start: 0, end: 0 },
        },
        binders: vec![],
        attrs: vec![],
        clauses: vec![Clause::Fails {
            error: Ident {
                text: "forbidden".to_string(),
                span: Span { start: 0, end: 0 },
            },
            when: Pred::Word(Ident {
                text: "no_permission".to_string(),
                span: Span { start: 0, end: 0 },
            }),
            preserves: Some(Ident {
                text: "state".to_string(),
                span: Span { start: 0, end: 0 },
            }),
        }],
        span: Span { start: 0, end: 0 },
    };

    let file = File {
        spec: spec_todo(),
        decls: vec![Decl::Behavior(behavior)],
    };

    let ir = elaborate::elaborate(&file);
    assert_eq!(ir.transitions[0].clauses.len(), 1);
    let clause_str = ir.transitions[0].clauses[0].print();
    assert!(clause_str.contains("fails"));
    assert!(clause_str.contains("forbidden"));
    assert!(clause_str.contains(":preserves"));
}

#[test]
fn test_fails_clause_without_preserves() {
    let behavior = BehaviorDecl {
        name: Ident {
            text: "create_task_impl".to_string(),
            span: Span { start: 0, end: 0 },
        },
        on_interface: Ident {
            text: "TodoService".to_string(),
            span: Span { start: 0, end: 0 },
        },
        on_op: Ident {
            text: "create_task".to_string(),
            span: Span { start: 0, end: 0 },
        },
        binders: vec![],
        attrs: vec![],
        clauses: vec![Clause::Fails {
            error: Ident {
                text: "conflict".to_string(),
                span: Span { start: 0, end: 0 },
            },
            when: Pred::Word(Ident {
                text: "already_exists".to_string(),
                span: Span { start: 0, end: 0 },
            }),
            preserves: None,
        }],
        span: Span { start: 0, end: 0 },
    };

    let file = File {
        spec: spec_todo(),
        decls: vec![Decl::Behavior(behavior)],
    };

    let ir = elaborate::elaborate(&file);
    assert_eq!(ir.transitions[0].clauses.len(), 1);
    let clause_str = ir.transitions[0].clauses[0].print();
    assert!(clause_str.contains("fails"));
    assert!(clause_str.contains("conflict"));
    assert!(!clause_str.contains(":preserves"));
}

#[test]
fn test_forall_invariant_lowering() {
    let inv = InvariantDecl {
        name: Ident {
            text: "task_count".to_string(),
            span: Span { start: 0, end: 0 },
        },
        scope: Ident {
            text: "todo_state".to_string(),
            span: Span { start: 0, end: 0 },
        },
        always: Pred::ForAll {
            mode: Ident {
                text: "Task".to_string(),
                span: Span { start: 0, end: 0 },
            },
            var: Ident {
                text: "t".to_string(),
                span: Span { start: 0, end: 0 },
            },
            body: Box::new(Pred::Word(Ident {
                text: "valid".to_string(),
                span: Span { start: 0, end: 0 },
            })),
        },
        span: Span { start: 0, end: 0 },
    };

    let file = File {
        spec: spec_todo(),
        decls: vec![Decl::Invariant(inv)],
    };

    let ir = elaborate::elaborate(&file);
    assert_eq!(ir.obligations.len(), 1);
    assert_eq!(ir.obligations[0].kind, "invariant");

    let fields_map: std::collections::HashMap<String, &Sexpr> = ir.obligations[0]
        .fields
        .iter()
        .map(|(k, v)| (k.clone(), v))
        .collect();

    assert!(fields_map.contains_key(":always"));
    if let Some(Sexpr::List(items)) = fields_map.get(":always") {
        let s = Sexpr::List(items.clone()).print();
        assert!(s.contains("forall"));
    }
}

#[test]
fn test_unknown_profile_warning() {
    let use_decl = UseDecl {
        path: vec![
            Ident {
                text: "unknown".to_string(),
                span: Span { start: 0, end: 0 },
            },
            Ident {
                text: "profile".to_string(),
                span: Span { start: 0, end: 0 },
            },
        ],
        version: "1.0".to_string(),
        args: vec![],
        span: Span { start: 0, end: 0 },
    };

    let file = File {
        spec: spec_todo(),
        decls: vec![Decl::Use(use_decl)],
    };

    let ir = elaborate::elaborate(&file);

    // Should have W303 warning
    let has_warning = ir.diagnostics.iter().any(|d| {
        if let Sexpr::List(items) = d {
            for item in items {
                if let Sexpr::List(pair) = item {
                    if pair.len() == 2 {
                        if let Sexpr::Sym(key) = &pair[0] {
                            if key == "code" {
                                if let Sexpr::Str(code) = &pair[1] {
                                    return code == "W303";
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    });
    assert!(has_warning);

    // Import node should still be present
    assert!(ir.design.iter().any(|n| n.kind == "import"));
}

#[test]
fn test_profile_expansion_generates_modes() {
    let use_decl = UseDecl {
        path: vec![
            Ident {
                text: "oddities".to_string(),
                span: Span { start: 0, end: 0 },
            },
            Ident {
                text: "profiles".to_string(),
                span: Span { start: 0, end: 0 },
            },
            Ident {
                text: "todo_standard".to_string(),
                span: Span { start: 0, end: 0 },
            },
        ],
        version: "1.0".to_string(),
        args: vec![
            PackItem {
                key: Ident {
                    text: "sharing_limit".to_string(),
                    span: Span { start: 0, end: 0 },
                },
                value: PackValue::Int(256),
                span: Span { start: 0, end: 0 },
            },
            PackItem {
                key: Ident {
                    text: "identity_provider".to_string(),
                    span: Span { start: 0, end: 0 },
                },
                value: PackValue::Word(Ident {
                    text: "google".to_string(),
                    span: Span { start: 0, end: 0 },
                }),
                span: Span { start: 0, end: 0 },
            },
        ],
        span: Span { start: 0, end: 0 },
    };

    let file = File {
        spec: spec_todo(),
        decls: vec![Decl::Use(use_decl)],
    };

    let ir = elaborate::elaborate(&file);

    // Should have both the import node and the 4 generated modes
    let type_nodes: Vec<&_> = ir.design.iter().filter(|n| n.kind == "type").collect();
    assert_eq!(type_nodes.len(), 4);

    // Generated modes should be Cursor, Page, Membership, Invitation
    let names: Vec<&str> = type_nodes.iter().map(|n| n.name.as_str()).collect();
    assert!(names.contains(&"Cursor"));
    assert!(names.contains(&"Page"));
    assert!(names.contains(&"Membership"));
    assert!(names.contains(&"Invitation"));
}

#[test]
fn test_expansion_order_and_sorting() {
    let use_decl = UseDecl {
        path: vec![
            Ident {
                text: "oddities".to_string(),
                span: Span { start: 0, end: 0 },
            },
            Ident {
                text: "profiles".to_string(),
                span: Span { start: 0, end: 0 },
            },
            Ident {
                text: "todo_standard".to_string(),
                span: Span { start: 0, end: 0 },
            },
        ],
        version: "1.0".to_string(),
        args: vec![
            PackItem {
                key: Ident {
                    text: "sharing_limit".to_string(),
                    span: Span { start: 0, end: 0 },
                },
                value: PackValue::Int(256),
                span: Span { start: 0, end: 0 },
            },
            PackItem {
                key: Ident {
                    text: "identity_provider".to_string(),
                    span: Span { start: 0, end: 0 },
                },
                value: PackValue::Word(Ident {
                    text: "google".to_string(),
                    span: Span { start: 0, end: 0 },
                }),
                span: Span { start: 0, end: 0 },
            },
        ],
        span: Span { start: 0, end: 0 },
    };

    let mode_z = ModeDecl {
        name: Ident {
            text: "ZebraMode".to_string(),
            span: Span { start: 0, end: 0 },
        },
        expr: ModeExpr::Named {
            name: Ident {
                text: "text".to_string(),
                span: Span { start: 0, end: 0 },
            },
            args: vec![],
        },
        span: Span { start: 0, end: 0 },
    };

    let file = File {
        spec: spec_todo(),
        decls: vec![Decl::Use(use_decl), Decl::Mode(mode_z)],
    };

    let ir = elaborate::elaborate(&file);

    // Generated nodes and declared nodes should all be sorted by id
    let all_types: Vec<&IrNode> = ir.design.iter().filter(|n| n.kind == "type").collect();
    assert!(all_types.len() >= 5); // 4 generated + 1 declared

    // Check that they're sorted
    for i in 1..all_types.len() {
        assert!(all_types[i - 1].id <= all_types[i].id);
    }
}

#[test]
fn test_mode_opaque_shape() {
    let mode = ModeDecl {
        name: Ident {
            text: "UserId".to_string(),
            span: Span { start: 0, end: 0 },
        },
        expr: ModeExpr::Opaque(Box::new(ModeExpr::Named {
            name: Ident {
                text: "text".to_string(),
                span: Span { start: 0, end: 0 },
            },
            args: vec![],
        })),
        span: Span { start: 0, end: 0 },
    };

    let file = File {
        spec: spec_todo(),
        decls: vec![Decl::Mode(mode)],
    };

    let ir = elaborate::elaborate(&file);
    assert_eq!(ir.design.len(), 1);

    let fields_map: std::collections::HashMap<String, &Sexpr> = ir.design[0]
        .fields
        .iter()
        .map(|(k, v)| (k.clone(), v))
        .collect();

    assert!(fields_map.contains_key(":opaque"));
}

#[test]
fn test_mode_enum_shape() {
    let mode = ModeDecl {
        name: Ident {
            text: "Status".to_string(),
            span: Span { start: 0, end: 0 },
        },
        expr: ModeExpr::Enum(vec![
            Ident {
                text: "open".to_string(),
                span: Span { start: 0, end: 0 },
            },
            Ident {
                text: "closed".to_string(),
                span: Span { start: 0, end: 0 },
            },
        ]),
        span: Span { start: 0, end: 0 },
    };

    let file = File {
        spec: spec_todo(),
        decls: vec![Decl::Mode(mode)],
    };

    let ir = elaborate::elaborate(&file);
    assert_eq!(ir.design.len(), 1);

    let fields_map: std::collections::HashMap<String, &Sexpr> = ir.design[0]
        .fields
        .iter()
        .map(|(k, v)| (k.clone(), v))
        .collect();

    assert!(fields_map.contains_key(":opaque") || fields_map.contains_key(":enum"));
}

#[test]
fn test_missing_required_profile_param() {
    let use_decl = UseDecl {
        path: vec![
            Ident {
                text: "oddities".to_string(),
                span: Span { start: 0, end: 0 },
            },
            Ident {
                text: "profiles".to_string(),
                span: Span { start: 0, end: 0 },
            },
            Ident {
                text: "todo_standard".to_string(),
                span: Span { start: 0, end: 0 },
            },
        ],
        version: "1.0".to_string(),
        args: vec![
            PackItem {
                key: Ident {
                    text: "sharing_limit".to_string(),
                    span: Span { start: 0, end: 0 },
                },
                value: PackValue::Int(256),
                span: Span { start: 0, end: 0 },
            },
            // Missing identity_provider
        ],
        span: Span { start: 0, end: 0 },
    };

    let file = File {
        spec: spec_todo(),
        decls: vec![Decl::Use(use_decl)],
    };

    let ir = elaborate::elaborate(&file);

    // Should have E302 error
    let has_e302 = ir.diagnostics.iter().any(|d| {
        if let Sexpr::List(items) = d {
            for item in items {
                if let Sexpr::List(pair) = item {
                    if pair.len() == 2 {
                        if let Sexpr::Sym(key) = &pair[0] {
                            if key == "code" {
                                if let Sexpr::Str(code) = &pair[1] {
                                    return code == "E302";
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    });
    assert!(has_e302);

    // No generated modes should exist since params were missing
    let type_nodes: Vec<&_> = ir.design.iter().filter(|n| n.kind == "type").collect();
    assert_eq!(type_nodes.len(), 0);
}

#[test]
fn test_constraint_scope_field() {
    let constraint = ConstraintDecl {
        name: Ident {
            text: "capacity_limit".to_string(),
            span: Span { start: 0, end: 0 },
        },
        class: Ident {
            text: "workload".to_string(),
            span: Span { start: 0, end: 0 },
        },
        scope: Ident {
            text: "interface".to_string(),
            span: Span { start: 0, end: 0 },
        },
        under: vec![],
        must: Pred::Word(Ident {
            text: "under_limit".to_string(),
            span: Span { start: 0, end: 0 },
        }),
        span: Span { start: 0, end: 0 },
    };

    let file = File {
        spec: spec_todo(),
        decls: vec![Decl::Constraint(constraint)],
    };

    let ir = elaborate::elaborate(&file);
    assert_eq!(ir.obligations.len(), 1);
    assert_eq!(ir.obligations[0].kind, "constraint");

    let fields_map: std::collections::HashMap<String, &Sexpr> = ir.obligations[0]
        .fields
        .iter()
        .map(|(k, v)| (k.clone(), v))
        .collect();

    assert!(fields_map.contains_key(":scope"));
    assert!(fields_map.contains_key(":class"));
    assert!(fields_map.contains_key(":must"));
}

// Regression tests for the shape-dispatch, provenance, and module-field
// fixes: these assert exact printed forms so a lowering regression cannot
// hide behind a loose structural check.

fn elaborate_source(src: &str) -> gymnast_rs::ir::Ir {
    let (ast, diags) = gymnast_rs::parser::parse(src);
    assert!(
        diags
            .iter()
            .all(|d| d.severity != gymnast_rs::diag::Severity::Error),
        "parse errors: {:?}",
        diags
    );
    elaborate::elaborate(&ast.unwrap())
}

fn node_fields_printed(ir: &gymnast_rs::ir::Ir, id: &str) -> String {
    let node = ir
        .all_nodes()
        .into_iter()
        .find(|n| n.id == id)
        .unwrap_or_else(|| panic!("node {} not found", id));
    Sexpr::List(
        node.fields
            .iter()
            .map(|(k, v)| Sexpr::List(vec![Sexpr::sym(k), v.clone()]))
            .collect(),
    )
    .print()
}

#[test]
fn test_mode_shape_fields_exact() {
    let ir = elaborate_source(
        r#"
spec m = v 0.1 owner o exports A

mode A = opaque text
mode B = enum (x, y)
mode C = union (l local_date, r text)
mode D = struct (A a, text (1..9) b)
"#,
    );
    assert_eq!(node_fields_printed(&ir, "m/type/A"), "((:opaque text))");
    assert_eq!(node_fields_printed(&ir, "m/type/B"), "((:enum (x y)))");
    assert_eq!(
        node_fields_printed(&ir, "m/type/C"),
        "((:variant ((l local_date) (r text))))"
    );
    assert_eq!(
        node_fields_printed(&ir, "m/type/D"),
        "((:record ((a A) (b (text :min 1 :max 9)))))"
    );
}

#[test]
fn test_profile_generated_nodes_carry_source() {
    let ir = elaborate_source(
        r#"
spec m = v 0.1 owner o exports A

use oddities/profiles/todo_standard @ 1.0 (sharing_limit 1, identity_provider g)

mode A = opaque text
mode ListId = opaque text
mode UserId = opaque text
mode Role = enum (r)
mode Version = opaque int
"#,
    );
    let cursor = node_fields_printed(&ir, "m/type/Cursor");
    assert!(
        cursor.contains("(:profile-source oddities/profiles/todo_standard)"),
        "generated node must carry provenance: {}",
        cursor
    );
    let a = node_fields_printed(&ir, "m/type/A");
    assert!(
        !a.contains(":profile-source"),
        "source-authored node must not carry provenance: {}",
        a
    );
}

#[test]
fn test_module_fields_from_spec_header() {
    let ir = elaborate_source(
        r#"
spec m = v 0.3 owner alice exports A, B

mode A = opaque text
mode B = opaque int
"#,
    );
    let printed = Sexpr::List(
        ir.module_fields
            .iter()
            .map(|(k, v)| Sexpr::List(vec![Sexpr::sym(k), v.clone()]))
            .collect(),
    )
    .print();
    assert_eq!(
        printed,
        "((:exports (A B)) (:owner alice) (:version \"0.3\"))"
    );
}
