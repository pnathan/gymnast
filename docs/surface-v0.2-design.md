# Surface language v0.2: flexibility under change

Status: DESIGN, implemented by phase 10 (`docs/rust-port-plan-phase10.md`).
Derived from the maintenance probes in `docs/change-study.md`; each
feature below names the probe that demanded it. v0.2 is deliberately
small: three features, no new pipeline stages, no semantic change to
any existing verdict.

## The flexibility requirements, derived

| Probe | Change class | Required flexibility |
|---|---|---|
| 1 (quota 10000→50000, 5 edits, drift invisible) | tune a business limit | ONE authoritative binding per business constant, referenced everywhere it means the same thing |
| 5 (`sharing_limit 512` changed nothing) | parameterize by profile | profile parameters must BE such bindings, not decoration |
| 2 (`assign_bug` invisible to verification) | grow the API | declared coverage intent must be checked against the interface, not filed as a skipped obligation |
| 4 (acceptance actors are free symbols) | grow the cast | acceptance generators must name which declared actor they impersonate |
| 3 (additive field = wholesale fingerprint change) | evolve schemas | additive-vs-breaking identity — OUT of v0.2; already designed in `docs/shared-domains-design.md` (wire-lock) |

## Feature 1: named constants (`const`) and live profile parameters

### Surface

```
const sharing_limit = 256
const max_title    = 200
```

- Top-level declaration; name is lowercase snake (the value namespace,
  disjoint from modes); value is an integer literal (text constants
  are NOT in v0.2 — every probe's constant was an int).
- `use <profile> @ <ver> (k v, ...)` — every parameter pair ALSO binds
  `k` as a constant in spec scope with value `v`. This is the
  unification of probes 1 and 5: the `use` clause becomes the single
  source of truth it already appears to be. A spec-level `const` with
  the same name as a profile parameter is a duplicate declaration
  (E201, same as any other collision).

### Reference positions

A **const-expr** is `<name>`, `<name> + <int>`, or `<name> - <int>`
(the `+ 1` form is required by the boundary probes: 257, 10001, 5001).
Const-exprs are accepted in every INTEGER-literal position:

- predicate comparison operands (`requires ... < sharing_limit`,
  `fails ... when ... = sharing_limit`, invariant bodies including
  under `for all`),
- scenario `when` step arguments (`when invite_distinct (owner,
  sharing_limit); ... when invite_distinct (owner, sharing_limit + 1)`),
- concurrency `must` right-hand sides,
- workload `under` values,
- mode refinement bounds (`text (1..max_title)`).

### Semantics: substitution at elaboration

Constants are resolved and SUBSTITUTED during elaboration: every IR
form downstream carries the literal value, exactly as if the author
had written it. Consequences, all deliberate:

- The planner, prompt compiler, firewall, transition calculus,
  verifier, cache, assembly, and adequacy machinery are UNTOUCHED —
  they consume the same literal shapes as today.
- Two specs that differ only in `const` spelling vs inline literals
  produce IDENTICAL semantics; the IR differs only by the new
  provenance header (below), keeping the honesty machinery's
  substitution-invariance testable.
- Drift becomes UNREPRESENTABLE rather than undetected: there is one
  binding, and every use site names it.

The IR header gains `(constants ((name value source) ...))` — source
is `spec` or the profile name — sorted by name, fingerprinted like
every other header field, so the binding survives into provenance.

### Diagnostics

- `E209 unresolved-constant`: a const-expr names no declared constant
  (closed world, hard error).
- `E210 invalid-constant-expression`: malformed const-expr (e.g.
  `name + name`), or a `const` whose value is not an integer literal.
- E201 covers duplicate `const` names and const/profile-param
  collisions, as it covers every duplicate.

## Feature 2: coverage clauses with teeth

The acceptance `coverage (...)` clause stops being purely
declarative. When it lists `every_operation`, elaboration checks each
interface operation against the acceptance block's execute/when steps
(resolved with the SAME suffix rule the trace machinery uses); when it
lists `every_transition`, each behavior is checked the same way.

- `W408 uncovered-operation`: an interface op no acceptance step
  exercises (probe 2's `assign_bug`; the flagship's `query_tasks`).
- `W409 unexercised-transition`: a behavior no acceptance step
  reaches.

Warnings, not errors — the corpus is honest about its gaps, and a gap
is information, not invalidity. They surface in `check`/`verify`
stderr and fold into the verification bundle's new
`(coverage-diagnostics (...))` field, placed after
`transition-diagnostics` (same pattern as W406). Flags the coverage
clause does not list produce no checks (declared intent only is
checked — no new vacuous claims).

## Feature 3: acceptance actors bind to declared actors

```
generate (actor authenticated_editor of user, task valid_task)
```

- The optional `of <actor-name>` binds the generator symbol to a
  declared actor; the elaborator resolves it (closed world: an unknown
  actor name is the existing unresolved-reference error class).
- Without `of`, `W410 unresolved-acceptance-actor` warns when the
  generator symbol matches no declared actor name — the current free-
  symbol style keeps working, visibly.
- The binding lands in the lowered obligation as `(actor-of <name>)`,
  available to future phases (generator synthesis, seam verification).

## Explicitly out of v0.2

- Text/enum constants; const arithmetic beyond ± int (no expressions
  over two constants — nothing in the corpus needs it, and every
  addition to the const grammar is surface the models' prompts must
  then explain).
- Additive schema identity (shared-domains wire-lock, designed).
- Profile parameters flowing into profile GENERATORS' output (the
  todo_standard generator still ignores its args for the four types it
  emits; what v0.2 fixes is that the parameters now bind constants the
  SPEC references — the generator-side plumbing belongs to the domains
  phase, where user-defined vocabulary makes it meaningful).

## Flagship consequence (the acceptance test for the whole feature)

`examples/todo.gym` after v0.2 carries `256` in EXACTLY ONE place —
the `use ... (sharing_limit 256, ...)` clause — with the requires/
fails/invariant/scenario/concurrency sites all referencing
`sharing_limit` (and `sharing_limit + 1`). Its verification summary
(1/2/4/2 of 9) and adequacy outcome (5 survivors, pass nil) must be
UNCHANGED — substitution preserves semantics by construction, and the
goldens regenerate once to pick up the new IR header and W408 entries
(`query_tasks` is now honestly flagged uncovered). Changing the limit
becomes a one-line edit that cannot drift.
