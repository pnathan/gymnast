# Gymnast surface language — design proposal (draft)

Status: IMPLEMENTED through v0.2 (Rust port phases 1–10); the open
questions near the end of this document remain open. Companion
artifact: `examples/todo.gym`, a form-for-form translation of
`examples/todo.lisp` into this surface; four further specifications
(`bug-tracker`, `gantt`, `chatbot`, `bi-ingest`) exercise it.

This document defines the human-facing specification language for the Rust
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

## Type checking (what "typed" buys — roadmap)

Implemented today (v0): closed-world name resolution — every mode,
actor, interface-op, and scope reference must resolve to a declared or
profile-provided name. The rest of this section is the phase-3+ roadmap,
not current behavior:

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

## Surface v0.2 (phase 10)

Three additive features, derived from the maintenance probes in
`docs/change-study.md` and specified in `docs/surface-v0.2-design.md`.
No new pipeline stages; no semantic change to any existing verdict —
the cardinal rule of the revision is that **substitution preserves
semantics**: a const-spelled spec and its literal-spelled twin produce
identical verification, adequacy, planning, and prompting results.

### Named constants and live profile parameters

```
const sharing_limit = 256
const max_title    = 200
```

- `const <snake_name> = <int-literal>` is a top-level declaration in
  the value namespace (disjoint from modes). A non-integer right-hand
  side is a parse-time `E210 invalid-constant-expression`.
- Every INTEGER parameter of a resolved `use` clause ALSO binds its
  key as a constant in spec scope (`use ... (sharing_limit 256, ...)`
  binds `sharing_limit` = 256, source: the profile). Non-integer
  parameters (`identity_provider google`) bind nothing. Duplicates —
  const/const, const/profile-param, or the same parameter from two
  `use` clauses — are `E201`, the standard duplicate shape.
- A **const-expr** is `<name>`, `<name> + <int>`, or `<name> - <int>`
  (nothing richer: `name + name` is E210). Const-exprs are accepted in
  every integer-literal position:
  - predicate comparison operands (`requires ... < sharing_limit`,
    `fails ... when ... = sharing_limit`, invariant bodies including
    under `for all`),
  - scenario `when` step arguments,
  - property / concurrency / fault `must` operands,
  - workload `under` values, including the quantity form
    (`duration dur min`),
  - mode refinement bounds (`text (1..max_title)`),
  - the synthesis budget slots (`attempts`, the model pack's
    `max_attempts`) and the concurrency `actors` slot.
- **Substitution happens at elaboration**: every IR form downstream
  carries the literal value, exactly as if the author had written it.
  Never substituted: clause heads, declaration names, error names and
  `!` error sets, generator symbols, scenario `given` and `then`
  values, and the import node's recorded `:arguments` (provenance).
  Substitution is name-driven, so a constant may not share a name with
  an author-declared variable: a constant whose name equals a behavior
  parameter, an acceptance generator variable, a scenario `given`
  variable, or `pre`/`post`/`result` is `E211
  constant-name-collision` (hard error — otherwise the substitution
  would silently rewrite the variable). An integer `use` argument the
  profile does not declare binds no constant and warns `W411
  undeclared-profile-parameter`. The binding itself
  survives into the IR module header's `:constants` field
  (`((name value source) ...)`, sorted by name, present when
  non-empty, fingerprinted like every header field).
- `E209 unresolved-constant` (closed world, hard error): an offset
  form `name + N` / `name - N` whose name is no declared constant,
  anywhere; or a bare IDENT in a refinement bound, `under` value,
  `actors`, `attempts`, or model `max_attempts` slot (positions where
  only constants are legal). Bare atoms in predicate positions remain
  abstract predicates, exactly as before.

### Coverage clauses with teeth

The acceptance `coverage (...)` clause is checked, not filed. When it
lists `every_operation`, each interface operation must be EXERCISED
THROUGH THE TRANSITION MACHINERY by some acceptance execute/when step
(the trace machinery's suffix rule): an operation with no backing
behavior is uncovered even when a step names it, and so is one whose
transition no step reaches. `every_transition` checks each behavior
the same way.

- `W408 uncovered-operation` — names the operation and the acceptance
  node.
- `W409 unexercised-transition` — names the behavior and the
  acceptance node.

Warnings, not errors; they surface on `check`/`verify` stderr and fold
into the verification bundle's `coverage-diagnostics` field (placed
after `transition-diagnostics`). Flags the coverage clause does not
list produce no checks.

### Acceptance actors bind to declared actors

```
generate (actor authenticated_editor of user, task valid_task)
```

- `of <actor-name>` binds the generator symbol to a declared actor;
  an unknown actor name is a closed-world error (E203, the existing
  unknown-actor reference class). The binding lands in the lowered
  obligation as `(actor-of <name>)`, immediately after
  `(generate ...)`.
- Without `of`, an `actor`-keyed pair whose generator symbol matches
  no declared actor warns `W410 unresolved-acceptance-actor` — the
  free-symbol style keeps working, visibly.

Flagship consequence: `examples/todo.gym` carries `256` in exactly one
place (the `use` clause); every other site references `sharing_limit`
or `sharing_limit + 1`, and its verification summary and adequacy
outcome are byte-for-byte unchanged from the literal spelling.

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
