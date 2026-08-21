# Gymnast documentation index

Every document in this directory, what it is for, and whether it is
binding. **Binding** documents constrain implementation; **design**
documents record an intended shape that nothing implements yet;
**record** documents report what a measurement finds.

## Language and contract

| Document | Kind | Subject |
|---|---|---|
| [`surface-language.md`](surface-language.md) | Binding | The `.gym` surface grammar and its rationale, including the v0.2 additions (constants, coverage teeth, actor binding). Companion artifact: `examples/todo.gym`. |
| [`ir-contract-deltas.md`](ir-contract-deltas.md) | Binding | Authoritative description of the IR's shape and its documented known limitations. Port consumers against this file, never against stale goldens. |
| [`surface-v0.2-design.md`](surface-v0.2-design.md) | Design → implemented | How the three v0.2 surface features follow from the maintenance probes, and what their substitution semantics guarantee. Phase 10 implements it. |
| [`shared-domains-design.md`](shared-domains-design.md) | Design | Shared domain compilation units and gRPC/OpenAPI interop projections for cross-application communication. Nothing implements this; it is the source an execution plan derives from. |

## Records

| Document | Kind | Subject |
|---|---|---|
| [`change-study.md`](change-study.md) | Record | What five common maintenance changes cost across the example corpus, with the pipeline's response measured at every stage. The v0.2 flexibility requirements derive from it. |
| `rust-port-plan*.md` (ten files) | Record | The execution plans behind Rust port phases 1–10. Every phase is complete and gate-accepted, so these are history: open one only to find out why a phase has the shape it does. Gate regression tests live in `rust/tests/gate*_regression_test.rs`. |

## Reading order

- **New to the project** — repository [`README.md`](../README.md), then
  `surface-language.md`, then `examples/todo.gym`.
- **Changing the IR or anything downstream of it** —
  `ir-contract-deltas.md`, which is where the IR's shape and every
  known limitation is recorded.
- **Proposing a surface-language change** — `change-study.md` for how
  the language behaves under maintenance, then `surface-v0.2-design.md`
  for the shape a derivation is expected to take.
- **Working on interop** — `shared-domains-design.md`.
