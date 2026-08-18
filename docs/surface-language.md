# Gymnast surface language — design proposal (draft)

Status: proposal for review. Companion artifact: `examples/todo.gym`, a
form-for-form translation of `examples/todo.lisp` into this surface.

This document proposes the human-facing specification language for the Rust
port of Gymnast. It replaces the S-expression/fexpr surface with a closed,
statically checked, Algol-family grammar. Everything downstream of
elaboration (IR, plan, prompts, calculus, verification) is unaffected: the
surface produces the same surface-record kinds the elaborator already
consumes.

## Goals

1. **Compact, for expert hands.** Density like Alloy or an ML signature, not
   like a YAML config. One declaration per idea; high-frequency forms get
   short keywords (`cmd`, `qry`, `inv`, `opt`), one-per-file forms keep full
   words (`spec`, `application`, `acceptance`).
2. **Typed.** The elaborator becomes a real type checker: every field, input,
   output, predicate, and generator is resolved against declared modes, with
   source-span diagnostics. Meaning is established by checking, not by shape
   convention.
3. **Closed.** The grammar is the grammar. No user macros, no evaluation, no
   escape hatches. Extensibility is exactly profile parameterization
   (`use ... (key value, ...)`). This is the surface-level form of the core
   invariant: nothing outside the compiler decides what a spec means.

## What we take from Algol 68

- **Identity declarations.** In Algol 68 every declaration is
  `KEYWORD name = value`. We adopt this uniformly: `spec x = ...`,
  `mode X = ...`, `actor u = ...`, `behavior b = ...`. One declaration shape
  to learn; every construct is a binding of a name to a closed term.
- **The orthogonal mode algebra.** Algol 68's `mode` composes `struct`,
  `union`, rows, and primitives through one grammar. We collapse the four
  Lisp-surface type forms (`:opaque`, `:enum`, `:variant`, `:record`) into
  one `mode` declaration over composable constructors:

  ```
  mode-expr := opaque <mode>
             | enum ( name, ... )
             | union ( tag <mode>, ... )        # A68 united modes = sum types
             | struct ( <mode> name, ... )
             | opt <mode>                       # sugar for union (void, M)
             | [] <mode>                        # row of M
             | Name | Name ( <mode>, ... )      # reference / parameterized
             | text ( lo..hi ) | text ( ..hi )  # refined primitives
  ```

- **Type-first declarers.** Fields and parameters read `ListId list`,
  `text (1..500) title`, `opt Due due` — Algol 68 order, and measurably
  terser than `name: Type` walls in record-heavy specs.
- **Parentheses as brackets.** A68 lets `( )` abbreviate `begin end`. We use
  parenthesized packs — comma-separated, semicolon-sequenced — as the only
  grouping construct. No braces, no layout sensitivity.
- **`proc`-shaped operation contracts.** An A68 proc is
  `proc f = (real x) real: body`. An interface operation is the same header
  with no body — the body is what synthesis must produce:

  ```
  cmd create_task = (ListId list, text title, opt Due due) Task
      ! (unauthenticated, forbidden, not_found, conflict)
  ```

  `!` introduces the error set (the operation's declared failure modes).

## What we deliberately leave behind

- **Stropping** and reversed closers (`fi`, `esac`, `od`) — historical
  costume, not load-bearing.
- **User-defined operators and priorities.** A68's extensible operator
  grammar is the opposite of a closed spec language.
- **Coercion richness.** A68's six coercions made meaning depend on context.
  A spec language wants zero implicit coercions; every widening is written.
- **Bodies.** Every A68 declaration has a yielding value. Gymnast
  declarations bind *contracts*; the only "expressions" are the predicate
  sublanguage (below), which is total and non-Turing-complete.

## Declaration catalog

Each kernel head from the Lisp surface maps to one declaration form.
Everything is `keyword name = body`; attribute packs are `(key value, ...)`.

| Lisp surface | Compact surface |
|---|---|
| `(defspec name ...)` | `spec name = v 0.1 owner product exports A, B, ...` |
| `(use-profile p "1.0" :k v)` | `use path/to/p @ 1.0 (k v, ...)` |
| `(application n :modules ...)` | `application n = (modules ..., default_acceptance ...)` |
| `(actor u :kind person :identity ...)` | `actor u = person (identity ...)` |
| `(type X :opaque/:enum/:variant/:record ...)` | `mode X = <mode-expr>` |
| `(component c :provides ...)` | `component c = (responsibility "...", provides ..., uses (...))` |
| `(interface i (command ...) ...)` | `interface i = for <actor> ( cmd/qry name = (params) Result ! (errors), ... )` |
| `(state s :of ... :durability ...)` | `state s = (of aggregate (...), owner ..., ...)` |
| `(flow f :from a :to b :kind ...)` | `flow f = a -> b : cmd (grant ..., deny ...)` |
| `(behavior b :on ... (requires ...) ...)` | `behavior b = on iface.op (actor, request) ( ...clauses... )` |
| `(invariant i :scope s :always p)` | `inv i = on s always <pred>` |
| `(constraint c :class w :under ... :must p)` | `constraint c = workload on s under (...) must <pred>` |
| `(synthesis y :target ...)` | `synthesis y = target lang / framework ( ... )` |
| `(acceptance a (property ...) ...)` | `acceptance a = of subject ( property/scenario/concurrency/fault/coverage/execution ..., ... )` |

Behavior clauses keep their Lisp-surface vocabulary as clause keywords,
semicolon-sequenced inside the pack:

```
behavior create_task = on todo_service.create_task (user, request) (
  reads (memberships, todo_lists), writes tasks,
  atomic list request.list, idempotency command_key;

  requires may_edit_list (pre, user, request.list);
  ensures  post = insert_task (pre, request, result);
  returns  result;
  fails forbidden when not may_edit_list (pre, user, request.list)
    preserves all_state;
  emits task_created exactly_once_logically )
```

## The predicate sublanguage

Exactly the closed evaluator already implemented in `transition.lisp`
(`gymnast-eval-predicate`), written infix, plus quantifiers for invariants:

```
pred := pred and pred | pred or pred | not pred
      | expr = expr | expr < expr | expr <= expr
      | for all <Mode> x : pred | exists <Mode> x : pred
      | name ( expr, ... )                  # declared abstract predicates
expr := literal | name | name.field | name ( expr, ... )
```

`pre`, `post`, `result`, and the behavior's bound names (`actor`, `request`)
are the only free variables, each with a checked mode. Abstract predicates
(`may_edit_list`, `insert_task`) must be declared by a profile or the spec;
unknown names are closed-world elaboration errors, exactly as today.

## Type checking (what "typed" buys)

Elaboration keeps its current phases and adds a checking pass:

- every mode reference resolves (already true, by name) **and** every use
  site is mode-correct: operation inputs/outputs, struct fields, generator
  bindings in acceptance properties, and state aggregates;
- predicates are checked: `<=` requires ordered operands, `=` requires
  same-mode operands, quantifier bodies are boolean, field projections
  (`request.list`, `t.title`) resolve against the declared struct;
- behavior `reads`/`writes` sets are checked against the state declaration;
  `on` targets must name a declared interface operation with matching arity;
- diagnostics carry source spans and keep the existing
  `(severity code subject message details)` shape.

Semantic IDs (`module/kind/name`), fingerprinting, planning, and everything
downstream are unchanged. The surface parser plus checker replaces
`surface.lisp` + the name-resolution part of `elaborate.lisp`; the IR
contract stays fixed.

## Lexical summary

- Comments: `#` to end of line (a nod to A68's brief comment symbol).
- Identifiers: `snake_case`; modes capitalized by convention (the checker
  warns, not errors).
- Separators: `,` within packs, `;` between sequenced clauses; both trailing
  forms permitted. No layout sensitivity; newlines are whitespace.
- Literals: integers, decimal strings, `"..."` strings, duration/latency
  units (`30 min`, `300 ms`) in constraint packs only.
- File extension: `.gym`.

## Open questions for review

1. **Type-first vs name-first fields.** `ListId id` (A68, terser) vs
   `id: ListId` (modern default). The draft commits to type-first; this is
   the largest taste decision in the grammar.
2. **`opt M` vs `M?`.** Draft uses `opt M` for orthogonality with the mode
   algebra; `?` postfix is terser but adds a second syntax class.
3. **Interface default actor.** `for user` at interface level with per-op
   override, vs mandatory per-op actor. Draft: interface-level default.
4. **Attribute packs: juxtaposition (`temperature 0`) vs equals
   (`temperature = 0`).** Draft uses juxtaposition, reserving `=` for
   identity declarations only.
5. **Keyword lengths.** Current split: `cmd`/`qry`/`inv`/`opt` short,
   everything else full. Push further (`beh`, `iface`) or pull back?

## Migration notes

Parsing is hand-rolled lexer + recursive descent in the Rust port (no parser
generator): the grammar is small and diagnostic quality is the product —
span-carrying errors from day one. The `.lisp` examples remain in-tree as
the reference corpus until the Rust elaborator's IR output is confirmed
equivalent (modulo surface-syntax fields) against the Lamedh implementation,
then the S-expression surface is retired.
