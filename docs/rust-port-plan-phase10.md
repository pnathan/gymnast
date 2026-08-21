# Rust port — phase 10: surface v0.2 (constants, coverage teeth, actor binding)

Implements `docs/surface-v0.2-design.md` (binding; read it in full —
it derives each feature from a measured maintenance probe in
`docs/change-study.md`). Process: committed-oracle exactly as phases
4–9 (Stage 1 commits `rust/tests/v02_oracle_test.rs` red, FMT-CLEAN,
message `"phase 10 stage 1: oracle tests-of-record (red)"`;
implementers may not touch it; integrator-only arbitration with
in-file notes).

Cardinal rule for this phase: **substitution preserves semantics.**
Two specs differing only in const-spelling vs inline literals must
produce identical verification, adequacy, planning, and prompting
results; the IR may differ ONLY by the `constants` header and the new
diagnostics. Every stage validates this invariant.

## A. Parser + AST

- New declaration: `const <snake_name> = <int-literal>` →
  `Decl::Const(ConstDecl { name, value: i64, span })`. A non-integer
  RHS is a parse-time E210 (`invalid-constant-expression`).
- **Const-expr grammar** at integer positions:
  `IDENT | IDENT '+' INT | IDENT '-' INT`. Representation in captured
  clause sexprs: a bare `Sexpr::Sym(name)` for the plain form and
  `(+ name N)` / `(- name N)` call-shaped for the offset forms (the
  existing call convention; nothing new to print).
- Positions that accept const-exprs (parser accepts an IDENT where it
  today requires INT): refinement bounds `text (1..IDENT)`,
  `(IDENT..N)`, `(..IDENT)` etc.; scenario `when` call arguments
  (already general call args — no parser change needed there);
  predicate operands (already general — none needed); workload `under`
  values (`virtual_users IDENT`, `duration IDENT min`, ...). The plan
  author has verified predicate/step positions already parse idents;
  the REQUIRED parser work is refinements and `under` values plus the
  `const` declaration itself.
- `generate` pair extension: `(actor <gen> of <actor-name>)` — the
  captured pair becomes `(gen of actor-name)` three-element; without
  `of`, unchanged two-element shape.

## B. Checker + elaborator

- Const registry from `Decl::Const` PLUS every `use` profile
  parameter (`sharing_limit 256` binds `sharing_limit` = 256).
  Duplicates (const/const, const/param, param across two `use`
  clauses) → E201 with the standard duplicate shape.
- **Substitution pass** during elaboration, applied to every clause
  and field sexpr headed anywhere in a declaration: a `Sym` that names
  a constant in an integer-expression position becomes `Int(value)`;
  `(+ name N)` / `(- name N)` become `Int(value ± N)`. Substitution is
  NAME-DRIVEN and total: any `Sym` matching a declared constant is
  substituted wherever it appears as a predicate/step/bound/under
  operand; `(+ sym N)` with sym not a constant → E209
  (`unresolved-constant`) — likewise a refinement/under IDENT that
  resolves to nothing. Bare unknown atoms in predicate positions stay
  what they are today (abstract predicates), so E209 fires ONLY for
  the explicitly-const-shaped forms: `(+/- name N)` anywhere, and
  IDENTs in refinement bounds / under values (positions where only
  constants are legal).
- IR header gains `(constants ((name value source) ...))` — sorted by
  name; `source` = sym `spec` or the profile name string; participates
  in the fingerprint like every header field.
- `of` binding: `(actor gen of actor_name)` → elaborator resolves
  `actor_name` against declared actors (unresolved → the existing
  E-class unresolved-reference error); lowered obligations carry
  `(actor-of actor_name)` after `(generate ...)`. A two-element pair
  whose gen symbol matches no declared actor → W410
  `unresolved-acceptance-actor` (warning; existing specs keep
  working).
- Coverage teeth (may live in verify.rs beside check_transition_refs,
  where the suffix-rule helpers already are): when the acceptance
  coverage clause lists `every_operation`, each interface op not
  matched (suffix rule) by any property execute step or scenario
  `when` step → W408 `uncovered-operation`; `every_transition`
  likewise per behavior → W409 `unexercised-transition`. Messages name
  the op/behavior id and the acceptance node. `compile_verification`
  folds them into a NEW bundle field `(coverage-diagnostics (...))`
  placed after `transition-diagnostics` (before `diagnostics`).
  Bundle field order becomes: schema obligations results summary
  coverage environment-diagnostics transition-diagnostics
  coverage-diagnostics diagnostics source-diagnostics fingerprint.

## C. Flagship + corpus updates (Stage 3)

- `examples/todo.gym`: the four inline `256`s and the scenario/
  concurrency sites become `sharing_limit` / `sharing_limit + 1`
  references (the `use` clause is the single source); `generate`
  pairs gain `of user`. Result: `256` appears exactly ONCE in the
  file.
- `examples/bi-ingest.gym`: `const daily_quota = 50000`, five sites
  → references. The other three new specs get consts for their limits
  (5000 / 2000 / 500 / 100) and `of` bindings for their acceptance
  actors.
- Goldens: ALL todo fixtures regenerate ONCE (ir/plan/prompts/verify/
  results/bundle/adequacy — new IR header, W408 for `query_tasks`,
  coverage-diagnostics field, `actor-of` in obligations). SEMANTIC
  INVARIANT CHECK before committing them: verification summary stays
  `(total 9) (passed 1) (failed 2) (skipped 4) (indeterminate 2)` and
  adequacy stays 5 survivors / pass nil — substitution changed no
  verdict. CI reproducibility steps unchanged (they diff whatever the
  fixtures say).
- `docs/surface-language.md` gains the v0.2 section (const grammar,
  coverage semantics, `of` binding); delta doc gains a "Surface v0.2
  (phase 10)" section (constants header, substitution-preserves-
  semantics contract, new W/E codes, bundle field order change).

## Oracle tests (`rust/tests/v02_oracle_test.rs`, Stage 1 commits red, ~24 tests)

01 const parsing: decl accepted; non-int RHS → E210; duplicate const
   → E201; const/profile-param collision → E201.
02 substitution: a hand-built spec pair (inline-literal vs const-ref,
   including `+ 1` and `- 1` forms in requires / fails-when /
   invariant / scenario / under / refinement) elaborates to IRs whose
   nodes are IDENTICAL except the constants header — assert node-list
   equality field-by-field, then assert header presence/shape/sorting
   and both sources (spec, profile).
03 E209: `(+ nosuch 1)` in a predicate; bare IDENT refinement bound
   naming nothing; under-value IDENT naming nothing.
04 profile params bind: a spec using todo_standard references
   `sharing_limit` and elaborates to Int 256 at every reference; the
   constants header lists `(sharing_limit 256 "oddities/profiles/
   todo_standard")`.
05 substitution-preserves-semantics END TO END: the two specs of 02
   produce byte-identical `verify` bundles except the fingerprint-
   bearing and constants-bearing fragments — derive precisely which
   fragments may differ and pin the rest equal; adequacy campaign
   outcomes identical except subject fingerprint.
06 W408/W409: a spec with coverage(every_operation) and one uncovered
   op → exactly one W408 naming it; covered-by-suffix op → none;
   coverage clause ABSENT → no W408 even with uncovered ops;
   every_transition analog for W409; bundle carries
   coverage-diagnostics in the pinned field order.
07 `of` binding: resolves (obligation carries actor-of); unknown actor
   in `of` → error; two-element pair with non-actor gen symbol →
   W410; two-element pair whose gen symbol IS a declared actor → no
   warning.
08 flagship pins: todo.gym post-update has exactly one literal 256
   (string scan); its verify summary and adequacy counts unchanged
   (the invariant check as an oracle test); `query_tasks` W408
   present; goldens match fixtures byte-for-byte (red until Stage 3
   regenerates them).

## Stage plan

- **Stage 1 — oracle author** (Sonnet): derives every pin from the
  design doc + current binary behavior, arithmetic in comments;
  `cargo fmt --all` BEFORE the commit; commits ONLY the oracle file.
- **Stage 2 — parser/checker/elaborator + coverage/actor checks**
  (Sonnet): sections A–B; all oracle tests except 08 green; NO example
  or golden edits; every pre-existing suite green (existing examples
  still parse — v0.2 is additive).
- **Stage 3 — corpus + goldens + docs** (Sonnet, first integrator):
  section C; regenerates todo goldens once with the semantic-invariant
  check recorded in its report; oracle test 08 green; full suite
  green.
- **Verify loop** (Sonnet), integrator verification, **Opus gate**.

Definition of done: warning-free `-D warnings --all-targets`; full
suite green with the oracle byte-identical to Stage 1's commit; todo
goldens regenerated exactly once with verification/adequacy outcomes
provably unchanged; `256` appears once in todo.gym; delta doc and
surface doc updated; no change to firewall/runner/cache/assembly
authority code.
