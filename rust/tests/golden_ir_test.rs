use gymnast_rs::elaborate;
use gymnast_rs::parser;
use gymnast_rs::sexpr;
use std::fs;

#[test]
fn test_todo_ir_deterministic() {
    let src = fs::read_to_string("../examples/todo.gym").expect("Cannot read ../examples/todo.gym");

    // Parse and elaborate twice
    let (ast1, _diags1) = parser::parse(&src);
    let (ast2, _diags2) = parser::parse(&src);

    let file1 = ast1.expect("Failed to parse todo.gym (first)");
    let file2 = ast2.expect("Failed to parse todo.gym (second)");

    let ir1 = elaborate::elaborate(&file1);
    let ir2 = elaborate::elaborate(&file2);

    // Serialize both
    let serialized1 = sexpr::canonical_serialize(&ir1.to_sexpr());
    let serialized2 = sexpr::canonical_serialize(&ir2.to_sexpr());

    // They must be byte-identical
    assert_eq!(
        serialized1, serialized2,
        "IR serializations should be deterministic"
    );
}

#[test]
fn test_todo_ir_no_errors() {
    let src = fs::read_to_string("../examples/todo.gym").expect("Cannot read ../examples/todo.gym");

    let (ast, _diags) = parser::parse(&src);
    let file = ast.expect("Failed to parse todo.gym");

    let ir = elaborate::elaborate(&file);

    // Check: no error diagnostics
    assert!(!ir.has_errors(), "todo.gym should not have errors");

    // After profile expansion, there should be NO W301 warnings (unknown modes)
    // because todo_standard provides Cursor, Page, Membership, Invitation
    for diag in &ir.diagnostics {
        if let gymnast_rs::sexpr::Sexpr::List(items) = diag {
            let mut code = String::new();
            let mut severity = String::new();
            for item in items {
                if let gymnast_rs::sexpr::Sexpr::List(pair) = item {
                    if pair.len() == 2 {
                        if let gymnast_rs::sexpr::Sexpr::Sym(key) = &pair[0] {
                            if key == "code" {
                                if let gymnast_rs::sexpr::Sexpr::Str(c) = &pair[1] {
                                    code = c.clone();
                                }
                            }
                            if key == "severity" {
                                if let gymnast_rs::sexpr::Sexpr::Sym(s) = &pair[1] {
                                    severity = s.clone();
                                }
                            }
                        }
                    }
                }
            }
            if severity == "warning" && code == "W301" {
                panic!("W301 warning found after profile expansion: {:?}", diag);
            }
        }
    }
}

#[test]
fn test_todo_ir_structure() {
    let src = fs::read_to_string("../examples/todo.gym").expect("Cannot read ../examples/todo.gym");

    let (ast, _diags) = parser::parse(&src);
    let file = ast.expect("Failed to parse todo.gym");

    let ir = elaborate::elaborate(&file);

    // Check structural counts
    // Design: 1 import + 1 application + 1 actor + 14 type (10 declared + 4 profile-generated)
    //         + 1 component + 1 interface + 1 state + 1 flow = 21
    assert_eq!(
        ir.design.len(),
        21,
        "Expected 21 design nodes, got: {}. Nodes: {:?}",
        ir.design.len(),
        ir.design
            .iter()
            .map(|n| format!("{}/{}", n.kind, n.name))
            .collect::<Vec<_>>()
    );

    // Transitions: 2 behaviors (create_task, invite_user)
    assert_eq!(
        ir.transitions.len(),
        2,
        "Expected 2 transition nodes, got: {}",
        ir.transitions.len()
    );

    // Obligations: 4 (1 acceptance + 2 invariants + 1 constraint)
    assert_eq!(
        ir.obligations.len(),
        4,
        "Expected 4 obligation nodes, got: {}",
        ir.obligations.len()
    );

    // Synthesis: 1
    assert_eq!(
        ir.synthesis.len(),
        1,
        "Expected 1 synthesis node, got: {}",
        ir.synthesis.len()
    );

    // Verify counts by kind
    let design_imports = ir.design.iter().filter(|n| n.kind == "import").count();
    assert_eq!(design_imports, 1, "Expected 1 import");

    let design_apps = ir.design.iter().filter(|n| n.kind == "application").count();
    assert_eq!(design_apps, 1, "Expected 1 application");

    let design_actors = ir.design.iter().filter(|n| n.kind == "actor").count();
    assert_eq!(design_actors, 1, "Expected 1 actor");

    let design_types = ir.design.iter().filter(|n| n.kind == "type").count();
    assert_eq!(
        design_types, 14,
        "Expected 14 types (10 declared + 4 profile-generated)"
    );

    let design_components = ir.design.iter().filter(|n| n.kind == "component").count();
    assert_eq!(design_components, 1, "Expected 1 component");

    let design_interfaces = ir.design.iter().filter(|n| n.kind == "interface").count();
    assert_eq!(design_interfaces, 1, "Expected 1 interface");

    let design_states = ir.design.iter().filter(|n| n.kind == "state").count();
    assert_eq!(design_states, 1, "Expected 1 state");

    let design_flows = ir.design.iter().filter(|n| n.kind == "flow").count();
    assert_eq!(design_flows, 1, "Expected 1 flow");

    let obs_acceptances = ir
        .obligations
        .iter()
        .filter(|n| n.kind == "acceptance")
        .count();
    assert_eq!(obs_acceptances, 1, "Expected 1 acceptance");

    let obs_invariants = ir
        .obligations
        .iter()
        .filter(|n| n.kind == "invariant")
        .count();
    assert_eq!(obs_invariants, 2, "Expected 2 invariants");

    let obs_constraints = ir
        .obligations
        .iter()
        .filter(|n| n.kind == "constraint")
        .count();
    assert_eq!(obs_constraints, 1, "Expected 1 constraint");
}

#[test]
fn test_todo_ir_matches_golden_fixture() {
    let src = fs::read_to_string("../examples/todo.gym").expect("Cannot read ../examples/todo.gym");

    let (ast, _diags) = parser::parse(&src);
    let file = ast.expect("Failed to parse todo.gym");

    let ir = elaborate::elaborate(&file);

    let serialized = sexpr::canonical_serialize(&ir.to_sexpr());

    let golden = fs::read_to_string("tests/fixtures/todo-ir.sexpr")
        .or_else(|_| fs::read_to_string("fixtures/todo-ir.sexpr"))
        .expect("Cannot read tests/fixtures/todo-ir.sexpr");

    assert_eq!(
        serialized, golden,
        "Elaborated IR does not match golden fixture.\n\
         To regenerate the fixture, run:\n\
         cargo run -- ir ../examples/todo.gym > tests/fixtures/todo-ir.sexpr"
    );
}
