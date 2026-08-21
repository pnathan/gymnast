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

## Records

| Document | Kind | Subject |
|---|---|---|
| [`change-study.md`](change-study.md) | Record | Five maintenance changes applied across the example corpus, with the pipeline's response measured at every stage. Source of the v0.2 flexibility requirements. |
| `rust-port-plan*.md` (ten files) | Record | The execution plans for Rust port phases 1–10, all complete and gate-accepted. Kept as history; read one only when you need to know why a phase was built the way it was. Gate regression tests live in `rust/tests/gate*_regression_test.rs`. |

## Reading order

- **New to the project** — repository [`README.md`](../README.md), then
  `surface-language.md`, then `examples/todo.gym`.
- **Changing the Rust IR or anything downstream of it** —
  `ir-contract-deltas.md`, which is where every deliberate deviation
  from the Lamedh reference and every known limitation is recorded.
- **Proposing a surface-language change** — `change-study.md` for how
  the language behaves under maintenance, then `surface-v0.2-design.md`
  for the shape a derivation is expected to take.
- **Working on interop** — `shared-domains-design.md`.
