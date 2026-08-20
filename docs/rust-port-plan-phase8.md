# Rust port — phase 8: assembly and promotion evidence bundles

Reference: `src/assembly.lisp` (behavioral intent; port against
`docs/ir-contract-deltas.md`, never Lamedh goldens). Process: the
committed-oracle discipline exactly as phases 4–7 (Stage 1 commits the
oracle file red, message `"phase 8 stage 1: oracle tests-of-record
(red)"`; implementers may not touch it; the integrator alone arbitrates
conflicts, with stated reasons).

Principle carried over from the reference, verbatim: **the assembler
only collects evidence; it never decides whether the evidence is
sufficient.** Promotion policy is a separate object evaluated over the
assembled bundle. No model output participates anywhere in this phase.

## A. Types (`rust/src/assembly.rs`, new)

```rust
pub struct Artifact {
    pub path: String,     // relative path from the candidate's files entry
    pub node_id: String,  // plan node that produced it
    pub digest: String,   // fingerprint_string(content) — FNV-1a, same as everywhere
    pub size: i64,        // content length in BYTES
}

pub struct TraceabilityEntry {
    pub semantic_id: String,        // IR node id
    pub kind: String,               // IR node kind
    pub plan_nodes: Vec<String>,    // plan node ids whose `inputs` contain semantic_id
    pub has_implementation: bool,   // !plan_nodes.is_empty()
    pub has_evidence: bool,         // some execution result's node_id ∈ plan_nodes
}
```

Serialization (canonical field order as listed; booleans are the
existing convention: sym `t` for true, `nil` — the empty list — for
false):

```
(artifact ((path "...") (node-id "...") (digest "fnv1a64:...") (size N)))
(traceability-entry ((semantic-id "...") (kind "...") (plan-nodes ("..." ...))
                     (has-implementation t|nil) (has-evidence t|nil)))
```

Schema global: `pub const BUNDLE_SCHEMA: &str = "gymnast.bundle/0.1";`

## B. Functions (total; no panics on any input)

```rust
pub fn collect_artifacts(results: &[ExecutionResult]) -> Vec<Artifact>;
```
For each result in input order whose `candidate` is `Some` and is a
well-formed `(candidate (...))` tagged form: read its `files` entries
(each `("path" "content")`) in order, producing one `Artifact` per
file. A result with `candidate: None`, a malformed candidate, or a
missing/empty `files` contributes nothing (never an error here — the
firewall already ruled on candidates; assembly only collects). Note
the digest is over the file CONTENT string, not the pair.

```rust
pub fn validate_artifacts(plan: &Plan, artifacts: &[Artifact]) -> Vec<Sexpr>;
```
declared = concatenation of every plan node's `may_write`, in plan-node
order; actual = the artifacts' paths in order. One
`(diagnostic ((severity error) (code untracked-artifact) ...))` per
actual path not in declared (subject = the path, message
`"artifact not declared in any plan node"`); one
`(severity warning) (code missing-artifact)` per declared path not in
actual (message `"declared artifact not produced"`). Order: all
untracked first (artifact order), then all missing (declared order).
Duplicates are NOT deduplicated (Lamedh parity: `filter` over the raw
lists).

```rust
pub fn validate_capability_edges(plan: &Plan) -> Vec<Sexpr>;
```
One `(severity error) (code prohibited-capability)` per capability that
appears in the concatenated `capabilities` of all nodes AND the
concatenated `prohibitions` of all nodes (capability order; no dedup).
Message `"capability is both used and prohibited"`.

```rust
pub fn traceability_entry(ir_node: &IrNode, plan: &Plan, results: &[ExecutionResult]) -> TraceabilityEntry;
pub fn build_traceability_map(ir: &Ir, plan: &Plan, results: &[ExecutionResult]) -> Vec<TraceabilityEntry>;
pub fn traceability_diagnostics(map: &[TraceabilityEntry]) -> Vec<Sexpr>;
```
`build_traceability_map` walks ALL IR nodes in IR order (every
partition — design, transitions, obligations, synthesis — in the same
all-nodes order the IR accessors already expose).
`traceability_diagnostics`: one `(severity warning)
(code unimplemented-semantic-node)` per entry with
`has_implementation == false` (message
`"semantic node has no implementation path"`). `has_evidence` does NOT
diagnose here (promotion checks it).

```rust
pub fn dependency_lock(plan: &Plan) -> Sexpr;
```
```
(dependency-lock ((plan-fingerprint "...")
  (node-locks ((node-lock ((node-id "...") (recipe "...") (model <sexpr>)
                           (fingerprint "..."))) ...))))
```
Node order = plan order. `model` is the node's model sexpr verbatim.

```rust
pub fn default_promotion_policy() -> Sexpr;
pub fn evaluate_promotion(policy: &Sexpr, bundle: &Sexpr) -> Sexpr;
```
Default policy:
```
(promotion-policy ((name default)
  (requires ((all-artifacts-present t) (no-untracked-artifacts t)
             (no-capability-violations t) (all-nodes-succeeded t)
             (verification-passed t) (traceability-complete t)))))
```
`evaluate_promotion` computes exactly FIVE checks over the bundle (the
`requires` list is descriptive policy metadata, Lamedh parity — the
computed checks are the contract):

| check | rule |
|---|---|
| `no-error-diagnostics` | bundle `diagnostics` contains no `(severity error)` |
| `all-nodes-succeeded` | bundle summary `failed-nodes == 0` (deferred does NOT block — Lamedh parity, documented) |
| `verification-passed` | bundle has no verification section, OR its summary `failed == 0` |
| `no-indeterminate-verification` | **DELTA (deliberate, new):** no verification section, OR summary `indeterminate == 0`. Phase 7 made undecidable verdicts honest; promotion must not launder a fully-indeterminate verification into `promote`. Record in the delta doc. |
| `traceability-complete` | every traceability entry has `has-implementation` AND `has-evidence` |

Result:
```
(promotion-result ((policy default) (decision promote|hold)
  (checks ((no-error-diagnostics t|nil) (all-nodes-succeeded t|nil)
           (verification-passed t|nil) (no-indeterminate-verification t|nil)
           (traceability-complete t|nil)))))
```
`decision` is `promote` iff all five are `t`. Missing/malformed bundle
fields evaluate their check to `nil` (fail-closed), never a panic.

```rust
pub fn assemble_bundle(ir: &Ir, plan: &Plan, results: &[ExecutionResult],
                       verification: Option<&Sexpr>) -> Sexpr;
```
```
(evidence-bundle ((schema "gymnast.bundle/0.1")
  (ir-fingerprint "...") (plan-fingerprint "...")
  (artifacts (...)) (traceability (...))
  (dependency-lock (...))
  (verification <bundle>|nil)
  (summary ((total-nodes N) (artifacts-produced N)
            (succeeded-nodes N) (failed-nodes N) (has-verification t|nil)))
  (diagnostics (...))
  (fingerprint "fnv1a64:...")))
```
- `succeeded-nodes` counts results with status `succeeded` (the Rust
  enum has no `passed`; the Lamedh `passed` arm is dead here — note in
  delta doc). `failed-nodes` counts `failed`. `deferred` counts toward
  neither.
- `diagnostics` = artifact diags ++ capability diags ++ traceability
  diags, in that order.
- `fingerprint`: phase-7 pattern verbatim — computed over the
  fingerprint-free form via a private
  `assemble_bundle_without_fingerprint`, appended as the LAST field so
  the two can never drift. (DELTA: reference bundle has no
  fingerprint; consistent with the phase-7 verification bundle.)

## C. CLI wiring

`compile` and `synthesize` gain one artifact each: after writing
`results.sexpr`, compute the verification bundle
(`compile_verification(&ir)` — already deterministic), assemble the
evidence bundle, and write `evidence-bundle.sexpr` (bundle print + the
promotion result of the default policy, as a two-element file:
`(assembly ((bundle <evidence-bundle>) (promotion <promotion-result>)))`
— one canonical printed form, trailing newline, byte-stable). The
existing artifacts and exit-code semantics are unchanged (a `hold`
decision does NOT change the exit code — compile already fails on
failed recipes; promotion is evidence, not a gate on compilation).

New golden: `rust/tests/fixtures/todo-bundle.sexpr` = the
`evidence-bundle.sexpr` produced by `compile ../examples/todo.gym`.
CI: extend the rust job's reproducible-compilation step to diff it
(the existing double-compile diff already covers byte-stability; add
the fixture comparison alongside the results golden).

## Derived pins for todo.gym (oracle author re-derives, showing arithmetic)

From the committed fixtures (`todo-ir.sexpr`, `todo-plan.sexpr`,
`todo-verify.sexpr`) and current `compile` behavior:

- 8 plan nodes; recipe nodes design-contracts, interface-contracts,
  acceptance-harness, application-assembly SUCCEED; the four
  model-class nodes are DEFERRED; 0 failed.
- Artifacts: exactly 5, in result order — design/contracts.rb,
  interfaces/contracts.rb, verification/acceptance.rb, application.rb,
  manifest.sexpr (application-assembly writes two). All paths
  `generated/...`-prefixed as declared. Digest/size: derive from
  actual emitter output; assert shape (`fnv1a64:` prefix, size > 0)
  plus EQUALITY between digest and an independently computed
  `fingerprint_string` of the file content read back from disk.
- Declared may-write paths: 10 total → missing-artifact warnings:
  exactly 5 (domain/transitions.rb, domain/authorization.rb,
  adapters/persistence.rb, adapters/schema.sexpr, service/handlers.rb);
  untracked: 0.
- Capability edges: capabilities ∩ prohibitions = ∅ → 0 errors (derive
  both sets from the plan fixture and show they are disjoint).
- Summary: total-nodes 8, artifacts-produced 5, succeeded-nodes 4,
  failed-nodes 0, has-verification t.
- Verification summary (phase 7): passed 1, failed 2, skipped 4,
  indeterminate 2 → `verification-passed` nil,
  `no-indeterminate-verification` nil.
- Traceability: derive per IR node from the fixture; pin the exact
  count of entries, the exact set of semantic-ids with
  `has-implementation nil`, and the resulting
  unimplemented-semantic-node warning count. Pin that
  `traceability-complete` evaluates from those bools (t or nil — derive,
  don't guess).
- Promotion decision: **hold** (verification-passed nil suffices;
  derive every check's value anyway).

## Edge semantics (total behavior — pin each in the oracle)

| input | behavior |
|---|---|
| empty `results` | artifacts [], summary zeros, missing = every declared path, decision from checks as computed (never a panic) |
| result with `candidate: None` | contributes no artifacts; still counts in succeeded/failed tallies by status |
| malformed candidate sexpr in a result | skipped by collect_artifacts (contributes nothing) |
| duplicate produced path across two nodes | two artifacts, both validated (no dedup) |
| `verification: None` | bundle `(verification nil)`, `has-verification nil`, verification checks vacuously t |
| policy with missing `requires` | evaluate_promotion still computes the five checks (requires is metadata) |

## Oracle tests (Stage 1 authors AND COMMITS; implementers may not touch)

`rust/tests/assembly_oracle_test.rs` (~22 tests):
01 collect_artifacts over todo compile results (count 5, order, digest
   = recomputed fingerprint of content, sizes); 01b candidate-less and
   malformed-candidate results contribute nothing;
02 validate_artifacts: todo (0 untracked / 5 missing, exact paths,
   severities, order), synthetic untracked-first ordering, no-dedup
   duplicate case;
03 validate_capability_edges: todo (0), synthetic overlap (exact
   diagnostic), synthetic duplicate overlap (no dedup);
04 traceability: todo map (pinned entry count, pinned
   has-implementation nil set, plan-nodes contents for two named
   entries, has-evidence derivation), diagnostics count;
05 dependency_lock: todo shape (8 node-locks, order, fields, plan
   fingerprint matches fixture);
06 default policy shape; evaluate_promotion over the todo bundle
   (five checks pinned, decision hold); synthetic all-green bundle
   (decision promote); fail-closed on missing summary; indeterminate>0
   forces no-indeterminate-verification nil on an otherwise green
   synthetic bundle (the check must have teeth on its own);
07 assemble_bundle: field order, schema, fingerprint self-consistency
   (recompute over fingerprint-free form and compare; mutate one
   artifact digest in a copy → fingerprint differs), byte-stability
   across two assemblies;
08 golden: compile todo → evidence-bundle.sexpr matches
   `tests/fixtures/todo-bundle.sexpr` byte-for-byte (fixture committed
   by Stage 3 after first generation, then frozen).

## Stage plan

- **Stage 1 — oracle author** (Sonnet): writes and COMMITS
  `assembly_oracle_test.rs` red, deriving every todo pin from the
  fixtures with the arithmetic shown in comments. Runs
  `cargo fmt --all` BEFORE committing (phase-7 lesson: stage-1 files
  must be fmt-clean so the verify round's fmt pass cannot touch a
  frozen oracle).
- **Stage 2 — assembly.rs** (Sonnet): sections A–B; library only; all
  oracle tests except 08 green; no CLI change; no golden change.
- **Stage 3 — CLI wiring + golden + CI** (Sonnet, first integrator):
  section C; generates and commits `todo-bundle.sexpr`; extends
  ci.yml; oracle test 08 green; full suite green.
- **Verify loop** (Sonnet), integrator verification, **Opus gate**.

Definition of done: warning-free `-D warnings --all-targets`, full
suite green with the oracle file unmodified since Stage 1's commit
(byte-identical, not merely token-identical — stage 1 commits
fmt-clean), `todo-bundle.sexpr` committed once and byte-stable across
double compiles, all prior goldens untouched, delta doc updated
(bundle fingerprint, no-indeterminate-verification check, dead
`passed` status arm, deferred-does-not-block note).
