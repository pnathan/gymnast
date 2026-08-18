# Rust port, phase 3: accessors, deterministic planner, prompt compiler

Execution plan for the third Rust increment: porting `src/plan.lisp` and
`src/prompt.lisp` onto the phase-2 IR. Like the earlier plans this is
over-specified on purpose — but the process rules below are new, and they
are not optional. They encode what the phase-1/2 retrospective measured.

## Process rules (learned, mandatory)

1. **The IR contract is fixed.** Implementations read the IR exactly as
   described in `docs/ir-contract-deltas.md` and the golden fixture
   `rust/tests/fixtures/todo-ir.sexpr`. Do NOT consult the Lamedh golden
   files for IR shapes; consult `src/plan.lisp`/`src/prompt.lisp` only
   for *behavioral intent*, then apply the delta table. Every "which
   shape?" question is answered by the delta doc; if it is not, that is
   a plan bug — note it in your report, pick the delta-doc-consistent
   reading, and do not invent.
2. **Tests-of-record are authored before and independently of the
   implementation.** Stage 1 writes `rust/tests/plan_oracle_test.rs` and
   `rust/tests/prompt_oracle_test.rs` from THIS DOCUMENT ONLY, against
   the public API specified below, with no implementation to peek at.
   Implementation stages MUST NOT edit any `*_oracle_test.rs` file or
   any file under `rust/tests/fixtures/`. The verify stage runs
   `git diff --name-only` and fails the round if an implementation agent
   touched them. If an oracle test seems wrong, the implementer reports
   the conflict; only the integrator resolves it.
3. **Every MUST below has a checkable predicate.** If you find a
   requirement in this plan you cannot express as an assertion, report
   it — do not approximate it in prose or comments. A comment is not an
   implementation.
4. **Progress and bounds.** Every loop consumes input every iteration;
   no recursion without a depth bound; no panics on any input (the
   planner's input is an `Ir` value, which may carry error diagnostics —
   handle it, don't unwrap it).
5. **Golden fixtures are generated once by the integrator's command**,
   never hand-edited, and regenerated only with a stated reason.

## Scope

Extend `rust/` (still std-only) with:

1. **Accessor layer** (deferred from earlier reviews) — `sexpr.rs` and
   `ir.rs` convenience methods, prerequisite for everything else.
2. **`plan.rs`** — deterministic lowering from `Ir` to the 8-node typed
   synthesis DAG with dependency and coverage diagnostics.
3. **`prompt.rs`** — prompt packages compiled as pure projections of
   plan-node contracts.
4. **CLI**: `plan` and `prompts` subcommands; golden fixtures;
   determinism and CI gates.

Out of scope: candidate validation, the transition calculus, recipes,
the runner, platform kits (capability contracts project name-only in
phase 3 — see the prompt section), verification, caching.

## A. Accessor layer

In `sexpr.rs` (add; do not change existing behavior):

```rust
impl Sexpr {
    pub fn as_sym(&self) -> Option<&str>;      // Sym(s) => Some(s)
    pub fn as_str(&self) -> Option<&str>;      // Str(s) => Some(s)
    pub fn as_int(&self) -> Option<i64>;
    pub fn as_list(&self) -> Option<&[Sexpr]>;
    /// Alist lookup over a List of (key value) pairs: returns the value
    /// of the first pair whose head symbol equals `key`. None otherwise.
    pub fn assoc(&self, key: &str) -> Option<&Sexpr>;
}
```

In `ir.rs`:

```rust
impl IrNode {
    /// Field lookup by exact key (keys carry the leading colon:
    /// `node.field(":owner")`).
    pub fn field(&self, key: &str) -> Option<&Sexpr>;
}

impl Ir {
    pub fn nodes_of_kind(&self, kind: &str) -> Vec<&IrNode>;
    pub fn find_node(&self, id: &str) -> Option<&IrNode>;
}
```

Refactor `Ir::has_errors` to use the accessors (behavior unchanged).

## B. plan.rs

### Types — copy verbatim

```rust
use crate::sexpr::Sexpr;

/// One node contract in the synthesis DAG. All list fields are sorted
/// at construction (byte-wise for strings, by canonical print for
/// Sexprs); the fingerprint is computed over the canonical contract
/// form at construction and stored.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanNode {
    pub id: String,                  // "<module>/plan/<local-name>"
    pub class: String,               // "structural" | "generative" | "verification" | "assembly"
    pub recipe: String,              // e.g. "design-contracts-v1"
    pub inputs: Vec<String>,         // IR node ids, sorted
    pub depends_on: Vec<String>,     // plan node ids, sorted
    pub target: Sexpr,               // from the synthesis node, verbatim
    pub model: Sexpr,                // model policy or Sym("none")
    pub may_write: Vec<String>,      // output paths, sorted
    pub capabilities: Vec<String>,   // sorted
    pub obligations: Vec<String>,    // sorted
    pub prohibitions: Vec<String>,   // sorted
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub schema: String,              // "gymnast.plan/0.1"
    pub ir_fingerprint: String,      // copied from the input Ir
    pub target: Sexpr,
    pub nodes: Vec<PlanNode>,        // in the fixed order of the table below
    pub coverage: Vec<(String, Vec<String>)>, // (ir-node-id, [plan-node-ids]), IR order
    pub diagnostics: Vec<Sexpr>,     // lowered diagnostic shapes
    pub fingerprint: String,
}

pub fn plan(ir: &Ir) -> Plan;
```

`PlanNode::new(...)` mirrors the Lamedh constructor exactly: sort
`inputs`/`depends_on`/`may_write`/`capabilities`/`obligations`/
`prohibitions`, build the canonical contract sexpr

```
(node-contract ((id "...") (class generative) (recipe transition-kernel-v1)
  (inputs (...)) (depends-on (...)) (target (...)) (model (...))
  (may-write (...)) (capabilities (...)) (obligations (...))
  (prohibitions (...))))
```

(one nested alist list, same convention as `ir-node`), fingerprint it
with `fingerprint::fingerprint`, store.

`PlanNode::to_sexpr()`:
`(plan-node ((id "...") ... (fingerprint "fnv1a64:...")))` — the same
eleven fields plus fingerprint, in the order of the struct.

`Plan::to_sexpr()`:
`(plan ((schema "gymnast.plan/0.1") (ir-fingerprint "...") (target (...))
(nodes (...)) (coverage (((ir-id) (plan-ids...)) ...))
(diagnostics (...)) (fingerprint "...")))` — fingerprint computed over
the fingerprint-free form and appended, exactly like `Ir`. Coverage
entries print as `("ir-id" ("plan-id" ...))` pairs.

### Target and model selection

From the FIRST synthesis node in `ir.nodes_of_kind("synthesis")` (they
are id-sorted; first = lowest id):

- `target` = the node's `:target` field verbatim, default
  `Sexpr::list(vec![Sexpr::sym("lamedh")])` when absent or when there is
  no synthesis node.
- `model` = the node's `:model` field verbatim, default
  `(small_code_model ((class nano)))` (built as Sexpr).

Target language = first element of the target list (or the whole value
if it is a bare symbol). File extension map (total, with default):
ruby→`.rb`, go→`.go`, java→`.java`, python→`.py`, typescript→`.ts`,
javascript→`.js`, rust→`.rs`, anything else→`.lisp`. Path rewriting:
a path ending in `.lisp` has that suffix replaced by the target
extension; other paths pass through.

### The eight nodes — fixed table, transcribe exactly

Node ids are `<module-name>/plan/<local>`. `inputs` = ids of every IR
node whose kind is in the listed set (from `ir.all_nodes()`, then
sorted). Kind names are the Rust IR strings. `target` is the selected
target; `model` is the selected model for generative nodes and
`Sym("none")` otherwise. Paths shown before extension rewriting.

| local | class | recipe | input kinds | depends_on (locals) | may_write | capabilities | obligations | prohibitions |
|---|---|---|---|---|---|---|---|---|
| design-contracts | structural | design-contracts-v1 | actor, type, component, flow | — | generated/design/contracts.lisp | — | well-formed-types, explicit-capability-edges | invent-product-semantics, add-dependencies |
| transition-kernel | generative | transition-kernel-v1 | type, state, behavior, invariant | design-contracts | generated/domain/transitions.lisp | clock, id-source | implements-transition-system, preserves-invariants, deterministic-under-same-input | perform-io, weaken-preconditions, invent-errors |
| authorization-policy | generative | authorization-policy-v1 | actor, flow, behavior, invariant | design-contracts, transition-kernel | generated/domain/authorization.lisp | — | deny-by-default, noninterference, owner-isolation | grant-undeclared-capabilities, reveal-resource-existence |
| persistence | generative | persistence-v1 | type, state, behavior, constraint | design-contracts, transition-kernel | generated/adapters/persistence.lisp, generated/adapters/schema.sexpr | durable-store, transactions | durable-commit, atomic-boundaries, retry-safety | perform-network-io, choose-unpinned-dependencies |
| interface-contracts | structural | interface-contracts-v1 | type, interface | design-contracts | generated/interfaces/contracts.lisp | — | complete-operation-surface, declared-errors-only | change-observable-contract |
| service-handlers | generative | service-handlers-v1 | interface, behavior, state, constraint | transition-kernel, authorization-policy, persistence, interface-contracts | generated/service/handlers.lisp | repository, identity, clock, id-source | contract-conformance, authorization-before-observation, idempotent-retries | access-filesystem, access-network, add-endpoints |
| acceptance-harness | verification | acceptance-harness-v1 | behavior, invariant, constraint, acceptance, interface, state | service-handlers | generated/verification/acceptance.lisp | — | independent-oracle, trace-equivalence, boundary-coverage, deterministic-execution | read-generated-rationale, weaken-obligations, skip-failures |
| application-assembly | assembly | application-assembly-v1 | application, import, component, synthesis, constraint | transition-kernel, authorization-policy, persistence, interface-contracts, service-handlers, acceptance-harness | generated/application.lisp, generated/manifest.sexpr | — | all-artifacts-linked, all-obligations-addressed | untracked-artifacts, undeclared-capabilities |

`nodes` keeps this table order (NOT id-sorted — plan order is the build
order; canonical validation of plan nodes covers their internal lists,
not the node sequence).

### Diagnostics

Reuse the phase-2 diagnostic Sexpr shape via a shared constructor.
Planning refuses invalid IR: if `ir.has_errors()`, return a `Plan` with
empty nodes/coverage and the single diagnostic `E401`
`invalid-ir-input` (span 0 0), and the schema/target defaults — do NOT
panic and do NOT plan over broken IR.

- **E402 missing-plan-dependency** — a node's `depends_on` entry names
  no node in the plan (span 0 0; message carries both ids). Structurally
  impossible with the fixed table; the check exists to catch table
  transcription errors and future dynamic planning.
- **E403 unplanned-semantic-node** — coverage: an IR node (from
  `all_nodes`) whose id appears in NO plan node's `inputs`. Message
  carries the ir node id. This is the project invariant "every
  normative semantic node must appear in at least one implementation
  path and one evidence path" — the coverage list itself records which
  plan nodes consume each IR node.

Coverage entries are computed for every IR node in `all_nodes()` order
(partition order, id-sorted within partitions — already deterministic).

## C. prompt.rs

A prompt package is a PURE function of `(ir, plan, node)`. No
formatting decision may depend on anything else.

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct PromptPackage {
    pub schema: String,                    // "gymnast.prompt/0.1"
    pub node_id: String,
    pub node_fingerprint: String,
    pub executor: String,                  // node.class
    pub model_policy: Sexpr,
    pub ir_slice: Vec<IrNode>,             // resolved from node.inputs, input order
    pub dependency_slice: Vec<(String, String)>, // (dep id, dep fingerprint | "missing")
    pub output_protocol: Sexpr,
    pub text: String,
    pub fingerprint: String,
}

pub fn compile_prompt(ir: &Ir, plan: &Plan, node: &PlanNode) -> PromptPackage;
pub fn compile_prompts(ir: &Ir, plan: &Plan) -> Vec<PromptPackage>;
```

`to_sexpr()` mirrors the Lamedh package:
`(prompt-package ((schema ...) (node-id ...) (node-fingerprint ...)
(executor ...) (model-policy ...) (ir-slice (...)) (dependency-slice
((id "fp") ...)) (output-protocol (...)) (text "...")))` plus appended
fingerprint over the fingerprint-free form.

### Output protocol

```
(candidate ((schema "gymnast.candidate/0.1") (node-id "...")
  (files (("path" "<valid Ruby source code>") ...))
  (implements "<ir-node-id-list>") (edge-uses nil) (assumptions nil)
  (unresolved nil)))
```

Content hints by target language (total map): ruby/go/java/python/rust
→ `<valid X source code>` with X the capitalized language name;
anything else → `<complete-content>`.

### Prompt text — section structure

Sections in this exact order, separated by blank lines; a projection
section is omitted entirely when its node set is empty:

1. Header: `GYMNAST NODE CONTRACT`, `Node: <id>`, `Recipe: <recipe>`,
   `Role: <role>` — role text per class, verbatim from the reference:
   - generative: "Produce one candidate implementation for this closed node contract."
   - verification: "Materialize the independent verifier projection. Do not inspect or trust generator rationale."
   - structural: "Apply the named deterministic compiler recipe exactly."
   - assembly (and any other): "Assemble only the declared artifacts and capability edges."
2. `TARGET` — the target sexpr printed, then the escaping/content rules
   text (port the Lamedh wording; the framework hints table ports
   verbatim including the Python string requirement; framework is the
   SECOND element of the target list under the Rust IR contract, not a
   `:framework` keyword).
3. `CAPABILITY CONTRACTS` — phase 3 has no platform kit: project each
   capability as its bare name on an indented line (this is the Lamedh
   fallback path when a capability definition is absent). A doc comment
   marks the platform-kit projection as phase 4.
4. `STATE MODEL` — one block per `state` node in the ir-slice:
   `  <name>:` then indented `Aggregate:`, `Versioning:`,
   `Consistency:`, `Durability:`, `Entities:` lines, each emitted only
   when the corresponding field (`:aggregate`, `:versioned`,
   `:consistency`, `:durability`, `:of`) is present. `Entities:` strips
   the `aggregate` head from `(aggregate A B C)` and joins the rest
   with ", ".
5. `TYPE REFERENCE` — one block per `type` node: opaque →
   `  Name (opaque <print>)`; enum → `  Name (enum): a | b | c`;
   record → `  Name (record):` then `    field: <print>` lines; variant
   → `  Name (variant): tag <print> | tag <print>`.
6. `BEHAVIORAL REFERENCE` — one block per `behavior` node, projected
   from the RUST IR shape (fields `:on`, `:reads`, `:writes`,
   `:atomic`, `:idempotency`; clauses `requires`/`ensures`/`returns`/
   `fails`/`emits`): `  <name> (<iface/op>):` then indented `Actor:`
   (binders after the op in `:on`), `Reads:`, `Writes:`,
   `Atomic scope:`, `Idempotency:`, `Preconditions:` (one printed pred
   per line from requires clauses), `Postconditions:` (from ensures),
   `Failures:` (`<error> when <pred>, preserves <sym>` with the when/
   preserves parts optional), `Emissions:` (printed emits clauses) —
   each emitted only when present.
7. `OBLIGATIONS` / `PROHIBITIONS` — one name per indented line.
8. `AUTHORIZED FILES` — the may_write list printed as a sexpr.
9. `DEPENDENCIES` — the dependency slice printed as a sexpr.
10. `OUTPUT PROTOCOL` — the protocol sexpr printed.
11. `AUTHORITATIVE INPUT (reference)` — the ir-slice printed as a sexpr
    (list of `ir-node` forms).
12. Closing instruction, verbatim: "Return only the candidate
    S-expression. Report no confidence score. If the contract is not
    locally closed, return an unresolved entry and no files."

Where this spec says "printed", use `Sexpr::print`. The golden fixture
pins the full byte-exact text; the oracle tests assert the structural
properties below, not full text.

## D. CLI, goldens, CI

- `gymnast-rs plan FILE.gym` — parse, elaborate, plan; stdout =
  `canonical_serialize(plan.to_sexpr())`; diagnostics (parse + IR +
  plan) rendered to stderr; exit 1 on any error-severity diagnostic.
- `gymnast-rs prompts FILE.gym` — same pipeline plus compile_prompts;
  stdout = canonical serialization of `(prompts ((prompt-package ...) ...))`
  (a plain list wrapper, no fingerprint of its own); same exit rules.
- Goldens: `rust/tests/fixtures/todo-plan.sexpr` and
  `rust/tests/fixtures/todo-prompts.sexpr`, generated by the integrator
  via the CLI, compared byte-exact in `golden_ir_test.rs`-style tests
  (in the oracle files) and in CI.
- CI `rust` job gains: reproducible plan + prompts (run twice, diff,
  compare to golden), and `plan` exit-code gate on a known-bad spec.

## Oracle tests (Stage 1 authors these; implementers may not touch)

`plan_oracle_test.rs` — the pre-paid invariants:

1. Determinism: two independent parse→elaborate→plan runs over
   `todo.gym` serialize byte-identically.
2. Plan/IR binding: `plan.ir_fingerprint == ir.fingerprint`.
3. Exactly 8 nodes, ids and classes exactly per the table, in table
   order.
4. Dependency closure: every `depends_on` entry is a plan node id
   (and the acceptance-harness node depends exactly on
   service-handlers, etc. — assert the full table).
5. DAG acyclicity, checked generically over `depends_on` (not assumed
   from the table).
6. Coverage totality on todo.gym: zero E403 diagnostics AND every IR
   node id appears in ≥1 coverage entry with a non-empty plan list.
7. Coverage failure path: elaborating a minimal spec, then removing a
   kind from planning inputs is not possible externally — instead
   assert E403 fires for a spec containing an IR kind no plan node
   consumes… there is none (the table covers all 13 kinds); assert
   instead that a synthetic Ir with an extra node of kind "type" and a
   fabricated id IS covered, and that the coverage list length equals
   `all_nodes().len()`.
8. Sorted-lists canonicality: for every node, inputs/depends_on/
   may_write/capabilities/obligations/prohibitions are byte-sorted.
9. Node fingerprint stability: rebuilding a PlanNode from the same
   arguments yields the same fingerprint; permuting input order yields
   the same fingerprint (sorting erases it).
10. Invalid-IR refusal: planning an Ir carrying an error diagnostic
    yields E401 and zero nodes, and the CLI exits 1.
11. Target/model selection: todo.gym's plan target is `(ruby rails)`
    and generative nodes carry the todo model; a spec with no
    synthesis node yields target `(lamedh)` and the default model;
    paths in may_write end in `.rb` for todo.gym.

`prompt_oracle_test.rs`:

1. Determinism: compile_prompts over todo.gym twice, byte-identical.
2. One package per plan node, `node_id`/`node_fingerprint`/`executor`
   match the plan node.
3. Projection totality (the anti-silent-drop invariant): for every
   package, every obligation name, every prohibition name, and every
   may_write path of its node appears verbatim in `text`; every id in
   `node.inputs` appears in the serialized ir_slice; every dependency
   id appears in the dependency slice with the depended node's actual
   fingerprint.
4. Section order: the section headers present in each package's text
   appear in the specified order.
5. Behavioral reference fidelity on todo.gym's transition-kernel
   package: contains `create_task`, its `Failures:` line contains
   `forbidden when` and `preserves all_state` — the fault-class
   regression guard at the prompt level.
6. The closing instruction is the last line of every text.
7. Prompt fingerprint recomputes over the fingerprint-free package
   form.
8. ir_slice resolution drops nothing: slice length equals
   `node.inputs.len()` for todo.gym (all ids resolve).

## Stage plan (Sonnet implementation, Opus review)

- **Stage 1 — oracle author** (writes ONLY the two oracle test files;
  they must compile against the specified API and fail red).
- **Stage 2 — accessors + plan.rs** (make plan oracle green).
- **Stage 3 — prompt.rs + CLI + CI** (make prompt oracle green;
  generate goldens per Section D; first integrator).
- **Verify loop** — fmt/build/test; enforces oracle-file immutability
  via git; never weakens a test.
- Integrator (main session) verifies first-hand, then an **Opus review
  sweep** runs before the work is considered done.

Definition of done: `cargo build` warning-free, full suite green
including both oracle files unmodified since Stage 1 (allowing only
compile-fix edits explicitly reported and re-reviewed), `cargo fmt
--check` clean, goldens byte-stable across runs, CI updated.
