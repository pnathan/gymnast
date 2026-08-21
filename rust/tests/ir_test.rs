use gymnast_rs::fingerprint;
use gymnast_rs::ir::{Ir, IrNode};
use gymnast_rs::sexpr::Sexpr;

#[test]
fn test_ir_node_new_sorts_fields_by_key() {
    let fields = vec![
        (":zebra".to_string(), Sexpr::sym("z")),
        (":apple".to_string(), Sexpr::sym("a")),
        (":monkey".to_string(), Sexpr::sym("m")),
    ];

    let node = IrNode::new(
        "test/type/Foo".to_string(),
        "type",
        "Foo".to_string(),
        fields,
        vec![],
    );

    assert_eq!(node.fields[0].0, ":apple");
    assert_eq!(node.fields[1].0, ":monkey");
    assert_eq!(node.fields[2].0, ":zebra");
}

#[test]
fn test_partition_design_import() {
    let node = IrNode::new(
        "test/import/profiles".to_string(),
        "import",
        "profiles".to_string(),
        vec![(":version".to_string(), Sexpr::Str("1.0".to_string()))],
        vec![],
    );

    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        vec![node],
        vec![],
        vec![],
        vec![],
        vec![],
    );

    assert_eq!(ir.design.len(), 1);
    assert_eq!(ir.design[0].kind, "import");
    assert_eq!(ir.transitions.len(), 0);
}

#[test]
fn test_partition_design_application() {
    let node = IrNode::new(
        "test/application/MyApp".to_string(),
        "application",
        "MyApp".to_string(),
        vec![],
        vec![],
    );

    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        vec![node],
        vec![],
        vec![],
        vec![],
        vec![],
    );

    assert_eq!(ir.design.len(), 1);
    assert_eq!(ir.design[0].kind, "application");
}

#[test]
fn test_partition_design_actor() {
    let node = IrNode::new(
        "test/actor/User".to_string(),
        "actor",
        "User".to_string(),
        vec![(":kind".to_string(), Sexpr::sym("person"))],
        vec![],
    );

    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        vec![node],
        vec![],
        vec![],
        vec![],
        vec![],
    );

    assert_eq!(ir.design.len(), 1);
    assert_eq!(ir.design[0].kind, "actor");
}

#[test]
fn test_partition_design_type() {
    let node = IrNode::new(
        "test/type/UserId".to_string(),
        "type",
        "UserId".to_string(),
        vec![(":opaque".to_string(), Sexpr::sym("text"))],
        vec![],
    );

    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        vec![node],
        vec![],
        vec![],
        vec![],
        vec![],
    );

    assert_eq!(ir.design.len(), 1);
    assert_eq!(ir.design[0].kind, "type");
}

#[test]
fn test_partition_design_component() {
    let node = IrNode::new(
        "test/component/Core".to_string(),
        "component",
        "Core".to_string(),
        vec![],
        vec![],
    );

    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        vec![node],
        vec![],
        vec![],
        vec![],
        vec![],
    );

    assert_eq!(ir.design.len(), 1);
    assert_eq!(ir.design[0].kind, "component");
}

#[test]
fn test_partition_design_interface() {
    let node = IrNode::new(
        "test/interface/API".to_string(),
        "interface",
        "API".to_string(),
        vec![(":for".to_string(), Sexpr::sym("user"))],
        vec![
            Sexpr::list(vec![
                Sexpr::sym("command"),
                Sexpr::sym("create"),
                Sexpr::sym(":actor"),
                Sexpr::sym("user"),
            ]),
            Sexpr::list(vec![
                Sexpr::sym("query"),
                Sexpr::sym("list"),
                Sexpr::sym(":actor"),
                Sexpr::sym("user"),
            ]),
        ],
    );

    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        vec![node],
        vec![],
        vec![],
        vec![],
        vec![],
    );

    assert_eq!(ir.design.len(), 1);
    assert_eq!(ir.design[0].kind, "interface");
    assert_eq!(ir.design[0].clauses.len(), 2);
}

#[test]
fn test_partition_design_state() {
    let node = IrNode::new(
        "test/state/Current".to_string(),
        "state",
        "Current".to_string(),
        vec![],
        vec![],
    );

    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        vec![node],
        vec![],
        vec![],
        vec![],
        vec![],
    );

    assert_eq!(ir.design.len(), 1);
    assert_eq!(ir.design[0].kind, "state");
}

#[test]
fn test_partition_design_flow() {
    let node = IrNode::new(
        "test/flow/Transition".to_string(),
        "flow",
        "Transition".to_string(),
        vec![
            (":from".to_string(), Sexpr::sym("start")),
            (":kind".to_string(), Sexpr::sym("cmd")),
            (":to".to_string(), Sexpr::sym("end")),
        ],
        vec![],
    );

    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        vec![node],
        vec![],
        vec![],
        vec![],
        vec![],
    );

    assert_eq!(ir.design.len(), 1);
    assert_eq!(ir.design[0].kind, "flow");
}

#[test]
fn test_partition_transitions_behavior() {
    let node = IrNode::new(
        "test/behavior/CreateTask".to_string(),
        "behavior",
        "CreateTask".to_string(),
        vec![(":on".to_string(), Sexpr::sym("api/create"))],
        vec![Sexpr::list(vec![
            Sexpr::sym("requires"),
            Sexpr::sym("authenticated"),
        ])],
    );

    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        vec![],
        vec![node],
        vec![],
        vec![],
        vec![],
    );

    assert_eq!(ir.transitions.len(), 1);
    assert_eq!(ir.transitions[0].kind, "behavior");
    assert_eq!(ir.design.len(), 0);
}

#[test]
fn test_partition_obligations_invariant() {
    let node = IrNode::new(
        "test/invariant/Isolation".to_string(),
        "invariant",
        "Isolation".to_string(),
        vec![(":scope".to_string(), Sexpr::sym("state"))],
        vec![],
    );

    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        vec![],
        vec![],
        vec![node],
        vec![],
        vec![],
    );

    assert_eq!(ir.obligations.len(), 1);
    assert_eq!(ir.obligations[0].kind, "invariant");
}

#[test]
fn test_partition_obligations_constraint() {
    let node = IrNode::new(
        "test/constraint/Capacity".to_string(),
        "constraint",
        "Capacity".to_string(),
        vec![
            (":class".to_string(), Sexpr::sym("workload")),
            (":scope".to_string(), Sexpr::sym("interface")),
        ],
        vec![],
    );

    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        vec![],
        vec![],
        vec![node],
        vec![],
        vec![],
    );

    assert_eq!(ir.obligations.len(), 1);
    assert_eq!(ir.obligations[0].kind, "constraint");
}

#[test]
fn test_partition_obligations_acceptance() {
    let node = IrNode::new(
        "test/acceptance/Production".to_string(),
        "acceptance",
        "Production".to_string(),
        vec![(":subject".to_string(), Sexpr::sym("app"))],
        vec![],
    );

    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        vec![],
        vec![],
        vec![node],
        vec![],
        vec![],
    );

    assert_eq!(ir.obligations.len(), 1);
    assert_eq!(ir.obligations[0].kind, "acceptance");
}

#[test]
fn test_partition_synthesis() {
    let node = IrNode::new(
        "test/synthesis/Prototype".to_string(),
        "synthesis",
        "Prototype".to_string(),
        vec![(
            ":target".to_string(),
            Sexpr::list(vec![Sexpr::sym("ruby"), Sexpr::sym("rails")]),
        )],
        vec![],
    );

    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![node],
        vec![],
    );

    assert_eq!(ir.synthesis.len(), 1);
    assert_eq!(ir.synthesis[0].kind, "synthesis");
}

#[test]
fn test_nodes_sorted_by_id_within_partition() {
    let nodes = vec![
        IrNode::new(
            "test/type/Zebra".to_string(),
            "type",
            "Zebra".to_string(),
            vec![],
            vec![],
        ),
        IrNode::new(
            "test/type/Apple".to_string(),
            "type",
            "Apple".to_string(),
            vec![],
            vec![],
        ),
        IrNode::new(
            "test/type/Monkey".to_string(),
            "type",
            "Monkey".to_string(),
            vec![],
            vec![],
        ),
    ];

    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        nodes,
        vec![],
        vec![],
        vec![],
        vec![],
    );

    assert_eq!(ir.design[0].id, "test/type/Apple");
    assert_eq!(ir.design[1].id, "test/type/Monkey");
    assert_eq!(ir.design[2].id, "test/type/Zebra");
}

#[test]
fn test_fingerprint_recomputation_matches_stored() {
    let node = IrNode::new(
        "test/type/Foo".to_string(),
        "type",
        "Foo".to_string(),
        vec![(":owner".to_string(), Sexpr::sym("app"))],
        vec![],
    );

    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        vec![node],
        vec![],
        vec![],
        vec![],
        vec![],
    );

    // Recompute fingerprint from fingerprint-free sexpr
    let fp_free = Ir::to_sexpr_without_fingerprint(
        &ir.schema,
        &ir.module_name,
        &ir.module_fields,
        &ir.design,
        &ir.transitions,
        &ir.obligations,
        &ir.synthesis,
        &ir.diagnostics,
    );

    let recomputed_fp = fingerprint::fingerprint(&fp_free);
    assert_eq!(ir.fingerprint, recomputed_fp);
}

#[test]
fn test_fingerprint_differs_on_content_change() {
    let node1 = IrNode::new(
        "test/type/Foo".to_string(),
        "type",
        "Foo".to_string(),
        vec![(":owner".to_string(), Sexpr::sym("app"))],
        vec![],
    );

    let node2 = IrNode::new(
        "test/type/Foo".to_string(),
        "type",
        "Foo".to_string(),
        vec![(":owner".to_string(), Sexpr::sym("other"))],
        vec![],
    );

    let ir1 = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        vec![node1],
        vec![],
        vec![],
        vec![],
        vec![],
    );

    let ir2 = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        vec![node2],
        vec![],
        vec![],
        vec![],
        vec![],
    );

    assert_ne!(ir1.fingerprint, ir2.fingerprint);
}

#[test]
fn test_fingerprint_deterministic() {
    let make_ir = || {
        Ir::new(
            "gymnast.ir/0.1".to_string(),
            "test".to_string(),
            vec![],
            vec![IrNode::new(
                "test/type/Foo".to_string(),
                "type",
                "Foo".to_string(),
                vec![(":owner".to_string(), Sexpr::sym("app"))],
                vec![],
            )],
            vec![],
            vec![],
            vec![],
            vec![],
        )
    };

    let ir1 = make_ir();
    let ir2 = make_ir();

    assert_eq!(ir1.fingerprint, ir2.fingerprint);
}

#[test]
fn test_to_sexpr_contains_all_fields() {
    let node = IrNode::new(
        "test/type/Foo".to_string(),
        "type",
        "Foo".to_string(),
        vec![(":owner".to_string(), Sexpr::sym("app"))],
        vec![],
    );

    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![(":version".to_string(), Sexpr::Str("0.1".to_string()))],
        vec![node],
        vec![],
        vec![],
        vec![],
        vec![],
    );

    let sexpr = ir.to_sexpr();
    let printed = sexpr.print();

    // Check that all expected keys are present
    assert!(printed.contains("schema"));
    assert!(printed.contains("module"));
    assert!(printed.contains("design"));
    assert!(printed.contains("transitions"));
    assert!(printed.contains("obligations"));
    assert!(printed.contains("synthesis"));
    assert!(printed.contains("diagnostics"));
    assert!(printed.contains("fingerprint"));
}

#[test]
fn test_ir_node_to_sexpr_structure() {
    let node = IrNode::new(
        "test/type/Foo".to_string(),
        "type",
        "Foo".to_string(),
        vec![(":owner".to_string(), Sexpr::sym("app"))],
        vec![],
    );

    let sexpr = node.to_sexpr();
    let printed = sexpr.print();

    assert!(printed.contains("ir-node"));
    assert!(printed.contains("\"test/type/Foo\""));
    assert!(printed.contains("type"));
    assert!(printed.contains("Foo"));
    assert!(printed.contains(":owner"));
}

#[test]
fn test_ir_node_mechanism_always_parsed() {
    let node = IrNode::new(
        "test/type/Foo".to_string(),
        "type",
        "Foo".to_string(),
        vec![],
        vec![],
    );

    assert_eq!(node.mechanism, "parsed");
}

#[test]
fn test_mixed_partitions() {
    let design_nodes = vec![IrNode::new(
        "test/type/A".to_string(),
        "type",
        "A".to_string(),
        vec![],
        vec![],
    )];

    let transition_nodes = vec![IrNode::new(
        "test/behavior/B".to_string(),
        "behavior",
        "B".to_string(),
        vec![],
        vec![],
    )];

    let obligation_nodes = vec![IrNode::new(
        "test/invariant/C".to_string(),
        "invariant",
        "C".to_string(),
        vec![],
        vec![],
    )];

    let synthesis_nodes = vec![IrNode::new(
        "test/synthesis/D".to_string(),
        "synthesis",
        "D".to_string(),
        vec![],
        vec![],
    )];

    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        design_nodes,
        transition_nodes,
        obligation_nodes,
        synthesis_nodes,
        vec![],
    );

    assert_eq!(ir.all_nodes().len(), 4);
    assert_eq!(ir.design.len(), 1);
    assert_eq!(ir.transitions.len(), 1);
    assert_eq!(ir.obligations.len(), 1);
    assert_eq!(ir.synthesis.len(), 1);
}

#[test]
fn test_diagnostics_with_error_severity() {
    let diag = Sexpr::list(vec![
        Sexpr::list(vec![Sexpr::sym("severity"), Sexpr::sym("error")]),
        Sexpr::list(vec![Sexpr::sym("code"), Sexpr::Str("E301".to_string())]),
    ]);

    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![diag],
    );

    assert!(ir.has_errors());
}

#[test]
fn test_diagnostics_with_warning_severity() {
    let diag = Sexpr::list(vec![
        Sexpr::list(vec![Sexpr::sym("severity"), Sexpr::sym("warning")]),
        Sexpr::list(vec![Sexpr::sym("code"), Sexpr::Str("W303".to_string())]),
    ]);

    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![diag],
    );

    assert!(!ir.has_errors());
}

#[test]
fn test_no_diagnostics() {
    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    );

    assert!(!ir.has_errors());
}

#[test]
fn test_all_nodes_returns_all_partitions() {
    let design = vec![IrNode::new(
        "a/type/Z".to_string(),
        "type",
        "Z".to_string(),
        vec![],
        vec![],
    )];
    let transitions = vec![IrNode::new(
        "a/behavior/Y".to_string(),
        "behavior",
        "Y".to_string(),
        vec![],
        vec![],
    )];
    let obligations = vec![IrNode::new(
        "a/invariant/X".to_string(),
        "invariant",
        "X".to_string(),
        vec![],
        vec![],
    )];
    let synthesis = vec![IrNode::new(
        "a/synthesis/W".to_string(),
        "synthesis",
        "W".to_string(),
        vec![],
        vec![],
    )];

    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        design,
        transitions,
        obligations,
        synthesis,
        vec![],
    );

    let all = ir.all_nodes();
    assert_eq!(all.len(), 4);
}

#[test]
fn test_empty_ir() {
    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    );

    assert_eq!(ir.design.len(), 0);
    assert_eq!(ir.transitions.len(), 0);
    assert_eq!(ir.obligations.len(), 0);
    assert_eq!(ir.synthesis.len(), 0);
    assert_eq!(ir.all_nodes().len(), 0);
}

#[test]
fn test_ir_node_clauses_preserved() {
    let clauses = vec![
        Sexpr::list(vec![Sexpr::sym("requires"), Sexpr::sym("auth")]),
        Sexpr::list(vec![Sexpr::sym("ensures"), Sexpr::sym("success")]),
    ];

    let node = IrNode::new(
        "test/behavior/Foo".to_string(),
        "behavior",
        "Foo".to_string(),
        vec![],
        clauses.clone(),
    );

    assert_eq!(node.clauses.len(), 2);
    assert_eq!(node.clauses[0].print(), clauses[0].print());
    assert_eq!(node.clauses[1].print(), clauses[1].print());
}

#[test]
fn test_module_fields_sorted() {
    let fields = vec![
        (":zebra".to_string(), Sexpr::sym("z")),
        (":apple".to_string(), Sexpr::sym("a")),
        (":monkey".to_string(), Sexpr::sym("m")),
    ];

    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        fields,
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    );

    assert_eq!(ir.module_fields[0].0, ":apple");
    assert_eq!(ir.module_fields[1].0, ":monkey");
    assert_eq!(ir.module_fields[2].0, ":zebra");
}

#[test]
fn test_large_field_sort() {
    let mut fields = vec![];
    for i in 0..100 {
        fields.push((format!(":field{:03}", 100 - i), Sexpr::Int(i as i64)));
    }

    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        fields,
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    );

    // After sorting, should be in ascending order
    for i in 0..99 {
        assert!(ir.module_fields[i].0 < ir.module_fields[i + 1].0);
    }
}

#[test]
fn test_ir_node_semanticid_format() {
    let node = IrNode::new(
        "mymodule/type/TaskId".to_string(),
        "type",
        "TaskId".to_string(),
        vec![],
        vec![],
    );

    assert!(node.id.contains("/"));
    let parts: Vec<&str> = node.id.split('/').collect();
    assert_eq!(parts.len(), 3);
    assert_eq!(parts[0], "mymodule");
    assert_eq!(parts[1], "type");
    assert_eq!(parts[2], "TaskId");
}

#[test]
fn test_ir_node_id_preserved() {
    let id = "mydecl/type/MyType".to_string();
    let node = IrNode::new(id.clone(), "type", "MyType".to_string(), vec![], vec![]);

    assert_eq!(node.id, id);
}

#[test]
fn test_fingerprint_excludes_fingerprint_entry() {
    // Create an IR and verify that its fingerprint doesn't include itself
    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "test".to_string(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    );

    let with_fp_sexpr = ir.to_sexpr();
    let printed = with_fp_sexpr.print();

    // Should contain fingerprint in the output
    assert!(printed.contains("fingerprint"));

    // Now verify that the computed fingerprint is based on fingerprint-free form
    let fp_free = Ir::to_sexpr_without_fingerprint(
        &ir.schema,
        &ir.module_name,
        &ir.module_fields,
        &ir.design,
        &ir.transitions,
        &ir.obligations,
        &ir.synthesis,
        &ir.diagnostics,
    );

    let recomputed = fingerprint::fingerprint(&fp_free);
    assert_eq!(ir.fingerprint, recomputed);
}
