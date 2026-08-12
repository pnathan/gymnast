# Gymnast

Gymnast is an experimental concept-synthesis compiler written in
[Lamedh](https://github.com/pnathan/lamedh). A programmer describes an
application in one high-level language; Gymnast elaborates that description
into a closed semantic IR, derives a deterministic synthesis plan, and emits
bounded work packets for deterministic recipes or small language models.

The durable unit is the typed node contract, not its prompt. Generated
candidates cannot change the plan, add capabilities, write outside their
declared paths, or decide whether they satisfy the specification.

## Current vertical slice

The first cut implements:

- a reflective surface built with Lamedh fexprs, `vau`, and macros;
- closed-world elaboration into design, transition, obligation, and synthesis
  graphs;
- stable semantic identifiers, diagnostics, canonical ordering, and
  fingerprints;
- a deterministic eight-node synthesis DAG with coverage checks;
- prompt packages compiled from typed node contracts;
- a candidate firewall enforcing node identity and write sets; and
- a Todo specification and compiler tests exercising the complete front half.

No model is invoked yet. This milestone proves that the authoritative compiler
stages are stable before adding a generative executor.

## Run

The repository pins the Lamedh revision in `LAMEDH_REVISION`.

```sh
scripts/bootstrap-lamedh.sh
bin/gymnast check examples/todo.lisp todo-spec
bin/gymnast compile examples/todo.lisp todo-spec build/todo
.tools/bin/lamedh --test tests
```

`compile` writes `ir.sexpr`, `plan.sexpr`, `prompts.sexpr`, and the complete
`compilation.sexpr`. Repeating a compilation with the same inputs must produce
byte-identical output.

## Trust boundary

Surface reflection ends at elaboration. The planner, prompt compiler,
candidate validator, and acceptance path consume ordinary immutable data.
Language-model output is only a candidate implementation; independent
obligations determine acceptance.

## Status

This is a bootstrap implementation. The surface syntax, semantic profile
system, cryptographic artifact hashing, model executor, platform kit, and
whole-system verifier remain under active design.
