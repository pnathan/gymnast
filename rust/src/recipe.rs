//! Deterministic recipe registry and executor (`docs/rust-port-plan-phase4.md`,
//! section C). Ports `src/recipe.lisp`'s behavioral intent — the static
//! registry, the executor's dispatch shape, and the four Ruby structural /
//! verification / assembly emitters — onto the Rust IR/plan/candidate
//! contracts. Recipes are not exempt from the candidate firewall: every
//! deterministic candidate this module produces is run back through
//! `candidate::candidate_diagnostics` before it is allowed to count as
//! `Succeeded` — nothing, including gymnast's own code, self-certifies its
//! output.
//!
//! Only the Ruby target emitter is ported (the Lamedh reference's protocol
//! dispatch has the same practical limit: `definstance` bodies exist only
//! for `gymnast-ruby-target`). A deterministic recipe asked to run against
//! any other target fails closed with E510 rather than emitting
//! target-mismatched content.

use crate::candidate::candidate_diagnostics;
use crate::diag::diag_sexpr;
use crate::ir::{resolve_ir_slice, Ir, IrNode};
use crate::plan::{target_language, Plan, PlanNode};
use crate::sexpr::Sexpr;

const CANDIDATE_SCHEMA: &str = "gymnast.candidate/0.1";

/// The four recipe classes from the phase-3 plan table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecipeClass {
    Structural,
    Generative,
    Verification,
    Assembly,
}

type Executor = fn(&[&IrNode], &PlanNode) -> Sexpr;

/// One registry entry. `execute` is `None` for the four generative
/// recipes (transition-kernel-v1, authorization-policy-v1, persistence-v1,
/// service-handlers-v1) — they produce prompt packages for the phase-5
/// model runner, not a candidate here.
#[derive(Debug, Clone, Copy)]
pub struct Recipe {
    pub name: &'static str,
    pub class: RecipeClass,
    pub execute: Option<Executor>,
}

/// Static, deterministic registry: the eight recipe names from the
/// phase-3 plan table, four deterministic Ruby executors plus four
/// generative placeholders. `pub(crate)` visibility is not needed: `pub`
/// so the oracle tests (and, later, the phase-5 runner) can enumerate it.
const REGISTRY: &[(&str, RecipeClass, Option<Executor>)] = &[
    (
        "design-contracts-v1",
        RecipeClass::Structural,
        Some(design_contracts_executor),
    ),
    ("transition-kernel-v1", RecipeClass::Generative, None),
    ("authorization-policy-v1", RecipeClass::Generative, None),
    ("persistence-v1", RecipeClass::Generative, None),
    (
        "interface-contracts-v1",
        RecipeClass::Structural,
        Some(interface_contracts_executor),
    ),
    ("service-handlers-v1", RecipeClass::Generative, None),
    (
        "acceptance-harness-v1",
        RecipeClass::Verification,
        Some(acceptance_harness_executor),
    ),
    (
        "application-assembly-v1",
        RecipeClass::Assembly,
        Some(application_assembly_executor),
    ),
];

/// Registry lookup by recipe name. `None` for anything not in the fixed
/// eight-entry table (the unknown-recipe case `execute_recipe` reports as
/// E509).
pub fn lookup(name: &str) -> Option<Recipe> {
    REGISTRY
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(n, c, e)| Recipe {
            name: n,
            class: *c,
            execute: *e,
        })
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionStatus {
    Succeeded,
    Failed,
    Deferred,
}

/// The outcome of running one plan node's recipe (`gymnast-execute-recipe`,
/// ported). `candidate`/`recipe_identity` are `Some` only when a
/// deterministic executor actually ran (whether or not its output then
/// passed the firewall) — never for an unknown recipe, an unsupported
/// target, or a deferred generative node.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionResult {
    pub node_id: String,
    pub status: ExecutionStatus,
    pub candidate: Option<Sexpr>,
    pub recipe_identity: Option<String>,
    pub diagnostics: Vec<Sexpr>,
}

fn status_symbol(status: &ExecutionStatus) -> &'static str {
    match status {
        ExecutionStatus::Succeeded => "succeeded",
        ExecutionStatus::Failed => "failed",
        ExecutionStatus::Deferred => "deferred",
    }
}

impl ExecutionResult {
    /// `(execution-result ((node-id "...") (status s) [(reason
    /// requires-model)] [(candidate (...))] [(recipe-identity "...")]
    /// (diagnostics (...))))` — `candidate`/`recipe-identity` entries
    /// omitted when absent; `reason` present only when `status` is
    /// `Deferred`, in the same field position `gymnast-execute-recipe`
    /// builds it (node-id, status, reason — no candidate/recipe-identity
    /// on the deferred branch).
    pub fn to_sexpr(&self) -> Sexpr {
        let mut items = vec![
            Sexpr::pair("node-id", Sexpr::Str(self.node_id.clone())),
            Sexpr::pair("status", Sexpr::sym(status_symbol(&self.status))),
        ];
        if self.status == ExecutionStatus::Deferred {
            items.push(Sexpr::pair("reason", Sexpr::sym("requires-model")));
        }
        if let Some(candidate) = &self.candidate {
            items.push(Sexpr::pair("candidate", candidate.clone()));
        }
        if let Some(identity) = &self.recipe_identity {
            items.push(Sexpr::pair("recipe-identity", Sexpr::Str(identity.clone())));
        }
        items.push(Sexpr::pair(
            "diagnostics",
            Sexpr::list(self.diagnostics.clone()),
        ));
        Sexpr::list(vec![Sexpr::sym("execution-result"), Sexpr::list(items)])
    }

    /// Parses a `(execution-result (...))` value back into an
    /// `ExecutionResult` (phase-5 fold-in scope item 1b: needed to read
    /// `results.sexpr` back, e.g. from a cache). `None` for anything not
    /// shaped as a two-element list headed by the bare symbol
    /// `execution-result` whose second element is a field-pairs list, or
    /// whose `status` is not one of the three known symbols. `reason` is
    /// never read back into a field: `to_sexpr` re-derives it from
    /// `status` alone, so round-tripping through `from_sexpr` and back
    /// through `to_sexpr` reproduces it exactly.
    ///
    /// Round-trip law: for every value `v` produced by
    /// `ExecutionResult::to_sexpr`, `ExecutionResult::from_sexpr(&v)
    /// .unwrap().to_sexpr()` reprints byte-identically to `v` — the
    /// canonical field order `to_sexpr` builds is a pure function of the
    /// struct's data, not of the input's own field order, so the
    /// round-trip holds regardless of any (nonexistent, in practice)
    /// reordering in `v`.
    pub fn from_sexpr(v: &Sexpr) -> Option<ExecutionResult> {
        let items = v.as_list()?;
        if items.len() != 2 {
            return None;
        }
        if items[0].as_sym() != Some("execution-result") {
            return None;
        }
        let body = &items[1];
        let node_id = body.assoc("node-id")?.as_str()?.to_string();
        let status = match body.assoc("status")?.as_sym()? {
            "succeeded" => ExecutionStatus::Succeeded,
            "failed" => ExecutionStatus::Failed,
            "deferred" => ExecutionStatus::Deferred,
            _ => return None,
        };
        let candidate = body.assoc("candidate").cloned();
        let recipe_identity = body
            .assoc("recipe-identity")
            .and_then(|s| s.as_str())
            .map(String::from);
        let diagnostics = body
            .assoc("diagnostics")
            .and_then(|d| d.as_list())
            .map(|items| items.to_vec())
            .unwrap_or_default();
        Some(ExecutionResult {
            node_id,
            status,
            candidate,
            recipe_identity,
            diagnostics,
        })
    }
}

/// Executes one plan node's recipe (mirrors `gymnast-execute-recipe`):
/// unknown recipe name -> Failed + E509; generative recipe -> Deferred;
/// deterministic recipe on a non-Ruby target -> Failed + E510 (only the
/// Ruby emitter is ported); deterministic recipe on the Ruby target ->
/// run the executor over the resolved ir-slice, then run the candidate
/// firewall over the result — Succeeded only when the firewall reports no
/// error-severity diagnostic. The firewall runs unconditionally: recipes
/// are not exempt from it.
pub fn execute_recipe(ir: &Ir, node: &PlanNode) -> ExecutionResult {
    let recipe = match lookup(&node.recipe) {
        Some(recipe) => recipe,
        None => {
            return ExecutionResult {
                node_id: node.id.clone(),
                status: ExecutionStatus::Failed,
                candidate: None,
                recipe_identity: None,
                diagnostics: vec![diag_sexpr(
                    "error",
                    "E509",
                    (0, 0),
                    format!("no registered recipe: {}", node.recipe),
                )],
            };
        }
    };

    let executor = match recipe.execute {
        Some(executor) => executor,
        None => {
            // Phase 5 fold-in scope item 1c: a deferred result still
            // names the recipe that will eventually run it — the trust
            // boundary the phase-5 model runner enforces needs recipe
            // identity in evidence even before a candidate exists.
            return ExecutionResult {
                node_id: node.id.clone(),
                status: ExecutionStatus::Deferred,
                candidate: None,
                recipe_identity: Some(recipe.name.to_string()),
                diagnostics: vec![],
            };
        }
    };

    let lang = target_language(&node.target);
    if lang.as_deref() != Some("ruby") {
        return ExecutionResult {
            node_id: node.id.clone(),
            status: ExecutionStatus::Failed,
            candidate: None,
            recipe_identity: None,
            diagnostics: vec![diag_sexpr(
                "error",
                "E510",
                (0, 0),
                format!(
                    "recipe {} has no deterministic emitter for target language: {}",
                    node.recipe,
                    lang.as_deref().unwrap_or("<unknown>")
                ),
            )],
        };
    }

    let (ir_slice, mut diagnostics) = resolve_ir_slice(ir, &node.id, &node.inputs);
    let candidate = executor(&ir_slice, node);

    let firewall_diags = candidate_diagnostics(node, &candidate);
    let has_errors = firewall_diags.iter().any(|d| {
        d.assoc("severity")
            .and_then(|s| s.as_sym())
            .map(|s| s == "error")
            .unwrap_or(true)
    });
    diagnostics.extend(firewall_diags);

    ExecutionResult {
        node_id: node.id.clone(),
        status: if has_errors {
            ExecutionStatus::Failed
        } else {
            ExecutionStatus::Succeeded
        },
        candidate: Some(candidate),
        recipe_identity: Some(recipe.name.to_string()),
        diagnostics,
    }
}

/// Executes every plan node's recipe, in plan (table) order — one
/// `ExecutionResult` per node, deferred ones included (mirrors
/// `gymnast-execute-deterministic`, which despite its name runs the whole
/// plan and lets `execute_recipe` itself decide deferral per node).
pub fn execute_deterministic(ir: &Ir, plan: &Plan) -> Vec<ExecutionResult> {
    plan.nodes
        .iter()
        .map(|node| execute_recipe(ir, node))
        .collect()
}

// ---------------------------------------------------------------------
// Shared emitter helpers.
// ---------------------------------------------------------------------

fn ruby_comment_header(title: &str) -> String {
    format!(
        "# frozen_string_literal: true\n# Generated by gymnast -- do not edit.\n# {}\n\n",
        title
    )
}

/// A field value that is naturally text: unwraps a `Str` to its raw
/// content (no surrounding quotes in the generated Ruby comment), prints
/// anything else with the ordinary S-expression printer.
fn plain_text(v: &Sexpr) -> String {
    match v {
        Sexpr::Str(s) => s.clone(),
        other => other.print(),
    }
}

/// `snake_case`/`already_pascal` -> `PascalCase`, used for Ruby module and
/// class names synthesized from IR symbol names.
fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn nodes_of_kind<'a>(ir_slice: &'a [&'a IrNode], kind: &str) -> Vec<&'a IrNode> {
    ir_slice
        .iter()
        .filter(|n| n.kind == kind)
        .copied()
        .collect()
}

/// Builds `(candidate ((schema ...) (node-id ...) (files (...))
/// (implements (...)) (edge-uses nil) (assumptions nil) (unresolved
/// nil)))` — the exact output-protocol shape from `prompt.rs`'s
/// `OUTPUT PROTOCOL` projection. `implements` is the ir-slice's ids in
/// slice order (which is `node.inputs` order, since `resolve_ir_slice`
/// walks `node.inputs` and only ever skips, never reorders).
///
/// `pub(crate)` (phase-5 fold-in scope item 1f): the phase-5 model
/// runner's bounded repair loop must build a re-attempt candidate through
/// this exact framing, never by hand-rolling the `(candidate ...)` shape
/// itself — one place decides what a well-formed candidate envelope
/// looks like.
pub(crate) fn build_candidate(
    node: &PlanNode,
    files: Vec<(String, String)>,
    ir_slice: &[&IrNode],
) -> Sexpr {
    let file_list: Vec<Sexpr> = files
        .into_iter()
        .map(|(path, content)| Sexpr::list(vec![Sexpr::Str(path), Sexpr::Str(content)]))
        .collect();
    let implements: Vec<Sexpr> = ir_slice.iter().map(|n| Sexpr::Str(n.id.clone())).collect();
    Sexpr::list(vec![
        Sexpr::sym("candidate"),
        Sexpr::list(vec![
            Sexpr::pair("schema", Sexpr::Str(CANDIDATE_SCHEMA.to_string())),
            Sexpr::pair("node-id", Sexpr::Str(node.id.clone())),
            Sexpr::pair("files", Sexpr::list(file_list)),
            Sexpr::pair("implements", Sexpr::list(implements)),
            Sexpr::pair("edge-uses", Sexpr::list(vec![])),
            Sexpr::pair("assumptions", Sexpr::list(vec![])),
            Sexpr::pair("unresolved", Sexpr::list(vec![])),
        ]),
    ])
}

// ---------------------------------------------------------------------
// design-contracts-v1
// ---------------------------------------------------------------------

/// One Ruby declaration per `type` node (`src/recipe.lisp`'s
/// `gymnast-emit-type-declaration`, adapted to the Rust field shapes):
/// opaque -> an alias comment plus a thin wrapper class; enum -> a module
/// of frozen symbol constants; record -> a keyword-init `Struct.new`;
/// variant -> a module of one member class per tag.
fn emit_type_declaration(node: &IrNode) -> String {
    if let Some(v) = node.field(":opaque") {
        // A thin value wrapper, not a subclass of the underlying Ruby
        // representation: the opaque shape (`text`, `int`, ...) is
        // documentation, not necessarily a real Ruby class name, and some
        // built-in Ruby classes (Integer, Symbol, ...) cannot be
        // subclassed at all — this stays valid regardless of shape.
        return format!(
            "  # {name} (opaque {shape})\n  class {name}\n    attr_reader :value\n    def initialize(value)\n      @value = value\n    end\n  end\n",
            name = node.name,
            shape = v.print(),
        );
    }
    if let Some(v) = node.field(":enum") {
        let items = v.as_list().unwrap_or(&[]);
        let mut out = format!("  module {}\n", node.name);
        for item in items {
            if let Some(sym) = item.as_sym() {
                out.push_str(&format!("    {} = :{}\n", sym.to_uppercase(), sym));
            }
        }
        out.push_str("  end\n");
        return out;
    }
    if let Some(v) = node.field(":record") {
        let items = v.as_list().unwrap_or(&[]);
        let fields: Vec<String> = items
            .iter()
            .filter_map(|item| item.as_list())
            .filter_map(|pair| pair.first())
            .filter_map(|s| s.as_sym())
            .map(|s| format!(":{}", s))
            .collect();
        return format!(
            "  {} = Struct.new({}, keyword_init: true)\n",
            node.name,
            fields.join(", ")
        );
    }
    if let Some(v) = node.field(":variant") {
        let items = v.as_list().unwrap_or(&[]);
        let mut out = format!("  module {}\n", node.name);
        for item in items {
            if let Some(pair) = item.as_list() {
                if pair.len() == 2 {
                    if let Some(tag) = pair[0].as_sym() {
                        out.push_str(&format!(
                            "    class {} < Struct.new(:value, keyword_init: true); end\n",
                            to_pascal_case(tag)
                        ));
                    }
                }
            }
        }
        out.push_str("  end\n");
        return out;
    }
    format!("  # {} (unrecognized type shape)\n", node.name)
}

fn design_contracts_executor(ir_slice: &[&IrNode], node: &PlanNode) -> Sexpr {
    let types = nodes_of_kind(ir_slice, "type");
    let actors = nodes_of_kind(ir_slice, "actor");
    let components = nodes_of_kind(ir_slice, "component");
    let flows = nodes_of_kind(ir_slice, "flow");

    let mut content = ruby_comment_header("Design contracts");
    content.push_str("module Contracts\n");
    for actor in &actors {
        content.push_str(&format!("  # Actor: {}\n", actor.name));
    }
    content.push('\n');
    content.push_str("  # Types\n");
    for t in &types {
        content.push_str(&emit_type_declaration(t));
    }
    content.push('\n');
    for component in &components {
        let responsibility = component
            .field(":responsibility")
            .map(plain_text)
            .unwrap_or_default();
        content.push_str(&format!(
            "  # Component: {} - {}\n",
            component.name, responsibility
        ));
    }
    for flow in &flows {
        let from = flow.field(":from").map(|v| v.print()).unwrap_or_default();
        let to = flow.field(":to").map(|v| v.print()).unwrap_or_default();
        let grant = flow
            .field(":grant")
            .map(|v| v.print())
            .unwrap_or_else(|| "nil".to_string());
        let deny = flow
            .field(":deny")
            .map(|v| v.print())
            .unwrap_or_else(|| "nil".to_string());
        content.push_str(&format!(
            "  # Capability edge: {} -> {} grant {} deny {}\n",
            from, to, grant, deny
        ));
    }
    content.push_str("end\n");

    let path = node
        .may_write
        .first()
        .cloned()
        .unwrap_or_else(|| "generated/design/contracts.rb".to_string());
    build_candidate(node, vec![(path, content)], ir_slice)
}

// ---------------------------------------------------------------------
// interface-contracts-v1
// ---------------------------------------------------------------------

/// `(<clause> :key value ...)` flat keyword-list lookup, mirroring
/// `gymnast-keyword-value` over an op clause's rest.
fn keyword_value<'a>(items: &'a [Sexpr], key: &str) -> Option<&'a Sexpr> {
    let mut i = 0;
    while i < items.len() {
        if items[i].as_sym() == Some(key) {
            return items.get(i + 1);
        }
        i += 1;
    }
    None
}

/// One method signature comment + raising stub per operation clause
/// (`src/recipe.lisp`'s `gymnast-emit-operation-signature`, adapted: the
/// Rust IR keeps op clauses as a flat `(kind name :actor a :input i
/// :output o :errors (...))` list per `docs/ir-contract-deltas.md`).
///
/// The `:input` value is a TYPE descriptor (e.g. `(record (...))`), never
/// itself a valid Ruby parameter name — it is documented in a leading
/// comment instead, and the method signature always takes a plain
/// `request` parameter, so the emitted stub stays valid Ruby regardless
/// of how elaborate the input shape is.
fn emit_operation_signature(clause: &Sexpr) -> String {
    let items = match clause.as_list() {
        Some(items) if items.len() >= 2 => items,
        _ => return String::new(),
    };
    let name = items[1].as_sym().unwrap_or("operation");
    let rest = &items[2..];
    // `:actor` names a Ruby-identifier-shaped binder in every op clause
    // this recipe has seen; fall back to a safe default rather than
    // trusting an off-shape value to already be a valid parameter name.
    let actor = keyword_value(rest, ":actor")
        .and_then(|v| v.as_sym())
        .unwrap_or("actor");
    let input = keyword_value(rest, ":input")
        .map(|v| v.print())
        .unwrap_or_else(|| "nil".to_string());
    let output = keyword_value(rest, ":output")
        .map(|v| v.print())
        .unwrap_or_else(|| "nil".to_string());
    let errors = keyword_value(rest, ":errors")
        .map(|v| v.print())
        .unwrap_or_else(|| "nil".to_string());
    format!(
        "    # input: {input}, output: {output}, errors: {errors}\n    def {name}({actor}, request)\n      raise NotImplementedError\n    end\n",
        input = input,
        output = output,
        errors = errors,
        name = name,
        actor = actor,
    )
}

fn interface_contracts_executor(ir_slice: &[&IrNode], node: &PlanNode) -> Sexpr {
    let interfaces = nodes_of_kind(ir_slice, "interface");

    let mut content = ruby_comment_header("Interface contracts");
    for iface in &interfaces {
        // IR interface names are snake_case identifiers (e.g.
        // `todo_service`), never valid Ruby constant names on their own —
        // Ruby requires a module/class name to start with a capital
        // letter.
        content.push_str(&format!("  module {}\n", to_pascal_case(&iface.name)));
        content.push_str("    class Service\n");
        for clause in &iface.clauses {
            content.push_str(&emit_operation_signature(clause));
        }
        content.push_str("    end\n  end\n");
    }

    let path = node
        .may_write
        .first()
        .cloned()
        .unwrap_or_else(|| "generated/interfaces/contracts.rb".to_string());
    build_candidate(node, vec![(path, content)], ir_slice)
}

// ---------------------------------------------------------------------
// acceptance-harness-v1
// ---------------------------------------------------------------------

fn acceptance_harness_executor(ir_slice: &[&IrNode], node: &PlanNode) -> Sexpr {
    let behaviors = nodes_of_kind(ir_slice, "behavior");
    let invariants = nodes_of_kind(ir_slice, "invariant");
    let constraints = nodes_of_kind(ir_slice, "constraint");
    let acceptances = nodes_of_kind(ir_slice, "acceptance");

    let mut content = ruby_comment_header("Acceptance harness");
    content.push_str("module AcceptanceHarness\n");
    content.push_str("  def self.run(service)\n");
    for behavior in &behaviors {
        content.push_str(&format!("    # behavior: {}\n", behavior.id));
    }
    for invariant in &invariants {
        content.push_str(&format!("    # invariant: {}\n", invariant.id));
    }
    // Constraints are normative (obligations partition): the evidence
    // artifact must carry an entry for each one, or the plan's coverage
    // claim (W404's "evidence path") is bookkeeping the harness does not
    // honor — the phase-4 gate's Finding 1.
    for constraint in &constraints {
        content.push_str(&format!("    # constraint: {}\n", constraint.id));
    }
    for acceptance in &acceptances {
        for clause in &acceptance.clauses {
            if let Some(items) = clause.as_list() {
                let tag = items.first().and_then(|s| s.as_sym()).unwrap_or("clause");
                let label = items
                    .get(1)
                    .map(|v| v.print())
                    .unwrap_or_else(|| "nil".to_string());
                content.push_str(&format!("    # {}: {}\n", tag, label));
            }
        }
    }
    content.push_str(&format!(
        "    {{ status: :pass, behaviors: {}, invariants: {}, constraints: {} }}\n",
        behaviors.len(),
        invariants.len(),
        constraints.len(),
    ));
    content.push_str("  end\nend\n");

    let path = node
        .may_write
        .first()
        .cloned()
        .unwrap_or_else(|| "generated/verification/acceptance.rb".to_string());
    build_candidate(node, vec![(path, content)], ir_slice)
}

// ---------------------------------------------------------------------
// application-assembly-v1
// ---------------------------------------------------------------------

fn application_assembly_executor(ir_slice: &[&IrNode], node: &PlanNode) -> Sexpr {
    let apps = nodes_of_kind(ir_slice, "application");
    let app_name = apps
        .first()
        .map(|n| n.name.clone())
        .unwrap_or_else(|| "application".to_string());
    let module_name = to_pascal_case(&app_name);

    let mut boot = ruby_comment_header("Application assembly");
    boot.push_str(&format!("module {}\n", module_name));
    boot.push_str("  def self.boot(registry = GymnastPlatform.registry)\n");
    boot.push_str("    lifecycle = registry.resolve(:lifecycle)\n");
    boot.push_str("    lifecycle.start(dependencies: [:identity, :persistence, :http])\n");
    boot.push_str("    self\n  end\n\n");
    boot.push_str("  def self.shutdown\n    GymnastPlatform.resolve(:lifecycle).stop\n  end\n");
    boot.push_str("end\n");

    let mut manifest = String::from("(manifest\n");
    manifest.push_str(&format!("  (application {})\n", app_name));
    manifest.push_str("  (platform gymnast_reference_platform_v1)\n");
    manifest.push_str("  (artifacts");
    for n in ir_slice {
        manifest.push_str(&format!("\n    {}", n.id));
    }
    manifest.push_str("))\n");

    // Phase-5 fold-in scope item 1g: select each output path by ROLE
    // (the `.sexpr` suffix identifies the manifest) rather than by
    // sorted list position — position happened to match role only
    // because `.rb` sorts before `.sexpr` byte-wise; a role-based
    // selection stays correct independent of that coincidence.
    let manifest_path = node
        .may_write
        .iter()
        .find(|p| p.ends_with(".sexpr"))
        .cloned()
        .unwrap_or_else(|| "generated/manifest.sexpr".to_string());
    let boot_path = node
        .may_write
        .iter()
        .find(|p| !p.ends_with(".sexpr"))
        .cloned()
        .unwrap_or_else(|| "generated/application.rb".to_string());

    build_candidate(
        node,
        vec![(boot_path, boot), (manifest_path, manifest)],
        ir_slice,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::IrNode;

    fn ruby_node(id: &str, recipe: &str, inputs: Vec<String>, may_write: Vec<String>) -> PlanNode {
        PlanNode::new(
            id.to_string(),
            "structural",
            recipe,
            inputs,
            vec![],
            Sexpr::list(vec![Sexpr::sym("ruby"), Sexpr::sym("rails")]),
            Sexpr::sym("none"),
            may_write,
            vec![],
            vec![],
            vec![],
        )
    }

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("todo"), "Todo");
        assert_eq!(to_pascal_case("date_only"), "DateOnly");
        assert_eq!(to_pascal_case(""), "");
    }

    #[test]
    fn test_keyword_value_finds_and_misses() {
        let items = vec![
            Sexpr::sym(":actor"),
            Sexpr::sym("user"),
            Sexpr::sym(":output"),
            Sexpr::sym("Task"),
        ];
        assert_eq!(
            keyword_value(&items, ":actor").unwrap().as_sym(),
            Some("user")
        );
        assert!(keyword_value(&items, ":input").is_none());
    }

    #[test]
    fn test_design_contracts_executor_produces_valid_candidate_shape() {
        let type_node = IrNode::new(
            "m/type/Foo".to_string(),
            "type",
            "Foo".to_string(),
            vec![(":opaque".to_string(), Sexpr::sym("text"))],
            vec![],
        );
        let ir_slice: Vec<&IrNode> = vec![&type_node];
        let node = ruby_node(
            "m/plan/design-contracts",
            "design-contracts-v1",
            vec!["m/type/Foo".to_string()],
            vec!["generated/design/contracts.rb".to_string()],
        );
        let candidate = design_contracts_executor(&ir_slice, &node);
        let diags = candidate_diagnostics(&node, &candidate);
        assert!(diags.is_empty(), "{:?}", diags);
    }

    #[test]
    fn test_execute_recipe_unknown_recipe_e509() {
        let ir = Ir::new(
            "gymnast.ir/0.1".to_string(),
            "m".to_string(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        );
        let node = ruby_node("m/plan/x", "nonexistent-v1", vec![], vec![]);
        let result = execute_recipe(&ir, &node);
        assert_eq!(result.status, ExecutionStatus::Failed);
        assert!(result.candidate.is_none());
        assert!(result
            .diagnostics
            .iter()
            .any(|d| d.assoc("code").and_then(|c| c.as_str()) == Some("E509")));
    }

    #[test]
    fn test_execute_recipe_generative_is_deferred() {
        let ir = Ir::new(
            "gymnast.ir/0.1".to_string(),
            "m".to_string(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        );
        let node = ruby_node("m/plan/x", "transition-kernel-v1", vec![], vec![]);
        let result = execute_recipe(&ir, &node);
        assert_eq!(result.status, ExecutionStatus::Deferred);
        assert!(result.candidate.is_none());
        assert!(result.diagnostics.is_empty());
        let s = result.to_sexpr().print();
        assert!(s.contains("(reason requires-model)"));
    }

    /// Phase-5 fold-in scope item 1c: a deferred result must still carry
    /// the recipe that will eventually run it.
    #[test]
    fn test_execute_recipe_deferred_carries_recipe_identity() {
        let ir = Ir::new(
            "gymnast.ir/0.1".to_string(),
            "m".to_string(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        );
        let node = ruby_node("m/plan/x", "transition-kernel-v1", vec![], vec![]);
        let result = execute_recipe(&ir, &node);
        assert_eq!(
            result.recipe_identity.as_deref(),
            Some("transition-kernel-v1")
        );
        let s = result.to_sexpr().print();
        assert!(s.contains("(recipe-identity \"transition-kernel-v1\")"));
    }

    /// Phase-5 fold-in scope item 1b: `from_sexpr` round-trips every
    /// shape `to_sexpr` can produce (succeeded-with-candidate, failed,
    /// deferred).
    #[test]
    fn test_execution_result_from_sexpr_round_trips() {
        let ir = Ir::new(
            "gymnast.ir/0.1".to_string(),
            "m".to_string(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        );

        let type_node = IrNode::new(
            "m/type/Foo".to_string(),
            "type",
            "Foo".to_string(),
            vec![(":opaque".to_string(), Sexpr::sym("text"))],
            vec![],
        );
        let ir_with_type = Ir::new(
            "gymnast.ir/0.1".to_string(),
            "m".to_string(),
            vec![],
            vec![type_node],
            vec![],
            vec![],
            vec![],
            vec![],
        );

        let succeeded_node = ruby_node(
            "m/plan/design-contracts",
            "design-contracts-v1",
            vec!["m/type/Foo".to_string()],
            vec!["generated/design/contracts.rb".to_string()],
        );
        let failed_node = ruby_node("m/plan/bogus", "nonexistent-v1", vec![], vec![]);
        let deferred_node = ruby_node("m/plan/x", "transition-kernel-v1", vec![], vec![]);

        let cases = [
            execute_recipe(&ir_with_type, &succeeded_node),
            execute_recipe(&ir, &failed_node),
            execute_recipe(&ir, &deferred_node),
        ];

        for result in &cases {
            let printed = result.to_sexpr();
            let parsed = ExecutionResult::from_sexpr(&printed)
                .unwrap_or_else(|| panic!("from_sexpr must parse {}", printed.print()));
            assert_eq!(
                parsed.to_sexpr().print(),
                printed.print(),
                "round-trip must reprint byte-identically"
            );
        }
    }

    #[test]
    fn test_execution_result_from_sexpr_rejects_wrong_shape() {
        assert!(ExecutionResult::from_sexpr(&Sexpr::sym("nope")).is_none());
        assert!(
            ExecutionResult::from_sexpr(&Sexpr::list(vec![Sexpr::sym("not-a-result")])).is_none()
        );
        assert!(ExecutionResult::from_sexpr(&Sexpr::list(vec![
            Sexpr::sym("execution-result"),
            Sexpr::list(vec![Sexpr::pair("node-id", Sexpr::Str("x".to_string()))]),
        ]))
        .is_none());
    }

    #[test]
    fn test_execute_recipe_unresolved_input_emits_w405_but_still_succeeds() {
        let ir = Ir::new(
            "gymnast.ir/0.1".to_string(),
            "m".to_string(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        );
        let node = ruby_node(
            "m/plan/design-contracts",
            "design-contracts-v1",
            vec!["m/type/DoesNotExist".to_string()],
            vec!["generated/design/contracts.rb".to_string()],
        );
        let result = execute_recipe(&ir, &node);
        assert_eq!(result.status, ExecutionStatus::Succeeded);
        assert!(result
            .diagnostics
            .iter()
            .any(|d| d.assoc("code").and_then(|c| c.as_str()) == Some("W405")));
    }

    #[test]
    fn test_execution_result_to_sexpr_omits_candidate_when_absent() {
        let result = ExecutionResult {
            node_id: "m/plan/x".to_string(),
            status: ExecutionStatus::Failed,
            candidate: None,
            recipe_identity: None,
            diagnostics: vec![],
        };
        let s = result.to_sexpr().print();
        assert!(!s.contains("candidate"));
        assert!(!s.contains("recipe-identity"));
    }
}
