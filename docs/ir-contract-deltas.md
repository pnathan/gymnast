# IR contract: deltas from the Lamedh reference

The Rust IR mirrors the Lamedh IR's structure (partitions, semantic IDs,
canonical field ordering, clause-order preservation, fingerprint over the
fingerprint-free form) but is **not** byte-compatible with it, and a few
shapes differ deliberately. This file is the authoritative enumeration.
Phase-3+ plans must instruct agents to port Lamedh consumers
(`src/plan.lisp`, `src/verify.lisp`, `src/prompt.lisp`, …) against **this
contract**, not against the Lamedh golden files.

Reference points: Lamedh golden `tests/fixtures/golden/ir.sexpr`
(from `examples/todo.lisp`), Rust golden `rust/tests/fixtures/todo-ir.sexpr`
(from `examples/todo.gym`).

## Shapes that intentionally differ

| Concern | Lamedh | Rust | Note |
|---|---|---|---|
| Symbol case | upper (`TODO-SERVICE`) | source spelling (`todo_service`) | Rust preserves author case; hyphens become underscores in the surface |
| Node/alist nesting | record-printed nodes | `(ir-node ((id ...) (kind ...) ...))` — every alist is one nested list | see plan-phase2 `to_sexpr` comments |
| `:under` (constraint) | plist `(:VIRTUAL-USERS 500 ...)` | pair list `((virtual_users 500) ...)` | |
| Duration/latency units | `(MINUTES 30)` / `(MILLISECONDS 300)` | `(min 30)` / `(ms 300)` | surface unit names kept verbatim |
| `:model` (synthesis) | plist after head | `(small_code_model ((class nano) (temperature 0) ...))` — head + ONE combined pack | |
| `:target` (synthesis) | `(RUBY :FRAMEWORK RAILS)` | `(ruby rails)` | framework positional |
| Interface op `:input` | `(record (list ListId) ...)` | `(record ((list ListId) ...))` | pair list, one more nesting level |
| Scenario steps | `(GIVEN OWNER (AUTHENTICATED-OWNER))` etc. | `(given ((owner authenticated_owner)))` / `(when (invite_distinct owner 256))` / `(then succeeds)` | bindings are single-item packs |
| Field projections | `REQUEST/LIST` symbol | `request/list` symbol | same shape, case aside |
| Import node id | `module/import/<name>` | same, but profile paths contain `/`, so ids like `todo/import/oddities/profiles/todo_standard` cannot be split on `/` into three parts | parse ids as `module / kind / rest` |
| Profiles | registered at runtime via `putp`; todo profile never registered in the Lamedh repo | static registry in `rust/src/profile.rs`; `todo_standard` built in | Lamedh todo golden therefore has NO profile-generated nodes; the Rust one has 4, marked `:profile-source` |
| Diagnostics | record values | `(diagnostic (severity s) (code "C") (span a b) (message "..."))` | parse+check+elaboration, in that order, inside the IR |

## Shapes deliberately aligned with the Lamedh consumers

These were bugs found in review and fixed to match what `verify.lisp` /
`plan.lisp` read via `gymnast-keyword-value`:

- `fault` clauses carry `:after`, `:inject`, `:must` as separate keyword
  entries (never fused into one multi-word `:after` value).
- `coverage` lowers to keyword pairs: `(coverage :every_operation t ...)`.
- `:generate` pairs are *(variable generator)*, e.g.
  `((actor authenticated_editor) ...)` — the binding order `verify.lisp`
  consumes.
- Plural-by-contract fields are always lists, even with one element:
  `:provides`, `:uses`, `:grant`, `:deny`, `:modules`, `:reads`,
  `:writes`.
- Flow `:kind` uses the same vocabulary as op clauses: `command`/`query`,
  never the surface short forms.
- Calls keep call shape everywhere: `:execute ((create_task actor task))`,
  not key/value pairs.

## Surface-level deltas in `examples/todo.gym` vs `examples/todo.lisp`

- Predicate names lose the Lisp `?` suffix; where that collided with a
  declared name the predicate was renamed (`owner?` → `is_owner`).
- `LocalDate`/`ZonedDateTime` are spelled with the built-in lowercase
  names `local_date`/`zoned_datetime`.
- `todo.gym`'s profile import actually expands (see the profiles row
  above), so its IR contains four more type nodes than the Lamedh golden.

## Known not-yet-implemented (tracked for phase 3+)

- `[] <mode>` rows: reserved in the grammar doc, not lexed;
  `ModeExpr::Row` is currently unreachable.
- Declaration-level mode aliases / `opt`-headed / parameterized modes fall
  back to `:opaque` without a dedicated shape or diagnostic.
- Predicate/expression *type* checking (see the roadmap section of
  `docs/surface-language.md`).
- Arity checking for parameterized mode references (`Page (Task)` against
  `Page`'s declaration).
