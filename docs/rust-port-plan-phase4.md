# Rust port, phase 4: sexpr reader, candidate firewall, recipes, compile

Execution plan for the fourth Rust increment: porting `src/candidate.lisp`
and `src/recipe.lisp` onto the phase-3 plan/prompt layer, plus the
required work the phase-3 review gate assigned. Process rules from
`docs/rust-port-plan-phase3.md` apply unchanged, with one upgrade:

**Process upgrade (from the phase-3 gate): Stage 1 commits.** The oracle
author commits its tests-of-record to the branch itself (message
`"phase 4 stage 1: oracle tests-of-record (red)"`, oracle files only)
before any implementation exists, so tests-first is auditable from git
history, not from agent self-reports. Later stages leave their work
uncommitted for the integrator.

The IR/plan/prompt contracts are fixed by `docs/ir-contract-deltas.md`,
the committed goldens, and the phase-3 plan doc. Lamedh sources are
behavioral intent only.

## Scope

1. **Required work from the phase-3 gate** (do these first — they change
   contracts the rest of this phase consumes):
   a. `PlanNode::contract_sexpr(&self) -> Sexpr` (public: the
      `node-contract`-headed, fingerprint-free form) and
      `PlanNode::verify_fingerprint(&self) -> bool` (recompute and
      compare) — the firewall must never hand-roll the head rewrite.
   b. Coverage evidence semantics: E403 keeps its current firing rule
      (no coverage at all) with its message corrected to
      "no implementation path"; NEW **W404 missing-evidence-path**
      (warning) fires for every node in the transitions or obligations
      partitions whose coverage list contains no verification-class
      plan node. Design-partition nodes are definitional, not normative
      — no W404. (`todo.gym` produces zero W404s: the acceptance
      harness consumes all of them. Plan golden is byte-unchanged.)
   c. A new oracle file `plan_table_oracle_test.rs` pinning the FULL
      8-node table against `docs/rust-port-plan-phase3.md`'s table:
      recipe strings, input-kind sets, capabilities, obligations,
      prohibitions, and pre-rewrite paths per node — the phase-3 gate
      showed the golden was the only guard on 72 transcription cells.
   d. Prompt section-presence: the 8 unconditional section headers each
      appear exactly once at line start in every package text.
2. **`sexpr::parse`** — a reader for the canonical printer's output and
   for untrusted model candidates.
3. **`candidate.rs`** — the candidate protocol and validation firewall.
4. **`recipe.rs`** — registry, executor, the four deterministic recipe
   emitters (Ruby target), generative deferral.
5. **CLI `compile`** — full front-half compilation to a directory, with
   the reproducible-compilation CI gate mirroring the Lamedh job.

Out of scope: the model runner and bounded repair (phase 5), platform
kits, verification obligations, caching, assembly evidence bundles.

## A. sexpr reader

```rust
/// Parse ONE S-expression from untrusted text. Total: never panics,
/// bounded recursion (depth limit 256 -> error), rejects trailing
/// non-whitespace. Accepts exactly the canonical printer's language:
/// symbols (any run of chars other than whitespace, parens, and `"`),
/// "..." strings with \" and \\ escapes, decimal integers (optional
/// leading -), lists, and `nil` reading as the empty list.
pub fn parse(text: &str) -> Result<Sexpr, String>;
```

Round-trip law: for every `Sexpr` value `v` built from the constructors,
`parse(&v.print())` == `Ok(v)` — EXCEPT `Sexpr::List(vec![])`, which
prints as `nil` and reads back as the empty list (assert that case
separately), and `Sexpr::Sym("nil")`, which is unrepresentable
round-trip (document, don't construct it). A symbol token that parses
as an i64 reads as `Int`; strings keep raw UTF-8; unknown escapes keep
the backslash (mirror of the lexer's string rule).

## B. candidate.rs

```rust
/// A parsed, UNTRUSTED model candidate. Field access is total; missing
/// fields read as empty.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub sexpr: Sexpr,                       // the raw form, retained
}

impl Candidate {
    /// Some(_) iff the value is a (candidate (...)) tagged alist.
    pub fn from_sexpr(v: Sexpr) -> Option<Candidate>;
    pub fn node_id(&self) -> Option<&str>;
    pub fn files(&self) -> Vec<(String, String)>;   // (path, content)
    pub fn implements(&self) -> Vec<String>;
    pub fn edge_uses(&self) -> Vec<String>;
    pub fn assumptions_empty(&self) -> bool;        // absent or nil
    pub fn unresolved_empty(&self) -> bool;
}

/// The firewall: every check from src/candidate.lisp, as diagnostics.
/// A model can propose data; it cannot mutate a plan node or decide
/// whether its own output is acceptable.
pub fn candidate_diagnostics(node: &PlanNode, candidate: &Sexpr) -> Vec<Sexpr>;
pub fn candidate_valid(node: &PlanNode, candidate: &Sexpr) -> bool;
```

Checks, in this order, all emitted via the shared `diag_sexpr` shape
(span (0 0) throughout — candidates have no source file):

| code | fires when |
|---|---|
| E501 invalid-candidate | value is not a `(candidate (...))` tagged alist (this one is exclusive: no further checks run) |
| E502 candidate-node-mismatch | candidate's node-id != the plan node's id |
| E503 unauthorized-output-path | a candidate file path not in `node.may_write` (one per path) |
| E504 missing-output-file | a `node.may_write` path absent from the candidate's files (one per path) |
| E505 candidate-added-assumptions | `assumptions` present and non-nil |
| E506 candidate-unresolved | `unresolved` present and non-nil |
| E507 target-language-violation | target language is not lamedh/lisp/scheme AND a file's content contains any of: `(defun `, `(defvar `, `(defmacro `, `(define `, `(lambda `, `(module `, `(setq `, `(let* ` (one per offending file, message names the path) |
| E508 undeclared-edge-use | an `edge-uses` entry not in `node.capabilities` (one per edge) |

Every code gets BOTH a firing test and a passing test in the oracle
file (the phase-3 gate demonstrated a diagnostics function that never
fires passes a fires-nothing-on-good-input suite).

## C. recipe.rs

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecipeClass { Structural, Generative, Verification, Assembly }

pub struct Recipe {
    pub name: &'static str,          // "design-contracts-v1"
    pub class: RecipeClass,
    // None for generative recipes (executed by the phase-5 runner).
    pub execute: Option<fn(&[&IrNode], &PlanNode) -> Sexpr>,  // -> candidate
}

/// Static, deterministic registry: the eight recipe names from the
/// phase-3 table, four deterministic executors + four generative
/// placeholders.
pub fn lookup(name: &str) -> Option<Recipe>;

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionStatus { Succeeded, Failed, Deferred }

#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionResult {
    pub node_id: String,
    pub status: ExecutionStatus,
    pub candidate: Option<Sexpr>,
    pub recipe_identity: Option<String>,
    pub diagnostics: Vec<Sexpr>,
}

impl ExecutionResult {
    pub fn to_sexpr(&self) -> Sexpr;
    // (execution-result ((node-id "...") (status succeeded)
    //   (candidate (...)) (recipe-identity "...") (diagnostics (...))))
    // candidate/recipe-identity entries omitted when absent;
    // (reason requires-model) entry added when status is deferred.
}

pub fn execute_recipe(ir: &Ir, node: &PlanNode) -> ExecutionResult;
pub fn execute_deterministic(ir: &Ir, plan: &Plan) -> Vec<ExecutionResult>;
```

Semantics (mirror `gymnast-execute-recipe`):
- unknown recipe name → Failed + `E509 unknown-recipe`;
- generative recipe → Deferred, reason requires-model, no candidate;
- deterministic recipe → run the executor over the node's resolved
  ir-slice (inputs order; unresolved ids are SKIPPED but each emits a
  `W405 unresolved-input` warning — never silently, per the phase-3
  gate's finding on `filter_map` slices), then run the FIREWALL over
  the produced candidate: status Succeeded only if
  `candidate_diagnostics` has no errors. Recipes are not exempt from
  the firewall — nothing is.
- non-Ruby target on a deterministic recipe → Failed +
  `E510 unsupported-target-emitter` naming the language (only the Ruby
  emitter is ported; the Lamedh protocol dispatch has the same
  practical limit).

### The four deterministic emitters

Port the emission behavior of `src/recipe.lisp` (lines 106–356) to the
Rust IR shapes per the delta doc. The emitted candidate has exactly the
`(candidate (...))` alist shape of the phase-3 output protocol, with
real content strings, `implements` = the ir-slice ids in slice order,
`edge-uses`/`assumptions`/`unresolved` all nil. Content is
deterministic; goldens pin the exact bytes — the plan pins structure:

- **design-contracts-v1** (structural): Ruby comment header
  ("# frozen_string_literal: true", "# Generated by gymnast — do not
  edit.", "# Design contracts"), then one Ruby declaration per type
  node in the slice (opaque → `X = String` style alias comment + class,
  enum → module with symbol constants, record → `Struct.new` with
  keyword_init, variant → module with member classes — follow the
  Lamedh emitter's output shapes, adapted from the Rust IR field
  shapes), then a capability-edge comment block per component/flow.
- **interface-contracts-v1** (structural): module per interface with
  one method signature comment + raising stub per operation clause,
  errors listed.
- **acceptance-harness-v1** (verification): the harness class from the
  Lamedh emitter — per behavior/invariant/acceptance a describe-style
  entry, closing with the `{ status: :pass, behaviors: N, invariants:
  M }` summary line.
- **application-assembly-v1** (assembly): two files — the boot module
  (first may_write path) and the `(manifest ...)` sexpr (second path)
  listing the application name, platform, and every ir-slice id as an
  artifact line.

Where the Lamedh emitter reads a Lamedh-only field shape, the delta doc
governs (e.g. records are `(:record ((name mode) ...))`, ops live in
interface CLAUSES not fields).

## D. CLI `compile` + CI

```
gymnast-rs compile FILE.gym OUT_DIR
```

Pipeline: parse → elaborate → plan → prompts → execute_deterministic.
Then write into OUT_DIR (creating it):

- `ir.sexpr`, `plan.sexpr`, `prompts.sexpr` — the canonical
  serializations (identical bytes to the ir/plan/prompts subcommands);
- `results.sexpr` — `(results ((execution-result ...) ...))` for all 8
  nodes (deferred ones included);
- every file of every SUCCEEDED candidate, materialized at
  `OUT_DIR/<path>` (paths are the may_write relative paths; create
  parent dirs; reject any path containing `..` or starting with `/`
  with `E511 unsafe-output-path` and skip writing it — the firewall
  constrains paths to the contract, but the filesystem write is the
  last line of defense).

stderr/exit contract identical to `ir`/`plan`. Two compiles of the same
spec into two directories must be byte-identical trees.

CI `rust` job gains:

```yaml
      - name: Reproducible compilation
        run: |
          cargo run -- compile ../examples/todo.gym /tmp/rust-build-one
          cargo run -- compile ../examples/todo.gym /tmp/rust-build-two
          diff -ru /tmp/rust-build-one /tmp/rust-build-two
```

## Oracle tests (Stage 1 authors AND COMMITS these; implementers may not touch)

`plan_table_oracle_test.rs` — scope item 1c: the full 8-row table
pinned field-by-field against the phase-3 plan doc's table (transcribe
the expected values from that doc into the test, not from plan.rs), plus
W404 (item 1b): a synthetic Ir with a behavior node and no
verification coverage... W404 needs plan-internal knowledge; simplest
oracle: assert todo.gym's plan has zero W404 diagnostics AND a
hand-built Plan-level check is NOT possible externally — the firing
path is pinned with a synthetic Ir whose obligations partition carries
a node of a kind no verification-class plan node consumes (Ir::new
does not re-partition). NOTE (integrator amendment): an earlier
revision's worked example here — "a behavior with no acceptance block
yields W404" — was a plan-doc bug the phase-4 crew correctly flagged
and refused to implement around: under the fixed kind-based table the
acceptance-harness's input kinds are a superset of every normative
kind, so elaborator-produced IR always has an evidence path by
construction, which is correct (the harness verifies behaviors and
invariants whether or not the spec declares acceptance content). W404
guards non-elaborator IR and future dynamic planning.
Also: `PlanNode::verify_fingerprint` is true for every
todo.gym plan node, and false after tampering with a cloned node's
`may_write`.

`candidate_oracle_test.rs` — for EACH of E501–E508: one test where it
fires (constructing the offending candidate sexpr by hand) and one
where the same dimension passes; plus: a fully-valid candidate for a
real todo.gym plan node yields zero diagnostics; E501 short-circuits
(an invalid tag yields exactly one diagnostic).

`recipe_oracle_test.rs` —
1. Registry totality: all 8 recipe names from the plan table resolve;
   the four generative ones have no executor; classes match.
2. Deterministic execution over todo.gym: statuses are exactly
   {design-contracts: Succeeded, interface-contracts: Succeeded,
   acceptance-harness: Succeeded, application-assembly: Succeeded,
   transition-kernel/authorization-policy/persistence/service-handlers:
   Deferred}.
3. Every succeeded candidate passes the firewall with zero diagnostics,
   writes exactly its node's may_write paths, and its `implements` list
   equals the node's inputs (slice order).
4. Determinism: two independent execute_deterministic runs serialize
   byte-identically.
5. Unknown recipe → Failed + E509 (construct a PlanNode by hand).
6. Non-ruby target → Failed + E510 (hand-built node with go target).
7. sexpr reader round-trip law over a structured corpus (every
   constructor, nesting, escapes, negative ints, nil) + parse rejects:
   unbalanced parens, trailing garbage, depth > 256, unterminated
   string — each with Err, never a panic.
8. Firewall-on-recipes: tamper one emitter output path (via a hand-built
   candidate) and assert E503/E504 fire — the firewall applies to
   recipe output, not only model output.
9. Section-presence (item 1d): every todo.gym prompt package text
   contains each of the 8 unconditional headers exactly once at line
   start.
10. compile: run via CLI into two temp dirs, assert identical file
    sets and identical bytes per file, results.sexpr contains 8
    execution-results, and generated/ contains exactly the succeeded
    candidates' files.

## Stage plan

- **Stage 1 — oracle author** (Sonnet): writes the three oracle files
  from this doc, verifies they reference only the specified API,
  COMMITS them (oracle files only) with the mandated message.
- **Stage 2 — required work + sexpr reader + candidate.rs** (Sonnet):
  scope items 1a/1b, section A, section B. Green:
  plan_table_oracle_test + candidate_oracle_test (+ all prior suites).
- **Stage 3 — recipe.rs + compile + CI** (Sonnet, first integrator):
  sections C and D; goldens for results are NOT committed (the compile
  reproducibility gate covers them); may touch any file except oracle
  bodies.
- **Verify loop** (Sonnet): as phase 3, plus oracle-integrity via
  `git diff` against Stage 1's commit — now a real check.
- Integrator verification, then the **Opus gate**.

Definition of done: warning-free `-D warnings --all-targets` build,
full suite green with oracle files unmodified since Stage 1's commit
(only integrator-reported compile fixes excepted), fmt clean, compile
reproducibility verified locally and in CI.
