# Gymnast documentation index

Every document in this directory, what it is for, and whether it is
binding. **Binding** documents constrain implementation; **design**
documents record an intended shape that is not yet built; **record**
documents report measurements already taken.

## Language and contract

| Document | Kind | Subject |
|---|---|---|
| [`surface-language.md`](surface-language.md) | Binding | The `.gym` surface grammar and its rationale, including the v0.2 additions (constants, coverage teeth, actor binding). Companion artifact: `examples/todo.gym`. |
| [`ir-contract-deltas.md`](ir-contract-deltas.md) | Binding | Authoritative enumeration of every deliberate difference between the Rust IR and the Lamedh reference IR, plus documented known limitations. Port consumers against this file, never against Lamedh goldens. |
| [`surface-v0.2-design.md`](surface-v0.2-design.md) | Design → implemented | Derivation of the three v0.2 surface features from the maintenance probes, and their substitution semantics. Implemented by phase 10. |
| [`shared-domains-design.md`](shared-domains-design.md) | Design | Shared domain compilation units and gRPC/OpenAPI interop projections for cross-application communication. Not implemented; the eventual execution plan derives from it. |

## Measurements

| Document | Kind | Subject |
|---|---|---|
| [`change-study.md`](change-study.md) | Record | Five maintenance changes applied across the example corpus, with the pipeline's response measured at every stage. Source of the v0.2 flexibility requirements. |

## Rust port execution plans

Each phase ran plan → committed-oracle crew (oracle tests committed red
before implementation; implementers may not touch them; integrator-only
arbitration with in-file notes) → adversarial phase gate that
mutation-tests the oracles. Gate regression tests live in
`rust/tests/gate*_regression_test.rs`.

| Plan | Phase | Delivers |
|---|---|---|
| [`rust-port-plan.md`](rust-port-plan.md) | 1 | Lexer, parser, typed AST, v0 checker |
| [`rust-port-plan-phase2.md`](rust-port-plan-phase2.md) | 2 | Profile resolution, elaboration, canonical IR |
| [`rust-port-plan-phase3.md`](rust-port-plan-phase3.md) | 3 | Accessors, deterministic planner, prompt compiler |
| [`rust-port-plan-phase4.md`](rust-port-plan-phase4.md) | 4 | Sexpr reader, candidate firewall, recipes, `compile` |
| [`rust-port-plan-phase5.md`](rust-port-plan-phase5.md) | 5 | Sandboxed model runner with bounded repair |
| [`rust-port-plan-phase6.md`](rust-port-plan-phase6.md) | 6 | Executable transition calculus and verification |
| [`rust-port-plan-phase7.md`](rust-port-plan-phase7.md) | 7 | Tri-state verification, live traces, caching |
| [`rust-port-plan-phase8.md`](rust-port-plan-phase8.md) | 8 | Assembly and promotion evidence bundles |
| [`rust-port-plan-phase9.md`](rust-port-plan-phase9.md) | 9 | Adequacy campaign with baseline-aware detection |
| [`rust-port-plan-phase10.md`](rust-port-plan-phase10.md) | 10 | Surface v0.2: constants, coverage teeth, actor binding |

All ten phases are complete and gate-accepted.

## Reading order

- **New to the project** — repository [`README.md`](../README.md), then
  `surface-language.md`, then `examples/todo.gym`.
- **Porting a Lamedh consumer to Rust** — `ir-contract-deltas.md`
  first, then the relevant phase plan.
- **Proposing a surface-language change** — `change-study.md` for how
  the language behaves under maintenance, then `surface-v0.2-design.md`
  for the shape a derivation is expected to take.
- **Working on interop** — `shared-domains-design.md`.
