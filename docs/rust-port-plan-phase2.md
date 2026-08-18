# Rust port, phase 2: profile resolution, elaboration, canonical IR

Execution plan for the second Rust increment. Like phase 1
(`docs/rust-port-plan.md`), this is deliberately over-specified: every
structural decision is made here so implementation agents only translate
spec to code. The Lamedh reference semantics live in `src/elaborate.lisp`,
`src/profile.lisp`, `src/serialize.lisp`, and `src/core.lisp` — mirror
their behavior, not their letter.

## Scope

Extend the existing `rust/` crate (still **std-only, zero dependencies**)
with:

1. an `Sexpr` value type and canonical printer (the serialization
   boundary — Lamedh-shaped text),
2. FNV-1a 64 fingerprinting,
3. a profile registry with the built-in `oddities/profiles/todo_standard`
   v1.0 profile,
4. an elaborator: checked AST → semantic IR with node partitioning,
   semantic IDs, closed-world diagnostics, and a fingerprint,
5. a CLI `ir` subcommand emitting the canonical serialization,
6. a determinism test (two elaborations → byte-identical bytes) and a
   golden IR fixture for `examples/todo.gym`.

Out of scope: planning (the 8-node DAG), prompts, anything downstream.
Predicate *type* checking also stays out — elaboration lowers predicates
structurally.

## New files

```
rust/src/
  sexpr.rs         Sexpr type + canonical printer
  fingerprint.rs   FNV-1a 64
  ir.rs            IrNode, Ir, partition logic, to_sexpr
  profile.rs       Profile registry + todo_standard built-in
  elaborate.rs     AST -> Ir lowering + elaboration diagnostics
rust/tests/
  sexpr_test.rs
  ir_test.rs
  profile_test.rs
  elaborate_test.rs
  golden_ir_test.rs
rust/tests/fixtures/
  todo-ir.sexpr    golden canonical IR for examples/todo.gym
```

`lib.rs` gains the five module declarations. Existing modules are not
modified except: `main.rs` (new subcommand) and `check.rs` (one new public
helper, below).

## sexpr.rs — copy verbatim

```rust
/// A Lisp-shaped value: the canonical serialization boundary.
#[derive(Debug, Clone, PartialEq)]
pub enum Sexpr {
    Sym(String),
    Str(String),
    Int(i64),
    List(Vec<Sexpr>),
}

impl Sexpr {
    pub fn sym(s: &str) -> Sexpr { Sexpr::Sym(s.to_string()) }
    pub fn list(items: Vec<Sexpr>) -> Sexpr { Sexpr::List(items) }
    /// (key value) pair used in field alists.
    pub fn pair(key: &str, value: Sexpr) -> Sexpr {
        Sexpr::List(vec![Sexpr::sym(key), value])
    }
    /// Canonical single-line printing:
    ///   Sym  -> bare text
    ///   Str  -> "..." with only \" and \\ escaped
    ///   Int  -> decimal
    ///   List -> ( item item ... ) with single spaces, no trailing space;
    ///           the empty list prints as nil
    pub fn print(&self) -> String;
}

/// Canonical serialization: print + one trailing newline (LF).
pub fn canonical_serialize(value: &Sexpr) -> String;
```

The printer is total and deterministic; no whitespace choices beyond the
rules above. `(a (b 1) "x")` prints exactly as `(a (b 1) "x")`.

## fingerprint.rs — copy verbatim

```rust
/// FNV-1a 64 over the UTF-8 bytes of `text`, formatted like the Lamedh
/// implementation: "fnv1a64:" + the hash printed as a SIGNED i64.
pub fn fingerprint_string(text: &str) -> String {
    let mut hash: u64 = 0xCBF29CE484222325;
    for b in text.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    format!("fnv1a64:{}", hash as i64)
}

/// Fingerprint of a value: hash of its canonical print (no newline).
pub fn fingerprint(value: &crate::sexpr::Sexpr) -> String {
    fingerprint_string(&value.print())
}
```

## ir.rs

```rust
use crate::sexpr::Sexpr;

/// One semantic IR node. Fields are canonically sorted by key at
/// construction; clause order is preserved (sequence can be semantic).
#[derive(Debug, Clone, PartialEq)]
pub struct IrNode {
    pub id: String,                       // module/kind/name
    pub kind: String,                     // "type", "behavior", ...
    pub name: String,
    pub fields: Vec<(String, Sexpr)>,     // keys like ":owner", sorted
    pub clauses: Vec<Sexpr>,
    pub mechanism: String,                // "parsed" for every phase-2 node
}

impl IrNode {
    /// Sorts `fields` by key (byte-wise string order) before storing.
    pub fn new(id: String, kind: &str, name: String,
               mut fields: Vec<(String, Sexpr)>, clauses: Vec<Sexpr>) -> IrNode;
    pub fn to_sexpr(&self) -> Sexpr;
    // (ir-node ((id "...") (kind type) (name Task)
    //           (fields ((:key value) ...)) (clauses (...)) (mechanism parsed)))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ir {
    pub schema: String,                   // "gymnast.ir/0.1"
    pub module_name: String,
    pub module_fields: Vec<(String, Sexpr)>,  // sorted like node fields
    pub design: Vec<IrNode>,
    pub transitions: Vec<IrNode>,
    pub obligations: Vec<IrNode>,
    pub synthesis: Vec<IrNode>,
    pub diagnostics: Vec<Sexpr>,          // already-lowered diagnostics
    pub fingerprint: String,
}

impl Ir {
    pub fn all_nodes(&self) -> Vec<&IrNode>;   // design ++ transitions ++ obligations ++ synthesis
    pub fn has_errors(&self) -> bool;          // any (severity error) diagnostic
    pub fn to_sexpr(&self) -> Sexpr;
    // (ir ((schema "gymnast.ir/0.1")
    //      (module ((name todo) (fields (...))))
    //      (design (...)) (transitions (...)) (obligations (...))
    //      (synthesis (...)) (diagnostics (...)) (fingerprint "fnv1a64:...")))
}
```

Partition membership (exactly the Lamedh sets):

- **design**: import, application, actor, type, component, interface,
  state, flow
- **transitions**: behavior
- **obligations**: invariant, constraint, acceptance
- **synthesis**: synthesis

Nodes are sorted by `id` (byte-wise) within each partition.

The IR fingerprint is computed over the `Ir::to_sexpr()` of the value
**with the fingerprint entry absent**, then appended — mirror of
`gymnast-elaborate`'s `(append base (list (list 'fingerprint ...)))`.

## profile.rs

```rust
use crate::ast::Decl;
use crate::sexpr::Sexpr;

pub enum ParamDefault { Required, Value(Sexpr) }

pub struct Param { pub key: &'static str, pub default: ParamDefault }

/// A resolved profile: name, version, parameters, and a generator that
/// produces ordinary declarations (as AST Decls) from validated args.
pub struct Profile {
    pub name: &'static str,               // "oddities/profiles/todo_standard"
    pub version: &'static str,            // "1.0"
    pub params: Vec<Param>,
    pub generate: fn(&[(String, Sexpr)]) -> Vec<Decl>,
}

/// Look up a built-in profile by (name, version). The registry is a
/// static table — deterministic, no global mutable state.
pub fn lookup(name: &str, version: &str) -> Option<Profile>;
```

Built-in `oddities/profiles/todo_standard` v1.0:

- params: `sharing_limit` (Required), `identity_provider` (Required)
- generator returns four mode declarations (construct AST `Decl::Mode`
  values directly with `Span { start: 0, end: 0 }` — they have no
  surface location):
  - `Cursor` = opaque `text`
  - `Page` = opaque `text`
  - `Membership` = struct (ListId list, UserId principal, Role role,
    Version version)
  - `Invitation` = struct (ListId list, UserId principal, Role role,
    Version version)

The elaborator marks every generated node with the field
`":profile-source"` = the profile name symbol, so provenance survives
into the IR.

`check.rs` change: add `pub fn profile_provided_names() -> &'static [&'static str]`
returning `["Cursor", "Page", "Membership", "Invitation"]` is **not**
done — instead the elaborator runs profile expansion BEFORE the checker,
so the checker sees the generated mode declarations, so profile-provided
names resolve like any declared name. Unknown names remain hard
closed-world errors (E202/E206); the W301 downgrade applies ONLY when
the file contains a `use` whose profile could not be resolved (W303),
since only then could a name plausibly be profile-provided. The public entry point
for this is in elaborate.rs (below); `check::check` itself is unchanged.

## elaborate.rs

```rust
use crate::ast::*;
use crate::diag::Diagnostic;
use crate::ir::Ir;

/// Elaborate a parsed file: expand profiles, check, lower to IR.
/// Diagnostics from checking and elaboration are folded into the IR;
/// the caller inspects Ir::has_errors().
pub fn elaborate(file: &File) -> Ir;
```

Pipeline inside `elaborate`:

1. **Profile expansion.** For each `Decl::Use`: look up
   `(path joined with "/", version)`. Unknown profile → diagnostic
   `W303 unknown-profile` (warning; the import node is still emitted).
   Known profile: validate args against params — each `Required` param
   with no argument → `E302 missing-profile-decision` (error, and the
   profile generates nothing); otherwise call the generator and splice
   the generated Decls into the working declaration list immediately
   after the use declaration.
2. **Check.** Run `check::check` on a `File` containing the expanded
   declaration list. Its diagnostics are folded into the IR diagnostics
   (lowered per the table below).
3. **Lowering.** Every declaration (including the `use` itself and the
   generated ones) becomes one `IrNode` per the lowering table.
   Semantic id: `<spec-name>/<kind>/<name>`, each part spelled exactly as
   written in source (no case folding).
4. **Duplicate ids.** After lowering, a second occurrence of any id →
   `E301 duplicate-semantic-id` (error), reported once per duplicate
   occurrence, subject = the id.
5. **Assemble.** Partition (sorted by id), build `Ir`, compute the
   fingerprint last.

Everything is pure; iteration orders are the source order or sorted —
never a hash map order.

### Lowering table: declarations

Field keys are strings with a leading colon (`":owner"`); values are
Sexprs. Symbols come through as `Sexpr::Sym` with source spelling.
`PackValue` lowering (used everywhere a pack appears):

| PackValue | Sexpr |
|---|---|
| Unit | `Sym("t")` |
| Word(w) | `Sym(w)` |
| Int(i) | `Int(i)` |
| Str(s) | `Str(s)` |
| Quantity{value, unit} | `(unit value)` e.g. `(min 30)` |
| List(vs) | `(v1 v2 ...)` |
| Path(segs) | `Sym(segs.join("/"))` e.g. `request/list` |
| Call{name, args} | `(name arg1 arg2 ...)` |
| Nested(items) | `((key1 v1) (key2 v2) ...)` — keys WITHOUT colon |

A whole attrs `Pack` on a declaration lowers to node fields: each
`PackItem{key, value}` becomes `(":" + key, lowered value)`.

Per declaration kind (kind string, name, extra fields, clauses):

| Decl | kind | fields | clauses |
|---|---|---|---|
| Use | `import` | `:version` Str, `:arguments` Nested-style list of `(key value)`, `:authority` Sym(`authoritative`) | none |
| Application | `application` | attrs pack | none |
| Actor | `actor` | `:kind` Sym(actor kind) + attrs pack | none |
| Mode | `type` | exactly one shape field, see mode table | none |
| Component | `component` | attrs pack | none |
| Interface | `interface` | `:for` Sym(default_actor) | one clause per op, see below |
| State | `state` | attrs pack | none |
| Flow | `flow` | `:from`, `:to`, `:kind` Syms + attrs pack | none |
| Behavior | `behavior` | `:on` `(interface/op binder1 binder2)` — first element is `Sym("iface/op")` — + attrs pack | one per clause, below |
| Invariant | `invariant` | `:scope` Sym, `:always` lowered pred | none |
| Constraint | `constraint` | `:class`, `:scope` Syms, `:under` fields-list `((key value) ...)`, `:must` lowered pred | none |
| Synthesis | `synthesis` | `:target` `(lang framework)` or `(lang)` + attrs pack | none |
| Acceptance | `acceptance` | `:subject` Sym | one per block, below |

Mode shape field (aligning with the Lamedh type vocabulary):

| ModeExpr at decl top level | field |
|---|---|
| Opaque(m) | `":opaque"` = lowered m |
| Enum(names) | `":enum"` = `(n1 n2 ...)` |
| Union(variants) | `":variant"` = `((tag mode) ...)` |
| Struct(fields) | `":record"` = `((name mode) ...)` — name FIRST in the pair |
| anything else | `":opaque"` = lowered expr |

ModeExpr lowering (inside shapes, params, outputs):

| ModeExpr | Sexpr |
|---|---|
| Named{name, args: []} | `Sym(name)` |
| Named{name, args} | `(name arg1 ...)` |
| Refined{name, lo, hi} | `(name :min lo :max hi)` — omit `:min`/`:max` pair when the bound is None |
| Opt(m) | `(Optional m)` |
| Row(m) | `(Row m)` |
| Opaque/Enum/Union/Struct nested | same shapes as the table above, as a `(shape ...)` list, e.g. `(record ((id TaskId) ...))` |

Interface op clause:

```
(command create_task :actor user
  :input (record ((list ListId) (title text) (due (Optional Due))))
  :output Task
  :errors (unauthenticated forbidden not_found conflict))
```

`cmd` → head `command`, `qry` → head `query`; `:actor` is the interface's
default actor.

Behavior clauses:

| Clause | Sexpr |
|---|---|
| Requires(p) | `(requires <pred>)` |
| Ensures(p) | `(ensures <pred>)` |
| Returns(e) | `(returns <expr>)` |
| Fails{error, when, preserves} | `(fails <error> :when <pred> :preserves <sym>)`, `:preserves` omitted when None |
| Emits{event, qualifier} | `(emits <event> q1 q2 ...)` |

Predicate / expression lowering:

| Pred / Expr | Sexpr |
|---|---|
| Cmp Eq/Lt/Le | `(= l r)` / `(< l r)` / `(<= l r)` |
| And(a,b) / Or(a,b) | `(and a b)` / `(or a b)` (binary, as parsed) |
| Not(p) | `(not p)` |
| ForAll{mode, var, body} | `(forall ((var Mode)) body)` |
| Exists{mode, var, body} | `(exists ((var Mode)) body)` |
| Pred::Call / Expr::Call | `(name a1 a2 ...)` |
| Pred::Word(w) | `Sym(w)` |
| Expr::Int / Str | Int / Str |
| Expr::Path(segs) | `Sym(segs.join("/"))` |

Acceptance block clauses:

| Block | Sexpr |
|---|---|
| Property{name, body} | `(property name :k1 v1 :k2 v2 ...)` — body pack items as keyword pairs in source order |
| Scenario{name, steps} | `(scenario name (step-key value) ...)` in source order |
| Concurrency{name, attrs, must} | `(concurrency name :k v ... :must <pred>)` |
| Fault{name, body} | `(fault name :k v ...)` |
| Coverage(names) | `(coverage n1 n2 ...)` |
| Execution(pack) | `(execution :k v ...)` |

### Diagnostics lowering

Every `diag::Diagnostic` (parser/checker/elaborator alike) lowers to:

```
(diagnostic (severity error|warning) (code "E202") (span start end)
            (message "..."))
```

Elaborator-specific codes: `E301` duplicate-semantic-id (span = whole
declaration), `E302` missing-profile-decision (span = the use
declaration), `W303` unknown-profile (span = the use declaration).
Diagnostics appear in the IR in this order: parse/check diagnostics
first (their existing order), then elaboration diagnostics in
declaration order. No sorting by span.

## main.rs

Add subcommand `ir`:

```
gymnast-rs ir FILE.gym
```

Parse → elaborate → print `canonical_serialize(ir.to_sexpr())` to stdout
(one line + newline). Diagnostics render to stderr as today. Exit 1 if
any error-severity diagnostic (parse or IR), else 0. `parse` and `check`
subcommands are unchanged.

## Tests (acceptance criteria)

`sexpr_test.rs` — print round-trips for each variant; empty list prints
`nil`; string escaping (`"` and `\` only, UTF-8 passes through);
`canonical_serialize` ends with exactly one LF.

`ir_test.rs` — IrNode::new sorts fields by key; partition membership for
each of the 13 kinds; nodes sorted by id inside partitions; fingerprint
excludes the fingerprint field (construct the same Ir twice, second time
via to_sexpr with fingerprint present, assert stored fingerprint matches
recomputation on the fingerprint-free form).

`profile_test.rs` — lookup hit and miss; missing required param produces
E302 via elaborate; generated declarations carry `:profile-source` in
the IR; providing both params generates exactly 4 type nodes.

`elaborate_test.rs` — at minimum, each as its own `#[test]`: semantic id
format for a mode; duplicate mode name yields E301 (and E201 from the
checker — both present); interface lowers to one node with 2 clauses and
`:for` field; behavior `:on` field is `(iface/op actor request)`; a
`fails` clause with and without `preserves`; forall invariant lowering
matches the table exactly (assert the printed string); unknown profile →
W303 warning, import node still present; expansion order — generated
nodes appear in the IR (they sort by id like everything else).

`golden_ir_test.rs` —

1. elaborate `../examples/todo.gym` twice from two separate parses:
   canonical serializations are byte-identical (determinism gate);
2. zero error diagnostics; the only warnings permitted are W302
   capitalization warnings, and after profile expansion there must be NO
   W301 warnings (todo_standard provides Cursor/Page/Membership/
   Invitation);
3. structural counts — derive by reading the fixture, then assert:
   design 21 (1 import + 1 application + 1 actor + 14 type [10 declared
   + 4 profile-generated] + 1 component + 1 interface + 1 state +
   1 flow), transitions 2, obligations 4, synthesis 1;
4. compare against `tests/fixtures/todo-ir.sexpr` byte-exact. The agent
   that writes this test GENERATES the fixture by running
   `cargo run -- ir ../examples/todo.gym > tests/fixtures/todo-ir.sexpr`
   AFTER verifying (1)–(3), and commits it as the golden. A comment at
   the top of the test explains regeneration.

Definition of done: `cargo build` warning-free, full `cargo test` green,
`cargo fmt --check` clean, `gymnast-rs ir ../examples/todo.gym` exits 0
and its output is stable across runs.

## CI

Extend the existing `rust` job in `.github/workflows/ci.yml` with a
reproducibility step after the fixture check:

```yaml
      - name: Reproducible IR
        run: |
          cargo run -- ir ../examples/todo.gym > /tmp/ir-one.sexpr
          cargo run -- ir ../examples/todo.gym > /tmp/ir-two.sexpr
          diff /tmp/ir-one.sexpr /tmp/ir-two.sexpr
          diff /tmp/ir-one.sexpr tests/fixtures/todo-ir.sexpr
```

## Agent etiquette

Same rules as phase 1: own only your assigned files (plus `lib.rs`
re-exports); every loop you write must consume input every iteration —
phase 1's review found infinite loops behind this exact omission; no
panics on user input; if the plan seems wrong, implement it as written
and note the concern in your report. Integration agents may touch
anything.
