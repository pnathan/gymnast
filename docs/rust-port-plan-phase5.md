# Rust port, phase 5: sandboxed model runner with bounded repair

Execution plan for the fifth Rust increment: porting `src/runner.lisp`
onto the phase-4 firewall, plus the API fold-ins the phase-4 gate
assigned. Process rules from phases 3–4 apply unchanged, including the
committed-oracle upgrade (Stage 1 commits its tests-of-record before
implementation, oracle files only, message
`"phase 5 stage 1: oracle tests-of-record (red)"`).

Contracts are fixed by `docs/ir-contract-deltas.md`, the committed
goldens, and the phase-3/4 plan docs. `src/runner.lisp` is behavioral
intent only.

## Scope

1. **Gate fold-ins first** (they reshape APIs the runner consumes):
   a. `Plan::node(&self, id: &str) -> Option<&PlanNode>`.
   b. `ExecutionResult::from_sexpr(&Sexpr) -> Option<ExecutionResult>`
      (round-trip with `to_sexpr`; needed to read `results.sexpr` back).
   c. Deferred execution results carry `recipe_identity` (the reference
      requires recipe identity in evidence for the trust boundary).
      `tests/fixtures/todo-results.sexpr` regenerates for this — the
      stated reason, recorded in the commit message.
   d. One shared `resolve_ir_slice(ir, inputs) -> (Vec<&IrNode>, Vec<Sexpr>)`
      (nodes + W405 warnings) used by BOTH `recipe.rs` and `prompt.rs` —
      the prompt side currently drops unresolved ids silently.
      Prompt-package shape is unchanged when nothing is unresolved
      (todo.gym goldens byte-stable); when something is, the warnings
      surface through the caller, not inside the package.
   e. `is_unsafe_output_path` moves from `main.rs` into the library
      (`candidate::is_unsafe_output_path`), gains rejection of `\\` and
      of any component that IS `..` (keeping the current
      contains-based over-rejection is fine), and gets unit tests.
   f. `recipe::build_candidate` becomes `pub(crate)`-visible to the
      runner module (repair never rebuilds candidate framing by hand).
   g. Assembly emitter selects output paths by role (`.sexpr` suffix →
      manifest) instead of sorted position. todo.gym output bytes are
      unchanged (assert against the results golden).
   h. The firewall performs no full-candidate clone (borrow in
      `candidate_diagnostics`; public API otherwise unchanged).
2. **`runner.rs`** — the bounded generate→firewall→repair loop.
3. **CLI `synthesize`** — the live path (Claude subprocess provider),
   never exercised in CI.

Out of scope: verification obligations, caching, assembly evidence
bundles, adequacy (phases 6+). No live model call occurs in any test
or CI step — the runner's tests use the scripted provider exclusively.

## A. runner.rs

```rust
use crate::plan::{Plan, PlanNode};
use crate::prompt::PromptPackage;
use crate::sexpr::Sexpr;

/// One prepared model request: a pure projection of a prompt package.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelRequest {
    pub node_id: String,
    pub prompt_text: String,
    pub model_policy: Sexpr,
    pub prompt_fingerprint: String,
}

/// A model provider. The runner calls `synthesize` and treats the
/// returned string as UNTRUSTED DATA: parsed, never evaluated.
/// `None` means the provider itself failed (subprocess error, etc.).
pub trait Provider {
    fn synthesize(&mut self, request: &ModelRequest) -> Option<String>;
}

/// Deterministic scripted provider for tests: returns its responses in
/// order, then `None` forever.
pub struct ScriptedProvider { /* responses: Vec<Option<String>>, cursor */ }

/// Attempt provenance. `response_fingerprint` is the FNV-1a of the raw
/// response text ("" when the provider returned None) — a delta from
/// the Lamedh reference (which records length only), recorded in
/// docs/ir-contract-deltas.md; response_length is kept too.
#[derive(Debug, Clone, PartialEq)]
pub struct Attempt {
    pub number: u32,
    pub prompt_fingerprint: String,
    pub response_length: i64,
    pub response_fingerprint: String,
    pub diagnostics: Vec<Sexpr>,
    pub status: AttemptStatus,           // Accepted | Rejected
}

#[derive(Debug, Clone, PartialEq)]
pub enum RunStatus { Succeeded, Exhausted }

#[derive(Debug, Clone, PartialEq)]
pub struct RunResult {
    pub node_id: String,
    pub status: RunStatus,
    pub attempts: Vec<Attempt>,
    pub candidate: Option<Sexpr>,
}

impl RunResult {
    pub fn to_sexpr(&self) -> Sexpr;
    // (run-result ((node-id "...") (status succeeded)
    //   (attempts ((attempt ((number 1) (prompt-fingerprint "...")
    //     (response-length 123) (response-fingerprint "fnv1a64:...")
    //     (diagnostics (...)) (status accepted))) ...))
    //   (candidate (...))))          — candidate entry omitted when None
}

pub fn prepare_model_request(package: &PromptPackage) -> ModelRequest;

/// Markdown-fence tolerance, ported exactly: the substring from the
/// FIRST `(` to the LAST `)` when both exist in that order; otherwise
/// the input unchanged.
pub fn extract_sexpr(text: &str) -> &str;

/// The bounded loop. Attempt numbers are 1-based; at most
/// `max_attempts` provider calls. Per attempt: synthesize → on None or
/// unparseable response, a single `E514 model-response-unparseable`
/// diagnostic (the response is DATA: sexpr::parse over extract_sexpr,
/// never evaluation) → otherwise `candidate_diagnostics` against the
/// node → accepted iff no error-severity diagnostics. On rejection,
/// the NEXT attempt's prompt is `repair_prompt(...)` below, and its
/// package fingerprint is RECOMPUTED over the repaired package (a
/// deliberate delta from Lamedh, which lets the stale fingerprint ride
/// along; provenance must identify the prompt actually sent).
pub fn run_node(
    ir: &Ir, plan: &Plan, node: &PlanNode,
    provider: &mut dyn Provider, max_attempts: u32,
) -> RunResult;

/// All generative-class nodes of the plan, in plan order.
pub fn run_generative_nodes(
    ir: &Ir, plan: &Plan,
    provider: &mut dyn Provider, max_attempts: u32,
) -> Vec<RunResult>;
```

Repair prompt (`repair_prompt(package, diagnostics, attempt, rejected)`),
ported with the same section shape:

```
<original prompt text>

REPAIR ATTEMPT <n>
The previous candidate was rejected. Fix these issues:
- <CODE>: <message>
...

YOUR REJECTED OUTPUT:
<rejected text, truncated at 2000 bytes with "... [truncated]">

Return only the corrected candidate S-expression.
```

The rejected-output section is omitted when the response was None or
empty. Diagnostic lines come from the lowered diagnostic shapes (code
and message via `assoc`). Truncation is byte-based at a UTF-8 boundary
(round DOWN to a char boundary — never split a codepoint).

Runner invariants (each is an oracle test):
- the runner NEVER mutates the plan or the node (only the prompt text
  travels between attempts);
- the firewall is the sole acceptance authority — a tampered node
  (E513) can never yield Succeeded regardless of provider output;
- `max_attempts = 0` yields Exhausted with zero attempts, no provider
  call;
- provider responses are never evaluated, only parsed (no API even
  exists to do otherwise — the invariant test asserts a response
  containing `(defun ...)` shapes lands as data in E507 diagnostics).

## B. Claude subprocess provider

```rust
/// Invokes the `claude` CLI as a subprocess (std::process::Command,
/// argument vector — NEVER a shell string; the Lamedh reference's
/// shell-concat is a known injection hazard we do not port).
/// Stdin carries the prompt text (no temp file). Model flag from the
/// policy: a list headed `small_code_model` → "haiku"; a list headed by
/// another symbol → that symbol's text; a bare symbol/string → itself;
/// anything else → "haiku".
pub struct ClaudeSubprocessProvider { /* max run config */ }
impl Provider for ClaudeSubprocessProvider { ... }
```

The system prompt constant ports verbatim from
`$gymnast-claude-system-prompt`. Non-zero exit or spawn failure →
`None`. No test may invoke it (guard: unit tests only construct it and
check the model-flag mapping, a pure function
`claude_model_flag(&Sexpr) -> String`).

## C. CLI

```
gymnast-rs synthesize FILE.gym OUT_DIR [MAX_ATTEMPTS]
```

Pipeline: parse → elaborate → plan → prompts → execute_deterministic
(as `compile`) → `run_generative_nodes` with the Claude subprocess
provider (default MAX_ATTEMPTS 3) → write everything `compile` writes
PLUS `run-results.sexpr` (`(run-results ((run-result ...) ...))`) and
the files of every SUCCEEDED run candidate (same E511 guard, now via
the library function). Exit 1 if any run result is Exhausted or any
compile-stage error exists. NOT added to CI (requires a live model);
the CI-safe surface is unchanged.

## Oracle tests (Stage 1 authors AND COMMITS; implementers may not touch)

`runner_oracle_test.rs`:
1. `extract_sexpr`: markdown-fenced candidate → the inner sexpr;
   no parens → unchanged; `)` before `(` → unchanged; nested content
   → first-`(`-to-last-`)`.
2. First-attempt success with ScriptedProvider: Succeeded, one
   Accepted attempt, candidate present, attempt.prompt_fingerprint ==
   the package fingerprint.
3. Reject-then-accept script: two attempts (Rejected then Accepted);
   the SECOND provider call's request text contains "REPAIR ATTEMPT 2",
   the rejecting diagnostic's code, and (for a >2000-byte rejected
   response) "... [truncated]"; the second attempt's
   prompt_fingerprint differs from the first (recomputed).
4. Exhaustion: always-invalid script, max_attempts 3 → Exhausted,
   3 Rejected attempts, candidate None.
5. None-response and unparseable-response attempts each record exactly
   one E514 and continue to the next attempt.
6. Determinism: identical scripts → byte-identical
   `RunResult::to_sexpr` serializations, twice.
7. Firewall supremacy: a tampered node (mutated may_write) with a
   provider returning a perfectly-shaped candidate → every attempt
   Rejected with E513, final Exhausted.
8. `run_generative_nodes` over todo.gym: exactly the four generative
   nodes, in plan order, each provider call's node_id matching.
9. No-evaluation invariant: a response whose file content contains
   `(defun evil)` for a ruby-target node → E507 in the attempt
   diagnostics (the content reached the firewall as data).
10. `max_attempts = 0` → Exhausted, zero attempts, provider never
    called (ScriptedProvider records call count).
11. Fold-ins: `Plan::node` hit and miss; `ExecutionResult::from_sexpr`
    round-trips every entry of tests/fixtures/todo-results.sexpr;
    deferred results carry `recipe_identity`;
    `candidate::is_unsafe_output_path` unit table (`../x`, `/abs`,
    `a/../b`, `a..b.rb` over-reject documented, `back\\slash`, clean
    path);
    `claude_model_flag` mapping table per section B.

## Stage plan

- **Stage 1 — oracle author** (Sonnet): writes and COMMITS
  `runner_oracle_test.rs` (oracle files only, mandated message).
- **Stage 2 — gate fold-ins** (Sonnet): scope item 1 (a–h), regenerating
  the results golden for 1c with the reason in its report; all existing
  suites plus the fold-in oracle tests green.
- **Stage 3 — runner + provider + CLI** (Sonnet, first integrator):
  sections A, B, C; full suite green.
- **Verify loop** (Sonnet): as phase 4, oracle integrity via git diff
  against Stage 1's commit.
- Integrator verification, then the **Opus gate**.

Definition of done: warning-free `-D warnings --all-targets`, full
suite green with the oracle file unmodified since Stage 1's commit,
fmt clean, compile + goldens byte-stable, no CI step invokes a model.
