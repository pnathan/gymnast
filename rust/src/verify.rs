//! Independent verification obligations and trace-equivalence checks
//! (`docs/rust-port-plan-phase6.md`, section B). Ports `src/verify.lisp`
//! against the Rust IR contract (`docs/ir-contract-deltas.md`):
//! execution-environment extraction, obligation lowering (one
//! `verification-obligation` per acceptance clause plus one per
//! invariant/constraint node), reference verification against the
//! `transition.rs` state machine, trace equivalence, normalized
//! counterexamples, coverage analysis, and the verification bundle.
//!
//! Every function here is TOTAL over arbitrary `Sexpr`/`IrNode`/`Ir`
//! input: no panics, and every loop (`compare_traces`, the coverage/
//! obligation folds) is a bounded iteration over an already-finite,
//! already-parsed structure (bounded by `sexpr::MAX_PARSE_DEPTH` and, for
//! trace execution, `transition::TRACE_BOUND`), never unbounded
//! recursion.
//!
//! Sexpr-shape convention (see `verify_oracle_test.rs`'s file-header
//! "Resolved ambiguities" item 1, which this implementation was written
//! to satisfy either reading of): the small ad-hoc shapes this module
//! builds (`verification-obligation`, `verification-result`,
//! `execution-environment`, `divergence`, `normalized-counterexample`,
//! `trace-equivalence-result`, `coverage-analysis`, the `(gap kind
//! count)` triple) are FLAT -- `(tag (k1 v1) (k2 v2) ...)`, matching
//! `src/verify.lisp`'s plain `(list 'tag (list 'k v) ...)` construction
//! and `transition.rs`'s established convention for the same kind of
//! ad-hoc shape (`counterexample`, `violation`). Only the top-level
//! `verification-bundle` (and its `summary` sub-value) nests one level,
//! per the plan's literal example and `IrNode`/`PlanNode`'s house
//! convention.

use crate::diag::diag_sexpr;
use crate::fingerprint;
use crate::ir::{Ir, IrNode};
use crate::sexpr::Sexpr;
use crate::transition::{
    self, apply_transition, counterexample, default_trace_step, eval_predicate3, execute_trace,
    extract_transitions, make_initial_state, state_to_sexpr, trace_to_sexpr, Trace, TraceStep,
    Verdict,
};

/// Schema tag for the compiled verification bundle.
const VERIFY_SCHEMA: &str = "gymnast.verify/0.1";

fn nil() -> Sexpr {
    Sexpr::List(vec![])
}

/// Any non-nil `Sexpr` is truthy (mirrors Lisp's generalized boolean:
/// everything but `nil` holds in an `if`), matching the reference's use
/// of a bare keyword-value result (`t`, or the value itself) directly in
/// `and`/`if` tests throughout `verify.lisp`.
fn is_truthy(v: &Sexpr) -> bool {
    !matches!(v, Sexpr::List(items) if items.is_empty())
}

// -----------------------------------------------------------------------
// Execution-environment extraction.
// -----------------------------------------------------------------------

/// Extracts `(execution-environment (clock c) (randomness r) (network n)
/// (locale l) (timezone tz))` from an acceptance node's `execution`
/// clause, if any. Missing keys (including a wholly absent `execution`
/// clause) fall back to the plan's defaults: `system`/`system`/`system`
/// for clock/randomness/network, `"en-US"`/`"UTC"` for locale/timezone.
/// Total over any `IrNode`, not just a well-formed `acceptance` one.
pub fn extract_execution_env(acceptance: &IrNode) -> Sexpr {
    let exec_clause = acceptance
        .clauses
        .iter()
        .find(|c| transition::clause_head_is(c, "execution"));
    let empty: Vec<Sexpr> = Vec::new();
    let tail: &[Sexpr] = exec_clause.map(transition::clause_tail).unwrap_or(&empty);

    let clock = transition::keyword_value(tail, ":clock")
        .cloned()
        .unwrap_or_else(|| Sexpr::sym("system"));
    let randomness = transition::keyword_value(tail, ":randomness")
        .cloned()
        .unwrap_or_else(|| Sexpr::sym("system"));
    let network = transition::keyword_value(tail, ":network")
        .cloned()
        .unwrap_or_else(|| Sexpr::sym("system"));
    let locale = transition::keyword_value(tail, ":locale")
        .cloned()
        .unwrap_or_else(|| Sexpr::Str("en-US".to_string()));
    let timezone = transition::keyword_value(tail, ":timezone")
        .cloned()
        .unwrap_or_else(|| Sexpr::Str("UTC".to_string()));

    Sexpr::List(vec![
        Sexpr::sym("execution-environment"),
        Sexpr::pair("clock", clock),
        Sexpr::pair("randomness", randomness),
        Sexpr::pair("network", network),
        Sexpr::pair("locale", locale),
        Sexpr::pair("timezone", timezone),
    ])
}

fn env_field_is_sym(env: &Sexpr, key: &str, expected: &str) -> bool {
    env.assoc(key).and_then(|v| v.as_sym()) == Some(expected)
}

/// True iff `env` is fully deterministic: clock `virtual`, randomness
/// `seeded`, network `controlled`.
pub fn env_deterministic(env: &Sexpr) -> bool {
    env_field_is_sym(env, "clock", "virtual")
        && env_field_is_sym(env, "randomness", "seeded")
        && env_field_is_sym(env, "network", "controlled")
}

/// One warning diagnostic per non-deterministic environment source
/// (clock/randomness/network), each naming `acceptance_id` in its
/// message. Zero, one, two, or three diagnostics, in that fixed order.
pub fn env_diagnostics(env: &Sexpr, acceptance_id: &str) -> Vec<Sexpr> {
    let mut diags = Vec::new();
    if !env_field_is_sym(env, "clock", "virtual") {
        diags.push(diag_sexpr(
            "warning",
            "non-deterministic-clock",
            (0, 0),
            format!(
                "clock is not virtual for {}; traces may not reproduce",
                acceptance_id
            ),
        ));
    }
    if !env_field_is_sym(env, "randomness", "seeded") {
        diags.push(diag_sexpr(
            "warning",
            "non-deterministic-randomness",
            (0, 0),
            format!(
                "randomness is not seeded for {}; traces may not reproduce",
                acceptance_id
            ),
        ));
    }
    if !env_field_is_sym(env, "network", "controlled") {
        diags.push(diag_sexpr(
            "warning",
            "non-deterministic-network",
            (0, 0),
            format!(
                "network is not controlled for {}; traces may not reproduce",
                acceptance_id
            ),
        ));
    }
    diags
}

// -----------------------------------------------------------------------
// Obligation lowering.
// -----------------------------------------------------------------------

fn obligation_id(acceptance_id: &str, kind: &str, name: Option<&str>) -> String {
    match name {
        Some(n) => format!("{}/{}/{}", acceptance_id, kind, n),
        None => format!("{}/{}", acceptance_id, kind),
    }
}

fn clause_name(tail: &[Sexpr]) -> (Sexpr, String) {
    let name = tail.first().cloned().unwrap_or_else(nil);
    let text = name.as_sym().unwrap_or("").to_string();
    (name, text)
}

fn lower_property_obligation(acceptance_id: &str, clause: &Sexpr, env: &Sexpr) -> Sexpr {
    let tail = transition::clause_tail(clause);
    let (name, name_str) = clause_name(tail);
    let rest = tail.get(1..).unwrap_or(&[]);
    let generate = transition::keyword_value(rest, ":generate")
        .cloned()
        .unwrap_or_else(nil);
    let execute = transition::keyword_value(rest, ":execute")
        .cloned()
        .unwrap_or_else(nil);
    let must = transition::keyword_value(rest, ":must")
        .cloned()
        .unwrap_or_else(nil);

    Sexpr::List(vec![
        Sexpr::sym("verification-obligation"),
        Sexpr::pair(
            "id",
            Sexpr::Str(obligation_id(acceptance_id, "property", Some(&name_str))),
        ),
        Sexpr::pair("kind", Sexpr::sym("property")),
        Sexpr::pair("source", Sexpr::Str(acceptance_id.to_string())),
        Sexpr::pair("name", name),
        Sexpr::pair("generate", generate),
        Sexpr::pair("execute", execute),
        Sexpr::pair("assertion", must),
        Sexpr::pair("environment", env.clone()),
    ])
}

fn lower_scenario_obligation(acceptance_id: &str, clause: &Sexpr, env: &Sexpr) -> Sexpr {
    let tail = transition::clause_tail(clause);
    let (name, name_str) = clause_name(tail);
    let steps: Vec<Sexpr> = tail
        .get(1..)
        .unwrap_or(&[])
        .iter()
        .filter(|s| s.as_list().is_some())
        .cloned()
        .collect();

    Sexpr::List(vec![
        Sexpr::sym("verification-obligation"),
        Sexpr::pair(
            "id",
            Sexpr::Str(obligation_id(acceptance_id, "scenario", Some(&name_str))),
        ),
        Sexpr::pair("kind", Sexpr::sym("scenario")),
        Sexpr::pair("source", Sexpr::Str(acceptance_id.to_string())),
        Sexpr::pair("name", name),
        Sexpr::pair("steps", Sexpr::List(steps)),
        Sexpr::pair("environment", env.clone()),
    ])
}

fn lower_concurrency_obligation(acceptance_id: &str, clause: &Sexpr, env: &Sexpr) -> Sexpr {
    let tail = transition::clause_tail(clause);
    let (name, name_str) = clause_name(tail);
    let rest = tail.get(1..).unwrap_or(&[]);
    let actors = transition::keyword_value(rest, ":actors")
        .cloned()
        .unwrap_or_else(nil);
    let schedule = transition::keyword_value(rest, ":schedule")
        .cloned()
        .unwrap_or_else(nil);
    let must = transition::keyword_value(rest, ":must")
        .cloned()
        .unwrap_or_else(nil);

    Sexpr::List(vec![
        Sexpr::sym("verification-obligation"),
        Sexpr::pair(
            "id",
            Sexpr::Str(obligation_id(acceptance_id, "concurrency", Some(&name_str))),
        ),
        Sexpr::pair("kind", Sexpr::sym("concurrency")),
        Sexpr::pair("source", Sexpr::Str(acceptance_id.to_string())),
        Sexpr::pair("name", name),
        Sexpr::pair("actors", actors),
        Sexpr::pair("schedule", schedule),
        Sexpr::pair("assertion", must),
        Sexpr::pair("environment", env.clone()),
    ])
}

fn lower_fault_obligation(acceptance_id: &str, clause: &Sexpr, env: &Sexpr) -> Sexpr {
    let tail = transition::clause_tail(clause);
    let (name, name_str) = clause_name(tail);
    let rest = tail.get(1..).unwrap_or(&[]);
    let after = transition::keyword_value(rest, ":after")
        .cloned()
        .unwrap_or_else(nil);
    let inject = transition::keyword_value(rest, ":inject")
        .cloned()
        .unwrap_or_else(nil);
    let must = transition::keyword_value(rest, ":must")
        .cloned()
        .unwrap_or_else(nil);

    Sexpr::List(vec![
        Sexpr::sym("verification-obligation"),
        Sexpr::pair(
            "id",
            Sexpr::Str(obligation_id(acceptance_id, "fault", Some(&name_str))),
        ),
        Sexpr::pair("kind", Sexpr::sym("fault")),
        Sexpr::pair("source", Sexpr::Str(acceptance_id.to_string())),
        Sexpr::pair("name", name),
        Sexpr::pair("after", after),
        Sexpr::pair("inject", inject),
        Sexpr::pair("assertion", must),
        Sexpr::pair("environment", env.clone()),
    ])
}

/// The `coverage` clause has no name element: its tail goes straight to
/// `:key value` pairs (`docs/ir-contract-deltas.md`: "`coverage` lowers
/// to keyword pairs"). Obligation field names use OUR underscore
/// spelling (`every_operation`, ...), not renamed to hyphenated style.
fn lower_coverage_obligation(acceptance_id: &str, clause: &Sexpr, env: &Sexpr) -> Sexpr {
    let tail = transition::clause_tail(clause);
    let every_operation = transition::keyword_value(tail, ":every_operation")
        .cloned()
        .unwrap_or_else(nil);
    let every_error = transition::keyword_value(tail, ":every_error")
        .cloned()
        .unwrap_or_else(nil);
    let every_transition = transition::keyword_value(tail, ":every_transition")
        .cloned()
        .unwrap_or_else(nil);
    let every_invariant = transition::keyword_value(tail, ":every_invariant")
        .cloned()
        .unwrap_or_else(nil);
    let boundaries = transition::keyword_value(tail, ":boundaries")
        .cloned()
        .unwrap_or_else(nil);

    Sexpr::List(vec![
        Sexpr::sym("verification-obligation"),
        Sexpr::pair(
            "id",
            Sexpr::Str(obligation_id(acceptance_id, "coverage", None)),
        ),
        Sexpr::pair("kind", Sexpr::sym("coverage")),
        Sexpr::pair("source", Sexpr::Str(acceptance_id.to_string())),
        Sexpr::pair("name", Sexpr::sym("coverage")),
        Sexpr::pair("every_operation", every_operation),
        Sexpr::pair("every_error", every_error),
        Sexpr::pair("every_transition", every_transition),
        Sexpr::pair("every_invariant", every_invariant),
        Sexpr::pair("boundaries", boundaries),
        Sexpr::pair("environment", env.clone()),
    ])
}

fn lower_model_obligation(acceptance_id: &str, clause: &Sexpr, env: &Sexpr) -> Sexpr {
    let tail = transition::clause_tail(clause);
    let (name, name_str) = clause_name(tail);
    let spec: Vec<Sexpr> = tail.get(1..).unwrap_or(&[]).to_vec();

    Sexpr::List(vec![
        Sexpr::sym("verification-obligation"),
        Sexpr::pair(
            "id",
            Sexpr::Str(obligation_id(acceptance_id, "model", Some(&name_str))),
        ),
        Sexpr::pair("kind", Sexpr::sym("model")),
        Sexpr::pair("source", Sexpr::Str(acceptance_id.to_string())),
        Sexpr::pair("name", name),
        Sexpr::pair("spec", Sexpr::List(spec)),
        Sexpr::pair("environment", env.clone()),
    ])
}

/// Lowers one acceptance clause into `Some(verification-obligation)`, or
/// `None` for an `execution` clause (env-only, no obligation of its own)
/// or any other unrecognized clause head.
fn lower_clause(acceptance_id: &str, clause: &Sexpr, env: &Sexpr) -> Option<Sexpr> {
    let head = clause.as_list()?.first()?.as_sym()?;
    match head {
        "property" => Some(lower_property_obligation(acceptance_id, clause, env)),
        "scenario" => Some(lower_scenario_obligation(acceptance_id, clause, env)),
        "concurrency" => Some(lower_concurrency_obligation(acceptance_id, clause, env)),
        "fault" => Some(lower_fault_obligation(acceptance_id, clause, env)),
        "coverage" => Some(lower_coverage_obligation(acceptance_id, clause, env)),
        "model" => Some(lower_model_obligation(acceptance_id, clause, env)),
        _ => None,
    }
}

fn lower_invariant_obligation(node: &IrNode) -> Sexpr {
    let scope = node.field(":scope").cloned().unwrap_or_else(nil);
    let always = node.field(":always").cloned().unwrap_or_else(nil);
    Sexpr::List(vec![
        Sexpr::sym("verification-obligation"),
        Sexpr::pair("id", Sexpr::Str(format!("{}/invariant-check", node.id))),
        Sexpr::pair("kind", Sexpr::sym("invariant")),
        Sexpr::pair("source", Sexpr::Str(node.id.clone())),
        Sexpr::pair("name", Sexpr::sym(&node.name)),
        Sexpr::pair("scope", scope),
        Sexpr::pair("predicate", always),
        Sexpr::pair("environment", nil()),
    ])
}

fn lower_constraint_obligation(node: &IrNode) -> Sexpr {
    let class = node.field(":class").cloned().unwrap_or_else(nil);
    let scope = node.field(":scope").cloned().unwrap_or_else(nil);
    let under = node.field(":under").cloned().unwrap_or_else(nil);
    let must = node.field(":must").cloned().unwrap_or_else(nil);
    Sexpr::List(vec![
        Sexpr::sym("verification-obligation"),
        Sexpr::pair("id", Sexpr::Str(format!("{}/constraint-check", node.id))),
        Sexpr::pair("kind", Sexpr::sym("constraint")),
        Sexpr::pair("source", Sexpr::Str(node.id.clone())),
        Sexpr::pair("name", Sexpr::sym(&node.name)),
        Sexpr::pair("class", class),
        Sexpr::pair("scope", scope),
        Sexpr::pair("under", under),
        Sexpr::pair("assertion", must),
        Sexpr::pair("environment", nil()),
    ])
}

/// All verification obligations lowered from `ir`: acceptance-clause
/// obligations (each acceptance node's clauses in declared order, nodes
/// in `nodes_of_kind("acceptance")` order), THEN invariant obligations,
/// THEN constraint obligations -- matching `src/verify.lisp`'s
/// `gymnast-lower-all-obligations` append order exactly.
pub fn lower_all_obligations(ir: &Ir) -> Vec<Sexpr> {
    let mut out = Vec::new();

    for acc in ir.nodes_of_kind("acceptance") {
        let env = extract_execution_env(acc);
        for clause in &acc.clauses {
            if let Some(ob) = lower_clause(&acc.id, clause, &env) {
                out.push(ob);
            }
        }
    }
    for inv in ir.nodes_of_kind("invariant") {
        out.push(lower_invariant_obligation(inv));
    }
    for c in ir.nodes_of_kind("constraint") {
        out.push(lower_constraint_obligation(c));
    }

    out
}

fn obligation_field<'a>(o: &'a Sexpr, key: &str) -> Option<&'a Sexpr> {
    o.assoc(key)
}

fn obligation_kind(o: &Sexpr) -> &str {
    obligation_field(o, "kind")
        .and_then(|k| k.as_sym())
        .unwrap_or("")
}

fn obligation_id_str(o: &Sexpr) -> String {
    obligation_field(o, "id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

// -----------------------------------------------------------------------
// Reference verification.
// -----------------------------------------------------------------------

fn make_verification_result(
    obligation_id: &str,
    status: &str,
    trace: Option<&Trace>,
    counterexamples: Vec<Sexpr>,
    diagnostics: Vec<Sexpr>,
) -> Sexpr {
    Sexpr::List(vec![
        Sexpr::sym("verification-result"),
        Sexpr::pair("schema", Sexpr::Str(VERIFY_SCHEMA.to_string())),
        Sexpr::pair("obligation-id", Sexpr::Str(obligation_id.to_string())),
        Sexpr::pair("status", Sexpr::sym(status)),
        Sexpr::pair("trace", trace.map(trace_to_sexpr).unwrap_or_else(nil)),
        Sexpr::pair("counterexamples", Sexpr::List(counterexamples)),
        Sexpr::pair("diagnostics", Sexpr::List(diagnostics)),
    ])
}

/// A verification result carrying an explicit `basis` field:
/// `(basis checked)` when every evaluation branch was actually computed,
/// `(basis symbolic)` when any permissive default participated in the
/// verdict (phase-6 gate, finding 1 — a consumer must be able to tell a
/// vacuous pass from a real one without archaeology).
fn make_verification_result_with_basis(
    obligation_id: &str,
    status: &str,
    trace: Option<&Trace>,
    counterexamples: Vec<Sexpr>,
    diagnostics: Vec<Sexpr>,
    symbolic: Option<bool>,
) -> Sexpr {
    let mut items = vec![
        Sexpr::sym("verification-result"),
        Sexpr::pair("schema", Sexpr::Str(VERIFY_SCHEMA.to_string())),
        Sexpr::pair("obligation-id", Sexpr::Str(obligation_id.to_string())),
        Sexpr::pair("status", Sexpr::sym(status)),
    ];
    if let Some(sym) = symbolic {
        items.push(Sexpr::pair(
            "basis",
            Sexpr::sym(if sym { "symbolic" } else { "checked" }),
        ));
    }
    items.push(Sexpr::pair(
        "trace",
        trace.map(trace_to_sexpr).unwrap_or_else(nil),
    ));
    items.push(Sexpr::pair("counterexamples", Sexpr::List(counterexamples)));
    items.push(Sexpr::pair("diagnostics", Sexpr::List(diagnostics)));
    Sexpr::List(items)
}

/// The property/scenario execute-value step-splitting rule (plan section
/// B, oracle ambiguity 5): if `execute`'s first element is itself a
/// list, `execute` is the multi-step case (each element one step);
/// otherwise the whole value is the one step. Total over any shape,
/// including nil.
fn execute_steps(execute: &Sexpr) -> Vec<Sexpr> {
    if let Some(items) = execute.as_list() {
        // Plan section B: a `(sequence step1 step2 ...)` form unwraps to
        // its steps (phase-6 gate, finding 6 — reachable from the
        // surface as `execute sequence (a (x), b (y))`).
        if items.first().and_then(|h| h.as_sym()) == Some("sequence") {
            return items[1..].to_vec();
        }
        if let Some(first) = items.first() {
            if first.as_list().is_some() {
                return items.to_vec();
            }
        }
    }
    vec![execute.clone()]
}

/// Reads the `(step-index N)` field a trace violation carries (see
/// `transition.rs`'s `with_step_index`). `None` for a violation without
/// one or with a non-Int / out-of-range value.
fn violation_step_index(violation: &Sexpr) -> Option<usize> {
    let items = violation.as_list()?;
    for entry in items.iter().skip(1) {
        if let Some(pair) = entry.as_list() {
            if pair.len() == 2 && pair[0].as_sym() == Some("step-index") {
                return usize::try_from(pair[1].as_int()?).ok();
            }
        }
    }
    None
}

fn counterexamples_for_trace(trace: &Trace) -> Vec<Sexpr> {
    // Phase-7 gate, finding 7: pair each violation with the step that
    // actually produced it (via its step-index), not the trace's first
    // step -- the reference's `(car steps)` pairing misattributes
    // outcome/input/pre-state once traces are live. Fallback to the
    // first step (then a default) only for a violation with no usable
    // index, preserving totality.
    trace
        .violations
        .iter()
        .map(|v| {
            let step = violation_step_index(v)
                .and_then(|i| trace.steps.get(i))
                .or_else(|| trace.steps.first())
                .cloned()
                .unwrap_or_else(default_trace_step);
            counterexample(v, &step)
        })
        .collect()
}

fn verify_property_against_reference(ir: &Ir, obligation: &Sexpr) -> Sexpr {
    let ob_id = obligation_id_str(obligation);
    let execute = obligation_field(obligation, "execute")
        .cloned()
        .unwrap_or_else(nil);
    if !is_truthy(&execute) {
        let diag = diag_sexpr(
            "warning",
            "no-execute-spec",
            (0, 0),
            format!("property {} has no execute clause", ob_id),
        );
        return make_verification_result(&ob_id, "skipped", None, vec![], vec![diag]);
    }

    let steps = execute_steps(&execute);
    let trace = execute_trace(ir, &steps);
    // Property basis (plan section A): symbolic iff any executed step's
    // own guard evaluation rested on a permissive default.
    let symbolic = trace.steps.iter().any(|s| s.symbolic);
    if trace.violations.is_empty() {
        make_verification_result_with_basis(
            &ob_id,
            "passed",
            Some(&trace),
            vec![],
            vec![],
            Some(symbolic),
        )
    } else {
        let ces = counterexamples_for_trace(&trace);
        make_verification_result_with_basis(
            &ob_id,
            "failed",
            Some(&trace),
            ces,
            vec![],
            Some(symbolic),
        )
    }
}

/// The scenario obligation's `when` entries' action lists only (`given`/
/// `then` contribute nothing): a `when` entry's action is its second
/// element when that element is itself a list.
fn scenario_trace_steps(steps: &[Sexpr]) -> Vec<Sexpr> {
    let mut out = Vec::new();
    for step in steps {
        let items = match step.as_list() {
            Some(items) => items,
            None => continue,
        };
        if items.first().and_then(|s| s.as_sym()) != Some("when") {
            continue;
        }
        if let Some(action) = items.get(1) {
            if action.as_list().is_some() {
                out.push(action.clone());
            }
        }
    }
    out
}

fn verify_scenario_against_reference(ir: &Ir, obligation: &Sexpr) -> Sexpr {
    let ob_id = obligation_id_str(obligation);
    let steps_field = obligation_field(obligation, "steps")
        .cloned()
        .unwrap_or_else(nil);
    let given_when_then = steps_field.as_list().unwrap_or(&[]);
    let trace_steps = scenario_trace_steps(given_when_then);

    if trace_steps.is_empty() {
        let diag = diag_sexpr(
            "warning",
            "no-trace-steps",
            (0, 0),
            format!("scenario {} produced no executable trace steps", ob_id),
        );
        return make_verification_result(&ob_id, "skipped", None, vec![], vec![diag]);
    }

    let trace = execute_trace(ir, &trace_steps);
    // Scenario basis (plan section A): symbolic iff any executed step's
    // own guard evaluation rested on a permissive default.
    let symbolic = trace.steps.iter().any(|s| s.symbolic);
    if trace.violations.is_empty() {
        make_verification_result_with_basis(
            &ob_id,
            "passed",
            Some(&trace),
            vec![],
            vec![],
            Some(symbolic),
        )
    } else {
        let ces = counterexamples_for_trace(&trace);
        make_verification_result_with_basis(
            &ob_id,
            "failed",
            Some(&trace),
            ces,
            vec![],
            Some(symbolic),
        )
    }
}
/// Verifies an invariant obligation using the TRI-STATE evaluator
/// (`docs/rust-port-plan-phase7.md` section A): its `:always` predicate
/// against `make_initial_state`, then (if the initial state `Holds`)
/// against the post-state of applying EVERY extracted transition once
/// (actor/input `None`) to that same initial state, in order -- the
/// first check point whose verdict is not `Holds` decides the result:
/// `Fails` yields `failed` (basis `checked`, a grounded violation);
/// `Unknown` yields the new `indeterminate` status (basis `symbolic`)
/// rather than the phase-6 behavior of laundering a permissive default
/// into a `passed`/`failed` verdict (phase-6 gate, findings 1 and 4).
/// `Holds` at every check point yields `passed` (basis `checked`).
fn verify_invariant_obligation(ir: &Ir, obligation: &Sexpr) -> Sexpr {
    let ob_id = obligation_id_str(obligation);
    let predicate = obligation_field(obligation, "predicate")
        .cloned()
        .unwrap_or_else(nil);
    let state = make_initial_state(ir);

    match eval_predicate3(&predicate, &state, None, None) {
        Verdict::Fails => {
            let ce = Sexpr::List(vec![
                Sexpr::sym("normalized-counterexample"),
                Sexpr::pair("obligation-id", Sexpr::Str(ob_id.clone())),
                Sexpr::pair("divergence-type", Sexpr::sym("invariant-violation")),
                Sexpr::pair("predicate", predicate.clone()),
                Sexpr::pair("state", state_to_sexpr(&state)),
            ]);
            return make_verification_result_with_basis(
                &ob_id,
                "failed",
                None,
                vec![ce],
                basis_diagnostics(false, &predicate, &ob_id),
                Some(false),
            );
        }
        Verdict::Unknown => {
            return make_verification_result_with_basis(
                &ob_id,
                "indeterminate",
                None,
                vec![],
                basis_diagnostics(true, &predicate, &ob_id),
                Some(true),
            );
        }
        Verdict::Holds => {}
    }

    let transitions = extract_transitions(ir);
    for t in &transitions {
        let step = apply_transition(t, &state, None, None);
        match eval_predicate3(&predicate, &step.post_state, None, None) {
            Verdict::Fails => {
                let ce = Sexpr::List(vec![
                    Sexpr::sym("normalized-counterexample"),
                    Sexpr::pair("obligation-id", Sexpr::Str(ob_id.clone())),
                    Sexpr::pair(
                        "divergence-type",
                        Sexpr::sym("invariant-violation-post-transition"),
                    ),
                    Sexpr::pair("predicate", predicate.clone()),
                    Sexpr::pair("state", state_to_sexpr(&step.post_state)),
                    Sexpr::pair("transition", Sexpr::Str(t.id.clone())),
                ]);
                return make_verification_result_with_basis(
                    &ob_id,
                    "failed",
                    None,
                    vec![ce],
                    basis_diagnostics(false, &predicate, &ob_id),
                    Some(false),
                );
            }
            Verdict::Unknown => {
                return make_verification_result_with_basis(
                    &ob_id,
                    "indeterminate",
                    None,
                    vec![],
                    basis_diagnostics(true, &predicate, &ob_id),
                    Some(true),
                );
            }
            Verdict::Holds => continue,
        }
    }
    make_verification_result_with_basis(
        &ob_id,
        "passed",
        None,
        vec![],
        basis_diagnostics(false, &predicate, &ob_id),
        Some(false),
    )
}

/// The info diagnostic marking a symbolically-based verdict, naming the
/// predicate whose evaluation hit a permissive default.
fn basis_diagnostics(symbolic: bool, predicate: &Sexpr, ob_id: &str) -> Vec<Sexpr> {
    if !symbolic {
        return vec![];
    }
    // "symbolically-undecided", not "-satisfied" (phase-7 gate, finding
    // 11): nothing was satisfied -- the verdict rests on a form the
    // closed evaluator could not decide.
    vec![diag_sexpr(
        "info",
        "I601",
        (0, 0),
        format!(
            "symbolically-undecided: the verdict for {} rests on an unevaluated predicate form ({})",
            ob_id,
            predicate.print()
        ),
    )]
}

/// Top-level obligation dispatch. `property`/`scenario`/`invariant` run
/// the reference checks above; everything else (`concurrency`/`fault`/
/// `coverage`/`model`/`constraint`) is `skipped` with an info
/// `deferred-verification` diagnostic naming the kind -- these all
/// require runtime execution this compiler does not perform (plan
/// section B).
pub fn verify_obligation(ir: &Ir, obligation: &Sexpr) -> Sexpr {
    let kind = obligation_kind(obligation);
    match kind {
        "property" => verify_property_against_reference(ir, obligation),
        "scenario" => verify_scenario_against_reference(ir, obligation),
        "invariant" => verify_invariant_obligation(ir, obligation),
        _ => {
            let ob_id = obligation_id_str(obligation);
            let diag = diag_sexpr(
                "info",
                "deferred-verification",
                (0, 0),
                format!(
                    "verification of {} obligation {} requires runtime execution",
                    if kind.is_empty() {
                        "unknown-kind"
                    } else {
                        kind
                    },
                    ob_id
                ),
            );
            make_verification_result(&ob_id, "skipped", None, vec![], vec![diag])
        }
    }
}

// -----------------------------------------------------------------------
// Trace equivalence.
// -----------------------------------------------------------------------

fn length_divergence(kind: &str, count: usize) -> Sexpr {
    Sexpr::List(vec![
        Sexpr::sym("divergence"),
        Sexpr::pair("type", Sexpr::sym(kind)),
        Sexpr::pair("count", Sexpr::Int(count as i64)),
    ])
}

/// One `divergence` between a reference and implementation step, or
/// `None` when they match: outcome checked first (`outcome-mismatch`),
/// then post-state (`state-mismatch`). Both divergence kinds embed the
/// REFERENCE step (as `trace_step_to_sexpr`) under key `step`, mirroring
/// the Lamedh reference's `(list 'step reference-step)`.
fn compare_trace_step(reference: &TraceStep, implementation: &TraceStep) -> Option<Sexpr> {
    if reference.outcome != implementation.outcome {
        return Some(Sexpr::List(vec![
            Sexpr::sym("divergence"),
            Sexpr::pair("type", Sexpr::sym("outcome-mismatch")),
            Sexpr::pair("reference", reference.outcome.clone()),
            Sexpr::pair("implementation", implementation.outcome.clone()),
            Sexpr::pair("step", transition::trace_step_to_sexpr(reference)),
        ]));
    }
    if reference.post_state != implementation.post_state {
        return Some(Sexpr::List(vec![
            Sexpr::sym("divergence"),
            Sexpr::pair("type", Sexpr::sym("state-mismatch")),
            Sexpr::pair("reference-state", state_to_sexpr(&reference.post_state)),
            Sexpr::pair(
                "implementation-state",
                state_to_sexpr(&implementation.post_state),
            ),
            Sexpr::pair("step", transition::trace_step_to_sexpr(reference)),
        ]));
    }
    None
}

/// Compares two step sequences pairwise, in lockstep, until either runs
/// out: a differing pair contributes at most one divergence (see
/// `compare_trace_step`); once one side runs out first, ONE final
/// `extra-implementation-steps`/`missing-implementation-steps`
/// divergence (carrying the remaining count) is appended and comparison
/// stops -- mirroring `src/verify.lisp`'s `gymnast-compare-traces`
/// exactly. An iterative index walk (not recursion): every iteration
/// either advances the index or breaks, so this is bounded by
/// `min(reference_steps.len(), impl_steps.len()) + 1` regardless of
/// input.
pub fn compare_traces(reference_steps: &[TraceStep], impl_steps: &[TraceStep]) -> Vec<Sexpr> {
    let mut divergences = Vec::new();
    let mut i = 0usize;
    loop {
        match (reference_steps.get(i), impl_steps.get(i)) {
            (None, None) => break,
            (None, Some(_)) => {
                divergences.push(length_divergence(
                    "extra-implementation-steps",
                    impl_steps.len() - i,
                ));
                break;
            }
            (Some(_), None) => {
                divergences.push(length_divergence(
                    "missing-implementation-steps",
                    reference_steps.len() - i,
                ));
                break;
            }
            (Some(r), Some(im)) => {
                if let Some(d) = compare_trace_step(r, im) {
                    divergences.push(d);
                }
                i += 1;
            }
        }
    }
    divergences
}

/// `(trace-equivalence-result (obligation-id ...) (equivalent t|nil)
/// (divergences (...)) (reference-violations (...))
/// (implementation-violations (...)))`. `ir` is accepted (and unused, as
/// in the Lamedh reference) to keep the signature the plan/oracle test
/// expect.
pub fn trace_equivalence_result(
    _ir: &Ir,
    reference_trace: &Trace,
    impl_trace: &Trace,
    obligation_id: &str,
) -> Sexpr {
    let divergences = compare_traces(&reference_trace.steps, &impl_trace.steps);
    let equivalent = divergences.is_empty();
    Sexpr::List(vec![
        Sexpr::sym("trace-equivalence-result"),
        Sexpr::pair("obligation-id", Sexpr::Str(obligation_id.to_string())),
        Sexpr::pair(
            "equivalent",
            if equivalent { Sexpr::sym("t") } else { nil() },
        ),
        Sexpr::pair("divergences", Sexpr::List(divergences)),
        Sexpr::pair(
            "reference-violations",
            Sexpr::List(reference_trace.violations.clone()),
        ),
        Sexpr::pair(
            "implementation-violations",
            Sexpr::List(impl_trace.violations.clone()),
        ),
    ])
}

// -----------------------------------------------------------------------
// Normalized counterexamples.
// -----------------------------------------------------------------------

/// Reads one field out of a `trace_step_to_sexpr`-shaped value: that
/// shape nests (`(trace-step ((k v) ...))`, `IrNode::to_sexpr`'s
/// convention), so this looks the field up under `items[1]` rather than
/// via a direct `assoc` on the outer list.
fn trace_step_field(step_sexpr: &Sexpr, key: &str) -> Option<Sexpr> {
    step_sexpr
        .as_list()
        .and_then(|items| items.get(1))
        .and_then(|inner| inner.assoc(key))
        .cloned()
}

/// Normalizes one `divergence` into a `normalized-counterexample`:
/// `obligation-id`, `divergence-type`, and (from the divergence's
/// embedded reference `step`, when present) `operation`/`actor`/`input`/
/// `pre-state`, plus `expected`/`actual` read off the divergence's OWN
/// `reference`/`implementation` keys.
///
/// REFERENCE QUIRK, ported verbatim (plan: "port structurally 1:1"): a
/// `state-mismatch` divergence stores its values under
/// `reference-state`/`implementation-state` instead of
/// `reference`/`implementation`, so `expected`/`actual` come back nil
/// for it even though the real states are present under the other keys.
/// A length-mismatch divergence (`extra-`/`missing-implementation-
/// steps`) carries no `step` at all, so `operation`/`actor`/`input`/
/// `pre-state` are all nil for it too.
pub fn normalize_counterexample(divergence: &Sexpr, obligation_id: &str) -> Sexpr {
    let div_type = divergence.assoc("type").cloned().unwrap_or_else(nil);
    let step = divergence.assoc("step");

    let operation = step
        .and_then(|s| trace_step_field(s, "transition-id"))
        .unwrap_or_else(nil);
    let actor = step
        .and_then(|s| trace_step_field(s, "actor"))
        .unwrap_or_else(nil);
    let input = step
        .and_then(|s| trace_step_field(s, "input"))
        .unwrap_or_else(nil);
    let pre_state = step
        .and_then(|s| trace_step_field(s, "pre-state"))
        .unwrap_or_else(nil);
    let expected = divergence.assoc("reference").cloned().unwrap_or_else(nil);
    let actual = divergence
        .assoc("implementation")
        .cloned()
        .unwrap_or_else(nil);

    Sexpr::List(vec![
        Sexpr::sym("normalized-counterexample"),
        Sexpr::pair("obligation-id", Sexpr::Str(obligation_id.to_string())),
        Sexpr::pair("divergence-type", div_type),
        Sexpr::pair("operation", operation),
        Sexpr::pair("actor", actor),
        Sexpr::pair("input", input),
        Sexpr::pair("pre-state", pre_state),
        Sexpr::pair("expected", expected),
        Sexpr::pair("actual", actual),
    ])
}

/// Maps `normalize_counterexample` over every divergence, in order.
pub fn normalize_counterexamples(divergences: &[Sexpr], obligation_id: &str) -> Vec<Sexpr> {
    divergences
        .iter()
        .map(|d| normalize_counterexample(d, obligation_id))
        .collect()
}

// -----------------------------------------------------------------------
// Coverage analysis.
// -----------------------------------------------------------------------

fn gap_sexpr(kind: &str, count: usize) -> Sexpr {
    Sexpr::List(vec![
        Sexpr::sym("gap"),
        Sexpr::sym(kind),
        Sexpr::Int(count as i64),
    ])
}

/// Coverage analysis against `ir`'s transitions/behaviors/invariants and
/// the given `obligations`, ported verbatim from
/// `gymnast-coverage-gaps`: `nil` when no `coverage` obligation is
/// present; otherwise `(coverage-analysis (property-obligations N)
/// (scenario-obligations N) (fault-obligations N) (total-obligations N)
/// (transitions-defined N) (invariants-defined N) (gaps (...)))`, where
/// `total-obligations` is the reference's `covered-count` (property +
/// scenario + fault), NOT the bundle's overall obligation total. Each of
/// the four gap kinds fires only when its coverage flag is truthy AND
/// the corresponding "defined" count exceeds the corresponding
/// "obligations" count.
pub fn coverage_gaps(ir: &Ir, obligations: &[Sexpr]) -> Sexpr {
    let coverage_ob = match obligations
        .iter()
        .find(|o| obligation_kind(o) == "coverage")
    {
        Some(o) => o,
        None => return nil(),
    };

    let want_transitions = obligation_field(coverage_ob, "every_transition")
        .map(is_truthy)
        .unwrap_or(false);
    let want_ops = obligation_field(coverage_ob, "every_operation")
        .map(is_truthy)
        .unwrap_or(false);
    let want_errors = obligation_field(coverage_ob, "every_error")
        .map(is_truthy)
        .unwrap_or(false);
    let want_invariants = obligation_field(coverage_ob, "every_invariant")
        .map(is_truthy)
        .unwrap_or(false);

    let property_count = obligations
        .iter()
        .filter(|o| obligation_kind(o) == "property")
        .count();
    let scenario_count = obligations
        .iter()
        .filter(|o| obligation_kind(o) == "scenario")
        .count();
    let fault_count = obligations
        .iter()
        .filter(|o| obligation_kind(o) == "fault")
        .count();
    let invariant_ob_count = obligations
        .iter()
        .filter(|o| obligation_kind(o) == "invariant")
        .count();

    let covered_count = property_count + scenario_count + fault_count;
    let transition_count = extract_transitions(ir).len();
    let invariant_count = ir.nodes_of_kind("invariant").len();
    let behavior_count = ir.nodes_of_kind("behavior").len();
    let operation_obs = property_count + scenario_count;
    let error_obs = fault_count;

    let mut gaps = Vec::new();
    if want_transitions && transition_count > covered_count {
        gaps.push(gap_sexpr(
            "uncovered-transitions",
            transition_count - covered_count,
        ));
    }
    if want_ops && behavior_count > operation_obs {
        gaps.push(gap_sexpr(
            "uncovered-operations",
            behavior_count - operation_obs,
        ));
    }
    if want_errors && behavior_count > error_obs {
        gaps.push(gap_sexpr(
            "uncovered-error-paths",
            behavior_count - error_obs,
        ));
    }
    if want_invariants && invariant_count > invariant_ob_count {
        gaps.push(gap_sexpr(
            "uncovered-invariants",
            invariant_count - invariant_ob_count,
        ));
    }

    Sexpr::List(vec![
        Sexpr::sym("coverage-analysis"),
        Sexpr::pair("property-obligations", Sexpr::Int(property_count as i64)),
        Sexpr::pair("scenario-obligations", Sexpr::Int(scenario_count as i64)),
        Sexpr::pair("fault-obligations", Sexpr::Int(fault_count as i64)),
        Sexpr::pair("total-obligations", Sexpr::Int(covered_count as i64)),
        Sexpr::pair("transitions-defined", Sexpr::Int(transition_count as i64)),
        Sexpr::pair("invariants-defined", Sexpr::Int(invariant_count as i64)),
        Sexpr::pair("gaps", Sexpr::List(gaps)),
    ])
}

// -----------------------------------------------------------------------
// The verification bundle.
// -----------------------------------------------------------------------

fn result_status(r: &Sexpr) -> &str {
    r.assoc("status").and_then(|s| s.as_sym()).unwrap_or("")
}

/// `E601 duplicate-obligation-id`: one error diagnostic per SECOND (and
/// later) occurrence of an obligation id already seen earlier in
/// `obligations` (plan section D) -- cache keys and assembly evidence
/// depend on id uniqueness, so a collision must be visible in the
/// bundle's own diagnostics rather than silently overwriting a lookup
/// downstream. A linear scan against a growing `Vec` (not a `HashSet`):
/// obligation counts are small and this keeps the diagnostic order
/// (and therefore the whole bundle's serialization) dependent only on
/// `obligations`' own deterministic order, never on hash iteration.
fn duplicate_obligation_diagnostics(obligations: &[Sexpr]) -> Vec<Sexpr> {
    let mut seen: Vec<String> = Vec::new();
    let mut diags = Vec::new();
    for o in obligations {
        let id = obligation_id_str(o);
        if seen.iter().any(|s| *s == id) {
            diags.push(diag_sexpr(
                "error",
                "E601",
                (0, 0),
                format!("duplicate-obligation-id: {}", id),
            ));
        } else {
            seen.push(id);
        }
    }
    diags
}

/// The bundle's typed summary counts (plan section D), for phase-8
/// assembly to read without re-parsing the raw `Sexpr`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerificationSummary {
    pub total: i64,
    pub passed: i64,
    pub failed: i64,
    pub skipped: i64,
    pub indeterminate: i64,
}

/// Reads a `verification-bundle`'s `summary` field into a
/// `VerificationSummary`. Total: `None` on any shape mismatch (not a
/// `verification-bundle`, no `summary` field, or a missing/non-`Int`
/// count) rather than panicking.
pub fn bundle_summary(bundle: &Sexpr) -> Option<VerificationSummary> {
    let inner = bundle.as_list()?.get(1)?;
    let summary = inner.assoc("summary")?;
    Some(VerificationSummary {
        total: summary.assoc("total")?.as_int()?,
        passed: summary.assoc("passed")?.as_int()?,
        failed: summary.assoc("failed")?.as_int()?,
        skipped: summary.assoc("skipped")?.as_int()?,
        indeterminate: summary.assoc("indeterminate")?.as_int()?,
    })
}

/// Builds the bundle WITHOUT its `fingerprint` field -- the form the
/// fingerprint itself is computed over, so the two can never drift
/// (`Ir`/`Plan`'s own discipline, now extended to the verification
/// bundle per plan section D).
fn compile_verification_without_fingerprint(ir: &Ir) -> Sexpr {
    let obligations = lower_all_obligations(ir);
    let results: Vec<Sexpr> = obligations
        .iter()
        .map(|o| verify_obligation(ir, o))
        .collect();

    let passed = results
        .iter()
        .filter(|r| result_status(r) == "passed")
        .count();
    let failed = results
        .iter()
        .filter(|r| result_status(r) == "failed")
        .count();
    let skipped = results
        .iter()
        .filter(|r| result_status(r) == "skipped")
        .count();
    let indeterminate = results
        .iter()
        .filter(|r| result_status(r) == "indeterminate")
        .count();
    let total = obligations.len();

    let acc_nodes = ir.nodes_of_kind("acceptance");
    let env_diags = match acc_nodes.first() {
        Some(node) => {
            let env = extract_execution_env(node);
            env_diagnostics(&env, &node.id)
        }
        None => vec![],
    };

    let coverage = coverage_gaps(ir, &obligations);
    let transition_diags = transition::check_transition_refs(ir);
    let bundle_diags = duplicate_obligation_diagnostics(&obligations);

    Sexpr::List(vec![
        Sexpr::sym("verification-bundle"),
        Sexpr::List(vec![
            Sexpr::pair("schema", Sexpr::Str(VERIFY_SCHEMA.to_string())),
            Sexpr::pair("obligations", Sexpr::List(obligations)),
            Sexpr::pair("results", Sexpr::List(results)),
            Sexpr::pair(
                "summary",
                Sexpr::List(vec![
                    Sexpr::pair("total", Sexpr::Int(total as i64)),
                    Sexpr::pair("passed", Sexpr::Int(passed as i64)),
                    Sexpr::pair("failed", Sexpr::Int(failed as i64)),
                    Sexpr::pair("skipped", Sexpr::Int(skipped as i64)),
                    Sexpr::pair("indeterminate", Sexpr::Int(indeterminate as i64)),
                ]),
            ),
            Sexpr::pair("coverage", coverage),
            Sexpr::pair("environment-diagnostics", Sexpr::List(env_diags)),
            // W406 unresolved-state-ref warnings (plan section C): every
            // `reads`/`writes` entry that names no `state` node, so the
            // calculus's total-but-silent write-to-undeclared behavior
            // becomes visible in the artifact.
            Sexpr::pair("transition-diagnostics", Sexpr::List(transition_diags)),
            // The bundle's own diagnostics (plan section D: E601
            // duplicate-obligation-id), distinct from `source-diagnostics`
            // below (the IR's diagnostics, unrelated to this bundle).
            Sexpr::pair("diagnostics", Sexpr::List(bundle_diags)),
            // The bundle is self-describing (phase-6 gate, finding 3): a
            // bundle built over an IR carrying error diagnostics says so
            // in the artifact itself, not only in a process exit code
            // that a file consumer never sees.
            Sexpr::pair("source-diagnostics", Sexpr::List(ir.diagnostics.clone())),
        ]),
    ])
}

/// Every ERROR-severity diagnostic message anywhere in a verification
/// bundle: the bundle-level `diagnostics` (where E601 lands),
/// `transition-diagnostics`, `environment-diagnostics`,
/// `source-diagnostics`, and each result's own `diagnostics`. The CLI
/// uses this to fail `verify` visibly on bundle-level errors (phase-7
/// gate, finding 8). Total over any Sexpr; a malformed bundle yields
/// an empty list rather than a panic.
pub fn bundle_error_diagnostics(bundle: &Sexpr) -> Vec<String> {
    fn errors_in(list: Option<&Sexpr>, out: &mut Vec<String>) {
        let items = match list.and_then(|l| l.as_list()) {
            Some(items) => items,
            None => return,
        };
        for d in items {
            let severity = d.assoc("severity").and_then(|s| s.as_sym());
            if severity == Some("error") {
                let msg = d
                    .assoc("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("(no message)");
                let code = d.assoc("code").and_then(|c| c.as_str()).unwrap_or("");
                out.push(format!("{} {}", code, msg).trim().to_string());
            }
        }
    }
    // The bundle nests its field pairs one level down:
    // `(verification-bundle ((k v) ...))` — unwrap before assoc.
    let inner = match bundle.as_list() {
        Some(items) if items.len() == 2 && items[0].as_sym() == Some("verification-bundle") => {
            &items[1]
        }
        _ => bundle,
    };
    let mut out = Vec::new();
    for key in [
        "diagnostics",
        "transition-diagnostics",
        "environment-diagnostics",
        "source-diagnostics",
    ] {
        errors_in(inner.assoc(key), &mut out);
    }
    if let Some(results) = inner.assoc("results").and_then(|r| r.as_list()) {
        for r in results {
            errors_in(r.assoc("diagnostics"), &mut out);
        }
    }
    out
}

/// Compiles the full verification bundle: lowers every obligation, runs
/// `verify_obligation` over each, tallies pass/fail/skip/indeterminate,
/// and folds in the first acceptance node's environment diagnostics,
/// transition ref-check warnings, duplicate-obligation-id diagnostics,
/// and coverage analysis. Pure and deterministic. Builds on the
/// fingerprint-free form so the two can never drift, exactly the
/// `Ir`/`Plan` discipline (plan section D; the delta doc's "no
/// fingerprint field" note is superseded by this contract).
pub fn compile_verification(ir: &Ir) -> Sexpr {
    let base = compile_verification_without_fingerprint(ir);
    let fp = fingerprint::fingerprint(&base);
    match base {
        Sexpr::List(mut outer) => {
            if let Some(Sexpr::List(items)) = outer.last_mut() {
                items.push(Sexpr::pair("fingerprint", Sexpr::Str(fp)));
            }
            Sexpr::List(outer)
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn behavior(id: &str, on: Sexpr, writes: Vec<&str>) -> IrNode {
        IrNode::new(
            id.to_string(),
            "behavior",
            "x".to_string(),
            vec![
                (":on".to_string(), on),
                (
                    ":writes".to_string(),
                    Sexpr::List(writes.into_iter().map(Sexpr::sym).collect()),
                ),
            ],
            vec![],
        )
    }

    #[test]
    fn extract_execution_env_is_total_with_no_clauses() {
        let node = IrNode::new(
            "m/acceptance/x".to_string(),
            "acceptance",
            "x".to_string(),
            vec![],
            vec![],
        );
        let env = extract_execution_env(&node);
        assert_eq!(env.assoc("clock"), Some(&Sexpr::sym("system")));
        assert_eq!(env.assoc("locale"), Some(&Sexpr::Str("en-US".to_string())));
        assert!(!env_deterministic(&env));
        assert_eq!(env_diagnostics(&env, "m/acceptance/x").len(), 3);
    }

    #[test]
    fn lower_all_obligations_is_total_over_an_empty_ir() {
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
        assert!(lower_all_obligations(&ir).is_empty());
        assert_eq!(
            compile_verification(&ir)
                .print()
                .contains("verification-bundle"),
            true
        );
    }

    #[test]
    fn coverage_gaps_nil_with_no_coverage_obligation() {
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
        assert_eq!(coverage_gaps(&ir, &[]), nil());
    }

    #[test]
    fn compare_traces_bounded_and_total_over_mismatched_lengths() {
        let step = TraceStep {
            transition_id: "t".to_string(),
            actor: None,
            input: None,
            pre_state: vec![],
            post_state: vec![],
            result: None,
            outcome: Sexpr::List(vec![Sexpr::sym("succeeded")]),
            symbolic: false,
        };
        let divs = compare_traces(&[step.clone(), step.clone()], &[step]);
        assert_eq!(divs.len(), 1);
        assert_eq!(
            divs[0].assoc("type").and_then(|s| s.as_sym()),
            Some("missing-implementation-steps")
        );
    }

    #[test]
    fn verify_obligation_is_total_on_a_hand_built_unknown_kind() {
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
        let ob = Sexpr::List(vec![
            Sexpr::sym("verification-obligation"),
            Sexpr::pair("id", Sexpr::Str("m/x/y".to_string())),
            Sexpr::pair("kind", Sexpr::sym("something-unrecognized")),
        ]);
        let result = verify_obligation(&ir, &ob);
        assert_eq!(result_status(&result), "skipped");
    }

    #[test]
    fn make_behavior_node_helper_is_reachable() {
        // Exercises the small local `behavior` test fixture builder so it
        // is not flagged dead code by future edits to this module.
        let node = behavior("m/behavior/x", Sexpr::sym("svc/op"), vec!["out"]);
        assert_eq!(node.kind, "behavior");
    }
}
