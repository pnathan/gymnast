# Gymnast

Concept-synthesis compiler in Lamedh. A programmer writes one high-level
application specification; Gymnast elaborates it into a closed semantic IR,
derives a deterministic synthesis plan, and emits bounded work packets for
deterministic recipes or small language models.

No model output may decide what the application means, what work exists, what
authority it has, or whether its own output is correct.

## Toolchain

Language: [Lamedh](https://github.com/pnathan/lamedh) (a Lisp with fexprs,
`vau` operatives, macros, `deftest`).  Runtime pinned in `LAMEDH_VERSION`
(currently v0.4.0).

```sh
scripts/bootstrap-lamedh.sh          # install pinned binary to .tools/bin/
```

## Commands

```sh
# Run all tests
.tools/bin/lamedh --test tests

# Format check (CI runs this)
find src tests examples -type f -name '*.lisp' -print0 | sort -z | xargs -0 .tools/bin/lamedh --fmt-check

# Auto-format a file
.tools/bin/lamedh --fmt <file>

# Elaborate a spec (validates closed-world diagnostics)
bin/gymnast check examples/todo.lisp todo-spec

# Print IR / plan / prompts
bin/gymnast ir examples/todo.lisp todo-spec
bin/gymnast plan examples/todo.lisp todo-spec
bin/gymnast prompts examples/todo.lisp todo-spec

# Full compilation to directory
bin/gymnast compile examples/todo.lisp todo-spec build/todo

# Reproducibility test (CI runs this)
bin/gymnast compile examples/todo.lisp todo-spec build/one
bin/gymnast compile examples/todo.lisp todo-spec build/two
diff -ru build/one build/two

# Synthesis benchmark (requires ANTHROPIC_API_KEY)
scripts/benchmark.sh
```

## File layout

```
src/
  gymnast.lisp       load unit; includes all 16 modules, defines gymnast-compile
  core.lisp          data constructors, helpers, FNV-1a fingerprinting, IR/plan node types
  surface.lisp       fexprs, vau, defspec macro, use-profile macro
  profile.lisp       versioned semantic profiles: registration, resolution, parameterization
  elaborate.lisp     surface-to-IR elaboration with closed-world diagnostics
  plan.lisp          deterministic lowering from IR to 8-node typed synthesis DAG
  candidate.lisp     candidate protocol validation / firewall
  transition.lisp    executable transition calculus: extraction, predicate eval, trace execution
  platform.lisp      platform kit registry: capability adapters per target language
  prompt.lisp        prompt compilation from node contracts
  recipe.lisp        deterministic recipe registry and executor
  runner.lisp        sandboxed small-model node runner with bounded repair
  verify.lisp        independent verification obligations and trace-equivalence checks
  cache.lisp         content-addressed caching and incremental regeneration
  assembly.lisp      assembly and promotion evidence bundles
  adequacy.lisp      adequacy campaign: mutation, concurrency, and fault injection
  serialize.lisp     canonical serialization contract and trust-boundary validation
  cli.lisp           CLI entrypoint: check, ir, plan, prompts, compile subcommands
tests/
  compiler.lisp      103 tests: front-half pipeline, platform, transitions, verification, multi-target
  core-types.lisp    48 tests: core data constructors and helpers
  transition-types.lisp  4 tests: transition record types
  recipe-types.lisp  4 tests: recipe record types
  cache-types.lisp   7 tests: cache record types
  assembly-types.lisp    5 tests: assembly record types
  synthesizer-types.lisp 13 tests: Claude subprocess synthesizer
  fixtures/golden/   golden files: ir.sexpr, plan.sexpr, prompts.sexpr, compilation.sexpr
examples/
  todo.lisp          vertical-slice Todo app (Ruby/Rails target)
  todo-go.lisp       Todo app targeting Go/stdlib
  todo-java.lisp     Todo app targeting Java/Spring
  todo-python.lisp   Todo app targeting Python/Django
  todo-rust.lisp     Todo app targeting Rust/Actix
  twitter.lisp       Twitter clone (Ruby/Rails target)
  twitter-go.lisp    Twitter clone targeting Go/stdlib
  twitter-java.lisp  Twitter clone targeting Java/Spring
  twitter-python.lisp Twitter clone targeting Python/Django
  twitter-rust.lisp  Twitter clone targeting Rust/Actix
platform/
  ruby/              Ruby platform kit: adapters, test doubles, model provider
scripts/
  benchmark.sh                 integration benchmark: 5 langs × 2 specs × 3 trials
  run-benchmark-target.lisp    per-target benchmark runner (called by benchmark.sh)
  bootstrap-lamedh.sh         install pinned Lamedh binary
  synthesis-trials.lisp        multi-language synthesis benchmark
  synthesize-enriched.lisp     enriched synthesis runner
  synthesize-java.lisp         Java synthesis runner
  synthesize-multi-target.lisp multi-target synthesis runner
  synthesize-persistence.lisp  persistence synthesis runner
  show-persistence-prompt.lisp prompt inspection utility
```

## Rust port (in progress)

A Rust reimplementation of the front half lives in `rust/` (std-only crate
`gymnast-rs`), targeting a new compact Algol 68-flavored surface language
(`.gym` files; design in `docs/surface-language.md`, example in
`examples/todo.gym`). The `.lisp` surface and Lamedh implementation remain
the reference until parity. Status: phases 1–2 done (lexer, parser, checker,
profile expansion, elaborator, canonical IR + FNV-1a fingerprints); phase 3
(planner) pending. IR shape differences from the Lamedh reference are
catalogued in `docs/ir-contract-deltas.md`; execution plans in
`docs/rust-port-plan.md` and `docs/rust-port-plan-phase2.md`.

```sh
# Rust crate (run from rust/)
cargo build            # warnings are errors in CI
cargo test             # full suite
cargo fmt --all -- --check
cargo run -- check ../examples/todo.gym
cargo run -- ir ../examples/todo.gym   # canonical IR; byte-stable, CI-diffed
```

## Compiler pipeline

1. **Surface** — fexpr/vau capture of declaration operands without evaluation
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

- All public functions prefixed `gymnast-` (e.g. `gymnast-elaborate`,
  `gymnast-plan-node-field`).
- Data is S-expression association lists: `(list 'tag (list 'key value) ...)`.
- Access via `gymnast-assoc-value`, predicates via `gymnast-tagged-p`.
- Canonical ordering via `gymnast-canonical-fields` / `gymnast-canonical-less-p`.
- Diagnostics use `(make-gymnast-diagnostic severity code subject message details)`.
- Schema versions in `$gymnast-*-schema` globals (e.g. `"gymnast.ir/0.1"`).
- Tests use `deftest` and `assert-equal` / `assert-true` / `assert-false`.
- Fingerprints use FNV-1a via `gymnast-fingerprint` / `gymnast-fingerprint-string`.
- IR nodes have stable semantic IDs: `module-name/kind/name`.
- Plan nodes have IDs: `module-name/plan/local-name`.
- Platform capabilities registered via `putp`/`getp` property system, looked up
  by target language.
- Record types declared with `defrecord`, accessed with `record-ref` and
  type-predicate `typename-p`.

## Architecture invariants

- Elaboration is pure and deterministic: identical inputs → byte-identical IR.
- The planner is not a model; models cannot modify the plan.
- Prompt text is a compiled projection of a typed node contract.
- Generated candidates are untrusted; they cannot add capabilities, write
  outside declared paths, or self-evaluate.
- Every normative semantic node must appear in at least one implementation path
  and one evidence path.
- Surface reflection ends at elaboration; planning onward consumes immutable data.
- Transition traces are bounded (default 1000 steps) and produce stable
  counterexamples for illegal state transitions.
- Verification checks invariants against both initial state and all
  post-transition states.
- Cache keys are derived from node contract fingerprint, IR-slice fingerprint,
  and dependency fingerprints for reproducible invalidation.

## Issue roadmap (dependency order)

Issues #1–#10 are closed. Issues #12, #17, #23 remain open.

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
| 17 | Update CLAUDE.md to match current codebase | Open |
| 23 | Benchmark for merges | Open |

## What is built

The complete compiler pipeline from surface through assembly, with 178 tests
across 7 test files and 10 example specifications:

- Surface capture and profile resolution
- Closed-world elaboration with semantic IDs and fingerprinting
- Deterministic 8-node synthesis planning with coverage checks
- Prompt compilation with structured projections (capabilities, state model,
  type reference, behavioral reference)
- Candidate validation firewall
- Executable transition calculus with bounded trace execution
- Characterized Ruby platform kit with adapters and test doubles
- Deterministic recipe registry and executor
- Sandboxed small-model node runner with bounded repair
- Independent verification obligations (invariant and post-transition checks)
- Content-addressed caching and incremental regeneration
- Assembly and promotion evidence bundles
- Adequacy campaign framework (mutation, concurrency, fault injection)
- Canonical serialization with trust-boundary validation
- Multi-target examples (Ruby, Go, Java, Python, Rust) for both Todo and Twitter specs
- CI: format check, test suite, Todo elaboration, reproducible compilation

## What is not built

- End-to-end generative execution (no LLM invoked in CI; runner infrastructure
  exists but is not wired to a live model endpoint)
- Cryptographic artifact hashing (FNV-1a placeholder; SHA-256 upgrade planned)
- North star architecture document (issue #12)
