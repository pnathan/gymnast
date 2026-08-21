# Rust port, phase 7: three-valued verification, live traces, caching

Execution plan for the seventh Rust increment: the two deferred semantic
decisions (the tri-state evaluator and the operation-match rule), the
verification-artifact hardening the phase-6 gate assigned, and the port
of `src/cache.lisp`. Process rules from phases 3–6 apply unchanged,
including the committed-oracle upgrade (Stage 1 commits first, message
`"phase 7 stage 1: oracle tests-of-record (red)"`).

This phase CHANGES verification semantics deliberately, with the gates'
findings as the motivating record:

- Phase-6 gate finding 5: no valid `.gym` syntax can express an execute
  step matching a slash-qualified operation, so all trace machinery is
  currently dead. The op-match rule below makes it live.
- Phase-6 gate findings 1+4: the boolean evaluator launders symbolic
  defaults into `passed` AND fabricates `failed` verdicts; the basis
  field was the honest minimum, the tri-state verdict is the fix.

`todo-verify.sexpr` regenerates once, at Stage 3, with the full new
semantics — the commit message records each behavioral change.

## Scope

1. **Tri-state evaluator** (`transition.rs`): `Verdict { Holds, Fails,
   Unknown }`.
2. **Operation matching** (`transition.rs`): suffix rule, making todo's
   properties actually execute transitions.
3. **Transition ref-checking** (`transition.rs` + bundle): port the
   reference's `unresolved-state-ref` warnings; stop silently
   fabricating phantom state entries without a trace.
4. **Bundle fingerprint + typed summary** (`verify.rs`).
5. **`cache.rs`** — content-addressed cache keys, in-memory store,
   plan diffing and invalidation closure, check/explain.
6. **Runner readback** (`runner.rs`): `RunResult::from_sexpr`,
   `Attempt::from_sexpr`, and `node_fingerprint` on `RunResult`.
7. Obligation-id uniqueness (`verify.rs`): `E601` duplicate obligation
   id (error diagnostic in the bundle's own diagnostics).

Out of scope: assembly evidence bundles and adequacy (phase 8), wiring
verification/caching into `compile`/`synthesize` artifact sets
(phase 8, so the artifact set changes once more, not twice), persistent
on-disk cache (the reference is in-memory; disk is a phase-8+ decision).

## A. Tri-state evaluator

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict { Holds, Fails, Unknown }

pub fn eval_predicate3(pred: &Sexpr, state: &State, actor: Option<&Sexpr>, input: Option<&Sexpr>) -> Verdict;
```

Semantics table (the boolean evaluator's defaults become `Unknown`):

| pred | verdict |
|---|---|
| nil / any atom | Unknown |
| `(= a b)` | Holds/Fails by structural equality |
| `(not p)` | Fails/Holds swap; Unknown stays Unknown |
| `(and p...)` | Fails if any Fails; else Unknown if any Unknown; else Holds |
| `(or p...)` | Holds if any Holds; else Unknown if any Unknown; else Fails |
| `(< a b)` / `(<= a b)` | Holds/Fails when both Int; Unknown otherwise |
| unrecognized head | Unknown |

`eval_predicate`/`eval_predicate_basis` remain, reimplemented over
`eval_predicate3` with the phase-6 mapping (Unknown → true + symbolic
for pred positions; the non-Int comparison maps Unknown → false +
symbolic there — preserve the existing boolean behavior EXACTLY; every
phase-6 test must pass unchanged). `apply_transition` keeps boolean
semantics for preconditions/failure `:when` guards (reference
behavior), but `TraceStep` gains a `symbolic: bool` field set when any
guard evaluation en route was Unknown — serialized as
`(basis symbolic|checked)` in the step form.

Verification statuses: invariant obligations now yield `indeterminate`
(new status) when the deciding evaluation is Unknown, instead of a
symbolic-passed/failed; `passed`/`failed` are reserved for grounded
verdicts (the `basis` field stays, now always `checked` on
passed/failed and `symbolic` on indeterminate — redundant but
backward-shaped). Property/scenario results: `basis` = symbolic iff any
executed step was symbolic. Summary gains `(indeterminate N)` after
`skipped`. todo.gym outcome after ALL phase-7 changes: derive it —
both invariants become `indeterminate`; properties/scenario now trace
live (section B) with symbolic steps (their preconditions are abstract
predicates), and pass/fail per trace violations.

## B. Operation matching

In trace-step dispatch (`execute_trace`), a step op `s` matches a
transition operation `op` when `op == s` OR `op` ends with `"/" + s`.
If EXACTLY one transition matches, it applies. If more than one
matches, the step records outcome `(ambiguous-operation s)` and a
violation `(violation (type ambiguous-operation) (operation s)
(candidates (op1 op2 ...)))` — ambiguity is an error, never a silent
pick. Zero matches: unchanged (`no-matching-transition`).

Consequence for todo.gym (derive and pin in the oracle): `create_task`
uniquely matches `todo_service/create_task`, `query_tasks` matches
nothing (no behavior declares it — still a no-match), `invite` and
`invite_distinct` — `invite_distinct` matches nothing (helper). So
create_then_read's trace runs create_task then fails on query_tasks's
no-match; viewer_cannot_mutate runs create_task cleanly and passes
(with symbolic basis); sharing_boundary's invite_distinct steps
no-match and fail. The oracle author re-derives all statuses and the
new summary from these rules against the fixture, showing the
arithmetic.

## C. Transition ref-checking

```rust
pub fn check_transition_refs(ir: &Ir) -> Vec<Sexpr>;   // W406 unresolved-state-ref
```

One `W406` warning per `reads`/`writes` entry naming no `state` node
(message carries the transition id and the ref, mirroring the
reference's `unresolved-state-ref`). `compile_verification` folds these
into a new bundle field `(transition-diagnostics (...))` placed after
`environment-diagnostics`. todo.gym produces SIX (memberships,
todo_lists, tasks ×1 from create_task; memberships, invitations ×2 from
invite_user — derive exactly; the point is they exist and are honest).
`apply_transition`'s write-to-undeclared behavior is UNCHANGED (the
calculus stays total); the warnings make it visible.

## D. Bundle fingerprint + typed access

- `compile_verification` appends `(fingerprint "fnv1a64:...")` over the
  bundle's fingerprint-free form, exactly the `Ir`/`Plan` discipline.
  The delta doc's no-fingerprint note is replaced by the new contract.
- New typed accessor for phase-8 assembly:
  `pub struct VerificationSummary { pub total: i64, pub passed: i64, pub failed: i64, pub skipped: i64, pub indeterminate: i64 }`
  and `pub fn bundle_summary(bundle: &Sexpr) -> Option<VerificationSummary>`.
- **E601 duplicate-obligation-id**: after lowering, a second occurrence
  of any obligation id adds an error diagnostic to a new bundle field
  `(diagnostics (...))` (before `source-diagnostics`); cache keys and
  assembly evidence depend on id uniqueness.

## E. cache.rs

Port of `src/cache.lisp` onto the Rust types — in-memory, no globals:

```rust
pub const CACHE_SCHEMA: &str = "gymnast.cache/0.1";

#[derive(Debug, Clone, PartialEq)]
pub struct CacheEntry {
    pub key: String,               // "fnv1a64:..."
    pub node_id: String,
    pub candidate: Sexpr,
    pub evidence: Sexpr,           // caller-supplied; nil allowed
    pub timestamp: Sexpr,          // caller-supplied symbol/string; the
                                   // cache NEVER reads a clock (determinism)
}

pub struct CacheStore { /* Vec<(String, CacheEntry)> — insertion order, first match wins on lookup, store replaces */ }
impl CacheStore {
    pub fn new() -> Self;
    pub fn clear(&mut self);
    pub fn store(&mut self, entry: CacheEntry);
    pub fn lookup(&self, key: &str) -> Option<&CacheEntry>;
    pub fn len(&self) -> usize;  pub fn is_empty(&self) -> bool;
    pub fn keys(&self) -> Vec<&str>;
}

/// (cache-key-material (node-fingerprint "...") (ir-slice-fingerprint
/// "...") (dependency-fingerprint "...") (recipe r) (model (...))
/// (capabilities (...))) — flat form, fingerprinted for the key.
pub fn cache_key_material(ir: &Ir, plan: &Plan, node: &PlanNode) -> Sexpr;
pub fn cache_key(ir: &Ir, plan: &Plan, node: &PlanNode) -> String;
```

- ir-slice fingerprint: over `(ir-slice (<node.to_sexpr()> ...))` of the
  node's resolved slice (shared `resolve_ir_slice`; unresolved inputs
  make the slice smaller AND the W405s are the caller's to surface —
  the key covers what was actually used).
- dependency fingerprint: over the dependency slice
  `((dep-id "fp") ...)` exactly as `prompt.rs` builds it — REUSE that
  builder (make it `pub(crate)` if needed), never a second copy.

Validity, dependents, diff — mirror the reference:

```rust
pub fn entry_valid(ir: &Ir, plan: &Plan, node: &PlanNode, entry: &CacheEntry) -> bool;  // key equality
pub fn node_dependents<'a>(plan: &'a Plan, node_id: &str) -> Vec<&'a PlanNode>;
pub fn transitive_dependents(plan: &Plan, node_id: &str) -> Vec<String>;   // visited-set, deterministic order (seed first, then BFS in plan order), includes the seed
pub fn invalidated_nodes(plan: &Plan, changed: &[String]) -> Vec<String>;  // union, first-seen order, deduped
pub fn plan_node_changed(old: &Plan, new: &Plan, node_id: &str) -> bool;   // fingerprint inequality; missing on either side = changed
pub fn diff_plans(old: &Plan, new: &Plan) -> Sexpr;
// (plan-diff (added (...)) (removed (...)) (modified (...))
//   (unchanged (...)) (affected-closure (...))) — flat form; id lists
//   in new-plan order (added/modified/unchanged) and old-plan order
//   (removed); affected-closure from invalidated_nodes over changed =
//   added ∪ removed ∪ modified.
pub fn cache_check_node(store: &CacheStore, ir: &Ir, plan: &Plan, node: &PlanNode) -> Sexpr;
// (cache-hit (node-id ...) (key ...) (candidate ...)) |
// (cache-miss (node-id ...) (key ...))
pub fn cache_check_plan(...) -> Vec<Sexpr>;
pub fn cache_explain_node(...) -> Sexpr;   // (explanation (node-id ...) (status hit|miss) (reason valid-entry|no-cache-entry|key-mismatch) (key ...)[ (stored-key ...)])
pub fn cache_store_result(store: &mut CacheStore, ir: &Ir, plan: &Plan, node: &PlanNode, candidate: Sexpr, evidence: Sexpr, timestamp: Sexpr);
```

Key properties (oracle-pinned): identical (ir, plan, node) → identical
key across processes; changing ONE component of the material (tamper a
clone's recipe / model / a dependency's fingerprint / the ir-slice)
changes the key; `entry_valid` is pure key equality; invalidation
closure over the fixed 8-node DAG from `design-contracts` = every node
except interface-contracts... derive it from the table: dependents of
design-contracts are {transition-kernel, authorization-policy,
persistence, interface-contracts} and transitively everything — the
oracle author derives the exact closure per seed and pins it.

## F. Runner readback

- `Attempt::from_sexpr`, `RunResult::from_sexpr` — STRICT readers
  (unknown fields, wrong types, or missing required fields → `None`;
  the phase-5 gate's finding 11 lesson: silent field loss on readback
  is worse than a miss), round-trip law against `to_sexpr` outputs.
- `RunResult` gains `node_fingerprint: String` (the plan node's
  contract fingerprint at run time), serialized after `node-id`.
  `run_node` fills it; readers require it. `run-results.sexpr` output
  shape changes; no committed golden exists for it (synthesize is not
  CI-gated), record the delta in the delta doc.

## Oracle tests (Stage 1 authors AND COMMITS; implementers may not touch)

`evaluator3_oracle_test.rs`: every row of the tri-state table including
the and/or Unknown-absorption cases and not-Unknown; boolean-wrapper
equivalence (for a corpus of predicates, `eval_predicate` ==
`eval_predicate3 mapped` under the phase-6 mapping); todo.gym
end-to-end statuses re-derived under sections A+B+C (show arithmetic:
which obligations become indeterminate/passed/failed, the new summary
line, the six W406s, E601 absent); ambiguous-op test (two behaviors
`a/op` and `b/op`, step `op` → ambiguous-operation violation naming
both candidates); suffix-match uniqueness (step `create_task` matches
exactly `todo_service/create_task`); TraceStep basis field present and
symbolic for an abstract-precondition transition, checked for a
grounded one.

`cache_oracle_test.rs`: key determinism across two independent
pipeline runs; key sensitivity per material component (four tamper
cases); store/lookup/replace/clear/len; entry_valid; transitive
closure per the 8-node table for seeds design-contracts,
transition-kernel, service-handlers, acceptance-harness (derived,
pinned, includes-seed asserted); diff_plans on identical plans (all
unchanged, empty closure) and on a plan from a modified spec (change
todo's sharing_limit constant → derive which nodes' fingerprints move
— the oracle author builds the modified spec inline and derives);
cache_check hit-after-store, miss-before, explain reasons all three;
bundle fingerprint recomputes over the fingerprint-free form;
bundle_summary reads the golden's summary including indeterminate;
E601 fires for a duplicated property name and is absent from todo;
RunResult/Attempt round-trip plus strict-rejection cases (unknown
field, missing node_fingerprint).

## Stage plan

- **Stage 1 — oracle author** (Sonnet): writes and COMMITS the two
  oracle files.
- **Stage 2 — evaluator + matching + refs + bundle** (Sonnet):
  sections A–D; regenerates `todo-verify.sexpr` (the ONE sanctioned
  regeneration, reason recorded); phase-6 boolean-behavior tests and
  `verify_semantics_test.rs` must pass UNCHANGED except where a status
  legitimately became `indeterminate` — those specific assertions may
  be updated ONLY by listing each one changed and why in the report
  (they are non-oracle files).
- **Stage 3 — cache.rs + runner readback** (Sonnet, first integrator):
  sections E–F; no new CLI (caching is a library until phase 8 wires
  it); full suite green.
- **Verify loop** (Sonnet), integrator verification, **Opus gate**.

Definition of done: warning-free `-D warnings --all-targets`, full
suite green with oracle files unmodified since Stage 1's commit, fmt
clean, `todo-verify.sexpr` regenerated once and byte-stable, all other
goldens untouched, delta doc updated (bundle fingerprint contract, new
statuses, W406, run-result shape).
