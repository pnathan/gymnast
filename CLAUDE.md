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
```

## File layout

```
src/
  gymnast.lisp       load unit; includes all modules, defines gymnast-compile
  core.lisp          data constructors, helpers, FNV-1a fingerprinting, IR/plan node types
  surface.lisp       fexprs, vau, defspec macro, use-profile macro
  elaborate.lisp     surface-to-IR elaboration with closed-world diagnostics
  plan.lisp          deterministic lowering from IR to 8-node typed synthesis DAG
  prompt.lisp        prompt compilation from node contracts
  candidate.lisp     candidate protocol validation / firewall
  cli.lisp           CLI entrypoint: check, ir, plan, prompts, compile subcommands
tests/
  compiler.lisp      8 test cases exercising the complete front half
examples/
  todo.lisp          vertical-slice Todo application specification
```

## Compiler pipeline

1. **Surface** — fexpr/vau capture of declaration operands without evaluation
2. **Elaboration** — closed-world validation, semantic IDs, partitioning into
   design/transition/obligation/synthesis graphs, fingerprinting
3. **Planning** — deterministic lowering to 8-node typed synthesis DAG with
   coverage and dependency checks
4. **Prompt compilation** — projects each plan node contract into a prompt package
5. **Candidate validation** — firewall enforcing node identity, write sets,
   no added assumptions or unresolved entries
6. **CLI** — subcommands exposing each stage

## Code conventions

- All public functions prefixed `gymnast-` (e.g. `gymnast-elaborate`,
  `gymnast-plan-node-field`).
- Data is S-expression association lists: `(list 'tag (list 'key value) ...)`.
- Access via `gymnast-assoc-value`, predicates via `gymnast-tagged-p`.
- Canonical ordering via `gymnast-canonical-fields` / `gymnast-canonical-less-p`.
- Diagnostics use `(diagnostic (severity ...) (code ...) (subject ...) ...)`.
- Schema versions in `$gymnast-*-schema` globals (e.g. `"gymnast.ir/0.1"`).
- Tests use `deftest` and `assert-equal` / `assert-true` / `assert-false`.
- Fingerprints currently use FNV-1a (placeholder; SHA-256 upgrade is issue #1).
- IR nodes have stable semantic IDs: `module-name/kind/name`.
- Plan nodes have IDs: `module-name/plan/local-name`.

## Architecture invariants

- Elaboration is pure and deterministic: identical inputs → byte-identical IR.
- The planner is not a model; models cannot modify the plan.
- Prompt text is a compiled projection of a typed node contract.
- Generated candidates are untrusted; they cannot add capabilities, write
  outside declared paths, or self-evaluate.
- Every normative semantic node must appear in at least one implementation path
  and one evidence path.
- Surface reflection ends at elaboration; planning onward consumes immutable data.

## Issue roadmap (dependency order)

Issues form a dependency chain. Lower numbers are prerequisites for higher ones.

| # | Title | Depends on |
|---|-------|-----------|
| 1 | Canonical serialization and SHA-256 identity | — |
| 2 | Versioned semantic profiles | #1 |
| 3 | Executable transition calculus | #1, #2 |
| 4 | Characterized Lamedh platform kit | #3 |
| 5 | Deterministic recipe registry and executor | #4 |
| 6 | Sandboxed small-model node runner | #5 |
| 7 | Independent verification obligations | #3, #6 |
| 8 | Content-addressed caching and incremental regen | #1, #5 |
| 9 | Assembly and promotion evidence bundles | #5, #6, #7, #8 |
| 10 | Adequacy campaign (mutation/concurrency/fault) | #7, #9 |
| 12 | North star architecture document | Reference; not blocking |

## What is built

The "front half" — surface through prompt compilation — with passing CI:
format check, 8 tests, Todo elaboration, and reproducible compilation.

## What is not built

- Generative executor (no LLM invoked)
- Semantic profile resolution (use-profile creates import nodes only)
- Cryptographic artifact hashing (FNV-1a placeholder)
- Platform kit (target field exists but no code generation)
- Recipe registry and deterministic executors
- Whole-system verifier and evidence bundles
- Content-addressed caching
- Incremental regeneration
