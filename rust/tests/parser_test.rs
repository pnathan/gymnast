use gymnast_rs::ast::*;
use gymnast_rs::parser;

#[test]
fn test_minimal_spec_header() {
    let src = r#"
spec todo = v 0.1 owner product exports UserId
"#;
    let (ast, diags) = parser::parse(src);
    assert!(
        diags.is_empty(),
        "Expected no diagnostics, got: {:?}",
        diags
    );
    assert!(ast.is_some());
    let file = ast.unwrap();
    assert_eq!(file.spec.name.text, "todo");
    assert_eq!(file.spec.version, "0.1");
    assert_eq!(file.spec.owner.text, "product");
    assert_eq!(file.spec.exports.len(), 1);
    assert_eq!(file.spec.exports[0].text, "UserId");
}

#[test]
fn test_spec_header_with_multiple_exports() {
    let src = r#"
spec todo = v 0.1 owner product exports UserId, ListId, TaskId
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    assert_eq!(file.spec.exports.len(), 3);
    assert_eq!(file.spec.exports[0].text, "UserId");
    assert_eq!(file.spec.exports[1].text, "ListId");
    assert_eq!(file.spec.exports[2].text, "TaskId");
}

#[test]
fn test_mode_opaque() {
    let src = r#"
spec test = v 0.1 owner o exports X

mode UserId = opaque text
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    assert_eq!(file.decls.len(), 1);
    match &file.decls[0] {
        Decl::Mode(m) => {
            assert_eq!(m.name.text, "UserId");
            matches!(m.expr, ModeExpr::Opaque(_));
        }
        _ => panic!("Expected Mode declaration"),
    }
}

#[test]
fn test_mode_enum() {
    let src = r#"
spec test = v 0.1 owner o exports X

mode Role = enum (owner, editor, viewer)
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    match &file.decls[0] {
        Decl::Mode(m) => {
            assert_eq!(m.name.text, "Role");
            if let ModeExpr::Enum(variants) = &m.expr {
                assert_eq!(variants.len(), 3);
                assert_eq!(variants[0].text, "owner");
                assert_eq!(variants[1].text, "editor");
                assert_eq!(variants[2].text, "viewer");
            } else {
                panic!("Expected Enum");
            }
        }
        _ => panic!("Expected Mode declaration"),
    }
}

#[test]
fn test_mode_union() {
    let src = r#"
spec test = v 0.1 owner o exports X

mode Due = union (date_only local_date, at zoned_datetime)
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    match &file.decls[0] {
        Decl::Mode(m) => {
            if let ModeExpr::Union(variants) = &m.expr {
                assert_eq!(variants.len(), 2);
                assert_eq!(variants[0].0.text, "date_only");
                assert_eq!(variants[1].0.text, "at");
            } else {
                panic!("Expected Union");
            }
        }
        _ => panic!("Expected Mode declaration"),
    }
}

#[test]
fn test_mode_struct() {
    let src = r#"
spec test = v 0.1 owner o exports X

mode TodoList = struct (
  ListId id,
  text title,
  UserId owner )
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    match &file.decls[0] {
        Decl::Mode(m) => {
            if let ModeExpr::Struct(fields) = &m.expr {
                assert_eq!(fields.len(), 3);
                assert_eq!(fields[0].name.text, "id");
                assert_eq!(fields[1].name.text, "title");
                assert_eq!(fields[2].name.text, "owner");
            } else {
                panic!("Expected Struct");
            }
        }
        _ => panic!("Expected Mode declaration"),
    }
}

#[test]
fn test_mode_opt() {
    let src = r#"
spec test = v 0.1 owner o exports X

mode OptDue = opt Due
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    match &file.decls[0] {
        Decl::Mode(m) => {
            matches!(m.expr, ModeExpr::Opt(_));
        }
        _ => panic!("Expected Mode declaration"),
    }
}

#[test]
fn test_mode_refined_text() {
    let src = r#"
spec test = v 0.1 owner o exports X

mode ShortText = text (1..200)
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    match &file.decls[0] {
        Decl::Mode(m) => {
            if let ModeExpr::Refined { name, lo, hi } = &m.expr {
                assert_eq!(name.text, "text");
                assert_eq!(lo, &Some(1));
                assert_eq!(hi, &Some(200));
            } else {
                panic!("Expected Refined");
            }
        }
        _ => panic!("Expected Mode declaration"),
    }
}

#[test]
fn test_interface_basic() {
    let src = r#"
spec test = v 0.1 owner o exports X

interface todo_service = for user (
  cmd create_task = (ListId list, text title) Task
      ! (unauthenticated, forbidden),
  qry query_tasks = (ListId list) PageTask
      ! (not_found) )
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    match &file.decls[0] {
        Decl::Interface(i) => {
            assert_eq!(i.name.text, "todo_service");
            assert_eq!(i.default_actor.text, "user");
            assert_eq!(i.ops.len(), 2);
            assert_eq!(i.ops[0].name.text, "create_task");
            assert_eq!(i.ops[0].errors.len(), 2);
            assert_eq!(i.ops[1].name.text, "query_tasks");
        }
        _ => panic!("Expected Interface declaration"),
    }
}

#[test]
fn test_behavior_all_clause_kinds() {
    let src = r#"
spec test = v 0.1 owner o exports X

behavior create_task = on todo_service.create_task (user, request) (
  reads tasks, writes results,
  atomic list request.list, idempotency command_key;

  requires authenticated (user);
  ensures post = result;
  returns result;
  fails forbidden when not may_edit (pre, user) preserves all_state;
  emits task_created exactly_once )
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    match &file.decls[0] {
        Decl::Behavior(b) => {
            assert_eq!(b.name.text, "create_task");
            assert_eq!(b.on_interface.text, "todo_service");
            assert_eq!(b.on_op.text, "create_task");
            assert_eq!(b.binders.len(), 2);
            assert!(b.clauses.len() >= 5);
            // Check all clause types are present
            let has_requires = b.clauses.iter().any(|c| matches!(c, Clause::Requires(_)));
            let has_ensures = b.clauses.iter().any(|c| matches!(c, Clause::Ensures(_)));
            let has_returns = b.clauses.iter().any(|c| matches!(c, Clause::Returns(_)));
            let has_fails = b.clauses.iter().any(|c| matches!(c, Clause::Fails { .. }));
            let has_emits = b.clauses.iter().any(|c| matches!(c, Clause::Emits { .. }));
            assert!(has_requires);
            assert!(has_ensures);
            assert!(has_returns);
            assert!(has_fails);
            assert!(has_emits);
        }
        _ => panic!("Expected Behavior declaration"),
    }
}

#[test]
fn test_invariant_with_forall() {
    let src = r#"
spec test = v 0.1 owner o exports X

inv owner_isolation = on todo_state
  always for all TodoList list: no_observation_without (list)
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    match &file.decls[0] {
        Decl::Invariant(i) => {
            assert_eq!(i.name.text, "owner_isolation");
            assert_eq!(i.scope.text, "todo_state");
            if let Pred::ForAll { mode, var, body: _ } = &i.always {
                assert_eq!(mode.text, "TodoList");
                assert_eq!(var.text, "list");
            } else {
                panic!("Expected ForAll in invariant");
            }
        }
        _ => panic!("Expected Invariant declaration"),
    }
}

#[test]
fn test_predicate_precedence_and_or() {
    let src = r#"
spec test = v 0.1 owner o exports X

inv test1 = on s always a or b and c
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    match &file.decls[0] {
        Decl::Invariant(i) => {
            // Should parse as: a or (b and c)
            if let Pred::Or(left, right) = &i.always {
                assert!(matches!(left.as_ref(), Pred::Word(_)));
                assert!(matches!(right.as_ref(), Pred::And(_, _)));
            } else {
                panic!("Expected Or at top level");
            }
        }
        _ => panic!("Expected Invariant declaration"),
    }
}

#[test]
fn test_predicate_not_precedence() {
    let src = r#"
spec test = v 0.1 owner o exports X

inv test1 = on s always not a and b
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    match &file.decls[0] {
        Decl::Invariant(i) => {
            // Should parse as: (not a) and b
            if let Pred::And(left, right) = &i.always {
                assert!(matches!(left.as_ref(), Pred::Not(_)));
                assert!(matches!(right.as_ref(), Pred::Word(_)));
            } else {
                panic!("Expected And at top level");
            }
        }
        _ => panic!("Expected Invariant declaration"),
    }
}

#[test]
fn test_predicate_comparison() {
    let src = r#"
spec test = v 0.1 owner o exports X

inv test1 = on s always x = y
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    match &file.decls[0] {
        Decl::Invariant(i) => {
            if let Pred::Cmp { op, lhs: _, rhs: _ } = &i.always {
                assert_eq!(op, &CmpOp::Eq);
            } else {
                panic!("Expected Cmp");
            }
        }
        _ => panic!("Expected Invariant declaration"),
    }
}

#[test]
fn test_predicate_less_than() {
    let src = r#"
spec test = v 0.1 owner o exports X

inv test1 = on s always x < 100
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    match &file.decls[0] {
        Decl::Invariant(i) => {
            if let Pred::Cmp { op, lhs: _, rhs: _ } = &i.always {
                assert_eq!(op, &CmpOp::Lt);
            } else {
                panic!("Expected Cmp");
            }
        }
        _ => panic!("Expected Invariant declaration"),
    }
}

#[test]
fn test_predicate_less_equal() {
    let src = r#"
spec test = v 0.1 owner o exports X

inv test1 = on s always x <= 100
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    match &file.decls[0] {
        Decl::Invariant(i) => {
            if let Pred::Cmp { op, lhs: _, rhs: _ } = &i.always {
                assert_eq!(op, &CmpOp::Le);
            } else {
                panic!("Expected Cmp");
            }
        }
        _ => panic!("Expected Invariant declaration"),
    }
}

#[test]
fn test_error_recovery_two_malformed() {
    let src = r#"
spec test = v 0.1 owner o exports X

mode X = !! invalid !!

mode Y = enum (a, b)

mode Z = !! also invalid !!

mode W = text
"#;
    let (ast, diags) = parser::parse(src);
    // Should have some error diagnostics
    let error_count = diags
        .iter()
        .filter(|d| d.severity == gymnast_rs::diag::Severity::Error)
        .count();
    assert!(
        error_count >= 2,
        "Expected at least 2 errors, got {}",
        error_count
    );

    // But should still parse valid declarations
    if let Some(file) = ast {
        // Should have parsed at least mode Y and W
        let mode_names: Vec<_> = file
            .decls
            .iter()
            .filter_map(|d| {
                if let Decl::Mode(m) = d {
                    Some(m.name.text.clone())
                } else {
                    None
                }
            })
            .collect();
        assert!(
            mode_names.contains(&"Y".to_string()),
            "Should have parsed mode Y"
        );
        assert!(
            mode_names.contains(&"W".to_string()),
            "Should have parsed mode W"
        );
    }
}

#[test]
fn test_use_declaration() {
    let src = r#"
spec test = v 0.1 owner o exports X

use oddities/profiles/todo_standard @ 1.0 (sharing_limit 256, identity_provider google)
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    match &file.decls[0] {
        Decl::Use(u) => {
            assert_eq!(u.path.len(), 3);
            assert_eq!(u.path[0].text, "oddities");
            assert_eq!(u.path[1].text, "profiles");
            assert_eq!(u.path[2].text, "todo_standard");
            assert_eq!(u.version, "1.0");
            assert_eq!(u.args.len(), 2);
        }
        _ => panic!("Expected Use declaration"),
    }
}

#[test]
fn test_actor_declaration() {
    let src = r#"
spec test = v 0.1 owner o exports X

actor user = person (identity google_openid (issuer, subject))
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    match &file.decls[0] {
        Decl::Actor(a) => {
            assert_eq!(a.name.text, "user");
            assert_eq!(a.kind.text, "person");
            assert_eq!(a.attrs.len(), 1);
        }
        _ => panic!("Expected Actor declaration"),
    }
}

#[test]
fn test_component_declaration() {
    let src = r#"
spec test = v 0.1 owner o exports X

component todo_app = (responsibility "Manage todos", provides todo_service)
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    match &file.decls[0] {
        Decl::Component(c) => {
            assert_eq!(c.name.text, "todo_app");
            assert_eq!(c.attrs.len(), 2);
        }
        _ => panic!("Expected Component declaration"),
    }
}

#[test]
fn test_state_declaration() {
    let src = r#"
spec test = v 0.1 owner o exports X

state todo_state = (of aggregate (TodoList, Task), owner todo_app)
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    match &file.decls[0] {
        Decl::State(s) => {
            assert_eq!(s.name.text, "todo_state");
            assert_eq!(s.attrs.len(), 2);
        }
        _ => panic!("Expected State declaration"),
    }
}

#[test]
fn test_flow_declaration() {
    let src = r#"
spec test = v 0.1 owner o exports X

flow auth_flow = user -> todo_service : cmd (grant authenticated_session)
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    match &file.decls[0] {
        Decl::Flow(f) => {
            assert_eq!(f.name.text, "auth_flow");
            assert_eq!(f.from.text, "user");
            assert_eq!(f.to.text, "todo_service");
            assert_eq!(f.kind.text, "cmd");
        }
        _ => panic!("Expected Flow declaration"),
    }
}

#[test]
fn test_synthesis_declaration() {
    let src = r#"
spec test = v 0.1 owner o exports X

synthesis prototype = target ruby / rails (model small_code_model (class nano))
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    match &file.decls[0] {
        Decl::Synthesis(s) => {
            assert_eq!(s.name.text, "prototype");
            assert_eq!(s.target_lang.text, "ruby");
            assert!(s.target_framework.is_some());
            assert_eq!(s.target_framework.as_ref().unwrap().text, "rails");
        }
        _ => panic!("Expected Synthesis declaration"),
    }
}

#[test]
fn test_acceptance_declaration() {
    let src = r#"
spec test = v 0.1 owner o exports X

acceptance production = of todo_app (
  property create_then_read =
    generate (authenticated_editor actor, valid_task task)
    must contains_equivalent (result, task),

  scenario sharing_boundary = (
    given owner = authenticated_owner;
    when invite_distinct (owner, 256);
    then succeeds;
    when invite_distinct (owner, 257);
    then fails_with sharing_limit ),

  concurrency boundary_race = (actors 500, schedule adversarial)
    must active_and_pending <= 256,

  coverage (every_operation, every_error, boundaries),

  execution (clock virtual, timezone "UTC") )
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty(), "unexpected diagnostics: {:#?}", diags);
    let file = ast.unwrap();
    match &file.decls[0] {
        Decl::Acceptance(a) => {
            assert_eq!(a.name.text, "production");
            assert_eq!(a.subject.text, "todo_app");
            assert_eq!(a.blocks.len(), 5);

            // Scenario steps must be captured, not silently discarded.
            let scenario = a
                .blocks
                .iter()
                .find_map(|b| match b {
                    AcceptanceBlock::Scenario { name, steps }
                        if name.text == "sharing_boundary" =>
                    {
                        Some(steps)
                    }
                    _ => None,
                })
                .expect("scenario block");
            assert_eq!(scenario.len(), 5, "steps: {:#?}", scenario);
            let keys: Vec<&str> = scenario.iter().map(|s| s.key.text.as_str()).collect();
            assert_eq!(keys, vec!["given", "when", "then", "when", "then"]);
            // `given owner = authenticated_owner` is a binding.
            match &scenario[0].value {
                PackValue::Nested(items) => {
                    assert_eq!(items.len(), 1);
                    assert_eq!(items[0].key.text, "owner");
                }
                other => panic!("expected Nested binding, got {:?}", other),
            }
            // `when invite_distinct (owner, 256)` is a call with plain args.
            match &scenario[1].value {
                PackValue::Call { name, args } => {
                    assert_eq!(name.text, "invite_distinct");
                    assert_eq!(args.len(), 2);
                    assert!(matches!(args[1], PackValue::Int(256)));
                }
                other => panic!("expected Call step, got {:?}", other),
            }
        }
        _ => panic!("Expected Acceptance declaration"),
    }
}

#[test]
fn test_malformed_op_recovers_without_hang() {
    // A missing `=` in an op previously made parse_interface loop forever.
    let src = r#"
spec test = v 0.1 owner o exports X

interface svc = for user (
  cmd broken (ListId list) Task ! (conflict),
  qry fine = (ListId list) Task ! (not_found) )

mode After = opaque text
"#;
    let (ast, diags) = parser::parse(src);
    assert!(!diags.is_empty(), "the malformed op must be diagnosed");
    let file = ast.expect("recovery must still produce a file");
    // The valid op and the following declaration both survive.
    let iface = file
        .decls
        .iter()
        .find_map(|d| match d {
            Decl::Interface(i) => Some(i),
            _ => None,
        })
        .expect("interface survives");
    assert!(iface.ops.iter().any(|o| o.name.text == "fine"));
    assert!(file
        .decls
        .iter()
        .any(|d| matches!(d, Decl::Mode(m) if m.name.text == "After")));
}

#[test]
fn test_error_recovery_inside_nested_parens_keeps_later_decls() {
    // Recovery entered at paren depth >= 2 previously swallowed the rest
    // of the file.
    let src = r#"
spec test = v 0.1 owner o exports X

application a = (key (inner 5 = ), other 1)

mode Good = opaque text
mode Also = opaque int
"#;
    let (ast, diags) = parser::parse(src);
    assert!(!diags.is_empty(), "the malformed pack must be diagnosed");
    let file = ast.expect("recovery must still produce a file");
    assert!(
        file.decls
            .iter()
            .any(|d| matches!(d, Decl::Mode(m) if m.name.text == "Good")),
        "decls after the error must survive recovery: {:#?}",
        file.decls
    );
    assert!(file
        .decls
        .iter()
        .any(|d| matches!(d, Decl::Mode(m) if m.name.text == "Also")));
}

#[test]
fn test_version_leading_zero_rejected() {
    let src = "spec test = v 1.05 owner o exports X";
    let (_, diags) = parser::parse(src);
    assert!(
        diags.iter().any(|d| d.code == "E103"),
        "1.05 must not silently collapse to 1.5: {:#?}",
        diags
    );
}

#[test]
fn test_empty_token_stream_does_not_panic() {
    let mut p = parser::Parser::new(Vec::new());
    let file = p.parse_file();
    assert!(file.is_none());
}

#[test]
fn test_constraint_declaration() {
    let src = r#"
spec test = v 0.1 owner o exports X

constraint capacity = workload on service under (virtual_users 500, duration 30 min) must lost_updates = 0 and violations = 0
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    match &file.decls[0] {
        Decl::Constraint(c) => {
            assert_eq!(c.name.text, "capacity");
            assert_eq!(c.class.text, "workload");
            assert_eq!(c.scope.text, "service");
        }
        _ => panic!("Expected Constraint declaration"),
    }
}

#[test]
fn test_pack_quantity() {
    let src = r#"
spec test = v 0.1 owner o exports X

constraint x = workload on s under (duration 30 min, latency 500 ms) must true
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    match &file.decls[0] {
        Decl::Constraint(c) => {
            // Check that quantities were parsed
            let has_quantity = c.under.iter().any(|item| {
                if let PackValue::Quantity { value: _, unit: _ } = item.value {
                    true
                } else {
                    false
                }
            });
            assert!(has_quantity, "Should have parsed quantity values");
        }
        _ => panic!("Expected Constraint declaration"),
    }
}

#[test]
fn test_pack_call_syntax() {
    let src = r#"
spec test = v 0.1 owner o exports X

actor user = person (identity google_openid (issuer, subject))
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    match &file.decls[0] {
        Decl::Actor(a) => {
            assert_eq!(a.attrs.len(), 1);
            if let PackValue::Call { name, args: _ } = &a.attrs[0].value {
                assert_eq!(name.text, "google_openid");
            } else {
                panic!("Expected Call syntax");
            }
        }
        _ => panic!("Expected Actor declaration"),
    }
}

#[test]
fn test_pack_nested_syntax() {
    let src = r#"
spec test = v 0.1 owner o exports X

synthesis prototype = target ruby / rails (
  model small_code_model (class nano, temperature 0, max_attempts 3),
  attempts 3 )
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty(), "unexpected diagnostics: {:#?}", diags);
    let file = ast.unwrap();
    match &file.decls[0] {
        Decl::Synthesis(s) => {
            let model = s
                .attrs
                .iter()
                .find(|i| i.key.text == "model")
                .expect("model attr");
            match &model.value {
                PackValue::Call { name, args } => {
                    assert_eq!(name.text, "small_code_model");
                    // Each key/value argument is a single-item nested pack.
                    assert_eq!(args.len(), 3);
                    match &args[1] {
                        PackValue::Nested(items) => {
                            assert_eq!(items[0].key.text, "temperature");
                            assert!(matches!(items[0].value, PackValue::Int(0)));
                        }
                        other => panic!("expected Nested arg, got {:?}", other),
                    }
                }
                other => panic!("expected Call value, got {:?}", other),
            }
        }
        _ => panic!("Expected Synthesis declaration"),
    }
}

#[test]
fn test_expression_path() {
    let src = r#"
spec test = v 0.1 owner o exports X

inv test1 = on s always request.list = 5
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    match &file.decls[0] {
        Decl::Invariant(i) => {
            if let Pred::Cmp { op: _, lhs, rhs: _ } = &i.always {
                if let Expr::Path(path) = lhs {
                    assert_eq!(path.len(), 2);
                    assert_eq!(path[0].text, "request");
                    assert_eq!(path[1].text, "list");
                } else {
                    panic!("Expected Path expression");
                }
            } else {
                panic!("Expected Cmp");
            }
        }
        _ => panic!("Expected Invariant declaration"),
    }
}

#[test]
fn test_multiple_declarations() {
    let src = r#"
spec app = v 1.0 owner acme exports Thing

application app = (name "MyApp")

actor user = person ()

mode Thing = text
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    assert_eq!(file.decls.len(), 3);
}

#[test]
fn test_behavior_with_pack_attrs() {
    let src = r#"
spec test = v 0.1 owner o exports X

behavior test_behavior = on svc.op (actor, req) (
  reads state1, writes state2, atomic x y;
  requires true )
"#;
    let (ast, diags) = parser::parse(src);
    assert!(diags.is_empty());
    let file = ast.unwrap();
    match &file.decls[0] {
        Decl::Behavior(b) => {
            assert!(b.attrs.len() >= 2);
        }
        _ => panic!("Expected Behavior declaration"),
    }
}
