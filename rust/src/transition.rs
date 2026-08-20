//! Executable transition calculus (`docs/rust-port-plan-phase6.md`,
//! section A). Ports `src/transition.lisp`'s reference state machine
//! against the Rust IR contract (`docs/ir-contract-deltas.md`):
//! transition extraction from `behavior` nodes, a bounded/total
//! predicate-and-expression evaluator, a reference `apply_transition`,
//! bounded trace execution, and invariant checking with
//! counterexamples.
//!
//! Every function here is TOTAL over arbitrary `Sexpr`/`IrNode` input:
//! no panics, no unbounded recursion (trace execution is an iterative
//! loop bounded by `TRACE_BOUND`; the evaluator recurses only on the
//! (already depth-bounded, since it was itself parsed under
//! `sexpr::MAX_PARSE_DEPTH`) predicate/expression tree, never on
//! attacker-controlled repetition).

use crate::diag::diag_sexpr;
use crate::ir::{Ir, IrNode};
use crate::sexpr::Sexpr;

/// The reference transition extracted from one `behavior` IR node.
#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    pub id: String,            // the behavior node id
    pub operation: String,     // "todo_service/create_task"
    pub actor: Option<String>, // first :on binder
    pub input: Option<String>, // second :on binder
    pub reads: Vec<String>,
    pub writes: Vec<String>,
    pub atomic: Option<Sexpr>,
    pub idempotency: Option<Sexpr>,
    pub preconditions: Vec<Sexpr>,  // requires clause bodies (the pred)
    pub postconditions: Vec<Sexpr>, // ensures clause bodies
    pub result: Option<Sexpr>,      // returns clause body
    pub failures: Vec<Sexpr>,       // whole fails clauses (tail after head)
    pub emissions: Vec<Sexpr>,      // whole emits clause tails
}

// -----------------------------------------------------------------------
// Transition extraction.
// -----------------------------------------------------------------------

/// Parses a `:on` field value into `(operation, actor, input)`.
///
/// Rust-IR adaptation (plan section A): `:on` is
/// `(iface/op binder1 binder2 ...)` with the slash already joined into
/// one symbol -- `operation` is the first element's text, `actor` the
/// second element (if any), `input` the third (if any). A bare-symbol
/// `:on` yields operation only. Total over any shape: a missing field,
/// an empty list, or a non-symbol head all degrade gracefully rather
/// than panicking.
fn parse_on_spec(on_spec: Option<&Sexpr>) -> (String, Option<String>, Option<String>) {
    match on_spec {
        None => (String::new(), None, None),
        Some(Sexpr::List(items)) => {
            let op = items
                .first()
                .map(|s| {
                    s.as_sym()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| s.print())
                })
                .unwrap_or_default();
            let actor = items.get(1).and_then(|s| s.as_sym()).map(|s| s.to_string());
            let input = items.get(2).and_then(|s| s.as_sym()).map(|s| s.to_string());
            (op, actor, input)
        }
        // Bare-symbol (or other atom) :on -> operation only.
        Some(other) => (
            other
                .as_sym()
                .map(|s| s.to_string())
                .unwrap_or_else(|| other.print()),
            None,
            None,
        ),
    }
}

/// `:reads`/`:writes` are always lists of symbols in the Rust IR
/// (phase-2 plural rule); a bare symbol is accepted defensively as a
/// 1-list. Non-symbol list entries are dropped rather than guessed at
/// (total, but silent on genuinely malformed input -- the IR's own
/// elaborator is the place that would diagnose that, not this reader).
fn extract_string_list(v: Option<&Sexpr>) -> Vec<String> {
    match v {
        None => vec![],
        Some(Sexpr::List(items)) => items
            .iter()
            .filter_map(|i| i.as_sym().map(|s| s.to_string()))
            .collect(),
        Some(Sexpr::Sym(s)) => vec![s.clone()],
        Some(_) => vec![],
    }
}

/// The tail of a clause list (every element after the head symbol).
/// `[]` for a non-list or empty-list clause -- defensive only; every
/// real clause Sexpr is a non-empty list headed by its clause keyword.
///
/// `pub(crate)`: shared with `verify.rs`, which lowers acceptance-clause
/// tails (`property`/`scenario`/`concurrency`/`fault`/`coverage`/`model`)
/// and the `execution` clause using the same clause-tail convention.
pub(crate) fn clause_tail(clause: &Sexpr) -> &[Sexpr] {
    match clause {
        Sexpr::List(items) if !items.is_empty() => &items[1..],
        _ => &[],
    }
}

pub(crate) fn clause_head_is(clause: &Sexpr, head: &str) -> bool {
    clause
        .as_list()
        .and_then(|items| items.first())
        .and_then(|s| s.as_sym())
        == Some(head)
}

/// `requires`/`ensures` clause extraction: `(requires <pred>)` ->
/// the pred, unwrapped. A clause whose head matches but whose tail is
/// not exactly one element (wrong arity: reserved shape, but not one
/// the elaborator diagnoses at this layer) is OFF-SHAPE -- kept WHOLE
/// (the entire clause, head included) in the same field rather than
/// dropped, per the plan's "visibility over silence" rule that has
/// applied to every off-shape clause since phase 3.
fn single_pred_field(clauses: &[Sexpr], head: &str) -> Vec<Sexpr> {
    let mut out = Vec::new();
    for clause in clauses {
        if !clause_head_is(clause, head) {
            continue;
        }
        let tail = clause_tail(clause);
        if tail.len() == 1 {
            out.push(tail[0].clone());
        } else {
            out.push(clause.clone());
        }
    }
    out
}

/// `returns` clause extraction: only the FIRST `returns` clause is
/// consulted (mirrors the Lamedh reference's `(car returns)`);
/// `(returns <expr>)` -> the expr, unwrapped; off-shape (tail length
/// != 1) keeps the whole clause, same rule as `single_pred_field`.
fn extract_result(clauses: &[Sexpr]) -> Option<Sexpr> {
    for clause in clauses {
        if !clause_head_is(clause, "returns") {
            continue;
        }
        let tail = clause_tail(clause);
        return Some(if tail.len() == 1 {
            tail[0].clone()
        } else {
            clause.clone()
        });
    }
    None
}

/// `fails`/`emits` clause extraction: the WHOLE tail after the head is
/// kept, for every matching clause, regardless of its arity -- this is
/// already the plan's "kept whole" rule for these two clause kinds, so
/// there is no separate off-shape case to special-case.
fn tail_field(clauses: &[Sexpr], head: &str) -> Vec<Sexpr> {
    clauses
        .iter()
        .filter(|c| clause_head_is(c, head))
        .map(|c| Sexpr::List(clause_tail(c).to_vec()))
        .collect()
}

/// Extracts one `Transition` from a `behavior` IR node. Total over any
/// `IrNode`, not just well-formed `behavior` nodes -- a node missing
/// every expected field degrades to a mostly-empty `Transition` rather
/// than panicking.
pub fn extract_transition(node: &IrNode) -> Transition {
    let (operation, actor, input) = parse_on_spec(node.field(":on"));
    let reads = extract_string_list(node.field(":reads"));
    let writes = extract_string_list(node.field(":writes"));
    let atomic = node.field(":atomic").cloned();
    let idempotency = node.field(":idempotency").cloned();

    let preconditions = single_pred_field(&node.clauses, "requires");
    let postconditions = single_pred_field(&node.clauses, "ensures");
    let result = extract_result(&node.clauses);
    let failures = tail_field(&node.clauses, "fails");
    let emissions = tail_field(&node.clauses, "emits");

    Transition {
        id: node.id.clone(),
        operation,
        actor,
        input,
        reads,
        writes,
        atomic,
        idempotency,
        preconditions,
        postconditions,
        result,
        failures,
        emissions,
    }
}

/// All `behavior`-kind nodes, in `all_nodes` order (which is already
/// id-sorted within the `transitions` partition where `behavior` nodes
/// live -- see `Ir::new`), each lowered to a `Transition`.
pub fn extract_transitions(ir: &Ir) -> Vec<Transition> {
    ir.nodes_of_kind("behavior")
        .into_iter()
        .map(extract_transition)
        .collect()
}

// -----------------------------------------------------------------------
// Reference state machine.
// -----------------------------------------------------------------------

/// An insertion-ordered association list: `(state-node-name, value)`.
pub type State = Vec<(String, Sexpr)>;

/// One entry per `state` IR node (`all_nodes` order): `:initial empty`
/// (or an absent `:initial` field) maps to `nil` (the empty list); any
/// other initial value is carried verbatim.
pub fn make_initial_state(ir: &Ir) -> State {
    ir.nodes_of_kind("state")
        .into_iter()
        .map(|node| {
            let value = match node.field(":initial") {
                Some(Sexpr::Sym(s)) if s == "empty" => Sexpr::List(vec![]),
                Some(v) => v.clone(),
                None => Sexpr::List(vec![]),
            };
            (node.name.clone(), value)
        })
        .collect()
}

fn state_lookup(state: &State, name: &str) -> Option<Sexpr> {
    state
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.clone())
}

/// Renders a `State` as its assoc-list `Sexpr`: `((name value) ...)`.
/// `pub`: shared with `verify.rs`, which embeds state snapshots in
/// invariant counterexamples and normalized-counterexample projections.
pub fn state_to_sexpr(state: &State) -> Sexpr {
    Sexpr::List(
        state
            .iter()
            .map(|(k, v)| Sexpr::List(vec![Sexpr::sym(k), v.clone()]))
            .collect(),
    )
}

/// The `n`th element of a call's argument list (i.e. `items[n]`,
/// `items` being the whole call including its head), defaulting to
/// `nil` when absent -- mirrors Lisp's `cadr`/`caddr`-of-short-list
/// returning `nil` rather than erroring, which the evaluator's
/// permissive-default table depends on (e.g. `(not)` with no argument
/// evaluates its missing operand as `nil`, which is truthy, so `(not)`
/// itself is `false`).
fn nth_arg(items: &[Sexpr], n: usize) -> Sexpr {
    items.get(n).cloned().unwrap_or_else(|| Sexpr::List(vec![]))
}

/// The tri-state verdict a predicate can reach under the phase-7 closed
/// evaluator (`docs/rust-port-plan-phase7.md`, section A): `Holds`/`Fails`
/// for a GROUNDED verdict (every branch that decided it was actually
/// computed), `Unknown` wherever the phase-6 boolean evaluator's
/// permissive defaults used to fire silently. `eval_predicate3` never
/// panics and never recurses beyond the (already depth-bounded) predicate
/// tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Holds,
    Fails,
    Unknown,
}

/// The TOTAL, closed tri-state predicate evaluator:
///
/// | pred | verdict |
/// |---|---|
/// | nil / any atom | `Unknown` |
/// | `(= a b)` | `Holds`/`Fails` by structural equality |
/// | `(not p)` | `Fails`/`Holds` swap; `Unknown` stays `Unknown` |
/// | `(and p...)` | `Fails` if any `Fails`; else `Unknown` if any `Unknown`; else `Holds` (vacuously `Holds` on an empty tail) |
/// | `(or p...)` | `Holds` if any `Holds`; else `Unknown` if any `Unknown`; else `Fails` (vacuously `Fails` on an empty tail) |
/// | `(< a b)` / `(<= a b)` | `Holds`/`Fails` when both sides eval to `Int`; `Unknown` otherwise |
/// | unrecognized head | `Unknown` |
pub fn eval_predicate3(
    pred: &Sexpr,
    state: &State,
    actor: Option<&Sexpr>,
    input: Option<&Sexpr>,
) -> Verdict {
    let items = match pred {
        Sexpr::List(items) if !items.is_empty() => items,
        _ => return Verdict::Unknown, // nil / any atom
    };
    match items[0].as_sym() {
        Some("=") => {
            // Groundedness qualification (phase-7 gate, finding 2 +
            // re-review residual): a bare symbol that resolves through
            // no binding evaluates to itself -- a failed LOOKUP, not
            // evidence. The verdict is grounded only when BOTH sides
            // resolved (literals, special heads, successful state
            // lookups, lists). The legitimate enum-literal case is
            // resolved-vs-literal (`(= status active)` with `status`
            // bound); two floating symbols have no grounded reading --
            // `(= current_status open)` over a state with neither entry
            // must be `Unknown`, never a fabricated `Holds`/`Fails`.
            let (a, a_resolved) = eval_expr_resolved(&nth_arg(items, 1), state, actor, input);
            let (b, b_resolved) = eval_expr_resolved(&nth_arg(items, 2), state, actor, input);
            let a_floating = !a_resolved;
            let b_floating = !b_resolved;
            if (a_floating && b_floating)
                || (a_floating && !matches!(b, Sexpr::Sym(_)))
                || (b_floating && !matches!(a, Sexpr::Sym(_)))
            {
                return Verdict::Unknown;
            }
            if a == b {
                Verdict::Holds
            } else {
                Verdict::Fails
            }
        }
        Some("not") => match eval_predicate3(&nth_arg(items, 1), state, actor, input) {
            Verdict::Holds => Verdict::Fails,
            Verdict::Fails => Verdict::Holds,
            Verdict::Unknown => Verdict::Unknown,
        },
        Some("and") => {
            let mut any_unknown = false;
            for p in &items[1..] {
                match eval_predicate3(p, state, actor, input) {
                    Verdict::Fails => return Verdict::Fails,
                    Verdict::Unknown => any_unknown = true,
                    Verdict::Holds => {}
                }
            }
            if any_unknown {
                Verdict::Unknown
            } else {
                Verdict::Holds
            }
        }
        Some("or") => {
            let mut any_unknown = false;
            for p in &items[1..] {
                match eval_predicate3(p, state, actor, input) {
                    Verdict::Holds => return Verdict::Holds,
                    Verdict::Unknown => any_unknown = true,
                    Verdict::Fails => {}
                }
            }
            if any_unknown {
                Verdict::Unknown
            } else {
                Verdict::Fails
            }
        }
        Some("<") => {
            let a = eval_expr(&nth_arg(items, 1), state, actor, input);
            let b = eval_expr(&nth_arg(items, 2), state, actor, input);
            match (a.as_int(), b.as_int()) {
                (Some(x), Some(y)) => {
                    if x < y {
                        Verdict::Holds
                    } else {
                        Verdict::Fails
                    }
                }
                _ => Verdict::Unknown,
            }
        }
        Some("<=") => {
            let a = eval_expr(&nth_arg(items, 1), state, actor, input);
            let b = eval_expr(&nth_arg(items, 2), state, actor, input);
            match (a.as_int(), b.as_int()) {
                (Some(x), Some(y)) => {
                    if x <= y {
                        Verdict::Holds
                    } else {
                        Verdict::Fails
                    }
                }
                _ => Verdict::Unknown,
            }
        }
        _ => Verdict::Unknown, // unrecognized head (calls, forall, ...)
    }
}

/// The TOTAL, closed BOOLEAN predicate evaluator. Preserves phase-6's
/// behavior EXACTLY, permissive defaults included:
///
/// | pred | result |
/// |---|---|
/// | nil / any atom | `true` |
/// | `(= a b)` | `eval_expr` equality (structural) |
/// | `(not p)` | negation |
/// | `(and p...)` / `(or p...)` | all / any over the tail |
/// | `(< a b)` / `(<= a b)` | integer comparison when BOTH sides eval to `Int`; else `false` (DELTA: the Lamedh reference errors on a non-number; this evaluator is total) |
/// | anything else | `true` (symbolic: unknown predicates hold) |
pub fn eval_predicate(
    pred: &Sexpr,
    state: &State,
    actor: Option<&Sexpr>,
    input: Option<&Sexpr>,
) -> bool {
    eval_predicate_basis(pred, state, actor, input).0
}

/// Like `eval_predicate`, but also reports the verdict's BASIS: `true`
/// in the second slot means every branch of the evaluation was actually
/// computed ("checked"); `false` means at least one permissive default
/// fired — an atom/nil predicate, an unrecognized head, or a `<`/`<=`
/// over non-integers — so the verdict is SYMBOLIC, not evidence
/// (phase-6 gate, findings 1 and 4: a verifier must never let a vacuous
/// verdict masquerade as a checked one).
pub fn eval_predicate_basis(
    pred: &Sexpr,
    state: &State,
    actor: Option<&Sexpr>,
    input: Option<&Sexpr>,
) -> (bool, bool) {
    let mut checked = true;
    let verdict = eval_predicate_inner(pred, state, actor, input, &mut checked);
    (verdict, checked)
}

/// The recursive walk behind `eval_predicate_basis`. `not`/`and`/`or`
/// keep the ORIGINAL phase-6 recursion (threading one mutable `checked`
/// flag that only ever moves true -> false, matching Rust's `.all()`/
/// `.any()` short-circuit exactly) rather than being expressed over
/// `eval_predicate3`: the tri-state `Verdict` for a composite predicate
/// is deliberately ORDER-INDEPENDENT (`docs/rust-port-plan-phase7.md`
/// section A's and/or table), while phase-6's `checked` propagation is
/// ORDER-DEPENDENT (an item after a short-circuiting one is never
/// evaluated, so it can never touch `checked`) — collapsing the two would
/// silently change which predicates come back `checked` on a passing
/// `and`/`or`, breaking the "EXACT phase-6 behavior" contract
/// (`evaluator3_oracle_test.rs`'s oracle_02 corpus pins several such
/// order-dependent cases explicitly). The LEAF operators (`=`, `<`,
/// `<=`, and the two permissive-default cases: a bare atom/nil predicate,
/// and an unrecognized head) have no such order dependency, so they ARE
/// expressed directly over `eval_predicate3`'s `Verdict` here — this is
/// the "reimplemented over `eval_predicate3`" the plan asks for, applied
/// exactly where it is order-independent and therefore safe.
fn eval_predicate_inner(
    pred: &Sexpr,
    state: &State,
    actor: Option<&Sexpr>,
    input: Option<&Sexpr>,
    checked: &mut bool,
) -> bool {
    let items = match pred {
        Sexpr::List(items) if !items.is_empty() => items,
        _ => {
            *checked = false;
            return true; // any atom (Sym/Str/Int) or nil: symbolic default holds
        }
    };
    match items[0].as_sym() {
        Some("not") => !eval_predicate_inner(&nth_arg(items, 1), state, actor, input, checked),
        Some("and") => items[1..]
            .iter()
            .all(|p| eval_predicate_inner(p, state, actor, input, checked)),
        Some("or") => items[1..]
            .iter()
            .any(|p| eval_predicate_inner(p, state, actor, input, checked)),
        Some("=") => match eval_predicate3(pred, state, actor, input) {
            Verdict::Holds => true,
            Verdict::Fails => false,
            // Reachable since the groundedness qualification (phase-7
            // gate finding 2 + residual): a floating-symbol comparison.
            // Phase-6 BOOLEAN parity is structural equality of the
            // evaluated sides (a floating symbol evaluates to itself),
            // while the `checked` flag honestly reads false.
            Verdict::Unknown => {
                *checked = false;
                eval_expr(&nth_arg(items, 1), state, actor, input)
                    == eval_expr(&nth_arg(items, 2), state, actor, input)
            }
        },
        Some("<") | Some("<=") => match eval_predicate3(pred, state, actor, input) {
            Verdict::Holds => true,
            Verdict::Fails => false,
            // A non-Int operand: matches the old total-false delta
            // exactly.
            Verdict::Unknown => {
                *checked = false;
                false
            }
        },
        _ => {
            *checked = false;
            true // unrecognized head (calls, forall, ...): symbolic default holds
        }
    }
}

/// The TOTAL, closed expression evaluator:
///
/// | expr | result |
/// |---|---|
/// | `Int`/`Str` | itself |
/// | symbols `pre`/`post` | the state, printed as its assoc-list `Sexpr` |
/// | `actor`/`input` | the given value or `nil` |
/// | `result` | `Sym("result-placeholder")` |
/// | any other symbol | the state entry with that name, else the symbol itself |
/// | any list | itself, verbatim (never recursively evaluated) |
pub fn eval_expr(
    expr: &Sexpr,
    state: &State,
    actor: Option<&Sexpr>,
    input: Option<&Sexpr>,
) -> Sexpr {
    match expr {
        Sexpr::Int(_) | Sexpr::Str(_) => expr.clone(),
        Sexpr::Sym(s) => match s.as_str() {
            "pre" | "post" => state_to_sexpr(state),
            "actor" => actor.cloned().unwrap_or_else(|| Sexpr::List(vec![])),
            "input" => input.cloned().unwrap_or_else(|| Sexpr::List(vec![])),
            "result" => Sexpr::sym("result-placeholder"),
            _ => state_lookup(state, s).unwrap_or_else(|| expr.clone()),
        },
        Sexpr::List(_) => expr.clone(),
    }
}

/// `eval_expr` plus a resolution flag: `false` iff the expression was a
/// bare symbol that resolved through NO binding (not a special head,
/// not a state entry) and therefore evaluated to itself. Every other
/// case -- literals, special heads, successful state lookups, lists --
/// is `true`. The tri-state `=` uses this to distinguish an enum
/// literal from a failed lookup (phase-7 gate, finding 2); the boolean
/// evaluator's behavior is deliberately unchanged.
pub fn eval_expr_resolved(
    expr: &Sexpr,
    state: &State,
    actor: Option<&Sexpr>,
    input: Option<&Sexpr>,
) -> (Sexpr, bool) {
    if let Sexpr::Sym(s) = expr {
        if !matches!(s.as_str(), "pre" | "post" | "actor" | "input" | "result")
            && state_lookup(state, s).is_none()
        {
            return (expr.clone(), false);
        }
    }
    (eval_expr(expr, state, actor, input), true)
}

// -----------------------------------------------------------------------
// Trace machinery.
// -----------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct TraceStep {
    pub transition_id: String, // "unknown" when no transition matched
    pub actor: Option<Sexpr>,
    pub input: Option<Sexpr>,
    pub pre_state: State,
    pub post_state: State,
    pub result: Option<Sexpr>,
    pub outcome: Sexpr, // (succeeded) | (failed <error>) | (precondition-failed) | (no-matching-transition <op>) | (ambiguous-operation <op>)
    /// `true` iff any precondition or matched failure-clause `:when`
    /// guard evaluated while producing this step rested on a permissive
    /// (unchecked) default (`docs/rust-port-plan-phase7.md` section A).
    /// A step that never applied a transition at all (no-match /
    /// ambiguous-operation) performed no guard evaluation, so it is
    /// vacuously grounded (`false`), never symbolic.
    pub symbolic: bool,
}

/// `pub(crate)`: shared with `verify.rs`'s obligation lowering, which
/// reads the same `:key value` clause-tail convention (`:generate`,
/// `:execute`, `:must`, `:actors`, `:schedule`, `:after`, `:inject`,
/// `:every_operation`, ..., `:clock`, `:randomness`, `:network`,
/// `:locale`, `:timezone`).
pub(crate) fn keyword_value<'a>(items: &'a [Sexpr], key: &str) -> Option<&'a Sexpr> {
    let pos = items.iter().position(|s| s.as_sym() == Some(key))?;
    items.get(pos + 1)
}

/// The first failure clause (in declared order) whose `:when` predicate
/// is PRESENT and holds, plus whether ANY `:when` guard actually
/// evaluated along the way (up to and including the matching one, if
/// any) rested on a permissive default. A failure clause with no
/// `:when` at all never matches, and contributes no evaluation (mirrors
/// the Lamedh reference's `(if when-pred ... nil)`). Boolean semantics
/// are UNCHANGED from phase 6 (`eval_predicate`); only the basis is new.
fn find_matching_failure_with_basis<'a>(
    failures: &'a [Sexpr],
    state: &State,
    actor: Option<&Sexpr>,
    input: Option<&Sexpr>,
) -> (Option<&'a Sexpr>, bool) {
    let mut symbolic = false;
    for f in failures {
        let items = f.as_list().unwrap_or(&[]);
        if let Some(pred) = keyword_value(items, ":when") {
            let (holds, checked) = eval_predicate_basis(pred, state, actor, input);
            symbolic = symbolic || !checked;
            if holds {
                return (Some(f), symbolic);
            }
        }
    }
    (None, symbolic)
}

/// Whether every precondition holds (boolean semantics UNCHANGED from
/// phase 6, short-circuiting at the first failing one exactly like
/// `.all()`), plus whether any precondition actually evaluated along the
/// way rested on a permissive default.
fn preconditions_hold_with_basis(
    preconditions: &[Sexpr],
    state: &State,
    actor: Option<&Sexpr>,
    input: Option<&Sexpr>,
) -> (bool, bool) {
    let mut symbolic = false;
    for p in preconditions {
        let (holds, checked) = eval_predicate_basis(p, state, actor, input);
        symbolic = symbolic || !checked;
        if !holds {
            return (false, symbolic);
        }
    }
    (true, symbolic)
}

/// Applies one `Transition` to `state`, in the reference's exact order:
///
/// 1. the first failure clause whose `:when` holds -> `(failed <error>)`,
///    post = pre (the `:preserves` field is recorded on the clause but
///    state is preserved either way, matching the reference exactly);
/// 2. else, if every precondition holds -> post = pre with each
///    `writes` entry appended with the input value, `(succeeded)`,
///    result = input;
/// 3. else -> `(precondition-failed)`, post = pre.
pub fn apply_transition(
    t: &Transition,
    state: &State,
    actor: Option<&Sexpr>,
    input: Option<&Sexpr>,
) -> TraceStep {
    let (matched_failure, failure_symbolic) =
        find_matching_failure_with_basis(&t.failures, state, actor, input);
    if let Some(failure) = matched_failure {
        let error_name = failure
            .as_list()
            .and_then(|items| items.first())
            .cloned()
            .unwrap_or_else(|| Sexpr::List(vec![]));
        return TraceStep {
            transition_id: t.id.clone(),
            actor: actor.cloned(),
            input: input.cloned(),
            pre_state: state.clone(),
            post_state: state.clone(),
            result: None,
            outcome: Sexpr::List(vec![Sexpr::sym("failed"), error_name]),
            symbolic: failure_symbolic,
        };
    }

    let (preconditions_hold, precondition_symbolic) =
        preconditions_hold_with_basis(&t.preconditions, state, actor, input);
    let symbolic = failure_symbolic || precondition_symbolic;
    if preconditions_hold {
        let input_val = input.cloned().unwrap_or_else(|| Sexpr::List(vec![]));
        let mut post = state.clone();
        for w in &t.writes {
            let existing_items = match state_lookup(&post, w) {
                Some(Sexpr::List(items)) => items,
                // Non-list existing value at a writes entry should not
                // occur given the state contract (entries are always
                // collections); total-but-defensive: treat it as empty
                // rather than panicking or guessing at a merge.
                Some(_) => vec![],
                None => vec![],
            };
            let mut new_items = existing_items;
            new_items.push(input_val.clone());
            let new_value = Sexpr::List(new_items);
            match post.iter_mut().find(|(k, _)| k == w) {
                Some(entry) => entry.1 = new_value,
                None => post.push((w.clone(), new_value)),
            }
        }
        return TraceStep {
            transition_id: t.id.clone(),
            actor: actor.cloned(),
            input: input.cloned(),
            pre_state: state.clone(),
            post_state: post,
            result: Some(input_val),
            outcome: Sexpr::List(vec![Sexpr::sym("succeeded")]),
            symbolic,
        };
    }

    TraceStep {
        transition_id: t.id.clone(),
        actor: actor.cloned(),
        input: input.cloned(),
        pre_state: state.clone(),
        post_state: state.clone(),
        result: None,
        outcome: Sexpr::List(vec![Sexpr::sym("precondition-failed")]),
        symbolic,
    }
}

/// Bound on the number of steps a trace may execute -- both to keep the
/// evaluator total over adversarial (or merely very long) step lists,
/// and to match the Lamedh reference's own bound exactly.
pub const TRACE_BOUND: usize = 1000;

#[derive(Debug, Clone, PartialEq)]
pub struct Trace {
    pub steps: Vec<TraceStep>,
    pub violations: Vec<Sexpr>,
    pub final_state: State,
}

/// Parses one execute/trace step into `(op, actor, input)`.
///
/// The Rust surface's step shape is `(op-name actor input)` with `actor`
/// and `input` as direct siblings of `op-name` (this is what
/// `examples/todo.gym`'s acceptance `:execute` calls actually look
/// like, e.g. `(create_task actor task)` -- see
/// `docs/ir-contract-deltas.md`'s "Calls keep call shape everywhere"
/// row). The plan additionally documents a NESTED form, `(op-name
/// (actor input))`, for a step whose second element is itself a list:
/// then `actor`/`input` are read out of THAT list instead. Both are
/// handled here; the flat form is what the oracle tests and
/// `examples/todo.gym` exercise, the nested form is accepted
/// defensively per the plan's literal wording. Total over any shape,
/// including a non-list step or one with fewer than 3 elements.
fn parse_step(step: &Sexpr) -> (String, Option<Sexpr>, Option<Sexpr>) {
    let items = match step.as_list() {
        Some(items) => items,
        None => return (step.print(), None, None),
    };
    let op = items
        .first()
        .map(|s| {
            s.as_sym()
                .map(|s| s.to_string())
                .unwrap_or_else(|| s.print())
        })
        .unwrap_or_default();
    match items.get(1) {
        Some(Sexpr::List(inner)) => (op, inner.first().cloned(), inner.get(1).cloned()),
        Some(other) => (op, Some(other.clone()), items.get(2).cloned()),
        None => (op, None, None),
    }
}

fn no_matching_transition_violation(op: &str) -> Sexpr {
    Sexpr::List(vec![
        Sexpr::sym("violation"),
        Sexpr::List(vec![
            Sexpr::sym("type"),
            Sexpr::sym("no-matching-transition"),
        ]),
        Sexpr::List(vec![Sexpr::sym("operation"), Sexpr::sym(op)]),
    ])
}

fn ambiguous_operation_violation(op: &str, candidates: &[&str], transition_ids: &[&str]) -> Sexpr {
    Sexpr::List(vec![
        Sexpr::sym("violation"),
        Sexpr::List(vec![Sexpr::sym("type"), Sexpr::sym("ambiguous-operation")]),
        Sexpr::List(vec![Sexpr::sym("operation"), Sexpr::sym(op)]),
        Sexpr::List(vec![
            Sexpr::sym("candidates"),
            Sexpr::List(candidates.iter().map(|c| Sexpr::sym(c)).collect()),
        ]),
        // Two behaviors may declare the SAME operation, making the
        // `candidates` ops indistinguishable; the transition ids are
        // the actionable identifiers (phase-7 gate, finding 10).
        // Additive field: `candidates` keeps the plan-section-B shape.
        Sexpr::List(vec![
            Sexpr::sym("candidate-transitions"),
            Sexpr::List(
                transition_ids
                    .iter()
                    .map(|c| Sexpr::Str(c.to_string()))
                    .collect(),
            ),
        ]),
    ])
}

/// Appends `(step-index N)` to a violation form, tying it to the trace
/// step that produced it, so counterexamples pair each violation with
/// the CORRECT step instead of the trace's first (phase-7 gate, finding
/// 7; documented delta from the reference's `(car steps)` pairing).
fn with_step_index(violation: Sexpr, index: usize) -> Sexpr {
    match violation {
        Sexpr::List(mut items) => {
            items.push(Sexpr::List(vec![
                Sexpr::sym("step-index"),
                Sexpr::Int(index as i64),
            ]));
            Sexpr::List(items)
        }
        other => other,
    }
}

/// A step op `s` matches a transition operation `op` when `op == s` OR
/// `op` ends with `"/" + s` (`docs/rust-port-plan-phase7.md` section B):
/// a bare helper name like `create_task` uniquely reaches a
/// slash-qualified operation like `todo_service/create_task`, making the
/// previously-dead trace machinery live for `.gym`'s actual syntax.
/// An EMPTY step op or transition operation never participates in
/// matching (phase-7 gate, finding 9): an empty-list step parses to op
/// `""`, and a transition with no `:on` extracts operation `""` — under
/// the plain rules `"" == ""` and `"svc/".ends_with("/" + "")` would
/// both match, silently applying a transition the step never named.
fn matches_operation(op: &str, s: &str) -> bool {
    if op.is_empty() || s.is_empty() {
        return false;
    }
    op == s
        || (op.len() > s.len() && op.ends_with(s) && op.as_bytes()[op.len() - s.len() - 1] == b'/')
}

/// Executes `steps` against `ir`'s extracted transitions, starting from
/// `make_initial_state(ir)`, up to `TRACE_BOUND` steps. Operation
/// matching is the suffix rule (`matches_operation`, plan section B):
/// zero matches records `(no-matching-transition <op>)` and a violation,
/// exactly as before; MORE than one match records `(ambiguous-operation
/// <op>)` and a violation naming every candidate (never a silent pick;
/// state is left unchanged, and no invariant re-check runs since nothing
/// mutated); exactly one match is applied via `apply_transition`, and
/// every invariant violated by the resulting state is appended to
/// `violations`. An iterative loop (not recursion) over at most
/// `TRACE_BOUND` steps, so this is bounded and total regardless of
/// `steps`' length.
pub fn execute_trace(ir: &Ir, steps: &[Sexpr]) -> Trace {
    let transitions = extract_transitions(ir);
    let mut state = make_initial_state(ir);
    let mut trace_steps = Vec::with_capacity(steps.len().min(TRACE_BOUND));
    let mut violations = Vec::new();

    for (index, step) in steps.iter().take(TRACE_BOUND).enumerate() {
        let (op, actor, input) = parse_step(step);
        let matched: Vec<&Transition> = transitions
            .iter()
            .filter(|t| matches_operation(&t.operation, &op))
            .collect();
        match matched.len() {
            0 => {
                trace_steps.push(TraceStep {
                    transition_id: "unknown".to_string(),
                    actor: actor.clone(),
                    input: input.clone(),
                    pre_state: state.clone(),
                    post_state: state.clone(),
                    result: None,
                    outcome: Sexpr::List(vec![
                        Sexpr::sym("no-matching-transition"),
                        Sexpr::sym(&op),
                    ]),
                    symbolic: false,
                });
                violations.push(with_step_index(
                    no_matching_transition_violation(&op),
                    index,
                ));
            }
            1 => {
                let mut step_result =
                    apply_transition(matched[0], &state, actor.as_ref(), input.as_ref());
                state = step_result.post_state.clone();
                // Phase-7 gate BLOCKER (finding 1): in-trace invariant
                // checks must use the TRI-STATE evaluator. A grounded
                // `Fails` is a violation, exactly as before; an
                // `Unknown` invariant contributes no violation but
                // marks THIS step symbolic, so a property verdict over
                // this trace can never claim `(basis checked)` while
                // the invariants it was supposed to check were
                // undecided.
                let (inv_violations, any_undecided) = check_invariants3(ir, &state);
                violations.extend(
                    inv_violations
                        .into_iter()
                        .map(|v| with_step_index(v, index)),
                );
                if any_undecided {
                    step_result.symbolic = true;
                }
                trace_steps.push(step_result);
            }
            _ => {
                let candidates: Vec<&str> = matched.iter().map(|t| t.operation.as_str()).collect();
                let transition_ids: Vec<&str> = matched.iter().map(|t| t.id.as_str()).collect();
                trace_steps.push(TraceStep {
                    transition_id: "unknown".to_string(),
                    actor: actor.clone(),
                    input: input.clone(),
                    pre_state: state.clone(),
                    post_state: state.clone(),
                    result: None,
                    outcome: Sexpr::List(vec![Sexpr::sym("ambiguous-operation"), Sexpr::sym(&op)]),
                    symbolic: false,
                });
                violations.push(with_step_index(
                    ambiguous_operation_violation(&op, &candidates, &transition_ids),
                    index,
                ));
            }
        }
    }

    Trace {
        steps: trace_steps,
        violations,
        final_state: state,
    }
}

/// A default/empty `TraceStep`, for callers that need a total fallback
/// when a trace has no steps to pair a violation with (`verify.rs`'s
/// property/scenario dispatch: `counterexample(v, first_step)` where
/// `first_step` must exist even if `trace.steps` is empty).
pub fn default_trace_step() -> TraceStep {
    TraceStep {
        transition_id: "unknown".to_string(),
        actor: None,
        input: None,
        pre_state: vec![],
        post_state: vec![],
        result: None,
        outcome: Sexpr::List(vec![]),
        symbolic: false,
    }
}

/// Projects a whole `Trace` into a `(trace (steps (...)) (violations
/// (...)) (final-state ...))` `Sexpr`, for embedding in a
/// `verification-result`'s `trace` field (`verify.rs`, plan section B).
/// Not part of the Lamedh reference's own serialization (its `trace` is
/// a `defrecord` value with no Rust equivalent, same rationale as
/// `trace_step_to_sexpr` above) and not pinned by any oracle test, so
/// this shape is free -- kept structurally consistent with
/// `trace_step_to_sexpr`'s flat-tag-plus-one-nested-field-list
/// convention.
pub fn trace_to_sexpr(trace: &Trace) -> Sexpr {
    Sexpr::List(vec![
        Sexpr::sym("trace"),
        Sexpr::List(vec![
            Sexpr::List(vec![
                Sexpr::sym("steps"),
                Sexpr::List(trace.steps.iter().map(trace_step_to_sexpr).collect()),
            ]),
            Sexpr::List(vec![
                Sexpr::sym("violations"),
                Sexpr::List(trace.violations.clone()),
            ]),
            Sexpr::List(vec![
                Sexpr::sym("final-state"),
                state_to_sexpr(&trace.final_state),
            ]),
        ]),
    ])
}

// -----------------------------------------------------------------------
// Invariant checking and counterexamples.
// -----------------------------------------------------------------------

/// Tri-state invariant check (phase-7 gate, finding 1): one violation
/// per invariant whose `:always` predicate is a grounded
/// `Verdict::Fails` against `state`, plus a flag that is `true` iff ANY
/// invariant's verdict was `Unknown` (undecided — neither a violation
/// nor evidence of holding). `execute_trace` uses this so that a trace
/// can never silently launder an undecidable invariant into "held";
/// the boolean `check_invariants` below is preserved verbatim for
/// phase-6 parity (its oracle pins the permissive-default behavior).
pub fn check_invariants3(ir: &Ir, state: &State) -> (Vec<Sexpr>, bool) {
    let mut violations = Vec::new();
    let mut any_undecided = false;
    for inv in ir.nodes_of_kind("invariant") {
        let always = inv
            .field(":always")
            .cloned()
            .unwrap_or_else(|| Sexpr::List(vec![]));
        match eval_predicate3(&always, state, None, None) {
            Verdict::Holds => {}
            Verdict::Unknown => any_undecided = true,
            Verdict::Fails => violations.push(Sexpr::List(vec![
                Sexpr::sym("violation"),
                Sexpr::List(vec![Sexpr::sym("invariant"), Sexpr::Str(inv.id.clone())]),
                Sexpr::List(vec![Sexpr::sym("predicate"), always]),
                Sexpr::List(vec![Sexpr::sym("state"), state_to_sexpr(state)]),
            ])),
        }
    }
    (violations, any_undecided)
}

/// One `(violation (invariant id) (predicate p) (state ...))` per
/// `invariant` node whose `:always` predicate fails against `state`.
pub fn check_invariants(ir: &Ir, state: &State) -> Vec<Sexpr> {
    ir.nodes_of_kind("invariant")
        .into_iter()
        .filter_map(|inv| {
            let always = inv
                .field(":always")
                .cloned()
                .unwrap_or_else(|| Sexpr::List(vec![]));
            if eval_predicate(&always, state, None, None) {
                None
            } else {
                Some(Sexpr::List(vec![
                    Sexpr::sym("violation"),
                    Sexpr::List(vec![Sexpr::sym("invariant"), Sexpr::Str(inv.id.clone())]),
                    Sexpr::List(vec![Sexpr::sym("predicate"), always]),
                    Sexpr::List(vec![Sexpr::sym("state"), state_to_sexpr(state)]),
                ]))
            }
        })
        .collect()
}

/// A flat, tagged projection of a `TraceStep`, for embedding inside a
/// `counterexample`. NOTE (ambiguity, reported per Process Rule 1): the
/// plan documents `counterexample`'s five top-level fields explicitly
/// (`violation`/`trace-step`/`pre-state`/`input`/`outcome`) but not the
/// `Sexpr` shape of the embedded `trace-step` value itself (the Lamedh
/// reference embeds its `defrecord` value directly, which has no Rust
/// equivalent). This implementation projects it the same way
/// `IrNode::to_sexpr` projects a node: a `trace-step` head tag plus one
/// nested list of `(key value)` pairs, covering every `TraceStep`
/// field. Not exercised by `transition_oracle_test.rs` (the oracle's
/// item list covers `check_invariants` but not `counterexample`
/// directly), so this shape was left free to be revisited by
/// `verify.rs`/`verify_oracle_test.rs`. STAGE 3 UPDATE: it turned out to
/// be exactly what `verify.rs`'s `compare_trace_step` and
/// `normalize_counterexample` need (a step's `transition-id`/`actor`/
/// `input`/`pre-state` recoverable from an embedded `Sexpr`), so this is
/// now `pub` and reused there rather than reinvented -- see
/// `verify.rs`'s `trace_step_field` doc comment for the nested-lookup
/// convention it reads this shape with.
pub fn trace_step_to_sexpr(step: &TraceStep) -> Sexpr {
    Sexpr::List(vec![
        Sexpr::sym("trace-step"),
        Sexpr::List(vec![
            Sexpr::List(vec![
                Sexpr::sym("transition-id"),
                Sexpr::Str(step.transition_id.clone()),
            ]),
            Sexpr::List(vec![
                Sexpr::sym("actor"),
                step.actor.clone().unwrap_or_else(|| Sexpr::List(vec![])),
            ]),
            Sexpr::List(vec![
                Sexpr::sym("input"),
                step.input.clone().unwrap_or_else(|| Sexpr::List(vec![])),
            ]),
            Sexpr::List(vec![
                Sexpr::sym("pre-state"),
                state_to_sexpr(&step.pre_state),
            ]),
            Sexpr::List(vec![
                Sexpr::sym("post-state"),
                state_to_sexpr(&step.post_state),
            ]),
            Sexpr::List(vec![
                Sexpr::sym("result"),
                step.result.clone().unwrap_or_else(|| Sexpr::List(vec![])),
            ]),
            Sexpr::List(vec![Sexpr::sym("outcome"), step.outcome.clone()]),
            Sexpr::List(vec![
                Sexpr::sym("basis"),
                Sexpr::sym(if step.symbolic { "symbolic" } else { "checked" }),
            ]),
        ]),
    ])
}

/// `(counterexample (violation v) (trace-step s) (pre-state ...)
/// (input ...) (outcome ...))`, mirroring the Lamedh reference's
/// `gymnast-counterexample` field-for-field.
pub fn counterexample(violation: &Sexpr, trace_step: &TraceStep) -> Sexpr {
    Sexpr::List(vec![
        Sexpr::sym("counterexample"),
        Sexpr::List(vec![Sexpr::sym("violation"), violation.clone()]),
        Sexpr::List(vec![
            Sexpr::sym("trace-step"),
            trace_step_to_sexpr(trace_step),
        ]),
        Sexpr::List(vec![
            Sexpr::sym("pre-state"),
            state_to_sexpr(&trace_step.pre_state),
        ]),
        Sexpr::List(vec![
            Sexpr::sym("input"),
            trace_step
                .input
                .clone()
                .unwrap_or_else(|| Sexpr::List(vec![])),
        ]),
        Sexpr::List(vec![Sexpr::sym("outcome"), trace_step.outcome.clone()]),
    ])
}

// -----------------------------------------------------------------------
// Transition ref-checking (plan section C).
// -----------------------------------------------------------------------

fn w406_unresolved_state_ref(transition_id: &str, state_ref: &str) -> Sexpr {
    diag_sexpr(
        "warning",
        "W406",
        (0, 0),
        format!(
            "unresolved-state-ref: transition {} references undeclared state {}",
            transition_id, state_ref
        ),
    )
}

/// One `W406 unresolved-state-ref` warning per `reads`/`writes` entry
/// that names no `state` IR node (`docs/rust-port-plan-phase7.md`
/// section C), mirroring the Lamedh reference's diagnostic. Every entry
/// is checked independently -- a name appearing in both `reads` and
/// `writes` produces two warnings, one per occurrence, matching the
/// plan's own worked arithmetic over `examples/todo.gym`. Total over any
/// `Ir`: a linear membership scan against the (small, already-parsed)
/// list of declared state-node names, no hashing, no iteration order
/// dependency that could reach the output.
pub fn check_transition_refs(ir: &Ir) -> Vec<Sexpr> {
    let state_names: Vec<&str> = ir
        .nodes_of_kind("state")
        .into_iter()
        .map(|n| n.name.as_str())
        .collect();
    let is_declared = |name: &str| state_names.iter().any(|s| *s == name);

    let mut warnings = Vec::new();
    for t in extract_transitions(ir) {
        for r in &t.reads {
            if !is_declared(r) {
                warnings.push(w406_unresolved_state_ref(&t.id, r));
            }
        }
        for w in &t.writes {
            if !is_declared(w) {
                warnings.push(w406_unresolved_state_ref(&t.id, w));
            }
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_behavior_node(id: &str, fields: Vec<(&str, Sexpr)>, clauses: Vec<Sexpr>) -> IrNode {
        IrNode::new(
            id.to_string(),
            "behavior",
            "x".to_string(),
            fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
            clauses,
        )
    }

    #[test]
    fn extract_transition_is_total_over_a_node_with_no_fields_or_clauses() {
        let node = make_behavior_node("m/behavior/empty", vec![], vec![]);
        let t = extract_transition(&node);
        assert_eq!(t.id, "m/behavior/empty");
        assert_eq!(t.operation, "");
        assert_eq!(t.actor, None);
        assert_eq!(t.input, None);
        assert!(t.reads.is_empty());
        assert!(t.writes.is_empty());
        assert!(t.preconditions.is_empty());
        assert!(t.postconditions.is_empty());
        assert_eq!(t.result, None);
        assert!(t.failures.is_empty());
        assert!(t.emissions.is_empty());
    }

    #[test]
    fn off_shape_requires_clause_is_kept_whole_rather_than_dropped() {
        // A `requires` clause with two predicates instead of one is
        // off-shape; it must survive (kept whole), not vanish.
        let bad = Sexpr::List(vec![
            Sexpr::sym("requires"),
            Sexpr::sym("p1"),
            Sexpr::sym("p2"),
        ]);
        let node = make_behavior_node("m/behavior/off", vec![], vec![bad.clone()]);
        let t = extract_transition(&node);
        assert_eq!(t.preconditions, vec![bad]);
    }

    #[test]
    fn on_spec_bare_symbol_yields_operation_only() {
        let node = make_behavior_node(
            "m/behavior/bare",
            vec![(":on", Sexpr::sym("svc/op"))],
            vec![],
        );
        let t = extract_transition(&node);
        assert_eq!(t.operation, "svc/op");
        assert_eq!(t.actor, None);
        assert_eq!(t.input, None);
    }

    #[test]
    fn eval_predicate_never_panics_on_deeply_ragged_shapes() {
        let state: State = vec![];
        // `=`/`<`/`<=` with missing operands must not panic; they fall
        // back to nil-valued operands via `nth_arg`.
        assert!(eval_predicate(
            &Sexpr::list(vec![Sexpr::sym("=")]),
            &state,
            None,
            None
        ));
        assert!(!eval_predicate(
            &Sexpr::list(vec![Sexpr::sym("<")]),
            &state,
            None,
            None
        ));
        assert!(!eval_predicate(
            &Sexpr::list(vec![Sexpr::sym("not")]),
            &state,
            None,
            None
        ));
    }

    #[test]
    fn apply_transition_is_total_when_writes_entry_is_absent_from_state() {
        let state: State = vec![];
        let t = Transition {
            id: "m/behavior/y".to_string(),
            operation: "svc/op".to_string(),
            actor: None,
            input: None,
            reads: vec![],
            writes: vec!["never_seen".to_string()],
            atomic: None,
            idempotency: None,
            preconditions: vec![],
            postconditions: vec![],
            result: None,
            failures: vec![],
            emissions: vec![],
        };
        let input_val = Sexpr::sym("payload");
        let step = apply_transition(&t, &state, None, Some(&input_val));
        assert_eq!(
            step.post_state,
            vec![("never_seen".to_string(), Sexpr::List(vec![input_val]))]
        );
    }

    #[test]
    fn execute_trace_is_total_over_an_empty_ir_and_empty_steps() {
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
        let trace = execute_trace(&ir, &[]);
        assert!(trace.steps.is_empty());
        assert!(trace.violations.is_empty());
        assert!(trace.final_state.is_empty());
    }

    #[test]
    fn counterexample_embeds_violation_and_step_fields() {
        let step = TraceStep {
            transition_id: "m/behavior/x".to_string(),
            actor: Some(Sexpr::sym("a")),
            input: Some(Sexpr::sym("i")),
            pre_state: vec![("s".to_string(), Sexpr::Int(1))],
            post_state: vec![("s".to_string(), Sexpr::Int(1))],
            result: None,
            outcome: Sexpr::List(vec![Sexpr::sym("precondition-failed")]),
            symbolic: false,
        };
        let violation = Sexpr::List(vec![Sexpr::sym("violation")]);
        let ce = counterexample(&violation, &step);
        assert_eq!(ce.assoc("violation"), Some(&violation));
        assert_eq!(ce.assoc("input"), Some(&Sexpr::sym("i")));
        assert_eq!(
            ce.assoc("outcome"),
            Some(&Sexpr::List(vec![Sexpr::sym("precondition-failed")]))
        );
    }
}
