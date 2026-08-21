# Rust port, phase 6: executable transition calculus + verification

Execution plan for the sixth Rust increment: porting `src/transition.lisp`
and `src/verify.lisp` onto the phase-2 IR. Process rules from phases 3–5
apply unchanged, including the committed-oracle upgrade (Stage 1 commits
its tests-of-record first, oracle files only, message
`"phase 6 stage 1: oracle tests-of-record (red)"`).

Contracts are fixed by `docs/ir-contract-deltas.md` and the committed
goldens. `src/transition.lisp` / `src/verify.lisp` are behavioral intent;
where their clause/field reads assume Lamedh IR shapes, the Rust IR
shapes in the delta doc govern, and every adaptation below is explicit.

## Scope

1. **`transition.rs`** — transition extraction, the reference state
   machine, the closed predicate evaluator, bounded trace execution,
   invariant checking, counterexamples.
2. **`verify.rs`** — execution-environment extraction, obligation
   lowering (property/scenario/concurrency/fault/coverage/model +
   invariant/constraint), reference verification, trace equivalence,
   normalized counterexamples, coverage analysis, the verification
   bundle.
3. **CLI `verify FILE.gym`** — canonical serialization of the bundle to
   stdout; golden `tests/fixtures/todo-verify.sexpr`; CI reproducible-
   verify gate.

Out of scope: caching (phase 7), assembly evidence bundles, adequacy,
wiring verification into `compile`/`synthesize` outputs (phase 7, with
caching, so the artifact set changes once).

## A. transition.rs

```rust
use crate::ir::{Ir, IrNode};
use crate::sexpr::Sexpr;

/// The reference transition extracted from one behavior IR node.
#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    pub id: String,                    // the behavior node id
    pub operation: String,             // "todo_service/create_task"
    pub actor: Option<String>,         // first :on binder
    pub input: Option<String>,         // second :on binder
    pub reads: Vec<String>,
    pub writes: Vec<String>,
    pub atomic: Option<Sexpr>,
    pub idempotency: Option<Sexpr>,
    pub preconditions: Vec<Sexpr>,     // requires clause bodies (the pred)
    pub postconditions: Vec<Sexpr>,    // ensures clause bodies
    pub result: Option<Sexpr>,         // returns clause body
    pub failures: Vec<Sexpr>,          // whole fails clauses (tail after head)
    pub emissions: Vec<Sexpr>,         // whole emits clause tails
}

pub fn extract_transition(node: &IrNode) -> Transition;
pub fn extract_transitions(ir: &Ir) -> Vec<Transition>;   // behavior kind, all_nodes order
```

Rust-IR adaptations (each mirrors the reference's INTENT over our shapes):

- `:on` is `(iface/op binder1 binder2 ...)` with the slash already
  joined into one symbol: `operation` = first element's text; `actor` =
  second element (if any); `input` = third (if any). A bare-symbol `:on`
  yields operation only.
- `:reads`/`:writes` are always lists in the Rust IR (phase-2 plural
  rule) of symbols; accept a bare symbol defensively as a 1-list.
- Clause tails: `(requires <pred>)` → the pred; `(ensures <pred>)` →
  the pred; `(returns <expr>)` → the expr; `(fails <error> :when <p>
  :preserves <s>)` → the tail `[<error> :when <p> :preserves <s>]`
  kept whole; `(emits ...)` → the tail kept whole. Off-shape clauses
  (wrong arity) are kept whole in the nearest field rather than
  dropped — visibility over silence, as everywhere since phase 3.

State machine and evaluator:

```rust
pub type State = Vec<(String, Sexpr)>;   // assoc list, insertion-ordered

pub fn make_initial_state(ir: &Ir) -> State;
// One entry per state node (all_nodes order): (name, value) where
// `:initial empty` (or absent) → Sexpr::List(vec![]) and any other
// initial value is carried verbatim.

pub fn eval_predicate(pred: &Sexpr, state: &State, actor: Option<&Sexpr>, input: Option<&Sexpr>) -> bool;
pub fn eval_expr(expr: &Sexpr, state: &State, actor: Option<&Sexpr>, input: Option<&Sexpr>) -> Sexpr;
```

Evaluator semantics, ported EXACTLY including the permissive defaults
(this is the closed evaluator the surface doc points at — nothing else
may creep in):

| pred | result |
|---|---|
| nil (empty list) / any atom | `true` |
| `(= a b)` | eval_expr equality (structural) |
| `(not p)` | negation |
| `(and p...)` / `(or p...)` | all / any over the tail |
| `(< a b)` / `(<= a b)` | integer comparison when BOTH sides eval to `Int`; otherwise `false` (delta: Lamedh would error on non-numbers — the Rust evaluator is total; record in the delta doc) |
| anything else (calls, forall, …) | `true` (symbolic: unknown predicates hold) |

| expr | result |
|---|---|
| `Int`/`Str` | itself |
| symbols `pre`/`post` | the state, printed as its assoc-list Sexpr |
| `actor`/`input` | the given value or `nil` |
| `result` | `Sym("result-placeholder")` |
| any other symbol | the state entry with that name, else the symbol itself |
| any list | itself verbatim |

Trace machinery:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct TraceStep {
    pub transition_id: String,       // "unknown" when no transition matched
    pub actor: Option<Sexpr>,
    pub input: Option<Sexpr>,
    pub pre_state: State,
    pub post_state: State,
    pub result: Option<Sexpr>,
    pub outcome: Sexpr,              // (succeeded) | (failed <error>) |
                                     // (precondition-failed) |
                                     // (no-matching-transition <op>)
}

pub fn apply_transition(t: &Transition, state: &State, actor: Option<&Sexpr>, input: Option<&Sexpr>) -> TraceStep;
pub const TRACE_BOUND: usize = 1000;
pub fn execute_trace(ir: &Ir, steps: &[Sexpr]) -> Trace;
pub struct Trace { pub steps: Vec<TraceStep>, pub violations: Vec<Sexpr>, pub final_state: State }
```

`apply_transition` order, exactly as the reference: (1) first failure
clause whose `:when` holds → outcome `(failed <error>)`, post = pre
(the `:preserves` field is recorded, state is preserved either way);
(2) else all preconditions hold → post = pre with each `writes` entry
appended with the input value, outcome `(succeeded)`, result = input;
(3) else `(precondition-failed)`, post = pre.

Trace steps: each element of `steps` is `(op-name actor input)` — for
the Rust surface, a step written `(create_task (actor task))` in an
execute clause parses as op = first symbol, actor = first element of
the following list (if a list follows), input = its second element.
Operation matching is EXACT equality against `Transition::operation`.
KNOWN REFERENCE BEHAVIOR, ported as-is: todo.gym's execute steps name
bare ops (`create_task`) while transitions carry qualified operations
(`todo_service/create_task`), so those steps produce
`no-matching-transition` violations — the Lamedh reference does the
same, its compilation artifact never embedded verification results,
and improving the match rule is a tracked candidate for phase 7, not a
silent fix here. The golden pins the honest outcome.

Invariant checking and counterexamples: `check_invariants(ir, state)`
returns one `(violation (invariant id) (predicate p) (state ...))` per
invariant whose `:always` fails against the state;
`counterexample(violation, step)` and the trace loop mirror the
reference (violations append after each applied step; bound 1000;
unmatched ops record both an error step and a violation).

## B. verify.rs

Execution environment, from the acceptance node's `(execution :clock c
:randomness r :network n :locale l :timezone tz)` clause — keys are the
Rust IR's colon-keyword symbols; defaults `system`/`system`/`system`/
`"en-US"`/`"UTC"`. `env_deterministic` and the three warning
diagnostics (`non-deterministic-clock`/`-randomness`/`-network`) port
directly, emitted via the shared `diag_sexpr` shape with span (0 0) and
the acceptance id embedded in the message.

Obligation lowering — one `(verification-obligation (...))` Sexpr per
clause, ids `<acceptance-id>/<kind>[/<name>]`:

| clause (Rust IR shape) | obligation fields |
|---|---|
| `(property n :generate G :execute E :must M)` | kind property, name n, generate G, execute E, assertion M |
| `(scenario n (given ...) (when ...) (then ...) ...)` | kind scenario, name n, steps = the list tail verbatim |
| `(concurrency n :actors A :schedule S :must M)` | kind concurrency + actors/schedule/assertion |
| `(fault n :after A :inject I :must M)` | kind fault + after/inject/assertion |
| `(coverage :every_operation t ...)` | kind coverage, the five flags read by OUR underscore keys (`:every_operation`, `:every_error`, `:every_transition`, `:every_invariant`, `:boundaries`) |
| `(model n ...)` | kind model, spec = tail |
| `(execution ...)` | no obligation (env only) |
| invariant node | id `<node-id>/invariant-check`, kind invariant, scope, predicate = `:always` |
| constraint node | id `<node-id>/constraint-check`, kind constraint, class/scope/under/assertion = `:must` |

`lower_all_obligations(ir)` = acceptance obligations (clause order per
node, nodes in all_nodes order) ++ invariant ++ constraint obligations.

Reference verification (`verify_obligation` dispatch):

- **property**: no execute → skipped + `no-execute-spec` warning; else
  trace over the execute steps (`(sequence ...)` unwraps; our IR's
  execute is a Nested/List of step forms — each list element is one
  step; a single non-list execute value is one step). Violations →
  failed with `counterexample(v, first step)` each; else passed.
- **scenario**: steps = the `when` entries' action lists only (given/
  then contribute none). In OUR IR a scenario clause tail is
  `(given ((owner v))) (when (invite_distinct owner 256)) (then ...)`
  — a `when` entry's action is its second element when that element is
  a list. No steps → skipped + `no-trace-steps`; else as property.
- **invariant**: predicate against `make_initial_state`; on pass, apply
  EVERY extracted transition (actor/input None) to the initial state
  and check the predicate on each post-state; first violation loses,
  shapes exactly as the reference's `normalized-counterexample`s.
- everything else (concurrency/fault/coverage/model/constraint):
  skipped with the info diagnostic `deferred-verification` naming the
  kind ("requires runtime execution").

Trace equivalence (`compare_traces`, `trace_equivalence_result`) and
`normalize_counterexample(s)` port structurally 1:1 — divergence kinds
`outcome-mismatch`, `state-mismatch`, `extra-implementation-steps`,
`missing-implementation-steps`.

Coverage analysis (`coverage_gaps`): port the reference's counting
logic verbatim (property+scenario+fault counts vs transitions/
behaviors/invariants, the four gap kinds, only when a coverage
obligation exists and only for flags that are set).

The bundle:

```
(verification-bundle ((schema "gymnast.verify/0.1")
  (obligations (...)) (results (...))
  (summary ((total N) (passed N) (failed N) (skipped N)))
  (coverage (...)) (environment-diagnostics (...))))
```

`compile_verification(ir) -> Sexpr` — pure, deterministic, no
fingerprint field (the reference has none; consistency note recorded in
the delta doc). Env diagnostics come from the FIRST acceptance node
only, as the reference does.

## C. CLI + golden + CI

- `gymnast-rs verify FILE.gym` — same pipeline/diagnostic/exit contract
  as `ir` (exit reflects parse/IR errors; verification FAILURES are
  results data, not process errors — a failed obligation exits 0, the
  bundle carries it; this mirrors the reference where verification is
  evidence, and promotion decisions belong to assembly in phase 7).
- Golden `rust/tests/fixtures/todo-verify.sexpr`, generated by the
  integrator via the CLI after the non-golden oracle tests pass.
- CI: reproducible-verify (run twice, diff, compare to golden).

## Oracle tests (Stage 1 authors AND COMMITS; implementers may not touch)

`transition_oracle_test.rs`:
1. Extraction over todo.gym: 2 transitions; create_task's operation
   `todo_service/create_task`, actor `user`, input `request`, reads
   `[memberships, todo_lists]`, writes `[tasks]`, 2 preconditions,
   1 postcondition, result present, 1 failure, 1 emission.
2. Evaluator table: every row of the pred/expr tables above as direct
   assertions, including the non-Int comparison → false delta and the
   unknown-predicate → true default.
3. apply_transition: failure-clause precedence over preconditions; a
   holding `:when` yields `(failed <error>)` with state preserved;
   passing preconditions appends input to every writes entry; failing
   preconditions yields `(precondition-failed)` with state unchanged.
4. Bounded trace: a steps list longer than TRACE_BOUND stops at the
   bound; unmatched op records the violation AND the error step;
   deterministic across two runs.
5. check_invariants: a violated always-predicate produces the violation
   shape; holding predicates produce none.
6. Initial state: one entry per state node, `empty` → nil.

`verify_oracle_test.rs`:
1. Env extraction from todo.gym: virtual/seeded/controlled/"UTC",
   deterministic → zero env warnings; a hand-built acceptance node
   with defaults → three warnings naming the acceptance id.
2. Lowering over todo.gym: obligation ids exactly
   {todo/acceptance/production/property/create_then_read,
   .../property/viewer_cannot_mutate, .../scenario/sharing_boundary,
   .../concurrency/boundary_race, .../fault/durable_restart,
   .../coverage, todo/invariant/owner_isolation/invariant-check,
   todo/invariant/sharing_limit/invariant-check,
   todo/constraint/collaborative_capacity/constraint-check}; coverage
   obligation's five flags all truthy; fault obligation's after/
   inject/assertion all present and distinct (the phase-4 fault-loss
   regression guard at the obligation level).
3. Dispatch statuses over todo.gym: both invariants passed;
   concurrency/fault/coverage/model/constraint skipped with
   deferred-verification; property/scenario statuses pinned to
   whatever the reference semantics yield (the oracle author derives
   them from the plan's stated exact-match rule and pins them
   explicitly — they are failed-with-no-matching-transition under
   that rule, and the test SAYS so in a comment).
4. Trace equivalence: equal traces → equivalent, no divergences; an
   outcome mismatch, a state mismatch, and length mismatches each
   produce their divergence kind; normalized counterexamples carry
   obligation id, divergence type, and the step projections.
5. Coverage analysis over todo.gym: counts (3 property+scenario+fault
   obligations wait — 2 property + 1 scenario + 1 fault = 4 total,
   2 transitions, 2 invariants, 2 invariant obligations) and the
   resulting gaps list computed per the reference logic — the oracle
   author derives and pins exact values.
6. Bundle: summary total == obligations len == 9; passed+failed+
   skipped == total; determinism (two compile_verification runs
   byte-identical); schema present.
7. CLI: verify on todo.gym exits 0 and stdout parses via sexpr::parse;
   verify on a spec with an IR error exits 1.

## Stage plan

- **Stage 1 — oracle author** (Sonnet): writes and COMMITS the two
  oracle files.
- **Stage 2 — transition.rs** (Sonnet): section A green against
  transition_oracle_test.
- **Stage 3 — verify.rs + CLI + golden + CI** (Sonnet, first
  integrator): sections B and C; generates the golden after non-golden
  tests pass.
- **Verify loop** (Sonnet): as phase 5, oracle integrity via git diff
  against Stage 1's commit.
- Integrator verification, then the **Opus gate**.

Definition of done: warning-free `-D warnings --all-targets`, full
suite green with oracle files unmodified since Stage 1's commit, fmt
clean, verify output byte-stable and CI-gated, all prior goldens
untouched.
