//! Prompt compilation: pure projections of plan-node contracts into
//! `PromptPackage` values (`docs/rust-port-plan-phase3.md`, section C).
//! Ports `src/prompt.lisp`'s behavioral intent against the Rust IR/plan
//! contracts (`docs/ir-contract-deltas.md`); text formatting choices not
//! pinned by either the plan document or the oracle tests (blank-line
//! spacing between multi-line blocks, trailing newline) follow a single
//! internal convention rather than Lamedh's exact whitespace, since this
//! shape has no Lamedh byte-compatible golden of its own.
//!
//! A `PromptPackage` is a pure function of `(ir, plan, node)`: nothing in
//! `compile_prompt` reads process time, environment, or map iteration
//! order — every collection here is a `Vec` built by walking sorted or
//! declaration-ordered inputs.

use crate::fingerprint;
use crate::ir::{resolve_ir_slice, Ir, IrNode};
use crate::plan::{extension_for, target_language, Plan, PlanNode};
use crate::sexpr::Sexpr;

const PROMPT_SCHEMA: &str = "gymnast.prompt/0.1";
const CANDIDATE_SCHEMA: &str = "gymnast.candidate/0.1";

const CLOSING_INSTRUCTION: &str = "Return only the candidate S-expression. Report no confidence score. If the contract is not locally closed, return an unresolved entry and no files.";

/// One compiled prompt package: the projection of a single plan node's
/// contract into model-facing text plus the structured pieces that back
/// it (section C).
#[derive(Debug, Clone, PartialEq)]
pub struct PromptPackage {
    pub schema: String,
    pub node_id: String,
    pub node_fingerprint: String,
    pub executor: String,
    pub model_policy: Sexpr,
    /// Resolved from `node.inputs`, in input order (already sorted at
    /// plan-construction time). Unresolved ids are dropped rather than
    /// panicking; `docs/rust-port-plan-phase3.md`'s oracle test 8 pins
    /// that nothing is dropped for `todo.gym`, where every id resolves.
    pub ir_slice: Vec<IrNode>,
    /// `(dep plan-node id, dep fingerprint | "missing")`.
    pub dependency_slice: Vec<(String, String)>,
    pub output_protocol: Sexpr,
    pub text: String,
    pub fingerprint: String,
}

impl PromptPackage {
    #[allow(clippy::too_many_arguments)]
    fn fields_sexpr(
        schema: &str,
        node_id: &str,
        node_fingerprint: &str,
        executor: &str,
        model_policy: &Sexpr,
        ir_slice: &[IrNode],
        dependency_slice: &[(String, String)],
        output_protocol: &Sexpr,
        text: &str,
    ) -> Sexpr {
        let items = vec![
            Sexpr::pair("schema", Sexpr::Str(schema.to_string())),
            Sexpr::pair("node-id", Sexpr::Str(node_id.to_string())),
            Sexpr::pair("node-fingerprint", Sexpr::Str(node_fingerprint.to_string())),
            Sexpr::pair("executor", Sexpr::sym(executor)),
            Sexpr::pair("model-policy", model_policy.clone()),
            Sexpr::pair(
                "ir-slice",
                Sexpr::list(ir_slice.iter().map(|n| n.to_sexpr()).collect()),
            ),
            Sexpr::pair(
                "dependency-slice",
                Sexpr::list(
                    dependency_slice
                        .iter()
                        .map(|(id, fp)| {
                            Sexpr::list(vec![Sexpr::Str(id.clone()), Sexpr::Str(fp.clone())])
                        })
                        .collect(),
                ),
            ),
            Sexpr::pair("output-protocol", output_protocol.clone()),
            Sexpr::pair("text", Sexpr::Str(text.to_string())),
        ];
        Sexpr::list(vec![Sexpr::sym("prompt-package"), Sexpr::list(items)])
    }

    /// `(prompt-package (... (fingerprint "fnv1a64:...")))` — the same
    /// nine fields as the fingerprinted form, plus the fingerprint,
    /// built on the fingerprint-free form so the two can never drift
    /// (same discipline as `Ir`/`Plan`).
    pub fn to_sexpr(&self) -> Sexpr {
        let base = Self::fields_sexpr(
            &self.schema,
            &self.node_id,
            &self.node_fingerprint,
            &self.executor,
            &self.model_policy,
            &self.ir_slice,
            &self.dependency_slice,
            &self.output_protocol,
            &self.text,
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

/// Role text per node class, verbatim from `src/prompt.lisp`'s
/// `gymnast-node-role` (plan section C.1).
fn role_text(class: &str) -> &'static str {
    match class {
        "generative" => "Produce one candidate implementation for this closed node contract.",
        "verification" => {
            "Materialize the independent verifier projection. Do not inspect or trust generator rationale."
        }
        "structural" => "Apply the named deterministic compiler recipe exactly.",
        _ => "Assemble only the declared artifacts and capability edges.",
    }
}

/// Content hint by target language (plan section C, output protocol):
/// ruby/go/java/python/rust -> capitalized language name; anything else
/// -> the generic placeholder.
fn content_hint(target: &Sexpr) -> &'static str {
    match target_language(target).as_deref() {
        Some("ruby") => "<valid Ruby source code>",
        Some("go") => "<valid Go source code>",
        Some("java") => "<valid Java source code>",
        Some("python") => "<valid Python source code>",
        Some("rust") => "<valid Rust source code>",
        _ => "<complete-content>",
    }
}

/// Content hint for one output PATH: non-source artifacts (.sexpr) must
/// never be told they contain target-language source — the extension
/// rewriter deliberately exempts them for exactly that reason.
fn content_hint_for_path<'a>(path: &str, language_hint: &'a str) -> &'a str {
    if path.ends_with(".sexpr") {
        "<canonical S-expression>"
    } else {
        language_hint
    }
}

fn build_output_protocol(node: &PlanNode) -> Sexpr {
    let hint = content_hint(&node.target);
    let files: Vec<Sexpr> = node
        .may_write
        .iter()
        .map(|path| {
            Sexpr::list(vec![
                Sexpr::Str(path.clone()),
                Sexpr::Str(content_hint_for_path(path, hint).to_string()),
            ])
        })
        .collect();

    let items = vec![
        Sexpr::pair("schema", Sexpr::Str(CANDIDATE_SCHEMA.to_string())),
        Sexpr::pair("node-id", Sexpr::Str(node.id.clone())),
        Sexpr::pair("files", Sexpr::list(files)),
        Sexpr::pair("implements", Sexpr::Str("<ir-node-id-list>".to_string())),
        Sexpr::pair("edge-uses", Sexpr::list(vec![])),
        Sexpr::pair("assumptions", Sexpr::list(vec![])),
        Sexpr::pair("unresolved", Sexpr::list(vec![])),
    ];
    Sexpr::list(vec![Sexpr::sym("candidate"), Sexpr::list(items)])
}

/// Framework conventions hint, ported verbatim from `src/prompt.lisp`'s
/// `gymnast-target-framework-hint`, but the framework is read as the
/// SECOND element of the target list (Rust IR contract: `(ruby rails)`)
/// rather than a `:framework` keyword pair (`docs/ir-contract-deltas.md`).
fn framework_hint(target: &Sexpr) -> String {
    let items = match target.as_list() {
        Some(items) => items,
        None => return String::new(),
    };
    let lang = items.first().and_then(|s| s.as_sym());
    let framework = items.get(1).and_then(|s| s.as_sym());
    match (lang, framework) {
        (Some("ruby"), Some("rails")) => "\nUse Rails conventions: ActiveRecord models, ApplicationRecord base class, standard Rails error handling.".to_string(),
        (Some("go"), Some("stdlib")) => "\nUse Go stdlib conventions: exported types with receiver methods, error returns, context.Context parameters.".to_string(),
        (Some("java"), Some("spring")) => "\nUse Spring conventions: @Repository/@Service annotations, JPA entities, @Transactional methods.".to_string(),
        (Some("python"), Some("django")) => "\nUse Django conventions: models.Model subclasses, Manager/QuerySet patterns, transaction.atomic blocks.\nPYTHON STRING REQUIREMENT: use ONLY single-quoted strings ('...') everywhere in the Python code. Never use double-quoted strings or triple-double-quoted docstrings — use triple-single-quoted ('''...''') for docstrings instead. This is mandatory because the code is inside an S-expression double-quoted string.".to_string(),
        (Some("rust"), Some("actix")) => "\nUse Actix-web conventions: extractors, App::new().service() routing, impl Handler patterns, Result<HttpResponse>.".to_string(),
        (_, Some(f)) => format!("\nUse {} conventions.", f),
        _ => String::new(),
    }
}

fn header_block(node: &PlanNode) -> String {
    format!(
        "GYMNAST NODE CONTRACT\nNode: {}\nRecipe: {}\nRole: {}",
        node.id,
        node.recipe,
        role_text(&node.class)
    )
}

/// TARGET section: the target sexpr printed, then the escaping/content
/// rules text ported from `src/prompt.lisp`'s `gymnast-target-section`
/// (plan section C.2).
fn target_section(node: &PlanNode) -> String {
    let lang_opt = target_language(&node.target);
    let lang = lang_opt.clone().unwrap_or_else(|| "unknown".to_string());
    let ext = extension_for(lang_opt.as_deref());
    format!(
        "TARGET\n{sexpr}\n\nCRITICAL: Every file content string in FILES must be valid {lang} source code.\nThe output files end in {ext} — their content is {lang}, never Lisp/Scheme/Clojure/pseudocode.\nThe S-expression envelope (candidate ...) is metadata framing; file content strings inside it are real {lang} source code.\nESCAPING: file content is inside S-expression double-quoted strings. Backslash-escape every literal double-quote and every literal backslash in file content. Prefer single-quoted strings in {lang} where the language allows it.{hint}",
        sexpr = node.target.print(),
        lang = lang,
        ext = ext,
        hint = framework_hint(&node.target),
    )
}

/// CAPABILITY CONTRACTS section: phase 3 has no platform kit, so every
/// capability projects as its bare name on an indented line (the Lamedh
/// fallback path in `gymnast-format-capability` for a missing capability
/// definition). Omitted entirely when the node carries no capabilities.
///
/// PHASE 4 TODO: once a Rust platform-kit registry exists, look each
/// capability up and project its version/guarantees/failure-modes the
/// way `gymnast-format-capability` does when a definition IS found.
fn capability_section(node: &PlanNode) -> String {
    if node.capabilities.is_empty() {
        return String::new();
    }
    let lines: Vec<String> = node
        .capabilities
        .iter()
        .map(|c| format!("  {}", c))
        .collect();
    format!("CAPABILITY CONTRACTS\n{}", lines.join("\n"))
}

/// Strips an `(aggregate A B C)` head, printing the plain list otherwise
/// (plan section C.4, "Entities:").
fn entities_list(of_val: &Sexpr) -> String {
    let items: Vec<&Sexpr> = match of_val {
        Sexpr::List(items) => match items.first().and_then(|s| s.as_sym()) {
            Some("aggregate") => items[1..].iter().collect(),
            _ => items.iter().collect(),
        },
        other => vec![other],
    };
    items
        .iter()
        .map(|s| s.print())
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_state_block(node: &IrNode) -> String {
    let mut lines = vec![format!("  {}:", node.name)];
    if let Some(v) = node.field(":aggregate") {
        lines.push(format!("    Aggregate: {}", v.print()));
    }
    if let Some(v) = node.field(":versioned") {
        lines.push(format!("    Versioning: {}", v.print()));
    }
    if let Some(v) = node.field(":consistency") {
        lines.push(format!("    Consistency: {}", v.print()));
    }
    if let Some(v) = node.field(":durability") {
        lines.push(format!("    Durability: {}", v.print()));
    }
    if let Some(v) = node.field(":of") {
        lines.push(format!("    Entities: {}", entities_list(v)));
    }
    lines.join("\n")
}

/// STATE MODEL section: one block per `state` node in the ir-slice
/// (plan section C.4). Omitted entirely when the ir-slice has no state
/// nodes.
fn state_model_section(ir_slice: &[IrNode]) -> String {
    let nodes: Vec<&IrNode> = ir_slice.iter().filter(|n| n.kind == "state").collect();
    if nodes.is_empty() {
        return String::new();
    }
    let blocks: Vec<String> = nodes.iter().map(|n| format_state_block(n)).collect();
    format!("STATE MODEL\n{}", blocks.join("\n\n"))
}

fn format_type_block(node: &IrNode) -> String {
    if let Some(v) = node.field(":opaque") {
        return format!("  {} (opaque {})", node.name, v.print());
    }
    if let Some(v) = node.field(":enum") {
        let joined = match v.as_list() {
            Some(items) => items
                .iter()
                .map(|s| s.print())
                .collect::<Vec<_>>()
                .join(" | "),
            // A non-list :enum value is off-shape; show it rather than
            // rendering an empty variant set.
            None => v.print(),
        };
        return format!("  {} (enum): {}", node.name, joined);
    }
    if let Some(v) = node.field(":record") {
        let items = v.as_list().unwrap_or(&[]);
        let mut lines = vec![format!("  {} (record):", node.name)];
        for item in items {
            match item {
                Sexpr::List(pair) if pair.len() == 2 => {
                    lines.push(format!("    {}: {}", pair[0].print(), pair[1].print()));
                }
                // Off-shape entries stay VISIBLE — a malformed field the
                // model can see beats one that silently vanishes.
                other => lines.push(format!("    {}", other.print())),
            }
        }
        return lines.join("\n");
    }
    if let Some(v) = node.field(":variant") {
        let items = v.as_list().unwrap_or(&[]);
        let joined = items
            .iter()
            .map(|item| match item {
                Sexpr::List(pair) if pair.len() == 2 => {
                    format!("{} {}", pair[0].print(), pair[1].print())
                }
                // Off-shape entries stay visible rather than vanishing.
                other => other.print(),
            })
            .collect::<Vec<_>>()
            .join(" | ");
        return format!("  {} (variant): {}", node.name, joined);
    }
    format!("  {}", node.name)
}

/// TYPE REFERENCE section: one block per `type` node in the ir-slice
/// (plan section C.5). Omitted entirely when the ir-slice has no type
/// nodes.
fn type_reference_section(ir_slice: &[IrNode]) -> String {
    let nodes: Vec<&IrNode> = ir_slice.iter().filter(|n| n.kind == "type").collect();
    if nodes.is_empty() {
        return String::new();
    }
    let blocks: Vec<String> = nodes.iter().map(|n| format_type_block(n)).collect();
    format!("TYPE REFERENCE\n{}", blocks.join("\n"))
}

/// The printed body of a `tag`-headed clause: the single argument for
/// the canonical `(tag arg)` shape, or the WHOLE clause printed when the
/// arity is off-shape — a malformed clause must stay visible in the
/// projection, never silently vanish (used for `requires`/`ensures`).
fn clause_body(c: &Sexpr, tag: &str) -> Option<String> {
    let items = c.as_list()?;
    if items.first().and_then(|h| h.as_sym()) != Some(tag) {
        return None;
    }
    if items.len() == 2 {
        Some(items[1].print())
    } else {
        Some(c.print())
    }
}

/// `(fails <error> :when <pred> :preserves <sym>)` -> `"<error> when
/// <pred>, preserves <sym>"`, with the `when`/`preserves` clauses
/// optional (plan section C.6, "Failures:").
fn format_fails_clause(c: &Sexpr) -> Option<String> {
    let items = c.as_list()?;
    if items.is_empty() || items[0].as_sym() != Some("fails") {
        return None;
    }
    // Off-shape fails clauses (missing or non-symbol error name) project
    // whole rather than silently vanishing from the failure list.
    let error_name = match items.get(1).and_then(|e| e.as_sym()) {
        Some(name) => name,
        None => return Some(c.print()),
    };

    let mut when_pred: Option<&Sexpr> = None;
    let mut preserves: Option<&Sexpr> = None;
    let mut i = 2;
    while i < items.len() {
        match items[i].as_sym() {
            Some(":when") if i + 1 < items.len() => {
                when_pred = Some(&items[i + 1]);
                i += 2;
            }
            Some(":preserves") if i + 1 < items.len() => {
                preserves = Some(&items[i + 1]);
                i += 2;
            }
            _ => i += 1,
        }
    }

    let mut s = error_name.to_string();
    if let Some(p) = when_pred {
        s.push_str(&format!(" when {}", p.print()));
    }
    if let Some(pv) = preserves {
        s.push_str(&format!(", preserves {}", pv.print()));
    }
    Some(s)
}

/// `(emits <event> <qualifier>...)` -> the clause with the `emits` tag
/// stripped, printed (plan section C.6, "Emissions:").
fn format_emits_clause(c: &Sexpr) -> Option<String> {
    let items = c.as_list()?;
    if items.is_empty() || items[0].as_sym() != Some("emits") {
        return None;
    }
    let rest = &items[1..];
    if rest.len() == 1 {
        Some(rest[0].print())
    } else {
        Some(Sexpr::list(rest.to_vec()).print())
    }
}

fn join_symbols(v: &Sexpr) -> String {
    match v {
        Sexpr::List(items) => items
            .iter()
            .map(|s| s.print())
            .collect::<Vec<_>>()
            .join(", "),
        other => other.print(),
    }
}

fn format_behavior_block(node: &IrNode) -> String {
    // :on = (interface/op binder...); the head is printed as the
    // "(iface/op)" suffix on the block header, the rest are the actor
    // binders (plan section C.6).
    let (op_label, actor_binders): (Option<&str>, &[Sexpr]) = match node.field(":on") {
        Some(Sexpr::List(items)) if !items.is_empty() => (items[0].as_sym(), &items[1..]),
        _ => (None, &[]),
    };

    let mut lines = vec![match op_label {
        Some(op) => format!("  {} ({}):", node.name, op),
        None => format!("  {}:", node.name),
    }];

    if !actor_binders.is_empty() {
        lines.push(format!(
            "    Actor: {}",
            Sexpr::list(actor_binders.to_vec()).print()
        ));
    }
    if let Some(v) = node.field(":reads") {
        lines.push(format!("    Reads: {}", join_symbols(v)));
    }
    if let Some(v) = node.field(":writes") {
        lines.push(format!("    Writes: {}", join_symbols(v)));
    }
    if let Some(v) = node.field(":atomic") {
        lines.push(format!("    Atomic scope: {}", v.print()));
    }
    if let Some(v) = node.field(":idempotency") {
        lines.push(format!("    Idempotency: {}", v.print()));
    }

    let preconditions: Vec<String> = node
        .clauses
        .iter()
        .filter_map(|c| clause_body(c, "requires"))
        .collect();
    if !preconditions.is_empty() {
        lines.push("    Preconditions:".to_string());
        lines.extend(preconditions.iter().map(|p| format!("      {}", p)));
    }

    let postconditions: Vec<String> = node
        .clauses
        .iter()
        .filter_map(|c| clause_body(c, "ensures"))
        .collect();
    if !postconditions.is_empty() {
        lines.push("    Postconditions:".to_string());
        lines.extend(postconditions.iter().map(|p| format!("      {}", p)));
    }

    let failures: Vec<String> = node
        .clauses
        .iter()
        .filter_map(format_fails_clause)
        .collect();
    if !failures.is_empty() {
        lines.push("    Failures:".to_string());
        lines.extend(failures.iter().map(|f| format!("      {}", f)));
    }

    let emissions: Vec<String> = node
        .clauses
        .iter()
        .filter_map(format_emits_clause)
        .collect();
    if !emissions.is_empty() {
        lines.push("    Emissions:".to_string());
        lines.extend(emissions.iter().map(|e| format!("      {}", e)));
    }

    lines.join("\n")
}

/// BEHAVIORAL REFERENCE section: one block per `behavior` node in the
/// ir-slice, projected from the Rust IR shape (plan section C.6).
/// Omitted entirely when the ir-slice has no behavior nodes.
fn behavioral_reference_section(ir_slice: &[IrNode]) -> String {
    let nodes: Vec<&IrNode> = ir_slice.iter().filter(|n| n.kind == "behavior").collect();
    if nodes.is_empty() {
        return String::new();
    }
    let blocks: Vec<String> = nodes.iter().map(|n| format_behavior_block(n)).collect();
    format!("BEHAVIORAL REFERENCE\n{}", blocks.join("\n\n"))
}

fn obligations_section(node: &PlanNode) -> String {
    let lines: Vec<String> = node
        .obligations
        .iter()
        .map(|o| format!("  {}", o))
        .collect();
    format!("OBLIGATIONS\n{}", lines.join("\n"))
}

fn prohibitions_section(node: &PlanNode) -> String {
    let lines: Vec<String> = node
        .prohibitions
        .iter()
        .map(|p| format!("  {}", p))
        .collect();
    format!("PROHIBITIONS\n{}", lines.join("\n"))
}

fn authorized_files_section(node: &PlanNode) -> String {
    let sexpr = Sexpr::list(
        node.may_write
            .iter()
            .map(|p| Sexpr::Str(p.clone()))
            .collect(),
    );
    format!("AUTHORIZED FILES\n{}", sexpr.print())
}

fn dependencies_section(dependency_slice: &[(String, String)]) -> String {
    let sexpr = Sexpr::list(
        dependency_slice
            .iter()
            .map(|(id, fp)| Sexpr::list(vec![Sexpr::Str(id.clone()), Sexpr::Str(fp.clone())]))
            .collect(),
    );
    format!("DEPENDENCIES\n{}", sexpr.print())
}

fn output_protocol_section(output_protocol: &Sexpr) -> String {
    format!("OUTPUT PROTOCOL\n{}", output_protocol.print())
}

fn authoritative_input_section(ir_slice: &[IrNode]) -> String {
    let sexpr = Sexpr::list(ir_slice.iter().map(|n| n.to_sexpr()).collect());
    format!("AUTHORITATIVE INPUT (reference)\n{}", sexpr.print())
}

/// Assembles the full prompt text: the twelve sections of plan section
/// C in order, blank-line separated, a projection section (capability
/// contracts / state model / type reference / behavioral reference)
/// dropped entirely when its backing node set is empty, closing
/// instruction verbatim and last.
fn build_text(
    node: &PlanNode,
    ir_slice: &[IrNode],
    dependency_slice: &[(String, String)],
    output_protocol: &Sexpr,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(header_block(node));
    parts.push(target_section(node));

    let capability = capability_section(node);
    if !capability.is_empty() {
        parts.push(capability);
    }
    let state = state_model_section(ir_slice);
    if !state.is_empty() {
        parts.push(state);
    }
    let types = type_reference_section(ir_slice);
    if !types.is_empty() {
        parts.push(types);
    }
    let behavior = behavioral_reference_section(ir_slice);
    if !behavior.is_empty() {
        parts.push(behavior);
    }

    parts.push(obligations_section(node));
    parts.push(prohibitions_section(node));
    parts.push(authorized_files_section(node));
    parts.push(dependencies_section(dependency_slice));
    parts.push(output_protocol_section(output_protocol));
    parts.push(authoritative_input_section(ir_slice));
    parts.push(CLOSING_INSTRUCTION.to_string());

    format!("{}\n", parts.join("\n\n"))
}

/// `(dep plan-node id, dep fingerprint | "missing")` pairs for
/// `node.depends_on`, in dependency-order (already sorted at
/// `PlanNode::new` construction time). Shared by `compile_prompt` and
/// `cache::cache_key_material` (`docs/rust-port-plan-phase7.md` section
/// E: "REUSE that builder ... never a second copy") -- `pub(crate)` so
/// `cache.rs` can call it without widening this module's public surface.
pub(crate) fn dependency_slice(plan: &Plan, node: &PlanNode) -> Vec<(String, String)> {
    node.depends_on
        .iter()
        .map(|dep_id| {
            let fp = plan
                .nodes
                .iter()
                .find(|n| &n.id == dep_id)
                .map(|n| n.fingerprint.clone())
                .unwrap_or_else(|| "missing".to_string());
            (dep_id.clone(), fp)
        })
        .collect()
}

/// Compiles one plan node's contract into a `PromptPackage`. Pure in
/// `(ir, plan, node)`: every collection built here walks a
/// declaration-ordered or already-sorted `Vec`, never a hash map, so two
/// independent calls over the same inputs always agree byte-for-byte.
pub fn compile_prompt(ir: &Ir, plan: &Plan, node: &PlanNode) -> PromptPackage {
    // Phase-5 fold-in scope item 1d: resolution now goes through the
    // SAME `resolve_ir_slice` `recipe.rs` uses, rather than this
    // function's own `filter_map`, so the two consumers of `node.inputs`
    // can never silently disagree about which ids resolve. The warnings
    // are deliberately dropped here (not carried in `PromptPackage`,
    // whose shape is otherwise pinned byte-for-byte by the todo.gym
    // prompt goldens) — a caller that wants them calls
    // `prompt_ir_slice_warnings` directly; see that function's doc.
    let (resolved, _warnings) = resolve_ir_slice(ir, &node.id, &node.inputs);
    let ir_slice: Vec<IrNode> = resolved.into_iter().cloned().collect();

    let dependency_slice = dependency_slice(plan, node);

    let output_protocol = build_output_protocol(node);
    let text = build_text(node, &ir_slice, &dependency_slice, &output_protocol);

    let schema = PROMPT_SCHEMA.to_string();
    let node_id = node.id.clone();
    let node_fingerprint = node.fingerprint.clone();
    let executor = node.class.clone();
    let model_policy = node.model.clone();

    let fingerprint_free = PromptPackage::fields_sexpr(
        &schema,
        &node_id,
        &node_fingerprint,
        &executor,
        &model_policy,
        &ir_slice,
        &dependency_slice,
        &output_protocol,
        &text,
    );
    let fingerprint = fingerprint::fingerprint(&fingerprint_free);

    PromptPackage {
        schema,
        node_id,
        node_fingerprint,
        executor,
        model_policy,
        ir_slice,
        dependency_slice,
        output_protocol,
        text,
        fingerprint,
    }
}

/// One `PromptPackage` per plan node, in plan (table) order.
pub fn compile_prompts(ir: &Ir, plan: &Plan) -> Vec<PromptPackage> {
    plan.nodes
        .iter()
        .map(|n| compile_prompt(ir, plan, n))
        .collect()
}

/// The `W405 unresolved-input` warnings `compile_prompt(ir, plan, node)`
/// would itself compute internally, exposed separately so a caller (the
/// `prompts`/`compile` CLI subcommands) can report them without changing
/// `PromptPackage`'s pinned shape (phase-5 fold-in scope item 1d: "the
/// warnings surface through the caller, not inside the package").
/// `todo.gym` has no unresolved input anywhere in its plan, so this is
/// always empty for it — the prompts/plan goldens are unaffected whether
/// or not a caller chooses to call this.
pub fn prompt_ir_slice_warnings(ir: &Ir, node: &PlanNode) -> Vec<Sexpr> {
    resolve_ir_slice(ir, &node.id, &node.inputs).1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::plan;

    fn type_node(id: &str, name: &str, fields: Vec<(String, Sexpr)>) -> IrNode {
        IrNode::new(id.to_string(), "type", name.to_string(), fields, vec![])
    }

    fn minimal_ir_with_type() -> Ir {
        Ir::new(
            "gymnast.ir/0.1".to_string(),
            "m".to_string(),
            vec![],
            vec![type_node(
                "m/type/Foo",
                "Foo",
                vec![(":opaque".to_string(), Sexpr::sym("text"))],
            )],
            vec![],
            vec![],
            vec![],
            vec![],
        )
    }

    #[test]
    fn test_prompt_ir_slice_warnings_fires_w405_but_compile_prompt_ignores_it() {
        let ir = minimal_ir_with_type();
        let node = PlanNode::new(
            "m/plan/x".to_string(),
            "structural",
            "design-contracts-v1",
            vec!["m/type/DoesNotExist".to_string()],
            vec![],
            Sexpr::sym("lamedh"),
            Sexpr::sym("none"),
            vec![],
            vec![],
            vec![],
            vec![],
        );
        let warnings = prompt_ir_slice_warnings(&ir, &node);
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].assoc("code").and_then(|c| c.as_str()),
            Some("W405")
        );

        let pkg = compile_prompt(&ir, &plan(&ir), &node);
        assert!(pkg.ir_slice.is_empty());
        assert!(!pkg.to_sexpr().print().contains("W405"));
    }

    #[test]
    fn test_compile_prompts_one_per_node() {
        let ir = minimal_ir_with_type();
        let p = plan(&ir);
        let pkgs = compile_prompts(&ir, &p);
        assert_eq!(pkgs.len(), p.nodes.len());
    }

    #[test]
    fn test_closing_instruction_last_line() {
        let ir = minimal_ir_with_type();
        let p = plan(&ir);
        let pkgs = compile_prompts(&ir, &p);
        for pkg in &pkgs {
            let trimmed = pkg.text.trim_end_matches('\n');
            assert_eq!(trimmed.lines().last().unwrap(), CLOSING_INSTRUCTION);
        }
    }

    #[test]
    fn test_capability_section_omitted_when_empty() {
        let ir = minimal_ir_with_type();
        let p = plan(&ir);
        for node in &p.nodes {
            if node.capabilities.is_empty() {
                assert!(!compile_prompt(&ir, &p, node)
                    .text
                    .contains("CAPABILITY CONTRACTS"));
            }
        }
    }

    #[test]
    fn test_format_fails_clause_when_and_preserves() {
        let clause = Sexpr::list(vec![
            Sexpr::sym("fails"),
            Sexpr::sym("forbidden"),
            Sexpr::sym(":when"),
            Sexpr::list(vec![Sexpr::sym("not"), Sexpr::sym("ok")]),
            Sexpr::sym(":preserves"),
            Sexpr::sym("all_state"),
        ]);
        let s = format_fails_clause(&clause).unwrap();
        assert_eq!(s, "forbidden when (not ok), preserves all_state");
    }

    #[test]
    fn test_format_emits_clause_strips_tag() {
        let clause = Sexpr::list(vec![
            Sexpr::sym("emits"),
            Sexpr::sym("task_created"),
            Sexpr::sym("exactly_once_logically"),
        ]);
        let s = format_emits_clause(&clause).unwrap();
        assert_eq!(s, "(task_created exactly_once_logically)");
    }

    #[test]
    fn test_fingerprint_recomputes() {
        let ir = minimal_ir_with_type();
        let p = plan(&ir);
        for node in &p.nodes {
            let pkg = compile_prompt(&ir, &p, node);
            let full = pkg.to_sexpr();
            let stripped = match full {
                Sexpr::List(outer) => {
                    let mut outer = outer;
                    if let Some(Sexpr::List(inner)) = outer.last_mut() {
                        inner.pop();
                    }
                    Sexpr::List(outer)
                }
                other => other,
            };
            assert_eq!(pkg.fingerprint, fingerprint::fingerprint(&stripped));
        }
    }
}
