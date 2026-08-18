//! Deterministic lowering from `Ir` to the fixed 8-node typed synthesis
//! DAG (`docs/rust-port-plan-phase3.md`, section B). Ports
//! `src/plan.lisp`'s behavior against the Rust IR contract
//! (`docs/ir-contract-deltas.md`); shapes here are new to the Rust port
//! (`plan.rs` has no Lamedh byte-compatible golden of its own).

use crate::diag::diag_sexpr;
use crate::fingerprint;
use crate::ir::{Ir, IrNode};
use crate::sexpr::Sexpr;

/// One node contract in the synthesis DAG. All list fields are sorted
/// at construction (byte-wise for the `String` fields); the fingerprint
/// is computed over the canonical contract form at construction and
/// stored.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanNode {
    pub id: String,
    pub class: String,
    pub recipe: String,
    pub inputs: Vec<String>,
    pub depends_on: Vec<String>,
    pub target: Sexpr,
    pub model: Sexpr,
    pub may_write: Vec<String>,
    pub capabilities: Vec<String>,
    pub obligations: Vec<String>,
    pub prohibitions: Vec<String>,
    pub fingerprint: String,
}

impl PlanNode {
    /// Mirrors the Lamedh `gymnast-plan-node` constructor: sorts the six
    /// list fields, builds the canonical `node-contract` form, and
    /// fingerprints it.
    ///
    /// NOTE (ambiguity, reported per Process Rule 1): the plan document
    /// gives this constructor in prose only ("mirrors the Lamedh
    /// constructor exactly") without a Rust signature or element-type
    /// choice for the six list fields. This implementation uses:
    /// positional parameters in struct-field order with `class`/`recipe`
    /// as `&str` (matching `IrNode::new`'s existing `kind: &str`
    /// convention) — confirmed correct by `plan_oracle_test.rs`'s
    /// `oracle_09_*` call shape, which was authored independently. The
    /// element type of `inputs`/`depends_on`/`may_write` (`Sexpr::Str`,
    /// matching how `IrNode`'s own `id` field always prints as a quoted
    /// string) versus `capabilities`/`obligations`/`prohibitions`
    /// (`Sexpr::sym`, matching how `class`/`recipe` print as bare
    /// symbols in the plan's own worked example) is not pinned by the
    /// oracle tests, which only assert structural properties. This
    /// implementation follows the `id`-vs-vocabulary-term distinction
    /// the rest of the IR contract uses throughout.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        class: &str,
        recipe: &str,
        mut inputs: Vec<String>,
        mut depends_on: Vec<String>,
        target: Sexpr,
        model: Sexpr,
        mut may_write: Vec<String>,
        mut capabilities: Vec<String>,
        mut obligations: Vec<String>,
        mut prohibitions: Vec<String>,
    ) -> PlanNode {
        inputs.sort();
        depends_on.sort();
        may_write.sort();
        capabilities.sort();
        obligations.sort();
        prohibitions.sort();

        let mut node = PlanNode {
            id,
            class: class.to_string(),
            recipe: recipe.to_string(),
            inputs,
            depends_on,
            target,
            model,
            may_write,
            capabilities,
            obligations,
            prohibitions,
            fingerprint: String::new(),
        };
        let contract = node.contract_sexpr();
        node.fingerprint = fingerprint::fingerprint(&contract);
        node
    }

    /// The eleven contract fields as `(key value)` pairs, in struct
    /// order. Shared by the fingerprinted `node-contract` form and the
    /// printed `plan-node` form so the two can never drift.
    fn field_pairs(&self) -> Vec<Sexpr> {
        vec![
            Sexpr::pair("id", Sexpr::Str(self.id.clone())),
            Sexpr::pair("class", Sexpr::sym(&self.class)),
            Sexpr::pair("recipe", Sexpr::sym(&self.recipe)),
            Sexpr::pair(
                "inputs",
                Sexpr::list(self.inputs.iter().map(|s| Sexpr::Str(s.clone())).collect()),
            ),
            Sexpr::pair(
                "depends-on",
                Sexpr::list(
                    self.depends_on
                        .iter()
                        .map(|s| Sexpr::Str(s.clone()))
                        .collect(),
                ),
            ),
            Sexpr::pair("target", self.target.clone()),
            Sexpr::pair("model", self.model.clone()),
            Sexpr::pair(
                "may-write",
                Sexpr::list(
                    self.may_write
                        .iter()
                        .map(|s| Sexpr::Str(s.clone()))
                        .collect(),
                ),
            ),
            Sexpr::pair(
                "capabilities",
                Sexpr::list(self.capabilities.iter().map(|s| Sexpr::sym(s)).collect()),
            ),
            Sexpr::pair(
                "obligations",
                Sexpr::list(self.obligations.iter().map(|s| Sexpr::sym(s)).collect()),
            ),
            Sexpr::pair(
                "prohibitions",
                Sexpr::list(self.prohibitions.iter().map(|s| Sexpr::sym(s)).collect()),
            ),
        ]
    }

    fn contract_sexpr(&self) -> Sexpr {
        Sexpr::list(vec![
            Sexpr::sym("node-contract"),
            Sexpr::list(self.field_pairs()),
        ])
    }

    /// `(plan-node ((id "...") ... (fingerprint "fnv1a64:...")))` — the
    /// same eleven fields as the fingerprinted contract, plus the
    /// fingerprint, in struct order.
    pub fn to_sexpr(&self) -> Sexpr {
        let mut pairs = self.field_pairs();
        pairs.push(Sexpr::pair(
            "fingerprint",
            Sexpr::Str(self.fingerprint.clone()),
        ));
        Sexpr::list(vec![Sexpr::sym("plan-node"), Sexpr::list(pairs)])
    }
}

/// A deterministic synthesis plan lowered from one `Ir`.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub schema: String,
    pub ir_fingerprint: String,
    pub target: Sexpr,
    pub nodes: Vec<PlanNode>,
    /// `(ir-node-id, [plan-node-ids])`, in `Ir::all_nodes()` order.
    pub coverage: Vec<(String, Vec<String>)>,
    pub diagnostics: Vec<Sexpr>,
    pub fingerprint: String,
}

impl Plan {
    #[allow(clippy::too_many_arguments)]
    fn new(
        schema: String,
        ir_fingerprint: String,
        target: Sexpr,
        nodes: Vec<PlanNode>,
        coverage: Vec<(String, Vec<String>)>,
        diagnostics: Vec<Sexpr>,
    ) -> Plan {
        let fingerprint_free = Self::to_sexpr_without_fingerprint(
            &schema,
            &ir_fingerprint,
            &target,
            &nodes,
            &coverage,
            &diagnostics,
        );
        let fingerprint = fingerprint::fingerprint(&fingerprint_free);
        Plan {
            schema,
            ir_fingerprint,
            target,
            nodes,
            coverage,
            diagnostics,
            fingerprint,
        }
    }

    fn to_sexpr_without_fingerprint(
        schema: &str,
        ir_fingerprint: &str,
        target: &Sexpr,
        nodes: &[PlanNode],
        coverage: &[(String, Vec<String>)],
        diagnostics: &[Sexpr],
    ) -> Sexpr {
        let mut items = vec![
            Sexpr::pair("schema", Sexpr::Str(schema.to_string())),
            Sexpr::pair("ir-fingerprint", Sexpr::Str(ir_fingerprint.to_string())),
            Sexpr::pair("target", target.clone()),
        ];

        let node_list: Vec<Sexpr> = nodes.iter().map(|n| n.to_sexpr()).collect();
        items.push(Sexpr::pair("nodes", Sexpr::list(node_list)));

        // Coverage entries print as ("ir-id" ("plan-id" ...)) pairs.
        let coverage_list: Vec<Sexpr> = coverage
            .iter()
            .map(|(id, plan_ids)| {
                Sexpr::list(vec![
                    Sexpr::Str(id.clone()),
                    Sexpr::list(plan_ids.iter().map(|p| Sexpr::Str(p.clone())).collect()),
                ])
            })
            .collect();
        items.push(Sexpr::pair("coverage", Sexpr::list(coverage_list)));

        items.push(Sexpr::pair(
            "diagnostics",
            Sexpr::list(diagnostics.to_vec()),
        ));

        Sexpr::list(vec![Sexpr::sym("plan"), Sexpr::list(items)])
    }

    /// Builds on the fingerprint-free form so the two can never drift,
    /// same discipline as `Ir::to_sexpr`.
    pub fn to_sexpr(&self) -> Sexpr {
        let base = Self::to_sexpr_without_fingerprint(
            &self.schema,
            &self.ir_fingerprint,
            &self.target,
            &self.nodes,
            &self.coverage,
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

const PLAN_SCHEMA: &str = "gymnast.plan/0.1";

fn default_target() -> Sexpr {
    Sexpr::list(vec![Sexpr::sym("lamedh")])
}

fn default_model() -> Sexpr {
    Sexpr::list(vec![
        Sexpr::sym("small_code_model"),
        Sexpr::list(vec![Sexpr::list(vec![
            Sexpr::sym("class"),
            Sexpr::sym("nano"),
        ])]),
    ])
}

/// The first synthesis node by id (`Ir::synthesis` is already id-sorted
/// at construction, and `nodes_of_kind` preserves partition order).
fn first_synthesis_node(ir: &Ir) -> Option<&IrNode> {
    ir.nodes_of_kind("synthesis").into_iter().next()
}

fn selected_target(ir: &Ir) -> Sexpr {
    first_synthesis_node(ir)
        .and_then(|n| n.field(":target"))
        .cloned()
        .unwrap_or_else(default_target)
}

fn selected_model(ir: &Ir) -> Sexpr {
    first_synthesis_node(ir)
        .and_then(|n| n.field(":model"))
        .cloned()
        .unwrap_or_else(default_model)
}

/// The target language: the first element of a list target, or the bare
/// symbol itself when the target is not a list. `pub(crate)`: shared
/// with `prompt.rs`'s TARGET-section and output-protocol projections
/// (plan section C), which must select the same language plan.rs used
/// to build the target path extensions.
pub(crate) fn target_language(target: &Sexpr) -> Option<String> {
    match target {
        Sexpr::List(items) => items.first().and_then(|s| s.as_sym()).map(str::to_string),
        Sexpr::Sym(s) => Some(s.clone()),
        _ => None,
    }
}

/// Total map (with default), per plan section B. `pub(crate)`: shared
/// with `prompt.rs`'s TARGET-section text (plan section C.2).
pub(crate) fn extension_for(lang: Option<&str>) -> &'static str {
    match lang {
        Some("ruby") => ".rb",
        Some("go") => ".go",
        Some("java") => ".java",
        Some("python") => ".py",
        Some("typescript") => ".ts",
        Some("javascript") => ".js",
        Some("rust") => ".rs",
        _ => ".lisp",
    }
}

/// A path ending in `.lisp` has that suffix replaced by `ext`; other
/// paths pass through unrewritten.
fn rewrite_extension(path: &str, ext: &str) -> String {
    match path.strip_suffix(".lisp") {
        Some(stem) => format!("{}{}", stem, ext),
        None => path.to_string(),
    }
}

fn target_paths(paths: &[&str], target: &Sexpr) -> Vec<String> {
    let lang = target_language(target);
    let ext = extension_for(lang.as_deref());
    paths.iter().map(|p| rewrite_extension(p, ext)).collect()
}

/// IR node ids (from `all_nodes()`) whose kind is in `kinds`, sorted.
fn ids_for_kinds(ir: &Ir, kinds: &[&str]) -> Vec<String> {
    let mut ids: Vec<String> = ir
        .all_nodes()
        .into_iter()
        .filter(|n| kinds.contains(&n.kind.as_str()))
        .map(|n| n.id.clone())
        .collect();
    ids.sort();
    ids
}

fn plan_id(ir: &Ir, local: &str) -> String {
    format!("{}/plan/{}", ir.module_name, local)
}

/// The fixed 8-node table, transcribed exactly (plan section B).
/// Returned in table order (build order), NOT id-sorted.
fn build_plan_nodes(ir: &Ir, target: &Sexpr, model: &Sexpr) -> Vec<PlanNode> {
    let none = Sexpr::sym("none");

    let design_id = plan_id(ir, "design-contracts");
    let transition_id = plan_id(ir, "transition-kernel");
    let auth_id = plan_id(ir, "authorization-policy");
    let persistence_id = plan_id(ir, "persistence");
    let interface_id = plan_id(ir, "interface-contracts");
    let handler_id = plan_id(ir, "service-handlers");
    let acceptance_id = plan_id(ir, "acceptance-harness");
    let assembly_id = plan_id(ir, "application-assembly");

    let design_contracts = PlanNode::new(
        design_id.clone(),
        "structural",
        "design-contracts-v1",
        ids_for_kinds(ir, &["actor", "type", "component", "flow"]),
        vec![],
        target.clone(),
        none.clone(),
        target_paths(&["generated/design/contracts.lisp"], target),
        vec![],
        vec![
            "well-formed-types".to_string(),
            "explicit-capability-edges".to_string(),
        ],
        vec![
            "invent-product-semantics".to_string(),
            "add-dependencies".to_string(),
        ],
    );

    let transition_kernel = PlanNode::new(
        transition_id.clone(),
        "generative",
        "transition-kernel-v1",
        ids_for_kinds(ir, &["type", "state", "behavior", "invariant"]),
        vec![design_id.clone()],
        target.clone(),
        model.clone(),
        target_paths(&["generated/domain/transitions.lisp"], target),
        vec!["clock".to_string(), "id-source".to_string()],
        vec![
            "implements-transition-system".to_string(),
            "preserves-invariants".to_string(),
            "deterministic-under-same-input".to_string(),
        ],
        vec![
            "perform-io".to_string(),
            "weaken-preconditions".to_string(),
            "invent-errors".to_string(),
        ],
    );

    let authorization_policy = PlanNode::new(
        auth_id.clone(),
        "generative",
        "authorization-policy-v1",
        ids_for_kinds(ir, &["actor", "flow", "behavior", "invariant"]),
        vec![design_id.clone(), transition_id.clone()],
        target.clone(),
        model.clone(),
        target_paths(&["generated/domain/authorization.lisp"], target),
        vec![],
        vec![
            "deny-by-default".to_string(),
            "noninterference".to_string(),
            "owner-isolation".to_string(),
        ],
        vec![
            "grant-undeclared-capabilities".to_string(),
            "reveal-resource-existence".to_string(),
        ],
    );

    let persistence = PlanNode::new(
        persistence_id.clone(),
        "generative",
        "persistence-v1",
        ids_for_kinds(ir, &["type", "state", "behavior", "constraint"]),
        vec![design_id.clone(), transition_id.clone()],
        target.clone(),
        model.clone(),
        target_paths(
            &[
                "generated/adapters/persistence.lisp",
                "generated/adapters/schema.sexpr",
            ],
            target,
        ),
        vec!["durable-store".to_string(), "transactions".to_string()],
        vec![
            "durable-commit".to_string(),
            "atomic-boundaries".to_string(),
            "retry-safety".to_string(),
        ],
        vec![
            "perform-network-io".to_string(),
            "choose-unpinned-dependencies".to_string(),
        ],
    );

    let interface_contracts = PlanNode::new(
        interface_id.clone(),
        "structural",
        "interface-contracts-v1",
        ids_for_kinds(ir, &["type", "interface"]),
        vec![design_id.clone()],
        target.clone(),
        none.clone(),
        target_paths(&["generated/interfaces/contracts.lisp"], target),
        vec![],
        vec![
            "complete-operation-surface".to_string(),
            "declared-errors-only".to_string(),
        ],
        vec!["change-observable-contract".to_string()],
    );

    let service_handlers = PlanNode::new(
        handler_id.clone(),
        "generative",
        "service-handlers-v1",
        ids_for_kinds(ir, &["interface", "behavior", "state", "constraint"]),
        vec![
            transition_id.clone(),
            auth_id.clone(),
            persistence_id.clone(),
            interface_id.clone(),
        ],
        target.clone(),
        model.clone(),
        target_paths(&["generated/service/handlers.lisp"], target),
        vec![
            "repository".to_string(),
            "identity".to_string(),
            "clock".to_string(),
            "id-source".to_string(),
        ],
        vec![
            "contract-conformance".to_string(),
            "authorization-before-observation".to_string(),
            "idempotent-retries".to_string(),
        ],
        vec![
            "access-filesystem".to_string(),
            "access-network".to_string(),
            "add-endpoints".to_string(),
        ],
    );

    let acceptance_harness = PlanNode::new(
        acceptance_id.clone(),
        "verification",
        "acceptance-harness-v1",
        ids_for_kinds(
            ir,
            &[
                "behavior",
                "invariant",
                "constraint",
                "acceptance",
                "interface",
                "state",
            ],
        ),
        vec![handler_id.clone()],
        target.clone(),
        none.clone(),
        target_paths(&["generated/verification/acceptance.lisp"], target),
        vec![],
        vec![
            "independent-oracle".to_string(),
            "trace-equivalence".to_string(),
            "boundary-coverage".to_string(),
            "deterministic-execution".to_string(),
        ],
        vec![
            "read-generated-rationale".to_string(),
            "weaken-obligations".to_string(),
            "skip-failures".to_string(),
        ],
    );

    let application_assembly = PlanNode::new(
        assembly_id,
        "assembly",
        "application-assembly-v1",
        ids_for_kinds(
            ir,
            &[
                "application",
                "import",
                "component",
                "synthesis",
                "constraint",
            ],
        ),
        vec![
            transition_id,
            auth_id,
            persistence_id,
            interface_id,
            handler_id,
            acceptance_id,
        ],
        target.clone(),
        none,
        target_paths(
            &["generated/application.lisp", "generated/manifest.sexpr"],
            target,
        ),
        vec![],
        vec![
            "all-artifacts-linked".to_string(),
            "all-obligations-addressed".to_string(),
        ],
        vec![
            "untracked-artifacts".to_string(),
            "undeclared-capabilities".to_string(),
        ],
    );

    vec![
        design_contracts,
        transition_kernel,
        authorization_policy,
        persistence,
        interface_contracts,
        service_handlers,
        acceptance_harness,
        application_assembly,
    ]
}

/// E402 missing-plan-dependency: a node's `depends_on` entry names no
/// node in the plan. Structurally impossible with the fixed table; the
/// check exists to catch table transcription errors and future dynamic
/// planning.
fn dependency_diagnostics(nodes: &[PlanNode]) -> Vec<Sexpr> {
    let ids: std::collections::HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    let mut diags = Vec::new();
    for node in nodes {
        for dep in &node.depends_on {
            if !ids.contains(dep.as_str()) {
                diags.push(diag_sexpr(
                    "error",
                    "E402",
                    (0, 0),
                    format!("plan node {} depends on unknown plan node {}", node.id, dep),
                ));
            }
        }
    }
    diags
}

/// Coverage entries for every IR node in `all_nodes()` order: which
/// plan nodes (by id) consume it as an input.
fn coverage_entries(ir: &Ir, nodes: &[PlanNode]) -> Vec<(String, Vec<String>)> {
    ir.all_nodes()
        .into_iter()
        .map(|ir_node| {
            let covering: Vec<String> = nodes
                .iter()
                .filter(|n| n.inputs.iter().any(|i| i == &ir_node.id))
                .map(|n| n.id.clone())
                .collect();
            (ir_node.id.clone(), covering)
        })
        .collect()
}

/// E403 unplanned-semantic-node: an IR node whose id appears in no plan
/// node's `inputs`.
fn coverage_diagnostics(coverage: &[(String, Vec<String>)]) -> Vec<Sexpr> {
    coverage
        .iter()
        .filter(|(_, plan_ids)| plan_ids.is_empty())
        .map(|(id, _)| {
            diag_sexpr(
                "error",
                "E403",
                (0, 0),
                format!(
                    "semantic node {} has no implementation or evidence path",
                    id
                ),
            )
        })
        .collect()
}

/// Deterministic lowering from `ir` to the fixed 8-node synthesis plan.
/// Refuses invalid IR: if `ir.has_errors()`, returns a `Plan` with empty
/// nodes/coverage and a single E401 diagnostic, never panicking on the
/// error-carrying `Ir` value.
pub fn plan(ir: &Ir) -> Plan {
    if ir.has_errors() {
        let diag = diag_sexpr(
            "error",
            "E401",
            (0, 0),
            "planning refused: input IR carries one or more error-severity diagnostics".to_string(),
        );
        return Plan::new(
            PLAN_SCHEMA.to_string(),
            ir.fingerprint.clone(),
            default_target(),
            vec![],
            vec![],
            vec![diag],
        );
    }

    let target = selected_target(ir);
    let model = selected_model(ir);
    let nodes = build_plan_nodes(ir, &target, &model);

    let mut diagnostics = dependency_diagnostics(&nodes);
    let coverage = coverage_entries(ir, &nodes);
    diagnostics.extend(coverage_diagnostics(&coverage));

    Plan::new(
        PLAN_SCHEMA.to_string(),
        ir.fingerprint.clone(),
        target,
        nodes,
        coverage,
        diagnostics,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::IrNode;

    fn type_node(id: &str, name: &str) -> IrNode {
        IrNode::new(id.to_string(), "type", name.to_string(), vec![], vec![])
    }

    fn minimal_ir() -> Ir {
        Ir::new(
            "gymnast.ir/0.1".to_string(),
            "m".to_string(),
            vec![],
            vec![type_node("m/type/Foo", "Foo")],
            vec![],
            vec![],
            vec![],
            vec![],
        )
    }

    #[test]
    fn test_default_target_and_model_no_synthesis_node() {
        let ir = minimal_ir();
        let p = plan(&ir);
        assert_eq!(p.target, Sexpr::list(vec![Sexpr::sym("lamedh")]));
        for node in &p.nodes {
            if node.class == "generative" {
                assert_eq!(node.model, default_model());
            } else {
                assert_eq!(node.model, Sexpr::sym("none"));
            }
        }
    }

    #[test]
    fn test_eight_nodes_in_table_order() {
        let ir = minimal_ir();
        let p = plan(&ir);
        assert_eq!(p.nodes.len(), 8);
        let expected_locals = [
            "design-contracts",
            "transition-kernel",
            "authorization-policy",
            "persistence",
            "interface-contracts",
            "service-handlers",
            "acceptance-harness",
            "application-assembly",
        ];
        for (node, local) in p.nodes.iter().zip(expected_locals.iter()) {
            assert_eq!(node.id, format!("m/plan/{}", local));
        }
    }

    #[test]
    fn test_ir_fingerprint_binding() {
        let ir = minimal_ir();
        let p = plan(&ir);
        assert_eq!(p.ir_fingerprint, ir.fingerprint);
    }

    #[test]
    fn test_coverage_totality_minimal_ir() {
        let ir = minimal_ir();
        let p = plan(&ir);
        assert_eq!(p.coverage.len(), ir.all_nodes().len());
        for (_, plan_ids) in &p.coverage {
            assert!(!plan_ids.is_empty());
        }
        assert!(p.diagnostics.is_empty());
    }

    #[test]
    fn test_invalid_ir_refusal() {
        let bad_diag = diag_sexpr("error", "E301", (0, 0), "duplicate id".to_string());
        let ir = Ir::new(
            "gymnast.ir/0.1".to_string(),
            "bad".to_string(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![bad_diag],
        );
        let p = plan(&ir);
        assert!(p.nodes.is_empty());
        assert!(p.coverage.is_empty());
        assert_eq!(p.diagnostics.len(), 1);
    }

    #[test]
    fn test_no_missing_dependency_diagnostics_on_fixed_table() {
        let ir = minimal_ir();
        let p = plan(&ir);
        assert!(dependency_diagnostics(&p.nodes).is_empty());
    }

    #[test]
    fn test_extension_map_total_default() {
        assert_eq!(extension_for(Some("ruby")), ".rb");
        assert_eq!(extension_for(Some("go")), ".go");
        assert_eq!(extension_for(Some("java")), ".java");
        assert_eq!(extension_for(Some("python")), ".py");
        assert_eq!(extension_for(Some("typescript")), ".ts");
        assert_eq!(extension_for(Some("javascript")), ".js");
        assert_eq!(extension_for(Some("rust")), ".rs");
        assert_eq!(extension_for(Some("cobol")), ".lisp");
        assert_eq!(extension_for(None), ".lisp");
    }

    #[test]
    fn test_rewrite_extension_only_lisp_suffix() {
        assert_eq!(rewrite_extension("a/b.lisp", ".rb"), "a/b.rb");
        assert_eq!(rewrite_extension("a/b.sexpr", ".rb"), "a/b.sexpr");
    }

    #[test]
    fn test_target_language_bare_symbol_and_list() {
        assert_eq!(
            target_language(&Sexpr::sym("lamedh")),
            Some("lamedh".to_string())
        );
        assert_eq!(
            target_language(&Sexpr::list(vec![Sexpr::sym("ruby"), Sexpr::sym("rails")])),
            Some("ruby".to_string())
        );
    }

    #[test]
    fn test_plan_node_new_sorts_and_permutation_invariant_fingerprint() {
        let a = PlanNode::new(
            "m/plan/x".to_string(),
            "generative",
            "recipe-v1",
            vec!["m/type/B".to_string(), "m/type/A".to_string()],
            vec![],
            Sexpr::sym("lamedh"),
            Sexpr::sym("none"),
            vec![],
            vec![],
            vec![],
            vec![],
        );
        assert_eq!(
            a.inputs,
            vec!["m/type/A".to_string(), "m/type/B".to_string()]
        );
    }

    #[test]
    fn test_plan_to_sexpr_round_trip_reprints() {
        let ir = minimal_ir();
        let p = plan(&ir);
        let s = p.to_sexpr().print();
        assert!(s.starts_with("(plan "));
        assert!(s.contains("gymnast.plan/0.1"));
    }
}
