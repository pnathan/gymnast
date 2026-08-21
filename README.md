# Gymnast

Gymnast is an experimental concept-synthesis compiler. A programmer
describes an application once, at a high level; Gymnast elaborates that
description into a closed semantic IR, derives a deterministic synthesis
plan, and emits bounded work packets for deterministic recipes or small
language models.

The durable unit is the typed node contract, not its prompt. Generated
candidates cannot change the plan, add capabilities, write outside their
declared paths, or decide whether they satisfy the specification.

> No model output may decide what the application means, what work
> exists, what authority it has, or whether its own output is correct.

## Implementation

Gymnast is implemented in Rust: `rust/` (crate `gymnast-rs`), std-only,
`#![forbid(unsafe_code)]`, no dependencies. The surface language is
compact and Algol-68-flavored (`.gym` files). 736 tests across 34
binaries.

The implementation covers the complete pipeline: surface → elaboration
→ planning → prompt compilation → candidate firewall → transition
calculus → recipes → model runner → verification → caching → assembly
→ adequacy.

The IR's shape and known limitations are catalogued in
[`docs/ir-contract-deltas.md`](docs/ir-contract-deltas.md), which is
the authority for anyone changing the IR or anything downstream of it.

## Pipeline

1. **Surface** — declaration capture without evaluation, via a
   hand-rolled lexer and recursive-descent parser with source spans
2. **Profile resolution** — versioned semantic profiles registered by
   name and version, resolved into the elaboration context
3. **Elaboration** — closed-world validation, stable semantic IDs,
   partitioning into design/transition/obligation/synthesis graphs,
   fingerprinting
4. **Planning** — deterministic lowering to a typed synthesis DAG with
   coverage and dependency checks
5. **Prompt compilation** — each plan-node contract projected into a
   prompt package (capability contracts, state model, type reference,
   port boundaries, behavioral reference)
6. **Candidate firewall** — node identity and write sets enforced; no
   added assumptions, no unresolved entries
7. **Transition calculus** — reference state machine, bounded trace
   execution, stable counterexamples
8. **Recipes** — deterministic recipes applied by structural nodes
9. **Model runner** — sandboxed small-model node runner with bounded
   repair
10. **Verification** — independent obligations checked against the
    initial state and every post-transition state, tri-state
    (`Holds`/`Fails`/`Unknown`)
11. **Caching** — content-addressed, keyed on node-contract, IR-slice,
    and dependency fingerprints
12. **Assembly** — declared artifacts linked into fail-closed promotion
    evidence bundles (`hold`/`promote`)
13. **Adequacy** — subject-bound mutation, concurrency, and fault
    injection campaigns
14. **Serialization** — canonical serialization with trust-boundary
    validation
15. **CLI** — subcommands exposing each stage

## Run

```sh
cd rust
cargo build                                   # warnings are errors in CI
cargo test
cargo run -- check    ../examples/todo.gym
cargo run -- ir       ../examples/todo.gym    # canonical IR; byte-stable, CI-diffed
cargo run -- plan     ../examples/todo.gym
cargo run -- prompts  ../examples/todo.gym
cargo run -- verify   ../examples/todo.gym    # verification bundle (golden-pinned)
cargo run -- adequacy ../examples/todo.gym    # mutation campaign (golden-pinned)
cargo run -- compile  ../examples/todo.gym /tmp/build     # + evidence-bundle.sexpr
cargo run -- synthesize ../examples/todo.gym /tmp/out 3   # LIVE model; never in CI
```

No test or CI step ever invokes a model.

## Examples

`.gym` specifications: `todo`, `bug-tracker`, `gantt`, `chatbot`,
`bi-ingest`, plus `todo` and `twitter` each targeting Ruby/Rails,
Go/stdlib, Java/Spring, Python/Django, and Rust/Actix.

## Documentation

Start at [`docs/README.md`](docs/README.md) for the full index. The
load-bearing documents are:

- [`docs/surface-language.md`](docs/surface-language.md) — the `.gym`
  grammar and its rationale
- [`docs/ir-contract-deltas.md`](docs/ir-contract-deltas.md) — the
  authoritative IR contract and its known limitations
- [`docs/change-study.md`](docs/change-study.md) — measured behavior of
  the surface language under maintenance
- [`docs/shared-domains-design.md`](docs/shared-domains-design.md) —
  cross-application interop, designed but not implemented

## Trust boundary

Surface reflection ends at elaboration. The planner, prompt compiler,
candidate validator, and acceptance path consume ordinary immutable
data. Language-model output is only a candidate implementation;
independent obligations determine acceptance.

## Status

The implementation is complete through the adequacy campaign, and
live synthesis runs end-to-end against a real model (12/12 candidates
firewall-accepted on first attempt). The following do not exist:

- acceptance `must`-assertion evaluation — the adequacy campaign's own
  measured blind spot, and the natural next verification phase
- shared domain compilation units and gRPC/OpenAPI interop projections
  (designed, not implemented)
- cryptographic artifact hashing (FNV-1a placeholder; an unkeyed hash
  is drift evidence, not authentication, and the planned SHA-256
  upgrade alone will not change that)
- cache CLI wiring (`cache.rs` is library-only)
- component port declarations for external interface boundaries
  (REST, gRPC, GraphQL, message queues — provides/requires contracts)
