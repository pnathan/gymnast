# Gymnast

Concept-synthesis compiler in Rust. A programmer writes one high-level
application specification; Gymnast elaborates it into a closed semantic IR,
derives a deterministic synthesis plan, and emits bounded work packets for
deterministic recipes or small language models.

No model output may decide what the application means, what work exists, what
authority it has, or whether its own output is correct.

## Toolchain

Implementation: Rust, std-only crate `gymnast-rs` in `rust/`, zero
dependencies, `#![forbid(unsafe_code)]`. Surface language: a compact
Algol-68-flavored language (`.gym` files; design in
`docs/surface-language.md`).

## Commands

```sh
# Rust crate (run from rust/)
cargo build            # warnings are errors in CI
cargo test              # full suite
cargo fmt --all -- --check
cargo run -- check ../examples/todo.gym
cargo run -- ir ../examples/todo.gym       # canonical IR; byte-stable, CI-diffed
cargo run -- plan ../examples/todo.gym     # 8-node synthesis DAG
cargo run -- prompts ../examples/todo.gym  # compiled prompt packages
cargo run -- verify ../examples/todo.gym   # verification bundle (golden-pinned)
cargo run -- adequacy ../examples/todo.gym # mutation campaign (golden-pinned)
cargo run -- compile ../examples/todo.gym /tmp/build   # + evidence-bundle.sexpr
cargo run -- synthesize ../examples/todo.gym /tmp/out 3  # LIVE model; never in CI
```

No test or CI step ever invokes a model.

## File layout

```
rust/                gymnast-rs: the compiler (std-only, zero dependencies)
  src/
    main.rs            CLI entrypoint: parse, check, ir, plan, prompts, verify,
                       adequacy, compile, synthesize
    lexer.rs           hand-rolled lexer over the .gym surface
    parser.rs          recursive-descent parser producing a typed AST with spans
    ast.rs             typed surface AST
    check.rs           closed-world surface checker
    profile.rs         versioned semantic profiles: static registry, expansion
    elaborate.rs       surface-to-IR elaboration, constant substitution
    ir.rs              canonical IR: nodes, partitions, semantic IDs
    plan.rs            deterministic lowering to the 8-node synthesis DAG
    prompt.rs          prompt compilation from node contracts
    platform.rs        platform kit registry: capabilities per target language
    candidate.rs       candidate protocol validation / firewall
    recipe.rs          deterministic recipe registry and executor
    runner.rs          sandboxed model node runner with bounded repair
    transition.rs      executable transition calculus, bounded trace execution
    verify.rs          verification obligations, tri-state evaluation
    cache.rs           content-addressed caching (library-only)
    assembly.rs        assembly and promotion evidence bundles
    adequacy.rs        mutation, concurrency, and fault injection campaigns
    sexpr.rs           canonical S-expression reader/printer
    diag.rs            diagnostics; span.rs, fingerprint.rs, lib.rs
  tests/               30+ test binaries; fixtures/ holds the byte-stable goldens
platform/
  ruby/              Ruby platform kit consumed by SYNTHESIZED applications:
                     nine capability adapters, test doubles, model provider.
                     Plain Ruby, not compiler code.
scripts/
  benchmark.sh       synthesis benchmark: 5 langs x 2 specs x 3 trials, driven
                     by `gymnast-rs synthesize`. Needs ANTHROPIC_API_KEY; CI
                     never runs it.
examples/
  todo.gym           vertical-slice Todo app in the compact surface (flagship fixture)
  bug-tracker.gym    bug tracker (project members file, triage, assign)
  gantt.gym          Gantt chart tool (dependency-linked task plans)
  chatbot.gym        chatbot service
  bi-ingest.gym      BI analytics ingest server (quota-bounded event journal)
  todo-go.gym        Todo app targeting Go/stdlib
  todo-java.gym      Todo app targeting Java/Spring
  todo-python.gym    Todo app targeting Python/Django
  todo-rust.gym      Todo app targeting Rust/Actix
  twitter.gym        Twitter clone (Ruby/Rails target)
  twitter-go.gym     Twitter clone targeting Go/stdlib
  twitter-java.gym   Twitter clone targeting Java/Spring
  twitter-python.gym Twitter clone targeting Python/Django
  twitter-rust.gym   Twitter clone targeting Rust/Actix
docs/
  README.md          documentation index: what each document is and whether it binds
```

## Rust implementation (complete pipeline)

The complete pipeline lives in `rust/` (std-only crate `gymnast-rs`, zero
dependencies, `#![forbid(unsafe_code)]`), targeting a compact Algol
68-flavored surface language (`.gym` files; design in
`docs/surface-language.md`; examples `todo.gym`, `bug-tracker.gym`,
`gantt.gym`, `chatbot.gym`, `bi-ingest.gym`). The IR's shape and known
limitations are catalogued in `docs/ir-contract-deltas.md` (the
authority — consumers port against it, never against stale goldens).
Execution plans for phases 1–10 are in `docs/rust-port-plan*.md`; the
shared-domains / gRPC / OpenAPI interop design (proposed, unscheduled)
is `docs/shared-domains-design.md`. `docs/README.md` indexes every
document and says which ones bind.

The pipeline is complete, all ten phases done and Opus-gate-accepted:
lexer/parser/checker, profile expansion, elaborator, canonical IR,
deterministic 8-node planner, prompt compiler, sexpr reader (models’
wire contract: `\n`/`\t`/`\r` interpreted on read; printer canonical),
candidate firewall, recipe registry, sandboxed model runner with
bounded repair, executable transition calculus with tri-state
(`Holds`/`Fails`/`Unknown`) verification and the `indeterminate`
status, content-addressed caching (library-only), assembly and
promotion evidence bundles (six fail-closed checks; `hold`/`promote`),
the adequacy campaign with baseline-aware mutation detection
(subject-bound, inapplicability-honest), and surface v0.2 (named
constants and live profile parameters substituted at elaboration,
coverage-flag teeth, acceptance-generator actor binding —
`docs/surface-v0.2-design.md`). 736 tests across 34 binaries. Live
synthesis runs end-to-end against a real model (12/12 candidates
firewall-accepted first attempt; no test or CI step ever invokes a
model).

Process: each phase runs plan → committed-oracle Sonnet crew (oracle
tests land red before implementation; implementers may not touch them;
integrator-only arbitration with in-file notes) → Opus phase gate
(adversarial review + mutation-testing the oracles). Gate regression
tests live in `rust/tests/gate*_regression_test.rs`.

## Compiler pipeline

1. **Surface** — lexer + recursive-descent parser capture declaration
   operands without evaluation
2. **Profile resolution** — versioned semantic profiles registered by name and
   version, resolved into the elaboration context
3. **Elaboration** — closed-world validation, semantic IDs, partitioning into
   design/transition/obligation/synthesis graphs, fingerprinting
4. **Planning** — deterministic lowering to 8-node typed synthesis DAG with
   coverage and dependency checks
5. **Prompt compilation** — projects each plan node contract into a prompt
   package with capability contracts, state model, type reference, and
   behavioral reference projections
6. **Candidate validation** — firewall enforcing node identity, write sets,
   no added assumptions or unresolved entries
7. **Transition calculus** — reference state machine with bounded trace
   execution and counterexample production
8. **Recipe execution** — deterministic recipes applied by structural nodes
9. **Model runner** — sandboxed small-model node runner with bounded repair
10. **Verification** — independent verification obligations checking invariants
    against initial state and post-transition states
11. **Caching** — content-addressed caching keyed on node contract fingerprints
12. **Assembly** — linking declared artifacts into promotion evidence bundles
13. **Adequacy** — mutation, concurrency, and fault injection campaigns
14. **Serialization** — canonical serialization with trust-boundary validation
15. **CLI** — subcommands exposing each stage

## Code conventions

- One module per pipeline stage in `rust/src/`, named for the stage.
- Stage output is canonical S-expressions built with `crate::sexpr::Sexpr`;
  `Sexpr::print()` is the single canonical printer, and the reader interprets
  `\n`/`\t`/`\r` on the way in (the models' wire contract).
- IR nodes carry stable semantic IDs `module/kind/name`; plan nodes use
  `module/plan/local-name`. Parse an ID as `module / kind / rest` — profile
  paths contain `/`, so a three-way split is wrong.
- Diagnostics come from `crate::diag`, carry a source span, and use coded
  identifiers: `E1xx` lexer/parser, `E2xx` check/elaborate, `E4xx` plan stage,
  `E5xx` candidate firewall, `W4xx` warnings.
- Schema versions are string constants like `"gymnast.ir/0.1"`.
- Fingerprints use FNV-1a via `crate::fingerprint`.
- Platform capabilities live in `crate::platform`'s static registry, looked up
  by target language. Capability NAMES are lookup keys and must match the
  vocabulary `crate::plan` emits verbatim (hyphenated: `id-source`,
  `durable-store`); guarantees and failure modes are projected text and use
  underscores.
- Goldens in `rust/tests/fixtures/` are byte-stable and CI-diffed. Regenerate
  one only when a change is deliberately meant to move it, and say so.
- Tests are `#[test]` fns. Oracle tests land before the implementation they
  pin (the committed-oracle discipline), and implementers do not edit them.
- `cargo build` must pass with `RUSTFLAGS='-D warnings'`; `cargo fmt` clean.

## Architecture invariants

- Elaboration is pure and deterministic: identical inputs → byte-identical IR.
- The planner is not a model; models cannot modify the plan.
- Prompt text is a compiled projection of a typed node contract.
- Generated candidates are untrusted; they cannot add capabilities, write
  outside declared paths, or self-evaluate.
- Every normative semantic node must appear in at least one implementation path
  and one evidence path.
- The surface is closed: no user macros, no evaluation, no escape hatches.
  Extensibility is exactly profile parameterization. Planning onward consumes
  immutable data.
- Transition traces are bounded (default 1000 steps) and produce stable
  counterexamples for illegal state transitions.
- Verification checks invariants against both initial state and all
  post-transition states.
- Cache keys are derived from node contract fingerprint, IR-slice fingerprint,
  and dependency fingerprints for reproducible invalidation.
- Port declarations characterize external boundaries (provides/requires) without
  importing foreign closed worlds; interop is by contract, not by inclusion.

## Issue roadmap (dependency order)

Closed: #1–#10, #17–#21, #23, #29, #36, #38. Open: #12, #30, #31,
#32, #33, #34, #37.

| # | Title | Status |
|---|-------|--------|
| 1 | Canonical serialization and SHA-256 identity | Closed |
| 2 | Versioned semantic profiles | Closed |
| 3 | Executable transition calculus | Closed |
| 4 | Characterized Lamedh platform kit | Closed |
| 5 | Deterministic recipe registry and executor | Closed |
| 6 | Sandboxed small-model node runner | Closed |
| 7 | Independent verification obligations | Closed |
| 8 | Content-addressed caching and incremental regen | Closed |
| 9 | Assembly and promotion evidence bundles | Closed |
| 10 | Adequacy campaign (mutation/concurrency/fault) | Closed |
| 12 | North star architecture document | Open |
| 17 | Update CLAUDE.md to match current codebase | Closed |
| 18 | Prompt capability contracts hardcoded to Ruby platform | Closed |
| 19 | Invariant verification only checks initial state | Closed |
| 20 | Remove dead code: canonical-data and unused bindings | Closed |
| 21 | Move gymnast-keyword-value to core.lisp | Closed |
| 23 | Benchmark for merges | Closed |
| 29 | Coverage-gap flags silently ignored | Closed |
| 30 | Promotion ignores verification results and policy | Open |
| 31 | Dead `(none)` sentinel checks in firewall and prompts | Open |
| 32 | CLI double-elaborates spec on `compile` | Open |
| 33 | Dead code: 9 functions with zero callers | Open |
| 34 | Consolidate duplicated patterns and accessors | Open |
| 36 | Unknown profile imports → hard error | Closed |
| 37 | Fixed 8-node plan template regardless of spec complexity | Open |
| 38 | Component port declarations for external boundaries | Closed |

## What is built

The complete compiler pipeline, in Rust (`rust/`): surface through
adequacy, plus deliberate hardening deltas and surface v0.2. 736 tests
across 34 binaries, 14 `.gym` example specifications, eight
byte-stable goldens (ir/plan/prompts/verify/results/bundle/adequacy +
reproducible compile trees), all CI-diffed. Verification is
tri-state-honest (no vacuous passes, no fabricated failures),
promotion is fail-closed, the adequacy campaign is subject-bound and
reports the verifier’s real blind spots (todo.gym today: all five
standard mutants survive — `pass nil` — because must-assertions are
not yet evaluated).

The Rust implementation provides:

- A compact `.gym` surface with span-carrying diagnostics from a
  hand-rolled lexer and recursive-descent parser
- Versioned semantic profile resolution
- Closed-world elaboration with semantic IDs and fingerprinting
- Deterministic 8-node synthesis planning with coverage checks
- Prompt compilation with structured projections (capabilities, state model,
  type reference, port boundaries, behavioral reference)
- Candidate validation firewall
- Executable transition calculus with bounded trace execution
- Characterized platform kit with adapters per target language
- Deterministic recipe registry and executor
- Sandboxed small-model node runner with bounded repair
- Independent verification obligations (invariant and post-transition checks),
  tri-state (`Holds`/`Fails`/`Unknown`) with an `indeterminate` status
  rather than vacuous passes
- Content-addressed caching and incremental regeneration
- Assembly and fail-closed promotion evidence bundles (`hold`/`promote`
  over six checks)
- Adequacy campaign framework (mutation, concurrency, fault injection)
- Canonical serialization with trust-boundary validation
- Surface v0.2: named constants and live profile parameters,
  substituted at elaboration so that a const-spelled spec and its
  literal-spelled twin produce identical downstream results;
  coverage-flag enforcement against the declared interface; acceptance
  generators bound to declared actors
- Multi-target examples (Ruby, Go, Java, Python, Rust) for both Todo and Twitter specs
- CI: fmt, warnings-as-errors build, tests, known-bad-spec rejection,
  and byte-diffed goldens for ir/plan/prompts/verify/results/bundle/
  adequacy plus reproducible compile trees

## What is not built

- Acceptance `must`-assertion evaluation (the adequacy campaign’s
  measured blind spot — the natural next verification phase)
- Shared domain compilation units and gRPC/OpenAPI interop projections
  (designed in `docs/shared-domains-design.md`, not implemented)
- Cryptographic artifact hashing (FNV-1a placeholder; SHA-256 upgrade
  planned — note: an unkeyed hash is drift evidence, not
  authentication, and the upgrade alone will not change that)
- Cache CLI wiring (cache.rs is library-only; any future hit path MUST
  re-run the candidate firewall — the key covers the contract, not the
  candidate’s conformance)
- Component port declarations for external interface boundaries
  (REST, gRPC, GraphQL, message queues — provides/requires contracts)
- North star architecture document (issue #12)
- Open code-health items: promotion policy wiring (#30), dead sentinel
  checks (#31), CLI double-elaboration (#32), dead functions (#33),
  duplicated accessor patterns (#34), spec-proportional planning (#37)
