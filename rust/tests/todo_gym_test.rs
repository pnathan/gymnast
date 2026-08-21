use gymnast_rs::diag::Severity;
use gymnast_rs::parser;

#[test]
fn test_todo_gym_parse() {
    let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/todo.gym"));

    let (ast, diags) = parser::parse(src);

    // Assert zero error diagnostics during parsing
    let error_diags: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        error_diags.is_empty(),
        "Expected zero parse errors, found {}:\n{:#?}",
        error_diags.len(),
        error_diags
    );

    // Must have a successful AST
    let file = ast.expect("parse should succeed with zero errors");

    // Check declaration count (spec is stored separately in file.spec, not in file.decls)
    assert_eq!(
        file.decls.len(),
        24,
        "Expected 24 declarations, found {}",
        file.decls.len()
    );

    // Verify Task struct has 9 fields
    let task_mode = file
        .decls
        .iter()
        .find_map(|d| {
            if let gymnast_rs::ast::Decl::Mode(m) = d {
                if m.name.text == "Task" {
                    return Some(m);
                }
            }
            None
        })
        .expect("should find Task mode");

    if let gymnast_rs::ast::ModeExpr::Struct(fields) = &task_mode.expr {
        assert_eq!(
            fields.len(),
            9,
            "Task should have 9 fields, found {}",
            fields.len()
        );
        // Verify field names match expected
        let field_names: Vec<String> = fields.iter().map(|f| f.name.text.clone()).collect();
        assert_eq!(
            field_names,
            vec!["id", "list", "title", "notes", "status", "state", "due", "assignee", "version"]
        );
    } else {
        panic!("Task should be a Struct mode");
    }

    // Verify todo_service interface has 3 ops
    let todo_service = file
        .decls
        .iter()
        .find_map(|d| {
            if let gymnast_rs::ast::Decl::Interface(i) = d {
                if i.name.text == "todo_service" {
                    return Some(i);
                }
            }
            None
        })
        .expect("should find todo_service interface");

    assert_eq!(
        todo_service.ops.len(),
        3,
        "todo_service should have 3 ops, found {}",
        todo_service.ops.len()
    );

    let op_names: Vec<String> = todo_service
        .ops
        .iter()
        .map(|o| o.name.text.clone())
        .collect();
    assert_eq!(op_names, vec!["create_task", "query_tasks", "invite"]);

    // Verify create_task behavior has 6 clauses
    let create_task_behavior = file
        .decls
        .iter()
        .find_map(|d| {
            if let gymnast_rs::ast::Decl::Behavior(b) = d {
                if b.name.text == "create_task" {
                    return Some(b);
                }
            }
            None
        })
        .expect("should find create_task behavior");

    assert_eq!(
        create_task_behavior.clauses.len(),
        6,
        "create_task behavior should have 6 clauses, found {}",
        create_task_behavior.clauses.len()
    );

    // Verify clause kinds in order:
    // requires, requires, ensures, returns, fails, emits
    use gymnast_rs::ast::Clause;

    assert!(matches!(
        create_task_behavior.clauses[0],
        Clause::Requires(_)
    ));
    assert!(matches!(
        create_task_behavior.clauses[1],
        Clause::Requires(_)
    ));
    assert!(matches!(
        create_task_behavior.clauses[2],
        Clause::Ensures(_)
    ));
    assert!(matches!(
        create_task_behavior.clauses[3],
        Clause::Returns(_)
    ));
    assert!(matches!(
        create_task_behavior.clauses[4],
        Clause::Fails { .. }
    ));
    assert!(matches!(
        create_task_behavior.clauses[5],
        Clause::Emits { .. }
    ));
}

#[test]
fn test_todo_gym_check() {
    let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/todo.gym"));

    let (ast, parse_diags) = parser::parse(src);

    // Assert zero parse errors
    let error_diags: Vec<_> = parse_diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        error_diags.is_empty(),
        "Expected zero parse errors, found {}",
        error_diags.len()
    );

    let file = ast.expect("parse should succeed");

    // Check through the full elaboration pipeline: the checker runs over
    // the profile-EXPANDED declarations (checking the raw file would flag
    // the profile-provided modes as unknown, by design — the closed world
    // admits only declared or profile-provided names).
    let (_, all_diags) = gymnast_rs::elaborate::elaborate_with_parse_diags(&file, &[]);
    let check_errors: Vec<_> = all_diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        check_errors.is_empty(),
        "Expected zero check errors, found {}:\n{:#?}",
        check_errors.len(),
        check_errors
    );
}
