# Change study: how the surface language behaves under maintenance

Five common maintenance changes were applied across the example corpus
(commit history shows each), and the pipeline's response was measured
at every stage. Method: capture check/plan/verify/adequacy state
before, apply the change the way a working engineer would, capture
after, and note what the language made easy, what it made dangerous,
and what it silently ignored. The probes deliberately include one
INCORRECT intermediate state to see whether anything catches it.

## The probes

### 1. Business-limit change (bi-ingest: daily quota 10000 → 50000)

The quota literal lives in FIVE places: the behavior's `requires`
boundary, its `fails ... when` boundary, the invariant ceiling, and
the scenario's two probe values. The change was first applied to only
the two behavior sites, leaving the invariant and scenario stale — an
internally CONTRADICTORY spec (behavior admits 49999 events; invariant
says the ceiling is 10000).

**Result: nothing noticed.** `check` exits 0; verification's summary
is byte-identical; no diagnostic class exists for cross-clause
constant drift. Two amplifiers: the invariant is forall-headed
(indeterminate — the tri-state evaluator honestly declines it, so it
cannot contradict the behavior), and the scenario's steps don't match
a transition (baseline-failed), so even the boundary probe is dead.
The full change needed 5 hand-edits.

**Verdict: the language INHIBITS this change.** There is no named
constant / `let` / profile-threading mechanism, so every business
limit is a copy-paste family, and the verifier as it stands cannot see
disagreement between the copies. This is the single highest-leverage
surface-language gap the study found.

### 2. New operation (bug-tracker: `assign_bug` cmd + behavior)

Two-line interface addition + an 11-line behavior; `check` clean on
the first try; W406 unresolved-state-ref count rose 6 → 9 (the new
behavior's reads/writes flagged as expected).

**Result: the plan did not change — 8 nodes before, 8 after** (live
confirmation of open issue #37: the plan template is fixed regardless
of spec complexity, so a third mutating operation adds zero synthesis
granularity). And the new operation acquired ZERO verification
pressure: obligations derive from acceptance/invariants/constraints,
none of which mention it, and nothing warns that `assign_bug` has no
acceptance coverage — the `coverage (every_operation, ...)` clause is
a skipped obligation, not a check.

**Verdict: the language SUPPORTS the edit (cheap, local,型-checked
names) but the toolchain under it is indifferent** — an uncovered new
operation should at least warn, and issue #37 means it cannot get its
own plan node.

### 3. Schema evolution (gantt: add `opt UserId owner` field + `blocked` enum member)

Two one-line edits; zero ripples; everything green.

**Result: frictionless locally, but identity is all-or-nothing.** The
IR fingerprint changed wholesale (fnv1a64:-2431663412487164394 →
fnv1a64:8554527482051068637), and nothing distinguishes this purely
ADDITIVE, wire-compatible change from a breaking one — a `compat`
check built on today's fingerprints would report MISMATCH either way.

**Verdict: SUPPORTS the edit; the identity model inhibits the
ecosystem around it.** This is precisely the gap the shared-domains
wire-lock design closes (append-only field identity, retired numbers)
— the probe gives it concrete motivation.

### 4. New actor with different authority (chatbot: `moderator` + moderation ops)

The grammar's one-actor-per-interface rule forced the change into a
second interface (`moderation_service = for moderator`), a second
flow, and its own behavior; `provides` accepted the list form.
~20 lines total, clean on the first check.

**Result: the constraint is a FEATURE.** Authority boundaries become
interface boundaries by construction — there is no way to quietly add
a privileged op to the end-user surface. Cost: some ceremony (a flow
clause per actor), and the acceptance block's actor names
(`session_owner`, `other_user`) are free symbols with no checked
relationship to declared actors — the binding between acceptance
actors and `actor` declarations is convention, not the closed world.

**Verdict: SUPPORTS, with the acceptance-actor loophole noted.**

### 5. Profile parameter change (todo scratch copy: `sharing_limit 256 → 512`)

Changed only the `use ... (sharing_limit 512, ...)` clause and diffed
the IR.

**Result: exactly two fragments changed — the import node's recorded
`:arguments` and the IR fingerprint. All five substantive `256`
literals (requires, fails-when, invariant, scenario, concurrency) are
untouched**, because `generate_todo_standard` ignores its arguments.
The language's sole parameterization mechanism is currently
decorative: it records intent and fingerprints it, but parameterizes
nothing.

**Verdict: INHIBITS — worse than absent, because it LOOKS like the
solution to probe 1.** A reader of todo.gym would reasonably believe
`sharing_limit 256` is the source of truth for the four 256s below it.

## Synthesis of findings

What the language did well under change: every probe's edit was LOCAL
(no cross-file coordination), name resolution caught typos
immediately, the one-actor-per-interface rule turned a security-
sensitive change into a structurally-visible one, and the deterministic
pipeline made every before/after measurable to the byte.

The inhibitors cluster into one theme: **numbers and names that mean
the same thing have no way to say so.**

1. **No named constants** (probes 1, 5). The fix and the mechanism are
   already adjacent: make profile parameters (or a spec-level
   `const sharing_limit = 256`) referenceable in predicates —
   `requires other_principal_count (pre, ...) < sharing_limit` — so
   one edit moves every occurrence and drift is unrepresentable. This
   should precede or accompany `must`-assertion evaluation: evaluated
   assertions over drifted copies would fail confusingly, while
   assertions over a shared name cannot drift.
2. **Coverage clauses should have teeth** (probe 2): `every_operation`
   is checkable TODAY against the interface's op list — a W-class
   diagnostic listing ops no acceptance clause exercises would have
   flagged `assign_bug` (and, in the flagship, `query_tasks`, whose
   lack of a behavior is why create_then_read baseline-fails).
3. **Additive vs breaking is invisible** (probe 3): already designed
   for in the shared-domains wire-lock; the probe is its evidence.
4. **Acceptance actors are unchecked symbols** (probe 4): closing that
   is a small elaborator rule (acceptance `generate` actor names must
   resolve to a declared actor or a profile-provided generator).

Ranked by leverage for the next language phase: named constants >
coverage teeth > acceptance-actor resolution (all small) — with
additive-identity belonging to the shared-domains phase where it is
already designed.
