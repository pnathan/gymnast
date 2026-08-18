use crate::fingerprint;
use crate::sexpr::Sexpr;

/// One semantic IR node. Fields are canonically sorted by key at
/// construction; clause order is preserved (sequence can be semantic).
#[derive(Debug, Clone, PartialEq)]
pub struct IrNode {
    pub id: String,   // module/kind/name
    pub kind: String, // "type", "behavior", ...
    pub name: String,
    pub fields: Vec<(String, Sexpr)>, // keys like ":owner", sorted
    pub clauses: Vec<Sexpr>,
    pub mechanism: String, // "parsed" for every phase-2 node
}

impl IrNode {
    /// Sorts `fields` by key (byte-wise string order) before storing.
    pub fn new(
        id: String,
        kind: &str,
        name: String,
        mut fields: Vec<(String, Sexpr)>,
        clauses: Vec<Sexpr>,
    ) -> IrNode {
        fields.sort_by(|a, b| a.0.cmp(&b.0));
        IrNode {
            id,
            kind: kind.to_string(),
            name,
            fields,
            clauses,
            mechanism: "parsed".to_string(),
        }
    }

    pub fn to_sexpr(&self) -> Sexpr {
        let mut items = vec![
            Sexpr::pair("id", Sexpr::Str(self.id.clone())),
            Sexpr::pair("kind", Sexpr::sym(&self.kind)),
            Sexpr::pair("name", Sexpr::sym(&self.name)),
        ];

        // Fields as alist
        let mut field_list = Vec::new();
        for (key, value) in &self.fields {
            field_list.push(Sexpr::list(vec![Sexpr::sym(key), value.clone()]));
        }
        items.push(Sexpr::pair("fields", Sexpr::list(field_list)));

        // Clauses
        items.push(Sexpr::pair("clauses", Sexpr::list(self.clauses.clone())));

        // Mechanism
        items.push(Sexpr::pair("mechanism", Sexpr::sym(&self.mechanism)));

        Sexpr::list(vec![Sexpr::sym("ir-node"), Sexpr::list(items)])
    }

    /// Field lookup by exact key (keys carry the leading colon:
    /// `node.field(":owner")`).
    pub fn field(&self, key: &str) -> Option<&Sexpr> {
        self.fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ir {
    pub schema: String, // "gymnast.ir/0.1"
    pub module_name: String,
    pub module_fields: Vec<(String, Sexpr)>, // sorted like node fields
    pub design: Vec<IrNode>,
    pub transitions: Vec<IrNode>,
    pub obligations: Vec<IrNode>,
    pub synthesis: Vec<IrNode>,
    pub diagnostics: Vec<Sexpr>, // already-lowered diagnostics
    pub fingerprint: String,
}

impl Ir {
    pub fn new(
        schema: String,
        module_name: String,
        mut module_fields: Vec<(String, Sexpr)>,
        mut design: Vec<IrNode>,
        mut transitions: Vec<IrNode>,
        mut obligations: Vec<IrNode>,
        mut synthesis: Vec<IrNode>,
        diagnostics: Vec<Sexpr>,
    ) -> Ir {
        // Sort fields by key
        module_fields.sort_by(|a, b| a.0.cmp(&b.0));

        // Sort nodes by id within each partition
        design.sort_by(|a, b| a.id.cmp(&b.id));
        transitions.sort_by(|a, b| a.id.cmp(&b.id));
        obligations.sort_by(|a, b| a.id.cmp(&b.id));
        synthesis.sort_by(|a, b| a.id.cmp(&b.id));

        // Compute fingerprint over to_sexpr without the fingerprint field
        let fingerprint_free = Self::to_sexpr_without_fingerprint(
            &schema,
            &module_name,
            &module_fields,
            &design,
            &transitions,
            &obligations,
            &synthesis,
            &diagnostics,
        );
        let fingerprint = fingerprint::fingerprint(&fingerprint_free);

        Ir {
            schema,
            module_name,
            module_fields,
            design,
            transitions,
            obligations,
            synthesis,
            diagnostics,
            fingerprint,
        }
    }

    pub fn all_nodes(&self) -> Vec<&IrNode> {
        let mut all = Vec::new();
        all.extend(self.design.iter());
        all.extend(self.transitions.iter());
        all.extend(self.obligations.iter());
        all.extend(self.synthesis.iter());
        all
    }

    /// All nodes (any partition) whose `kind` equals `kind`, in
    /// partition order and id-sorted within each partition (the order
    /// `all_nodes` already establishes).
    pub fn nodes_of_kind(&self, kind: &str) -> Vec<&IrNode> {
        self.all_nodes()
            .into_iter()
            .filter(|n| n.kind == kind)
            .collect()
    }

    /// The node (any partition) with the given id, if any.
    pub fn find_node(&self, id: &str) -> Option<&IrNode> {
        self.all_nodes().into_iter().find(|n| n.id == id)
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|diag| {
            diag.assoc("severity")
                .and_then(|s| s.as_sym())
                .map(|s| s == "error")
                .unwrap_or(false)
        })
    }

    pub fn to_sexpr_without_fingerprint(
        schema: &str,
        module_name: &str,
        module_fields: &[(String, Sexpr)],
        design: &[IrNode],
        transitions: &[IrNode],
        obligations: &[IrNode],
        synthesis: &[IrNode],
        diagnostics: &[Sexpr],
    ) -> Sexpr {
        let mut items = vec![Sexpr::pair("schema", Sexpr::Str(schema.to_string()))];

        // Module
        let mut module_items = vec![Sexpr::pair("name", Sexpr::sym(module_name))];
        let mut field_list = Vec::new();
        for (key, value) in module_fields {
            field_list.push(Sexpr::list(vec![Sexpr::sym(key), value.clone()]));
        }
        module_items.push(Sexpr::pair("fields", Sexpr::list(field_list)));
        items.push(Sexpr::pair("module", Sexpr::list(module_items)));

        // Design, transitions, obligations, synthesis
        let mut design_list = Vec::new();
        for node in design {
            design_list.push(node.to_sexpr());
        }
        items.push(Sexpr::pair("design", Sexpr::list(design_list)));

        let mut trans_list = Vec::new();
        for node in transitions {
            trans_list.push(node.to_sexpr());
        }
        items.push(Sexpr::pair("transitions", Sexpr::list(trans_list)));

        let mut oblig_list = Vec::new();
        for node in obligations {
            oblig_list.push(node.to_sexpr());
        }
        items.push(Sexpr::pair("obligations", Sexpr::list(oblig_list)));

        let mut synth_list = Vec::new();
        for node in synthesis {
            synth_list.push(node.to_sexpr());
        }
        items.push(Sexpr::pair("synthesis", Sexpr::list(synth_list)));

        // Diagnostics
        items.push(Sexpr::pair(
            "diagnostics",
            Sexpr::list(diagnostics.to_vec()),
        ));

        Sexpr::list(vec![Sexpr::sym("ir"), Sexpr::list(items)])
    }

    pub fn to_sexpr(&self) -> Sexpr {
        // Build on the fingerprint-free form so the two can never drift:
        // the stored fingerprint must always equal the hash of the
        // serialization minus its fingerprint entry.
        let base = Ir::to_sexpr_without_fingerprint(
            &self.schema,
            &self.module_name,
            &self.module_fields,
            &self.design,
            &self.transitions,
            &self.obligations,
            &self.synthesis,
            &self.diagnostics,
        );
        match base {
            Sexpr::List(mut outer) => {
                if let Some(Sexpr::List(items)) = outer.last_mut() {
                    items.push(Sexpr::pair(
                        "fingerprint",
                        Sexpr::Str(self.fingerprint.clone()),
                    ));
                }
                Sexpr::List(outer)
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ir_node_sorts_fields() {
        let fields = vec![
            (":z".to_string(), Sexpr::sym("last")),
            (":a".to_string(), Sexpr::sym("first")),
            (":m".to_string(), Sexpr::sym("middle")),
        ];
        let node = IrNode::new(
            "test/type/Foo".to_string(),
            "type",
            "Foo".to_string(),
            fields,
            vec![],
        );

        assert_eq!(node.fields[0].0, ":a");
        assert_eq!(node.fields[1].0, ":m");
        assert_eq!(node.fields[2].0, ":z");
    }

    #[test]
    fn test_ir_node_to_sexpr() {
        let node = IrNode::new(
            "test/type/Foo".to_string(),
            "type",
            "Foo".to_string(),
            vec![(":owner".to_string(), Sexpr::sym("app"))],
            vec![],
        );

        let sexpr = node.to_sexpr();
        let s = sexpr.print();

        assert!(s.contains("ir-node"));
        assert!(s.contains("\"test/type/Foo\""));
        assert!(s.contains("type"));
        assert!(s.contains("Foo"));
        assert!(s.contains(":owner"));
    }

    #[test]
    fn test_design_node_partition() {
        let nodes = vec![IrNode::new(
            "test/type/Mode".to_string(),
            "type",
            "Mode".to_string(),
            vec![],
            vec![],
        )];

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

        assert_eq!(ir.design.len(), 1);
        assert_eq!(ir.design[0].kind, "type");
    }

    #[test]
    fn test_transitions_node_partition() {
        let nodes = vec![IrNode::new(
            "test/behavior/Act".to_string(),
            "behavior",
            "Act".to_string(),
            vec![],
            vec![],
        )];

        let ir = Ir::new(
            "gymnast.ir/0.1".to_string(),
            "test".to_string(),
            vec![],
            vec![],
            nodes,
            vec![],
            vec![],
            vec![],
        );

        assert_eq!(ir.transitions.len(), 1);
        assert_eq!(ir.transitions[0].kind, "behavior");
    }

    #[test]
    fn test_obligations_node_partition() {
        let nodes = vec![IrNode::new(
            "test/invariant/Rule".to_string(),
            "invariant",
            "Rule".to_string(),
            vec![],
            vec![],
        )];

        let ir = Ir::new(
            "gymnast.ir/0.1".to_string(),
            "test".to_string(),
            vec![],
            vec![],
            vec![],
            nodes,
            vec![],
            vec![],
        );

        assert_eq!(ir.obligations.len(), 1);
        assert_eq!(ir.obligations[0].kind, "invariant");
    }

    #[test]
    fn test_synthesis_node_partition() {
        let nodes = vec![IrNode::new(
            "test/synthesis/Proto".to_string(),
            "synthesis",
            "Proto".to_string(),
            vec![],
            vec![],
        )];

        let ir = Ir::new(
            "gymnast.ir/0.1".to_string(),
            "test".to_string(),
            vec![],
            vec![],
            vec![],
            vec![],
            nodes,
            vec![],
        );

        assert_eq!(ir.synthesis.len(), 1);
        assert_eq!(ir.synthesis[0].kind, "synthesis");
    }

    #[test]
    fn test_nodes_sorted_by_id() {
        let nodes = vec![
            IrNode::new(
                "test/type/Z".to_string(),
                "type",
                "Z".to_string(),
                vec![],
                vec![],
            ),
            IrNode::new(
                "test/type/A".to_string(),
                "type",
                "A".to_string(),
                vec![],
                vec![],
            ),
            IrNode::new(
                "test/type/M".to_string(),
                "type",
                "M".to_string(),
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

        assert_eq!(ir.design[0].id, "test/type/A");
        assert_eq!(ir.design[1].id, "test/type/M");
        assert_eq!(ir.design[2].id, "test/type/Z");
    }

    #[test]
    fn test_all_nodes() {
        let design = vec![IrNode::new(
            "test/type/Foo".to_string(),
            "type",
            "Foo".to_string(),
            vec![],
            vec![],
        )];
        let transitions = vec![IrNode::new(
            "test/behavior/Bar".to_string(),
            "behavior",
            "Bar".to_string(),
            vec![],
            vec![],
        )];
        let obligations = vec![IrNode::new(
            "test/invariant/Baz".to_string(),
            "invariant",
            "Baz".to_string(),
            vec![],
            vec![],
        )];
        let synthesis = vec![IrNode::new(
            "test/synthesis/Qux".to_string(),
            "synthesis",
            "Qux".to_string(),
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
    fn test_has_errors_false() {
        let ir = Ir::new(
            "gymnast.ir/0.1".to_string(),
            "test".to_string(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![Sexpr::list(vec![Sexpr::list(vec![
                Sexpr::sym("severity"),
                Sexpr::sym("warning"),
            ])])],
        );

        assert!(!ir.has_errors());
    }

    #[test]
    fn test_has_errors_true() {
        let ir = Ir::new(
            "gymnast.ir/0.1".to_string(),
            "test".to_string(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![Sexpr::list(vec![Sexpr::list(vec![
                Sexpr::sym("severity"),
                Sexpr::sym("error"),
            ])])],
        );

        assert!(ir.has_errors());
    }

    #[test]
    fn test_fingerprint_consistency() {
        let ir1 = Ir::new(
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
        );

        let ir2 = Ir::new(
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
        );

        assert_eq!(ir1.fingerprint, ir2.fingerprint);
    }

    #[test]
    fn test_fingerprint_excludes_fingerprint_field() {
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

        let _sexpr = ir.to_sexpr();
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

        let computed_fp = fingerprint::fingerprint(&fp_free);
        assert_eq!(ir.fingerprint, computed_fp);
    }

    #[test]
    fn test_ir_to_sexpr_structure() {
        let ir = Ir::new(
            "gymnast.ir/0.1".to_string(),
            "test".to_string(),
            vec![(":version".to_string(), Sexpr::Str("0.1".to_string()))],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        );

        let sexpr = ir.to_sexpr();
        let s = sexpr.print();

        assert!(s.contains("ir"));
        assert!(s.contains("schema"));
        assert!(s.contains("module"));
        assert!(s.contains("design"));
        assert!(s.contains("transitions"));
        assert!(s.contains("obligations"));
        assert!(s.contains("synthesis"));
        assert!(s.contains("diagnostics"));
        assert!(s.contains("fingerprint"));
    }

    #[test]
    fn test_module_fields_sorted() {
        let fields = vec![
            (":z".to_string(), Sexpr::sym("last")),
            (":a".to_string(), Sexpr::sym("first")),
            (":m".to_string(), Sexpr::sym("middle")),
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

        assert_eq!(ir.module_fields[0].0, ":a");
        assert_eq!(ir.module_fields[1].0, ":m");
        assert_eq!(ir.module_fields[2].0, ":z");
    }

    #[test]
    fn test_import_kind() {
        let node = IrNode::new(
            "test/import/profiles".to_string(),
            "import",
            "profiles".to_string(),
            vec![(":version".to_string(), Sexpr::Str("1.0".to_string()))],
            vec![],
        );

        assert_eq!(node.kind, "import");
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
        assert_eq!(ir.design[0].kind, "import");
    }

    #[test]
    fn test_application_kind() {
        let node = IrNode::new(
            "test/application/MyApp".to_string(),
            "application",
            "MyApp".to_string(),
            vec![],
            vec![],
        );

        assert_eq!(node.kind, "application");
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
        assert_eq!(ir.design[0].kind, "application");
    }

    #[test]
    fn test_actor_kind() {
        let node = IrNode::new(
            "test/actor/User".to_string(),
            "actor",
            "User".to_string(),
            vec![(":kind".to_string(), Sexpr::sym("person"))],
            vec![],
        );

        assert_eq!(node.kind, "actor");
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
        assert_eq!(ir.design[0].kind, "actor");
    }

    #[test]
    fn test_component_kind() {
        let node = IrNode::new(
            "test/component/Core".to_string(),
            "component",
            "Core".to_string(),
            vec![],
            vec![],
        );

        assert_eq!(node.kind, "component");
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
        assert_eq!(ir.design[0].kind, "component");
    }

    #[test]
    fn test_interface_kind() {
        let node = IrNode::new(
            "test/interface/API".to_string(),
            "interface",
            "API".to_string(),
            vec![(":for".to_string(), Sexpr::sym("user"))],
            vec![Sexpr::list(vec![
                Sexpr::sym("command"),
                Sexpr::sym("create"),
            ])],
        );

        assert_eq!(node.kind, "interface");
        assert_eq!(node.clauses.len(), 1);
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
        assert_eq!(ir.design[0].kind, "interface");
    }

    #[test]
    fn test_state_kind() {
        let node = IrNode::new(
            "test/state/Current".to_string(),
            "state",
            "Current".to_string(),
            vec![],
            vec![],
        );

        assert_eq!(node.kind, "state");
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
        assert_eq!(ir.design[0].kind, "state");
    }

    #[test]
    fn test_flow_kind() {
        let node = IrNode::new(
            "test/flow/Transition".to_string(),
            "flow",
            "Transition".to_string(),
            vec![
                (":from".to_string(), Sexpr::sym("start")),
                (":to".to_string(), Sexpr::sym("end")),
                (":kind".to_string(), Sexpr::sym("cmd")),
            ],
            vec![],
        );

        assert_eq!(node.kind, "flow");
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
        assert_eq!(ir.design[0].kind, "flow");
    }

    #[test]
    fn test_constraint_kind() {
        let node = IrNode::new(
            "test/constraint/Limit".to_string(),
            "constraint",
            "Limit".to_string(),
            vec![
                (":class".to_string(), Sexpr::sym("workload")),
                (":scope".to_string(), Sexpr::sym("interface")),
            ],
            vec![],
        );

        assert_eq!(node.kind, "constraint");
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
        assert_eq!(ir.obligations[0].kind, "constraint");
    }

    #[test]
    fn test_acceptance_kind() {
        let node = IrNode::new(
            "test/acceptance/Test".to_string(),
            "acceptance",
            "Test".to_string(),
            vec![(":subject".to_string(), Sexpr::sym("app"))],
            vec![Sexpr::list(vec![
                Sexpr::sym("property"),
                Sexpr::sym("prop1"),
            ])],
        );

        assert_eq!(node.kind, "acceptance");
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
        assert_eq!(ir.obligations[0].kind, "acceptance");
    }
}
