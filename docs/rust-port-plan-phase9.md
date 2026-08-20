# Rust port — phase 9: adequacy campaign

Reference: `src/adequacy.lisp` (behavioral intent; port against
`docs/ir-contract-deltas.md`). Process: committed-oracle exactly as
phases 4–8 (Stage 1 commits `rust/tests/adequacy_oracle_test.rs` red,
fmt-clean, message `"phase 9 stage 1: oracle tests-of-record (red)"`;
implementers may not touch it; integrator-only arbitration).

Purpose, from the reference verbatim: passing happy-path tests is
insufficient evidence that the verifier can detect realistic synthesis
defects. This module seeds known defects into the IR, re-runs
verification, and reports which obligations detected each defect. A
campaign fails when critical mutants survive undetected.

## The one deliberate semantic delta: baseline-aware detection

The reference counts a mutant "killed" when ANY obligation is `failed`
after mutation (`gymnast-run-mutant`). Against todo.gym that is
VACUOUS: the baseline already has two `failed` obligations
(create_then_read, sharing_boundary), so every mutant — including the
identity mutation — would count as killed. That is exactly the
laundering this project keeps refusing (phase-6/7 gates: no vacuous
verdicts).

Rust rule (record in the delta doc): run verification over the
BASELINE IR once, then over each mutated IR. A mutant is **killed**
iff some obligation is `failed` in the mutated results AND was not
`failed` in the baseline (a NEW failure — including an obligation id
that only exists post-mutation). Obligations whose status moved to
`indeterminate` from anything else are reported as **degraded**
(visibility, not detection — an undecidable verdict detects nothing).
`detecting-obligations` lists only the NEW failures' ids.

Consequence, derived and pinned in the oracle: **all five standard
todo.gym mutants survive** (see the table below) — the campaign result
is `(pass nil)` with five blind spots. That is the honest state of the
verifier today: property/scenario `must` assertions are not yet
evaluated (recorded in the delta doc since phase 6), so precondition/
write-set mutations are invisible to it. The reference's vacuous rule
would have reported `pass t` on the same facts. This pin is the whole
point of the module.

## A. Types (`rust/src/adequacy.rs`, new)

```rust
pub const ADEQUACY_SCHEMA: &str = "gymnast.adequacy/0.1";

pub enum Mutation {                       // closed set, no closures
    WeakenPrecondition { behavior_name: String },  // drop all `requires` clauses
    RemoveInvariant { invariant_name: String },    // remove the node entirely
    WeakenLimit { invariant_name: String, new_limit: i64 }, // rewrite <=/< limit
    RemoveFailureMode { behavior_name: String },   // drop all `fails` clauses
    SkipStateWrite { behavior_name: String },      // :writes -> empty list
}

pub struct Mutant {
    pub id: String,
    pub class: String,        // "weaken-precondition" | "remove-invariant" | ...
    pub description: String,
    pub mutation: Mutation,
    pub critical: bool,       // constructor sets true, matching the reference
}

pub struct MutantResult {
    pub mutant_id: String,
    pub class: String,
    pub critical: bool,
    pub killed: bool,
    pub detecting_obligations: Vec<String>, // NEW failures only
    pub degraded_obligations: Vec<String>,  // moved to indeterminate (DELTA)
    pub description: String,
}
```

Targets are matched by node NAME within kind (`behavior` /
`invariant`), first match only, exactly like the reference's
`(car (filter ...))`; a missing target returns the IR unchanged (the
mutant then trivially survives — total, never a panic).

## B. Mutation application (pure IR surgery)

```rust
pub fn apply_mutation(ir: &Ir, mutation: &Mutation) -> Ir;
```

- WeakenPrecondition / RemoveFailureMode: rebuild the target node with
  clauses filtered on head `requires` / `fails` (use the existing
  clause helpers; do NOT re-fingerprint the IR — the mutated IR is a
  transient verification input, never serialized; document this).
- RemoveInvariant: drop the node from every partition it appears in.
- WeakenLimit: rewrite the `:always` field via `replace_limit`, the
  reference's recursion ported exactly: `(<= a N)`/`(< a N)` with an
  Int in third position gets the new limit; `(forall binders body)`
  recurses into the body; anything else unchanged. Total, bounded by
  the (already depth-bounded) predicate tree.
- SkipStateWrite: `:writes` becomes the empty list.

## C. Concurrency and fault scaffolding (reference parity, data only)

```rust
pub fn boundary_interleaving(ir: &Ir, boundary_count: i64) -> Option<Sexpr>;
// (interleaving-scenario (operation op) (boundary N) (steps (...)) (expected-violations 0))
// over the FIRST transition with a non-empty write set; None when there is none.
// Steps are (op "actor-N" "input-N") counting DOWN from boundary_count to 1,
// exactly the reference's recursion order.

pub fn standard_fault_scenarios() -> Vec<Sexpr>;
// four (fault-scenario (name ...) (type ...) (after ...) (expected detected))
// forms: restart-after-write/restart/acknowledged-write,
// timeout-mid-operation/timeout/operation-start,
// duplicate-delivery/duplicate-delivery/acknowledged-write,
// stale-version/stale-version/read.
```

These are DATA (scenario descriptions), not executed — same as the
reference; the campaign runs mutants only.

## D. Campaign execution

```rust
pub fn standard_todo_mutants() -> Vec<Mutant>;   // the five below, ids m1..m5
pub fn run_mutant(ir: &Ir, baseline: &[Sexpr], mutant: &Mutant) -> MutantResult;
pub fn run_campaign(ir: &Ir, mutants: &[Mutant]) -> Sexpr;
```

`run_campaign` computes the baseline (lower + verify all obligations
over the unmutated IR) ONCE, then each mutant. Result shape (nested
house convention, fingerprint over the fingerprint-free form appended
last — same discipline as every artifact since phase 7):

```
(campaign-result ((schema "gymnast.adequacy/0.1")
  (total N) (killed N) (survived N) (degraded-only N) (critical-survived N)
  (pass t|nil)
  (results ((mutant-result (mutant-id ...) (class ...) (critical t|nil)
             (killed t|nil) (detecting-obligations (...))
             (degraded-obligations (...)) (description "...")) ...))
  (blind-spots ((blind-spot (mutant-id ...) (class ...) (description "...")) ...))
  (fingerprint "fnv1a64:...")))
```

`pass` iff no critical mutant survived. `degraded-only` counts mutants
with no new failure but at least one degradation (DELTA field).
mutant-result forms are FLAT (reference record projection), the root
nested — the phase-6 convention split, already documented.

Standard todo mutants and their DERIVED outcomes (oracle author
re-derives each from the phase-7 verification semantics, showing why):

| id | mutation | outcome |
|---|---|---|
| m1 | WeakenPrecondition create_task | survived: create_task's requires were abstract (symbolic) — dropping them changes no status |
| m2 | RemoveInvariant sharing_limit | survived: its obligation vanishes; nothing else moves (a REMOVED indeterminate is not a new failure) |
| m3 | WeakenLimit sharing_limit 512 | survived: predicate stays forall-headed → still indeterminate |
| m4 | RemoveFailureMode invite_user | survived: invite steps never matched anyway (invite_distinct no-match, baseline-failed already) |
| m5 | SkipStateWrite create_task | survived: viewer_cannot_mutate still applies cleanly and passes; create_then_read still fails only on query_tasks |

→ `(total 5) (killed 0) (survived 5) (critical-survived 5) (pass nil)`,
five blind spots. Additionally pin one SYNTHETIC killed case: a
hand-built IR whose invariant `(< count 10)` is `passed` at baseline
(behavior writes elsewhere) and `failed` under
`WeakenLimit { new_limit: -1 }` (initial state 0 < -1 grounded Fails)
→ killed with exactly that obligation id detecting, proving the
detection path has teeth.

## E. CLI

New subcommand `adequacy FILE.gym` (arity like `verify`): runs the
standard campaign over the elaborated IR, stdout = canonical
serialization of the campaign result, stderr diagnostics as in
`verify`, exit 1 on parse/IR errors ONLY — a failing campaign
(`pass nil`) is evidence data, exit 0, same rationale as `hold` in
phase 8. (DELTA: the reference has no adequacy subcommand; document.)
New golden `rust/tests/fixtures/todo-adequacy.sexpr`, CI: reproducible
`adequacy` double-run diff + fixture comparison alongside the verify
gates.

## Edge semantics (pin each)

| input | behavior |
|---|---|
| mutant naming a missing behavior/invariant | IR unchanged → mutant survives, never a panic |
| empty mutant list | total 0, pass t (no critical survivors), empty results/blind-spots |
| WeakenLimit on a non-comparison predicate | predicate unchanged (replace_limit's fallthrough) |
| boundary_interleaving over an IR with no writing transitions | None |
| boundary_count <= 0 | Some scenario with empty steps (reference recursion base) |
| RemoveInvariant of a name shared by zero nodes | unchanged IR |

## Oracle tests (Stage 1 authors AND COMMITS; ~20 tests)

`rust/tests/adequacy_oracle_test.rs`:
01 apply_mutation per operator over todo IR (node-level effects:
   clauses dropped, node removed, limit rewritten inside forall,
   writes emptied; untouched nodes byte-identical);
02 replace_limit table (all six rows incl. forall recursion and
   fallthrough);
03 boundary_interleaving over todo (first writing transition =
   create_task's; steps count down; expected-violations 0) + the two
   edge rows;
04 standard_fault_scenarios exact four forms;
05 run_mutant baseline-aware semantics over the synthetic killed case
   (killed, detecting id exact) and a synthetic degraded case
   (invariant passed → indeterminate under a mutation → degraded, NOT
   killed);
06 the five standard todo mutants each survived, with empty
   detecting-obligations (derivations in comments);
07 run_campaign over todo: summary counts, pass nil, five blind-spots
   (ids/classes), fingerprint self-consistency (recompute over the
   fingerprint-free form; mutate a copy → differs), byte-stability
   across two runs;
08 golden: `adequacy ../examples/todo.gym` matches
   `tests/fixtures/todo-adequacy.sexpr` byte-for-byte (red until
   Stage 3 lands the fixture).

## Stage plan

- **Stage 1 — oracle author** (Sonnet): derives every pin from the
  committed fixtures and phase-7 semantics, arithmetic in comments;
  `cargo fmt --all` BEFORE the commit; commits only the oracle file.
- **Stage 2 — adequacy.rs** (Sonnet): sections A–D; library only; all
  oracle tests except 08 green.
- **Stage 3 — CLI + golden + CI** (Sonnet, first integrator): section
  E; generates the fixture; extends ci.yml; test 08 green; delta doc
  updated (baseline-aware detection, degraded status, campaign
  fingerprint, adequacy subcommand, mutated-IR-never-serialized note).
- **Verify loop** (Sonnet), integrator verification, **Opus gate**.

Definition of done: warning-free `-D warnings --all-targets`; full
suite green with the oracle byte-identical to Stage 1's commit;
`todo-adequacy.sexpr` committed once, byte-stable across double runs;
all prior goldens untouched; delta doc updated.
