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

## Assembly and promotion shapes (phase 8)

`assembly.rs` ports `src/assembly.lisp`'s behavioral intent
(`docs/rust-port-plan-phase8.md`, sections A-B). Deliberate deltas from
the reference, pinned by `assembly_oracle_test.rs`:

- **The evidence bundle carries a trailing `(fingerprint "fnv1a64:...")`
  field** computed over the fingerprint-free form via a private
  `assemble_bundle_without_fingerprint` — the phase-7 verification-bundle
  pattern verbatim. The reference bundle has no fingerprint.
- **`evaluate_promotion` computes a FIFTH check,
  `no-indeterminate-verification`**: no verification section, OR the
  verification summary's `indeterminate == 0`. Phase 7 made undecidable
  verdicts honest; promotion must not launder a fully-indeterminate
  verification into `promote`. The reference computes four checks. All
  checks are fail-closed: a missing or malformed bundle field evaluates
  its check to `nil` (an ABSENT or `nil` verification section is the one
  vacuous case — both verification checks hold, exactly as the
  reference's `(or (not verification) ...)` reads); a PRESENT
  verification section whose summary cannot be read fails both.
- **The reference's `passed` status arm is dead here**: the Rust
  `ExecutionStatus` enum has no `passed` variant, so `succeeded-nodes`
  counts only `succeeded`. `deferred` counts toward neither tally —
  Lamedh parity, so a deferred node does NOT block the
  `all-nodes-succeeded` promotion check (documented, deliberate).
- **Assembly diagnostics use the nested house shape**
  `(diagnostic ((severity s) (code c) (subject "...") (message "...")))`
  with severity/code as bare symbols — unlike `diag::diag_sexpr`'s flat
  span-carrying shape (assembly diagnostics have no source span). The
  reference's trailing `details` field, which duplicates `subject` at
  every call site, is not carried.
- `has_evidence` on a traceability entry is STATUS-BLIND (reference
  parity): any execution result whose node-id is in the entry's
  plan-nodes counts, deferred included.
- `Artifact.size` is content length in BYTES (the reference's `length`
  counts characters); the digest is `fingerprint_string` over the file
  CONTENT string alone, never the `(path content)` pair.
- `collect_artifacts` is total where the reference's `car`/`cadr` would
  error: a malformed candidate form, a missing/empty `files`, or an
  individual `files` entry that is not a `(string string)` pair
  contributes nothing — never a diagnostic (the firewall already ruled
  on candidates; assembly only collects).
- **CLI artifact (new; the reference has no assembly CLI stage)**:
  `compile` and `synthesize` write `evidence-bundle.sexpr` after
  `results.sexpr` — one canonically printed two-element form,
  `(assembly ((bundle <evidence-bundle>) (promotion <promotion-result>)))`,
  with a trailing newline, assembled over the deterministic execution
  results and `compile_verification`'s bundle, judged by the default
  policy. Byte-stable across compiles; pinned byte-for-byte by
  `tests/fixtures/todo-bundle.sexpr` and CI's reproducible-compilation
  diff. A `hold` decision never changes the exit code: promotion is
  evidence, not a gate on compilation.

## Phase-8 gate fixes (promotion honesty)

All deliberate, each motivated by a phase-8 gate finding and pinned by
`gate8_regression_test.rs`; the frozen oracle's promotion prints were
amended under integrator arbitration (in-file INTEGRATOR RESOLUTION
notes) because the gate mandate supersedes the plan's five-check
contract:

- **The synthesize bundle sees model outcomes** (finding 1, the
  blocker): `evidence-bundle.sexpr` is written AFTER the generative
  half, over `runner::merge_run_results(results, run_results)` — a
  `Succeeded` run becomes a `Succeeded` execution result carrying the
  firewall-ACCEPTED candidate; an `Exhausted` run becomes `Failed`
  with `candidate: None` (a rejected candidate never enters the
  ledger) and a `synthesis-exhausted` error diagnostic. The
  upstream-errors early exit still writes the deterministic-only
  bundle (models never ran; deferred is the honest state there).
- **Shadow-proof promotion reads** (finding 2): every field
  `evaluate_promotion` consults goes through `assoc_unique` — a
  duplicated key fails the affected checks closed (the same
  parser-differential rule as the strict runner readback). A new
  `verify_bundle_fingerprint` recomputes the fingerprint over the
  fingerprint-free form; promotion itself checks STRUCTURE, not
  provenance — the in-process compile/synthesize path evaluates the
  bundle it just assembled, and any consumer reading a bundle back
  from disk MUST call `verify_bundle_fingerprint` first. The
  fingerprint detects drift and corruption only: FNV-1a is unkeyed and
  stored inside the document it covers, so a deliberate tamperer can
  recompute it — authenticity against an adversary requires a keyed
  MAC or signature, which nothing in this crate provides (and the
  planned SHA-256 upgrade alone will not either).
- **Only `Succeeded` results contribute artifacts** (finding 3;
  delta — the reference collects blindly): a Failed result's
  firewall-rejected candidate is provenance, not a produced artifact,
  and must not suppress the missing-artifact warning for a path that
  was never written.
- **Six computed checks** (finding 4): `all-artifacts-present` (no
  `missing-artifact` diagnostic) is now computed — the advertised
  `requires` line has a check behind it, and a build with
  declared-but-unproduced artifacts cannot promote (the oracle's
  edge_02, which pinned `promote` over 4 never-executed nodes and 5
  missing artifacts, was the vacuous composition the gate flagged; it
  now pins `hold`). `verification-passed` additionally requires
  `passed + failed > 0`: a zero-obligation verification section is no
  evidence, and neither is an all-SKIPPED one — `skipped` means the
  verifier could not run the obligation (gate re-review residual 1).
  An explicit `(verification nil)` pair stays vacuously true; a bundle
  MISSING the pair entirely (a shape `assemble_bundle` never emits)
  fails closed.
- **`no-error-diagnostics` folds the nested verification section's
  error census in** (finding 5) via `verify::bundle_error_diagnostics`
  — an E601 or source-diagnostic error inside the verification bundle
  can no longer read as "no error diagnostics" one level up.
- **Artifact `size` is BYTES** — now pinned with multi-byte content
  (finding 6), and the four fail-closed promotion behaviors are pinned
  outside the implementer-authored module tests (finding 7).

- **Execution-result diagnostics fold into the bundle** (gate
  re-review, residual 2): bundle `diagnostics` = artifact ++
  capability ++ traceability ++ every execution result's own
  diagnostics, in result order — a merged synthesize bundle records
  WHY a node failed (`synthesis-exhausted`, recipe errors), and
  `no-error-diagnostics` means what its name says.
- **`CLAUDE_SYSTEM_PROMPT` gained a NEWLINES clause** (no longer a
  verbatim port of `$gymnast-claude-system-prompt`): the sexpr string
  grammar interprets only `\"` and `\\`, so a model writing C-style
  `\n` produces two literal characters and a one-line corrupted source
  file — observed in 3 of 4 accepted candidates on the first live
  bi-ingest synthesis. The prompt now states that real newline
  characters are required and `\n` is not an escape. Model output is
  still never rewritten; the contract is stated, not patched over.
- **The sexpr reader interprets `\n`, `\t`, `\r`** (live-synthesis
  finding, post-phase-8): two live runs showed models write C-style
  `\n` inside file-content strings regardless of prompt instruction,
  which under the keep-the-backslash rule emitted one-line corrupted
  source files. The reader now decodes the three whitespace escapes on
  untrusted input; the PRINTER is unchanged (real whitespace, escaping
  only `"` and `\`), so canonical forms, goldens, and the
  parse(print(x)) round-trip law are untouched — both spellings
  normalize to one canonical form. Truly unknown escapes still keep
  the backslash. The `.gym` surface lexer deliberately keeps the old
  rule (surface strings are ours; the reader is the model wire
  contract). Frozen-oracle pin oracle_07c amended under integrator
  arbitration with the reasoning in-file.

## Adequacy campaign shapes (phase 9)

`adequacy.rs` ports `src/adequacy.lisp`'s behavioral intent
(`docs/rust-port-plan-phase9.md`, sections A-E). Deliberate deltas from
the reference, pinned by `adequacy_oracle_test.rs`:

- **Baseline-aware detection (the one deliberate SEMANTIC delta)**: the
  reference counts a mutant killed when ANY obligation is `failed`
  after mutation. Against `todo.gym` that is vacuous — the baseline
  already has two `failed` obligations (`create_then_read`,
  `sharing_boundary`), so every mutant, including the identity
  mutation, would count as killed. Here `run_campaign` runs
  verification over the BASELINE IR once, then over each mutated IR; a
  mutant is **killed** iff some obligation is `failed` in the mutated
  results AND was not `failed` in the baseline (a NEW failure,
  including an obligation id that only exists post-mutation).
  `detecting-obligations` lists only the NEW failures' ids.
  Consequence, pinned in the oracle and the committed fixture: all
  five standard todo.gym mutants SURVIVE — the campaign result is
  `(pass nil)` with five blind spots — where the reference's rule
  would have laundered the same facts into `pass t`. That is the
  honest state of the verifier today (property/scenario `must`
  assertions are still unevaluated, recorded since phase 6).
- **Degraded status (new field, no reference counterpart)**: an
  obligation whose status moved to `indeterminate` from anything else
  is reported in the mutant-result's `degraded-obligations` — a
  visibility loss, never a detection (an undecidable verdict detects
  nothing), so it never kills a mutant. The campaign summary's
  `degraded-only` counts mutants with no new failure but at least one
  degradation.
- **Campaign fingerprint**: the `campaign-result` root uses the nested
  house convention with a trailing `(fingerprint "fnv1a64:...")`
  computed over the fingerprint-free form — the phase-7/8 artifact
  discipline verbatim. The reference result carries no fingerprint.
  `mutant-result`, `blind-spot`, `interleaving-scenario`, and
  `fault-scenario` forms stay FLAT (the phase-6 record-projection
  convention split).
- **Mutated IRs are never serialized**: `apply_mutation` is pure
  clone-and-edit IR surgery and does NOT re-fingerprint — the mutated
  value still carries the ORIGINAL `Ir.fingerprint`. A mutated IR is a
  transient verification input consumed by `run_mutant` and dropped;
  it never reaches `canonical_serialize`, a cache key, or any on-disk
  artifact, so a stale fingerprint can never be mistaken for a real
  one. Targeting is by node NAME within kind, first match only
  (reference `car`/`filter` parity); a missing target returns the IR
  unchanged — total, never a panic.
- **CLI `adequacy` subcommand (new; the reference has no adequacy CLI
  stage)**: `adequacy FILE.gym` runs the standard five-mutant campaign
  over the elaborated IR; stdout is the canonical serialization of
  `(campaign-result ...)`, stderr diagnostics as in `verify`, exit 1
  on parse/IR errors ONLY. A failing campaign (`pass nil`) is
  evidence data, exit 0 — the same rationale as `hold` in the phase-8
  evidence bundle. Pinned byte-for-byte by
  `tests/fixtures/todo-adequacy.sexpr` and CI's reproducible-adequacy
  double-run diff.
- Concurrency and fault scaffolding (`boundary_interleaving`,
  `standard_fault_scenarios`) are DATA descriptions only, reference
  parity — the campaign executes mutants, never scenarios.
