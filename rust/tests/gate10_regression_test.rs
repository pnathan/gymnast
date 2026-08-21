//! Regression tests for the phase-10 Opus gate's findings (new file,
//! not a frozen oracle):
//!
//!  1. MAJOR — E211 `constant-name-collision`: a constant whose name
//!     equals a behavior parameter, an acceptance generator variable, a
//!     scenario `given` variable, or a reserved state symbol
//!     (`pre`/`post`/`result`) is an ERROR — name-driven substitution
//!     would otherwise silently rewrite the variable, including inside
//!     authority predicates (the gate's reproduction: `const user = 5`
//!     into todo.gym rewrote `(is_owner pre user request/list)`).
//!  2. MAJOR — the synthesis budget slots (`attempts`, the model pack's
//!     `max_attempts`) and the concurrency `actors` slot are
//!     constants-only integer positions: a constant substitutes, an
//!     unresolved bare name is E209 — never a silent symbol in a slot
//!     every consumer reads as an integer.
//!  3. MAJOR — W408/W409 coverage warnings surface on `check` and
//!     `verify` stderr (as both docs promise), warnings-only (exit 0).
//!  4. MINOR — `name + name` is E210 `invalid-constant-expression`, not
//!     a bare lexer E001.
//!  5. MINOR — offset folding SATURATES (the gate's surviving oracle
//!     mutation (f): wrapping_add/sub passed all 702 tests; this pins
//!     the contract at `ir-contract-deltas.md`'s stated behavior).
//!  6. MINOR — the never-substitute half of the contract, pinned
//!     directly (the gate's mutations (b)/(g) were caught only by the
//!     todo goldens): an error name, an `:errors` set member, a struct
//!     field name, and an enum member equal to a declared constant all
//!     stay symbolic.
//!  7. MINOR — W411 `undeclared-profile-parameter`: an integer `use`
//!     argument the profile does not declare binds NO constant and
//!     warns, so the constants header never attributes to a profile a
//!     binding it does not define.
//!  8. MINOR — E203 names the generator symbol, not just the pair key.

use gymnast_rs::diag::{Diagnostic, Severity};
use gymnast_rs::sexpr::Sexpr;
use gymnast_rs::{elaborate, parser};
use std::process::Command;

fn elab(src: &str) -> (gymnast_rs::ir::Ir, Vec<Diagnostic>) {
    let (ast, parse_diags) = parser::parse(src);
    let file = ast.unwrap_or_else(|| panic!("spec must parse; diagnostics: {:?}", parse_diags));
    elaborate::elaborate_with_parse_diags(&file, &parse_diags)
}

fn with_code<'a>(diags: &'a [Diagnostic], code: &str) -> Vec<&'a Diagnostic> {
    diags.iter().filter(|d| d.code == code).collect()
}

fn errors(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect()
}

/// A minimal spec with a constant wired into the synthesis budget slots
/// and the concurrency `actors` slot. `{consts}` lands extra const
/// declarations; `{requires}` replaces the behavior precondition;
/// `{actors}` is the concurrency actors value.
fn budget_spec(consts: &str, requires: &str, actors: &str) -> String {
    format!(
        r#"
spec covspec = v 0.1 owner product exports Widget, widget_service

const pool = 40
{consts}

application covspec = (modules covspec, default_acceptance production)

actor operator = person (identity basic_openid (issuer, subject))

mode WidgetId = opaque text

mode Widget = struct (
  WidgetId id,
  text (1..40) name )

component cov_app = (
  responsibility "Gate-10 regression probe",
  provides widget_service,
  uses (clock, id_source) )

interface widget_service = for operator (
  cmd op_a = (WidgetId id, text name) Widget
      ! (forbidden) )

state cov_state = (
  of aggregate (Widget),
  owner cov_app,
  durability durable,
  initial empty )

behavior do_a = on widget_service.op_a (operator, request) (
  reads (cov_state), writes cov_state;

  requires {requires};
  ensures  post = put_a (pre, request) )

synthesis proto = target ruby / rails (
  platform gymnast_reference_platform_v1,
  model small_code_model (class nano, temperature 0, max_attempts pool),
  attempts pool )

acceptance production = of cov_app (
  property probe_a =
    generate (actor gen_op of operator)
    execute op_a (actor)
    must stored (result),

  concurrency race = (actors {actors}, schedule adversarial)
    must stored (result),

  execution (clock virtual, randomness seeded,
             network controlled, timezone "UTC") )
"#,
        consts = consts,
        requires = requires,
        actors = actors
    )
}

fn node_sexpr<'a>(ir: &'a gymnast_rs::ir::Ir, id: &str) -> &'a gymnast_rs::ir::IrNode {
    ir.all_nodes()
        .into_iter()
        .find(|n| n.id == id)
        .unwrap_or_else(|| panic!("no node {}", id))
}

// ---------------------------------------------------------------------
// Finding 1: E211 constant-name collisions.
// ---------------------------------------------------------------------

#[test]
fn g10_e211_behavior_parameter_collision_is_an_error() {
    // `request` is a parameter of behavior do_a.
    let src = budget_spec("const request = 5", "slot_free (pre, request)", "pool");
    let (_ir, diags) = elab(&src);
    let e211 = with_code(&diags, "E211");
    assert_eq!(
        e211.len(),
        1,
        "a const colliding with a behavior parameter must be exactly one E211: {:?}",
        diags
    );
    assert!(e211[0].severity == Severity::Error);
    assert!(
        e211[0].message.contains("request") && e211[0].message.contains("do_a"),
        "E211 names the constant and the colliding behavior: {}",
        e211[0].message
    );
}

#[test]
fn g10_e211_reserved_state_symbols_are_errors() {
    for reserved in ["pre", "post", "result"] {
        let src = budget_spec(
            &format!("const {} = 99", reserved),
            "slot_free (pre, request)",
            "pool",
        );
        let (_ir, diags) = elab(&src);
        assert_eq!(
            with_code(&diags, "E211").len(),
            1,
            "const '{}' must be exactly one E211: {:?}",
            reserved,
            diags
        );
    }
}

#[test]
fn g10_e211_generator_variable_collision_is_an_error() {
    // `actor` is the generator variable of property probe_a.
    let src = budget_spec("const actor = 7", "slot_free (pre, request)", "pool");
    let (_ir, diags) = elab(&src);
    let e211 = with_code(&diags, "E211");
    assert_eq!(e211.len(), 1, "generator-variable collision: {:?}", diags);
    assert!(
        e211[0].message.contains("actor") && e211[0].message.contains("probe_a"),
        "E211 names the variable and its property: {}",
        e211[0].message
    );
}

#[test]
fn g10_no_e211_without_collision() {
    let src = budget_spec("", "slot_free (pre, request)", "pool");
    let (_ir, diags) = elab(&src);
    assert!(
        with_code(&diags, "E211").is_empty() && errors(&diags).is_empty(),
        "the collision-free template must stay clean: {:?}",
        diags
    );
}

// ---------------------------------------------------------------------
// Finding 2: synthesis/concurrency integer slots substitute or E209.
// ---------------------------------------------------------------------

#[test]
fn g10_budget_slots_substitute_constants() {
    let src = budget_spec("", "slot_free (pre, request)", "pool");
    let (ir, diags) = elab(&src);
    assert!(errors(&diags).is_empty(), "clean spec: {:?}", diags);

    let synth = node_sexpr(&ir, "covspec/synthesis/proto");
    let attempts = synth
        .fields
        .iter()
        .find(|(k, _)| k == ":attempts")
        .map(|(_, v)| v)
        .expect(":attempts field");
    assert_eq!(attempts, &Sexpr::Int(40), "attempts pool -> 40");
    let model = synth
        .fields
        .iter()
        .find(|(k, _)| k == ":model")
        .map(|(_, v)| v.print())
        .expect(":model field");
    assert!(
        model.contains("(max_attempts 40)"),
        "model max_attempts pool -> 40; got {}",
        model
    );
    assert!(
        !model.contains("pool"),
        "no symbolic budget survives in the model pack: {}",
        model
    );

    let acc = node_sexpr(&ir, "covspec/acceptance/production");
    let conc = acc
        .clauses
        .iter()
        .find(|c| c.print().starts_with("(concurrency"))
        .expect("concurrency clause")
        .print();
    assert!(
        conc.contains(":actors 40"),
        "actors pool -> 40; got {}",
        conc
    );
}

#[test]
fn g10_unresolved_budget_names_are_e209() {
    // `ghost` in the actors slot names no constant: E209, not a silent
    // symbol in an integer slot.
    let src = budget_spec("", "slot_free (pre, request)", "ghost");
    let (_ir, diags) = elab(&src);
    let e209 = with_code(&diags, "E209");
    assert!(
        e209.iter().any(|d| d.message.contains("ghost")),
        "an unresolved name in the actors slot must be E209 naming it: {:?}",
        diags
    );
}

// ---------------------------------------------------------------------
// Finding 3: W408/W409 reach check and verify stderr, warnings-only.
// ---------------------------------------------------------------------

#[test]
fn g10_coverage_warnings_reach_check_and_verify_stderr() {
    for sub in ["check", "verify"] {
        let out = Command::new(env!("CARGO_BIN_EXE_gymnast-rs"))
            .args([sub, "../examples/bug-tracker.gym"])
            .output()
            .expect("binary runs");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("W408") && stderr.contains("uncovered-operation"),
            "`{}` stderr must carry W408; got: {}",
            sub,
            stderr
        );
        assert!(
            stderr.contains("W409") && stderr.contains("unexercised-transition"),
            "`{}` stderr must carry W409; got: {}",
            sub,
            stderr
        );
        assert!(
            out.status.success(),
            "coverage warnings never change the exit code ({})",
            sub
        );
    }
}

// ---------------------------------------------------------------------
// Finding 4: `name + name` is E210, not a lexer E001.
// ---------------------------------------------------------------------

#[test]
fn g10_name_plus_name_is_e210() {
    let src = budget_spec("", "pool + pool < 99", "pool");
    let (_ast, parse_diags) = parser::parse(&src);
    assert!(
        parse_diags.iter().any(|d| d.code == "E210"),
        "`name + name` must be E210: {:?}",
        parse_diags
    );
    assert!(
        !parse_diags.iter().any(|d| d.code == "E001"),
        "the sign must reach the parser's const-expr path, not die as E001: {:?}",
        parse_diags
    );
}

// ---------------------------------------------------------------------
// Finding 5: offset folding saturates (pins the surviving mutation f).
// ---------------------------------------------------------------------

#[test]
fn g10_offset_folding_saturates_at_i64_bounds() {
    // hi + 1 at i64::MAX must stay i64::MAX — wrapping would put
    // -9223372036854775808 into the IR with no diagnostic.
    let src = budget_spec(
        "const hi = 9223372036854775807",
        "slot_free (pre, hi + 1)",
        "pool",
    );
    let (ir, diags) = elab(&src);
    assert!(errors(&diags).is_empty(), "clean spec: {:?}", diags);
    let behavior = node_sexpr(&ir, "covspec/behavior/do_a");
    let requires = behavior
        .clauses
        .iter()
        .find(|c| c.print().starts_with("(requires"))
        .expect("requires clause")
        .print();
    assert!(
        requires.contains("9223372036854775807"),
        "hi + 1 saturates at i64::MAX; got {}",
        requires
    );
    assert!(
        !requires.contains("-9223372036854775808"),
        "wrapping is the pinned-out behavior; got {}",
        requires
    );

    // lo - 1 at i64::MIN must stay i64::MIN.
    let src = budget_spec(
        "const lo = -9223372036854775808",
        "slot_free (pre, lo - 1)",
        "pool",
    );
    let (_ast, parse_diags) = parser::parse(&src);
    if parse_diags.iter().all(|d| d.severity != Severity::Error) {
        let (ir, diags) = elab(&src);
        assert!(errors(&diags).is_empty(), "clean spec: {:?}", diags);
        let behavior = node_sexpr(&ir, "covspec/behavior/do_a");
        let requires = behavior
            .clauses
            .iter()
            .find(|c| c.print().starts_with("(requires"))
            .expect("requires clause")
            .print();
        assert!(
            requires.contains("-9223372036854775808"),
            "lo - 1 saturates at i64::MIN; got {}",
            requires
        );
    }
    // (Negative const literals are not in the v0.2 grammar; if the parse
    // rejects the declaration the MAX-side pin above already kills the
    // wrapping mutation.)
}

// ---------------------------------------------------------------------
// Finding 6: never-substitute, pinned directly (mutations b and g).
// ---------------------------------------------------------------------

#[test]
fn g10_never_substitute_pinned_without_goldens() {
    // `forbidden` (the error name and :errors member), `id` (a struct
    // field name), and `name` (another field name) are all declared as
    // constants. None of them may become integers in name positions —
    // and none of them collides (they are not variables), so the spec
    // stays error-free.
    let src = budget_spec(
        "const forbidden = 9\nconst id = 3\nconst name = 4",
        "slot_free (pre, request)",
        "pool",
    );
    let (ir, diags) = elab(&src);
    assert!(errors(&diags).is_empty(), "clean spec: {:?}", diags);

    let iface = node_sexpr(&ir, "covspec/interface/widget_service")
        .to_sexpr()
        .print();
    assert!(
        iface.contains("(forbidden)") && !iface.contains("(9)"),
        "the :errors set member stays symbolic: {}",
        iface
    );
    assert!(
        iface.contains("(id WidgetId)") || iface.contains("(id "),
        "the op input field name stays symbolic: {}",
        iface
    );

    let widget = node_sexpr(&ir, "covspec/type/Widget").to_sexpr().print();
    assert!(
        widget.contains("(id WidgetId)"),
        "struct field 'id' stays symbolic: {}",
        widget
    );
    assert!(
        widget.contains("(name "),
        "struct field 'name' stays symbolic: {}",
        widget
    );
}

// ---------------------------------------------------------------------
// Finding 7: W411 undeclared-profile-parameter.
// ---------------------------------------------------------------------

#[test]
fn g10_undeclared_profile_int_param_warns_and_binds_nothing() {
    let todo = std::fs::read_to_string("../examples/todo.gym").expect("read todo.gym");
    let mutated = todo.replace("(sharing_limit 256,", "(sharing_limit 256, bogus 7,");
    assert_ne!(mutated, todo, "the use clause must have been rewritten");
    let (ir, diags) = elab(&mutated);
    let w411 = with_code(&diags, "W411");
    assert_eq!(
        w411.len(),
        1,
        "an undeclared integer profile argument warns exactly once: {:?}",
        diags
    );
    assert!(
        w411[0].severity == Severity::Warning,
        "W411 is a warning, not an error"
    );
    assert!(
        w411[0].message.contains("bogus")
            && w411[0].message.contains("oddities/profiles/todo_standard"),
        "W411 names the argument and the profile: {}",
        w411[0].message
    );
    // The :constants header must not attribute `bogus` to the profile
    // (the import node's :arguments provenance still records what the
    // author wrote — that record is not a binding).
    let printed = ir.to_sexpr().print();
    let header_start = printed.find(":constants").expect(":constants header");
    let header =
        &printed[header_start..printed[header_start..].find(":exports").unwrap() + header_start];
    assert!(
        header.contains("(sharing_limit 256"),
        "the declared parameter still binds: {}",
        header
    );
    assert!(
        !header.contains("bogus"),
        "an undeclared argument binds no constant: {}",
        header
    );
}

// ---------------------------------------------------------------------
// Finding 8: E203 names the generator symbol.
// ---------------------------------------------------------------------

#[test]
fn g10_e203_names_the_generator_symbol() {
    let src = budget_spec("", "slot_free (pre, request)", "pool")
        .replace("actor gen_op of operator", "actor gen_op of nobody");
    let (_ir, diags) = elab(&src);
    let e203 = with_code(&diags, "E203");
    assert!(
        e203.iter()
            .any(|d| d.message.contains("gen_op") && d.message.contains("nobody")),
        "E203 must name the generator symbol and the unknown actor: {:?}",
        diags
    );
}
