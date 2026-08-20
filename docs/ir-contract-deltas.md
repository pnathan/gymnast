# IR contract: deltas from the Lamedh reference

The Rust IR mirrors the Lamedh IR's structure (partitions, semantic IDs,
canonical field ordering, clause-order preservation, fingerprint over the
fingerprint-free form) but is **not** byte-compatible with it, and a few
shapes differ deliberately. This file is the authoritative enumeration.
Phase-3+ plans must instruct agents to port Lamedh consumers
(`src/plan.lisp`, `src/verify.lisp`, `src/prompt.lisp`, …) against **this
contract**, not against the Lamedh golden files.

Reference points: Lamedh golden `tests/fixtures/golden/ir.sexpr`
(from `examples/todo.lisp`), Rust golden `rust/tests/fixtures/todo-ir.sexpr`
(from `examples/todo.gym`).

## Shapes that intentionally differ

| Concern | Lamedh | Rust | Note |
|---|---|---|---|
| Symbol case | upper (`TODO-SERVICE`) | source spelling (`todo_service`) | Rust preserves author case; hyphens become underscores in the surface |
| Node/alist nesting | record-printed nodes | `(ir-node ((id ...) (kind ...) ...))` — every alist is one nested list | see plan-phase2 `to_sexpr` comments |
| `:under` (constraint) | plist `(:VIRTUAL-USERS 500 ...)` | pair list `((virtual_users 500) ...)` | |
| Duration/latency units | `(MINUTES 30)` / `(MILLISECONDS 300)` | `(min 30)` / `(ms 300)` | surface unit names kept verbatim |
| `:model` (synthesis) | plist after head | `(small_code_model ((class nano) (temperature 0) ...))` — head + ONE combined pack | |
| `:target` (synthesis) | `(RUBY :FRAMEWORK RAILS)` | `(ruby rails)` | framework positional |
| Interface op `:input` | `(record (list ListId) ...)` | `(record ((list ListId) ...))` | pair list, one more nesting level |
| Scenario steps | `(GIVEN OWNER (AUTHENTICATED-OWNER))` etc. | `(given ((owner authenticated_owner)))` / `(when (invite_distinct owner 256))` / `(then succeeds)` | bindings are single-item packs |
| Field projections | `REQUEST/LIST` symbol | `request/list` symbol | same shape, case aside |
| Import node id | `module/import/<name>` | same, but profile paths contain `/`, so ids like `todo/import/oddities/profiles/todo_standard` cannot be split on `/` into three parts | parse ids as `module / kind / rest` |
| Profiles | registered at runtime via `putp`; todo profile never registered in the Lamedh repo | static registry in `rust/src/profile.rs`; `todo_standard` built in | Lamedh todo golden therefore has NO profile-generated nodes; the Rust one has 4, marked `:profile-source` |
| Diagnostics | record values | `(diagnostic (severity s) (code "C") (span a b) (message "..."))` | parse+check+elaboration, in that order, inside the IR |

## Shapes deliberately aligned with the Lamedh consumers

These were bugs found in review and fixed to match what `verify.lisp` /
`plan.lisp` read via `gymnast-keyword-value`:

- `fault` clauses carry `:after`, `:inject`, `:must` as separate keyword
  entries (never fused into one multi-word `:after` value).
- `coverage` lowers to keyword pairs: `(coverage :every_operation t ...)`.
- `:generate` pairs are *(variable generator)*, e.g.
  `((actor authenticated_editor) ...)` — the binding order `verify.lisp`
  consumes.
- Plural-by-contract fields are always lists, even with one element:
  `:provides`, `:uses`, `:grant`, `:deny`, `:modules`, `:reads`,
  `:writes`.
- Flow `:kind` uses the same vocabulary as op clauses: `command`/`query`,
  never the surface short forms.
- Calls keep call shape everywhere: `:execute ((create_task actor task))`,
  not key/value pairs.

## Surface-level deltas in `examples/todo.gym` vs `examples/todo.lisp`

- Predicate names lose the Lisp `?` suffix; where that collided with a
  declared name the predicate was renamed (`owner?` → `is_owner`).
- `LocalDate`/`ZonedDateTime` are spelled with the built-in lowercase
  names `local_date`/`zoned_datetime`.
- `todo.gym`'s profile import actually expands (see the profiles row
  above), so its IR contains four more type nodes than the Lamedh golden.

## Known not-yet-implemented (tracked for phase 3+)

- `[] <mode>` rows: reserved in the grammar doc, not lexed;
  `ModeExpr::Row` is currently unreachable.
- Declaration-level mode aliases / `opt`-headed / parameterized modes fall
  back to `:opaque` without a dedicated shape or diagnostic.
- Predicate/expression *type* checking (see the roadmap section of
  `docs/surface-language.md`).
- Arity checking for parameterized mode references (`Page (Task)` against
  `Page`'s declaration).

## Runner deltas vs `src/runner.lisp` (phase 5)

- Attempt records carry a `response-fingerprint` (FNV-1a of the raw,
  lossily-UTF-8-decoded response text) in addition to the reference's
  `response-length`; length is raw BYTES.
- Repair packages recompute their prompt fingerprint over the repaired
  text; the reference lets the original (stale) fingerprint ride along.
  Provenance must identify the prompt actually sent.
- Rejected-output truncation is byte-based, rounded DOWN to a UTF-8
  boundary; the reference truncates by character count.
- Every repair prompt is rebuilt from the ORIGINAL prompt package, never
  from the previous repaired text; the reference chains repairs, which
  compounds prompt size and accumulates re-embedded model output
  (phase-5 gate, findings 1–2).
- The rejected-output block is fenced with a nonce derived from its own
  fingerprint and line-prefixed with `> `; diagnostic lines in repair
  prompts truncate at 200 bytes and cap at 20 lines plus an elision
  line. The reference embeds both channels unbounded and unframed.
- The subprocess provider uses an argument-vector `Command` with stdin
  prompt delivery; the reference's shell-string concatenation (an
  injection hazard) is not ported. A failed/short stdin write is a
  provider failure, never a silently truncated prompt.

## Verification shapes (phase 6)

- Verification forms (`verification-obligation`, `verification-result`,
  `violation`, `divergence`, `normalized-counterexample`,
  `trace-equivalence-result`, `coverage-analysis`,
  `execution-environment`) use the FLAT alist shape
  `(tag (k v) (k v) ...)` — faithful to `src/verify.lisp`'s literal
  `(list 'tag ...)` builds — while the `verification-bundle` root uses
  the nested house convention `(tag ((k v) ...))` like every other
  fingerprinted artifact. Consumers must not assume one uniform depth
  across verification forms.
- Coverage-obligation flags keep the surface's underscore spelling
  (`every_operation`, ...), not the reference's hyphens.
- The evaluator's `<`/`<=` on non-integers is total-false; the
  reference errors. Unknown predicates and quantifiers hold
  symbolically (`true`), exactly as the reference.
- Execute steps naming bare helper ops against slash-qualified
  transition operations produce `no-matching-transition` results —
  reference behavior, pinned in `todo-verify.sexpr` (summary: 9
  obligations, 2 passed, 3 failed, 4 skipped). **SUPERSEDED in phase
  7**: see "Verification shapes (phase 7)" below — the exact-match rule
  and its `todo-verify.sexpr` pinning no longer hold.
- `verification-result` carries a `basis` field on invariant results —
  `checked` when every evaluation branch was computed, `symbolic` when
  any permissive default participated — plus an `I601
  symbolically-undecided` info diagnostic on symbolic verdicts. The
  reference has neither; without them a vacuous pass is
  indistinguishable from a real one (phase-6 gate, finding 1). Phase 7
  keeps this field and extends its meaning; see below.
- The `verification-bundle` carries `source-diagnostics` (the IR's own
  diagnostics), so a bundle over a broken spec is self-describing.
- The bundle carries NO fingerprint field, matching the reference —
  unlike every other artifact in the crate. **SUPERSEDED in phase 7**:
  the bundle now carries a `fingerprint` field, see below.
- Behavioral deltas from the reference state machine, all deliberate:
  state writes update entries IN PLACE (the reference's put-assoc moves
  the key to the tail, so state-key ordering differs and is
  load-bearing for state-mismatch comparison); trace violations append
  uniformly in order (the reference front-conses no-match violations
  but appends invariant violations); a state entry holding nil
  evaluates to nil (the reference's `(or value expr)` falls back to
  the SYMBOL when the value is nil).
- `ensures` postconditions are extracted but never checked by
  `(succeeded)` outcomes — same as the reference; recorded here so the
  honesty boundary is explicit.

## Verification shapes (phase 7)

`docs/rust-port-plan-phase7.md` sections A-D, deliberately changing
phase-6 semantics with the phase-6 gate's findings as the motivating
record (findings 1, 4, and 5).

- **Tri-state evaluator.** `transition::eval_predicate3` returns
  `Verdict::{Holds, Fails, Unknown}` — `Unknown` wherever the phase-6
  boolean evaluator's permissive defaults used to fire silently.
  `eval_predicate`/`eval_predicate_basis` are reimplemented over it and
  preserve phase-6's exact boolean behavior for every predicate shape
  (pinned by `evaluator3_oracle_test.rs`'s corpus, including the
  order-dependent `and`/`or` short-circuit cases); `not`/`and`/`or`
  keep the original checked-flag-threading recursion rather than being
  expressed over `Verdict` directly, because `Verdict`'s and/or
  combination is deliberately ORDER-INDEPENDENT while phase-6's
  `checked` propagation is ORDER-DEPENDENT (an item after a
  short-circuiting one is never evaluated). Only the leaf operators
  (`=`, `<`, `<=`, and the two permissive-default cases) are expressed
  directly over `eval_predicate3`.
- **Invariant obligations** now yield a new `indeterminate` status
  (never `passed`/`failed`) when the deciding check point's verdict is
  `Unknown` — checking stops at the FIRST check point (initial state,
  then each transition's post-state, in order) whose verdict is not
  `Holds`: `Fails` there decides `failed`, `Unknown` decides
  `indeterminate`. `basis` is now always `checked` on `passed`/`failed`
  and always `symbolic` on `indeterminate` (previously a permissive
  default could still produce a `passed`/`failed` verdict with
  `basis symbolic` — the phase-6 gate's findings 1 and 4). An
  `indeterminate` result carries no counterexample (same empty-list
  shape as `passed`) and the same `I601 symbolically-undecided`
  diagnostic phase 6 attached to a symbolic verdict.
- **Operation matching** in `execute_trace` is now the SUFFIX rule: a
  step op `s` matches a transition operation `op` when `op == s` OR
  `op` ends with `"/" + s`. Zero matches is unchanged
  (`no-matching-transition`). MORE than one match is a NEW outcome,
  `(ambiguous-operation s)`, with a violation `(violation (type
  ambiguous-operation) (operation s) (candidates (op1 op2 ...)))` —
  state is left unmutated and no invariant re-check runs. This makes
  `todo.gym`'s bare-helper-name execute steps (`create_task`,
  `invite_distinct`) actually reach their slash-qualified transitions
  where a unique suffix exists, superseding the phase-6 pinning above:
  `todo-verify.sexpr`'s summary is now `(total 9) (passed 1) (failed 2)
  (skipped 4) (indeterminate 2)`.
- **`TraceStep` gains a `symbolic: bool` field**: `true` iff any
  precondition or matched-failure-clause `:when` guard evaluated while
  producing the step rested on a permissive default. A step that never
  applied a transition (no-match / ambiguous-operation) is vacuously
  `false`. Serialized as a trailing `(basis symbolic|checked)` entry in
  `trace_step_to_sexpr`'s field list. A property/scenario
  `verification-result`'s own `basis` is `symbolic` iff any of its
  trace's executed steps was symbolic.
- **`check_transition_refs(ir) -> Vec<Sexpr>`** (`transition.rs`) emits
  one `W406 unresolved-state-ref` warning per `reads`/`writes` entry
  naming no `state` IR node (one warning per occurrence — an entry in
  both `reads` and `writes` produces two). `compile_verification` folds
  these into a new bundle field `(transition-diagnostics (...))`,
  placed after `environment-diagnostics`. `todo.gym` produces six (its
  only `state` node is `todo_state`; every `reads`/`writes` entry on
  either behavior names something else).
- **Bundle fingerprint and typed summary.** `compile_verification`
  appends `(fingerprint "fnv1a64:...")` over the bundle's
  fingerprint-free form, the same `Ir`/`Plan` discipline used
  everywhere else in the crate — superseding the phase-6 "no
  fingerprint field" note above. `verify::VerificationSummary` /
  `verify::bundle_summary` read the bundle's `summary` typed, including
  the new `indeterminate` count.
- **`E601 duplicate-obligation-id`**: after lowering, a second (and
  later) occurrence of any obligation id adds an error diagnostic to a
  NEW bundle field `(diagnostics (...))`, placed before
  `source-diagnostics` (distinct from `source-diagnostics`, which
  carries the IR's own diagnostics, unrelated to the bundle itself).
  Always present, empty when there is nothing to report (`todo.gym` has
  no duplicate obligation ids).
- Final bundle field order:
  `schema obligations results summary coverage environment-diagnostics
  transition-diagnostics diagnostics source-diagnostics fingerprint`.

## Cache shapes (phase 7, section E) — new module, no Lamedh golden

`cache.rs` ports `src/cache.lisp`'s behavioral intent (in-memory,
content-addressed cache keys, dependency-closure invalidation, plan
diffing) with no globals — the reference's `$gymnast-cache` special is
replaced by an explicit `CacheStore` value every caller threads through
— and NO CLOCK: `CacheEntry.timestamp` is always caller-supplied opaque
data. This is a brand-new module with no Lamedh byte-compatible golden
of its own (unlike `Ir`/`Plan`/the verification bundle), so its shapes
are pinned by `cache_oracle_test.rs` alone, not a fixture file.

- **`cache-key-material` is FLAT**, `(cache-key-material
  (node-fingerprint "...") (ir-slice-fingerprint "...")
  (dependency-fingerprint "...") (recipe r) (model (...)) (capabilities
  (...)))` — unlike the nested `(tag ((k v) ...))` shape every other
  fingerprinted contract in the crate uses (`PlanNode`, `PromptPackage`,
  the verification bundle). `cache_key` is the fingerprint of this flat
  form. The `cache-hit`/`cache-miss`/`explanation`/`plan-diff` result
  shapes are ALSO flat, matching the reference's literal `(list 'tag
  ...)` builds exactly (same convention `verify.rs`'s flat-vs-nested
  forms already established in phase 6).
- The dependency-fingerprint is computed over the RAW dependency-slice
  pair list, `((dep-id "fp") ...)`, with no wrapping tag — built by
  `prompt.rs`'s `pub(crate) fn dependency_slice`, extracted from
  `compile_prompt` so `cache.rs` reuses the identical builder rather
  than a second copy (never a second copy of the dependency-slice
  computation exists in the crate). The ir-slice-fingerprint DOES carry
  a wrapping tag, `(ir-slice (<node.to_sexpr()> ...))`, over the node's
  RESOLVED input slice (`resolve_ir_slice`, shared with `recipe.rs` and
  `prompt.rs`) — unresolved inputs shrink the slice and their W405s are
  the caller's own concern, not the cache key's; the key covers what was
  actually used.
- `cache_explain_node`'s `key-mismatch` reason is reachable ONLY by
  scanning the store for a stale entry sharing the queried node's
  `node_id` under a now-superseded key — `CacheStore` exposes lookup by
  key alone, and any entry found via the CURRENT key is trivially valid
  by construction, so a direct `lookup(current_key)` can never itself
  surface a mismatch (`cache_oracle_test.rs`'s file-header ambiguity 2).
- `transitive_dependents`/`invalidated_nodes` are breadth-first over
  `node_dependents` (seed first, then each frontier's direct dependents
  walked in plan/table order); a `HashSet` guards membership only and is
  never iterated to produce output, so two independent runs over the
  same `(Ir, Plan)` always agree on closure order byte-for-byte.
- `diff_plans`' `affected-closure` is `invalidated_nodes(new_plan,
  added ++ removed ++ modified)` — over the NEW plan, matching the
  reference's `(gymnast-invalidated-nodes new-plan changed)` exactly.

## Runner readback shapes (phase 7, section F)

- `RunResult` gains a `node_fingerprint: String` field — the plan
  node's contract fingerprint AT RUN TIME — filled by `run_node` from
  `node.fingerprint` and serialized as `(node-fingerprint "...")`
  IMMEDIATELY AFTER `(node-id ...)` and before `(status ...)` in
  `run-result`'s field list. `RunResult::from_sexpr` REQUIRES it: a
  run-result missing `node-fingerprint` is rejected, never defaulted —
  a stale candidate must never silently pass for a node whose contract
  has since moved. This changes `run-result`'s printed shape from phase
  5's; no committed golden exists for it (`synthesize`, the only
  producer, is not CI-gated), so this delta note is the shape's only
  pinning outside `cache_oracle_test.rs`'s round-trip oracle.
- `Attempt::to_sexpr` is now `pub` (was private to `runner.rs`), and
  both `Attempt::from_sexpr` and `RunResult::from_sexpr` are STRICT
  readers: any field key outside the exact pinned set, a missing
  required field, or a value of the wrong shape/type all yield `None`
  rather than a partially-populated or defaulted value (the phase-5
  gate's finding 11 lesson generalized to readback: silent field loss
  on read is worse than an outright miss). `RunResult`'s `candidate`
  field remains the sole optional one, present iff `Some` — unchanged
  from phase 5.

## Phase-7 gate fixes (verdict honesty in live traces)

All deliberate deltas, each motivated by a phase-7 gate finding and
pinned by `gate7_regression_test.rs`:

- **In-trace invariant checks are tri-state** (gate finding 1, the
  blocker): `execute_trace` now checks invariants after each applied
  step via `check_invariants3` (tri-state), not the boolean
  `check_invariants` (which is retained verbatim for phase-6 parity and
  its oracle). A grounded `Fails` is a violation exactly as before; an
  `Unknown` invariant contributes no violation but marks that step
  `symbolic`, so a property/scenario over the trace can never claim
  `(basis checked)` while its invariant checks were undecided. The
  reference (boolean `check-invariants` inside traces) can silently
  launder an undecidable invariant into "held"; this port refuses to.
- **`=` groundedness is qualified** (finding 2 + re-review residual):
  in `eval_predicate3`, a bare symbol that resolves through no binding
  (`eval_expr_resolved` returns it unresolved) makes the comparison
  `Unknown` unless the OTHER side is a resolved symbol value —
  `(= lost_updates 0)` (failed lookup vs Int) and
  `(= current_status open)` (BOTH sides floating) are `Unknown`, never
  a fabricated grounded verdict. The one grounded enum-literal case is
  resolved-vs-floating-literal: `(= status active)` with `status`
  bound to a symbol compares structurally. The boolean VALUE of
  `eval_predicate` is unchanged for every shape, but
  `eval_predicate_basis`'s `checked` flag now correctly reads `false`
  where the tri-state verdict is `Unknown` — so a precondition of the
  failed-lookup shape marks its step symbolic where phase 6 did not
  (the honest direction, consistent with the oracle's "Unknown implies
  checked == false" law).
- **Trace violations carry `(step-index N)`** and counterexamples pair
  each violation with the step at that index (finding 7) — a deliberate
  deviation from the reference's `(car steps)` pairing, which
  misattributes outcome/input/pre-state once traces are live. A
  violation with no usable index falls back to the first step.
- **Ambiguous-operation violations carry `(candidate-transitions
  (ids...))`** in addition to `candidates` (finding 10): two behaviors
  may declare the same operation, making the ops indistinguishable; the
  transition ids are the actionable identifiers.
- **Empty operations never match** (finding 9): `matches_operation`
  returns false when either side is empty — an empty-list step (op
  `""`) or a transition with no `:on` (operation `""`) can otherwise
  silently match and apply under `op == s` or the suffix rule.
- **Strict runner readback rejects duplicate keys** (finding 6):
  `Attempt::from_sexpr` / `RunResult::from_sexpr` return `None` when
  any key repeats — a first-wins `assoc` and any last-wins reader
  disagree about which value the record names (parser differential).
- **`verify` exits nonzero on bundle-level error diagnostics**
  (finding 8): `verify::bundle_error_diagnostics` collects
  error-severity diagnostics from the bundle's `diagnostics`,
  `transition-diagnostics`, `environment-diagnostics`,
  `source-diagnostics`, and every result's diagnostics; the CLI prints
  them to stderr and exits 1 (E601 is no longer invisible to
  automation). Warnings (W406) and infos keep exit 0.
- **I601 renamed** to `symbolically-undecided` (finding 11): nothing
  was satisfied; the verdict rests on a form the closed evaluator could
  not decide.
- **Cache hits are lookups, never acceptances** (finding 12): recorded
  in `cache.rs`'s module contract — any future wiring MUST re-run the
  candidate firewall on every hit; the key covers the node contract,
  not the candidate's conformance to it.
