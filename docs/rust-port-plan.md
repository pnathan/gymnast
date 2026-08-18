# Rust port, phase 1: lexer, parser, typed AST, v0 checker

Execution plan for the first Rust increment of the Lamedh → Rust migration.
This plan is deliberately over-specified: every structural decision is made
here so implementation agents only translate spec to code. Read together
with `docs/surface-language.md` (the grammar's rationale) and
`examples/todo.gym` (the acceptance fixture).

## Scope

A `rust/` cargo package, **std-only (zero dependencies)**, providing:

1. a lexer for the `.gym` surface (tokens with byte spans),
2. a recursive-descent parser producing the typed AST below,
3. span-carrying diagnostics with source-line rendering,
4. a v0 checker: symbol table, duplicate/unknown-name resolution
   (closed-world, with profile-provided names downgraded to warnings),
5. a CLI: `gymnast-rs parse <file>` and `gymnast-rs check <file>`.

Out of scope for phase 1: profile resolution, IR elaboration, planning,
predicate *type* checking (name resolution only), everything downstream.

## File tree

```
rust/
  Cargo.toml            package gymnast-rs, edition 2021, no deps
  src/
    lib.rs              pub mod span; pub mod diag; pub mod lexer;
                        pub mod ast; pub mod parser; pub mod check;
    main.rs             CLI
    span.rs             Span, Spanned helpers
    diag.rs             Diagnostic, Severity, rendering
    lexer.rs            Token, TokenKind, Lexer
    ast.rs              the AST types below, verbatim
    parser.rs           Parser
    check.rs            v0 checker
  tests/
    lexer_test.rs
    parser_test.rs
    check_test.rs
    todo_gym_test.rs    end-to-end against ../examples/todo.gym
```

Rules for all modules: `#![forbid(unsafe_code)]` in lib.rs; no panics on
user input — every malformed input becomes a `Diagnostic`; `cargo fmt`
clean; every public item has a one-line doc comment.

## span.rs

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span { pub start: usize, pub end: usize }   // byte offsets, half-open

impl Span {
    pub fn join(self, other: Span) -> Span;             // min start, max end
}
```

## diag.rs

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity { Error, Warning }

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,      // "E101" etc., from the tables below
    pub span: Span,
    pub message: String,
}
```

`pub fn render(diags: &[Diagnostic], src: &str, path: &str) -> String`
renders each as, computing line/column from the span:

```
error[E101]: unexpected token `!`, expected `=`
  --> examples/todo.gym:12:9
   |
12 | actor user ! person (...)
   |            ^
```

Sort by span start. Keep rendering simple: one label line per diagnostic,
no multi-span notes.

## lexer.rs

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Ident(String),        // [a-z_][a-z0-9_]* and [A-Z][A-Za-z0-9_]* both
    Int(i64),
    Str(String),          // "..." with \" and \\ escapes only
    // punctuation
    LParen, RParen, Comma, Semi, Colon, Eq, Bang, Dot, At, Slash,
    Lt,                   // <
    Le,                   // <=
    Arrow,                // ->
    DotDot,               // ..
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token { pub kind: TokenKind, pub span: Span }

pub struct Lexer<'a> { /* src bytes, pos */ }
impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self;
    pub fn tokenize(src: &str) -> (Vec<Token>, Vec<Diagnostic>);
}
```

Rules:

- `#` starts a comment to end of line. Newlines are whitespace.
- There are **no keyword tokens**: keywords are contextual. The lexer emits
  `Ident`; the parser matches expected identifier text. (This keeps the
  keyword set out of the lexer and makes `owner`, `list`, etc. usable as
  both attribute keys and names, which `todo.gym` requires.)
- `-` only appears in `->`; a bare `-` is diagnostic E001.
- Comparison/predicate operators: `=` lexes as `Eq`, `<` as `Lt`, `<=` as
  `Le` (longest match); `and or not` are contextual identifiers.
- Unterminated string: E002. Unknown character: E001. Integer overflow: E003.
  On any lexer error, emit the diagnostic, skip the byte, continue.

Lexer error codes: E001 unknown character, E002 unterminated string,
E003 integer literal out of range.

## ast.rs — copy these types verbatim

```rust
use crate::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Ident { pub text: String, pub span: Span }

#[derive(Debug, Clone, PartialEq)]
pub struct File {
    pub spec: SpecDecl,
    pub decls: Vec<Decl>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpecDecl {
    pub name: Ident,
    pub version: String,          // "0.1" — from `v 0.1` (Int Dot Int lexes
                                  // as Int(0) Dot Int(1); parser rebuilds the
                                  // dotted string)
    pub owner: Ident,
    pub exports: Vec<Ident>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decl {
    Use(UseDecl),
    Application(ApplicationDecl),
    Actor(ActorDecl),
    Mode(ModeDecl),
    Component(ComponentDecl),
    Interface(InterfaceDecl),
    State(StateDecl),
    Flow(FlowDecl),
    Behavior(BehaviorDecl),
    Invariant(InvariantDecl),
    Constraint(ConstraintDecl),
    Synthesis(SynthesisDecl),
    Acceptance(AcceptanceDecl),
}

#[derive(Debug, Clone, PartialEq)]
pub struct UseDecl {
    pub path: Vec<Ident>,         // oddities/profiles/todo_standard
    pub version: String,          // after @, dotted ints
    pub args: Pack,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApplicationDecl { pub name: Ident, pub attrs: Pack, pub span: Span }

#[derive(Debug, Clone, PartialEq)]
pub struct ActorDecl {
    pub name: Ident,
    pub kind: Ident,              // person
    pub attrs: Pack,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModeDecl { pub name: Ident, pub expr: ModeExpr, pub span: Span }

#[derive(Debug, Clone, PartialEq)]
pub enum ModeExpr {
    Opaque(Box<ModeExpr>),
    Enum(Vec<Ident>),
    Union(Vec<(Ident, ModeExpr)>),          // (tag, mode)
    Struct(Vec<Field>),
    Opt(Box<ModeExpr>),
    Row(Box<ModeExpr>),                     // [] M — phase-1 grammar reserves it
    Named { name: Ident, args: Vec<ModeExpr> },   // Task, Page (Task)
    Refined { name: Ident, lo: Option<i64>, hi: Option<i64> }, // text (1..200)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field { pub mode: ModeExpr, pub name: Ident }   // type-first

#[derive(Debug, Clone, PartialEq)]
pub struct ComponentDecl { pub name: Ident, pub attrs: Pack, pub span: Span }

#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceDecl {
    pub name: Ident,
    pub default_actor: Ident,     // `for user`
    pub ops: Vec<OpDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OpKind { Cmd, Qry }

#[derive(Debug, Clone, PartialEq)]
pub struct OpDecl {
    pub kind: OpKind,
    pub name: Ident,
    pub params: Vec<Field>,
    pub output: ModeExpr,
    pub errors: Vec<Ident>,       // after !
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StateDecl { pub name: Ident, pub attrs: Pack, pub span: Span }

#[derive(Debug, Clone, PartialEq)]
pub struct FlowDecl {
    pub name: Ident,
    pub from: Ident,
    pub to: Ident,
    pub kind: Ident,              // after :  (cmd)
    pub attrs: Pack,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BehaviorDecl {
    pub name: Ident,
    pub on_interface: Ident,      // todo_service
    pub on_op: Ident,             // create_task
    pub binders: Vec<Ident>,      // (user, request)
    pub attrs: Pack,              // reads/writes/atomic/idempotency items
    pub clauses: Vec<Clause>,     // semicolon-sequenced tail
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Clause {
    Requires(Pred),
    Ensures(Pred),
    Returns(Expr),
    Fails { error: Ident, when: Pred, preserves: Option<Ident> },
    Emits { event: Ident, qualifier: Vec<Ident> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct InvariantDecl {
    pub name: Ident,
    pub scope: Ident,             // on <scope>
    pub always: Pred,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstraintDecl {
    pub name: Ident,
    pub class: Ident,             // workload
    pub scope: Ident,             // on <scope>
    pub under: Pack,
    pub must: Pred,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SynthesisDecl {
    pub name: Ident,
    pub target_lang: Ident,
    pub target_framework: Option<Ident>,   // ruby / rails
    pub attrs: Pack,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AcceptanceDecl {
    pub name: Ident,
    pub subject: Ident,           // of <subject>
    pub blocks: Vec<AcceptanceBlock>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AcceptanceBlock {
    Property { name: Ident, body: Pack },
    Scenario { name: Ident, steps: Pack },
    Concurrency { name: Ident, attrs: Pack, must: Pred },
    Fault { name: Ident, body: Pack },
    Coverage(Vec<Ident>),
    Execution(Pack),
}

// ---- generic attribute packs ----

pub type Pack = Vec<PackItem>;

#[derive(Debug, Clone, PartialEq)]
pub struct PackItem { pub key: Ident, pub value: PackValue, pub span: Span }

#[derive(Debug, Clone, PartialEq)]
pub enum PackValue {
    Unit,                              // bare key: `boundaries`
    Word(Ident),                       // owner todo_app
    Int(i64),
    Str(String),
    Quantity { value: i64, unit: Ident },   // 30 min, 300 ms
    List(Vec<PackValue>),              // parenthesized value list, or
                                       // multi-word value in source order:
                                       // `aggregate per_list ListId` →
                                       // List [Word(per_list), Word(ListId)]
    Path(Vec<Ident>),                  // request.list
    Call { name: Ident, args: Vec<PackValue> },  // google_openid (issuer, subject)
    Nested(Pack),                      // small_code_model (class nano, ...) —
                                       // wrapped as Call { name,
                                       // args: [Nested(pack)] } when the
                                       // parenthesized items are key/value
                                       // pairs rather than a plain list
}
```

Pack parsing rule (the one genuinely ambiguous spot — implement exactly
this): inside `( ... )`, each comma-separated item is parsed as
`key value*` where `key` is an Ident. Then:

- no following value tokens → `Unit`
- one Int followed by an Ident that is one of `min|ms|s|sec` → `Quantity`
- one Int → `Int`; one Str → `Str`
- `(`: if the contents parse as key/value items with at least one non-Unit
  value → `Nested(pack)`, else a plain ident/value list → `List`
- an Ident chain with dots → `Path`
- Ident followed by `(` → `Call`; two-plus bare idents after key → the first
  becomes `Word` of a `Nested`-style chain: reparse as
  `key (word, rest...)`? **No.** Multi-word values are kept as
  `List` of `Word`s in source order (e.g. `aggregate per_list ListId` →
  key `aggregate`, `List [Word(per_list), Word(ListId)]`).
- single Ident → `Word`.

// ---- predicates and expressions ----

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Pred {
    And(Box<Pred>, Box<Pred>),
    Or(Box<Pred>, Box<Pred>),
    Not(Box<Pred>),
    Cmp { op: CmpOp, lhs: Expr, rhs: Expr },
    ForAll { mode: Ident, var: Ident, body: Box<Pred> },
    Exists { mode: Ident, var: Ident, body: Box<Pred> },
    Call { name: Ident, args: Vec<Expr> },   // may_edit_list (pre, user, x)
    Word(Ident),                              // bare abstract predicate name
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp { Eq, Lt, Le }

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int(i64),
    Str(String),
    Path(Vec<Ident>),                         // result, request.list, t.title
    Call { name: Ident, args: Vec<Expr> },
}
```

Predicate grammar (precedence low→high): `or` < `and` < `not` < comparison.
`for all Mode x : pred` and `exists Mode x : pred` bind loosest. A bare
call or ident in predicate position is `Pred::Call`/`Pred::Word`.
Multi-word bare predicates (`no_observation_without_active_membership` is
one ident; but `lost_updates = 0 and invariant_violations = 0` is two
comparisons) need no special case.

## parser.rs

```rust
pub struct Parser { /* tokens, pos, diags */ }
pub fn parse(src: &str) -> (Option<File>, Vec<Diagnostic>);
```

One parse function per production, named exactly:

| Production | Function |
|---|---|
| file | `parse_file` |
| spec header | `parse_spec` |
| declaration dispatch (peek first ident) | `parse_decl` |
| `use` | `parse_use` |
| `application`/`component`/`state` (name = pack) | `parse_named_pack(kind)` |
| `actor` | `parse_actor` |
| `mode` | `parse_mode_decl`, `parse_mode_expr` |
| struct fields / op params | `parse_fields` |
| `interface` | `parse_interface`, `parse_op` |
| `flow` | `parse_flow` |
| `behavior` | `parse_behavior`, `parse_clause` |
| `inv` | `parse_invariant` |
| `constraint` | `parse_constraint` |
| `synthesis` | `parse_synthesis` |
| `acceptance` | `parse_acceptance`, `parse_acceptance_block` |
| packs | `parse_pack`, `parse_pack_item`, `parse_pack_value` |
| predicates | `parse_pred`, `parse_pred_or`, `parse_pred_and`, `parse_pred_not`, `parse_pred_atom` |
| expressions | `parse_expr` |

Behavior body layout (matches `todo.gym`): after
`on iface.op (binders) (`, parse pack items (comma-separated) until a `;`
follows an item instead of a comma; from there, semicolon-separated
clauses (`requires` / `ensures` / `returns` / `fails` / `emits`) until `)`.
Implementation: parse comma-separated pack items; when the next separator
is `;`, switch to clause mode. A clause keyword appearing where a pack key
is expected also switches modes (tolerates a comma before the first
clause).

`fails E when P preserves X` — `when` and `preserves` are contextual
idents inside the clause; `preserves` optional.

Error recovery: on unexpected token, emit E101
(`unexpected token, expected ...`), then skip tokens until the next
top-level declaration keyword (`use application actor mode component
interface state flow behavior inv constraint synthesis acceptance`) at
paren-depth 0, and continue. Parser never aborts; it returns all
diagnostics found.

Parser error codes: E101 unexpected token, E102 unclosed `(` (report at
the opening paren), E103 malformed version literal, E104 unknown
declaration keyword.

## check.rs — v0 checker

```rust
pub fn check(file: &File) -> Vec<Diagnostic>;
```

Build one symbol table over declaration names by namespace: modes, actors,
interfaces (and their ops), states, components, behaviors, flows.
Built-in modes (pre-populated): `text int bool local_date zoned_datetime
void`. Checks, in order:

- E201 duplicate declaration name within a namespace (second site).
- E202 unknown mode referenced from a struct field, op param, op output,
  opaque/opt/row argument. **Downgrade to W301** only when the file has a
  `use` declaration whose profile cannot be resolved (the name may be
  provided by that unresolvable profile). Historical note: before phase
  2 ported profile resolution, the downgrade applied to any `use`; that
  loophole is closed. Suggest the nearest declared name by
  edit distance ≤ 2 in the message when one exists.
- E203 `interface ... for X`: X must be a declared actor.
- E204 behavior `on iface.op`: iface must be a declared interface, op a
  declared op in it (E204 covers both, message distinguishes).
- E205 `inv ... on S` / `constraint ... on S`: S must be a declared state,
  interface, or component.
- E206 export list: every exported name is declared somewhere (W301
  downgrade rule applies).
- W302 mode name not capitalized / non-mode declaration name capitalized.

Predicate/expr name resolution and typing are phase 2 — do not check
inside `Pred`/`Expr` at all in v0.

## main.rs

```
usage: gymnast-rs <parse|check> FILE.gym
```

- `parse`: lex+parse; print `{:#?}` of the AST to stdout; render
  diagnostics to stderr; exit 1 if any Error-severity diagnostic, else 0.
- `check`: parse then check; render all diagnostics; same exit rule.
- Bad usage: print usage, exit 2. Unreadable file: message to stderr, exit 2.

## Tests (acceptance criteria)

`tests/lexer_test.rs` — at minimum: punctuation round-trip including `->`
vs `-`, `..` vs `.`, `<=` vs `<`; comments skipped; string escapes;
unterminated string produces E002 and lexing continues; spans are correct
byte offsets (assert exact start/end for a two-line input).

`tests/parser_test.rs` — at minimum, each as its own `#[test]`: a minimal
spec header; one of each mode form (`opaque`, `enum`, `union`, `struct`
with refined text and `opt`); an interface with two ops and error sets; a
behavior with all five clause kinds; an invariant with `for all`; a
predicate with `and`/`or`/`not` precedence asserted structurally; error
recovery — two malformed declarations still yield diagnostics for both
plus the valid declarations around them.

`tests/check_test.rs` — duplicate mode (E201); unknown mode without `use`
(E202) and with `use` (W301); bad `for` actor (E203); bad behavior target
(E204); capitalization warning (W302).

`tests/todo_gym_test.rs` — read `../examples/todo.gym`
(`concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/todo.gym")`):
parse yields a `File` with **zero error diagnostics**; assert the decl
count and that mode `Task` is a 9-field struct, interface `todo_service`
has 3 ops, behavior `create_task` has 6 clauses... count clauses from the
fixture; `check` yields zero errors (warnings allowed) — the fixture has
a `use`, so unknown profile names are W301.

Definition of done for the phase: `cargo build` warning-free,
`cargo test` green, `cargo fmt --check` clean, both CLI commands work on
`examples/todo.gym`.

## Agent etiquette

Implementation agents: own only your assigned file(s); never edit another
module except `lib.rs` re-exports if missing. If the AST or plan seems
wrong, implement the plan as written and note the concern in your report —
do not redesign. Integration agents may touch anything.
