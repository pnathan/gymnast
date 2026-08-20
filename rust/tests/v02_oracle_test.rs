//! Tests-of-record for surface v0.2 (phase 10: constants, coverage
//! teeth, actor binding), authored from `docs/rust-port-plan-phase10.md`
//! (the "Oracle tests" list, items 01-08) and
//! `docs/surface-v0.2-design.md` ALONE, BEFORE any implementation of the
//! v0.2 features exists (the committed-oracle process of phases 4-9:
//! Stage 1 commits this file red; implementers may not touch it;
//! integrator-only arbitration with in-file notes).
//!
//! Every pin was derived from the design doc plus the CURRENT binary's
//! behavior. Each hand-built spec below with pre-v0.2 syntax was run
//! through the current `check`/`ir`/`verify` before being frozen here
//! (all check-clean, zero diagnostics), and each expected clause shape
//! is copied from the current binary's `ir` output for the spec's
//! literal-spelled twin; the derivations are written out at each site.
//!
//! CARDINAL RULE of the phase, pinned throughout: substitution
//! preserves semantics. A const-spelled spec and its literal-spelled
//! twin produce identical verdicts everywhere; the IR may differ ONLY
//! by the constants header and the new diagnostics.
//!
//! Uses only pre-v0.2 crate APIs (parser/elaborate/verify/adequacy/
//! sexpr/diag), so this file compiles TODAY and is red purely through
//! test outcomes. Expected state at the Stage 1 commit:
//!   - GREEN: oracle_07d (guard: no W410 for a declared-actor
//!     generator -- vacuously true until W410 exists, and it must STAY
//!     true after), oracle_08b / oracle_08c (the semantic invariant,
//!     which must hold BOTH before and after the corpus update), and
//!     oracle_08e (goldens match fixtures -- true now, red during
//!     Stage 2, true again after Stage 3 regenerates the fixtures).
//!   - RED: everything else, exactly because the v0.2 features do not
//!     exist yet (new syntax fails to parse; new diagnostics, header,
//!     and bundle field are absent).
//!
//! RESOLVED AMBIGUITIES (each also called out at its use site):
//!
//!  1. CONSTANTS HEADER LOCATION. The plan says "IR header gains
//!     `(constants ((name value source) ...))` ... participates in the
//!     fingerprint like every header field" but does not say whether
//!     that lands as a new top-level entry of the `(ir (...))` form
//!     (beside `schema`/`module`) or as a module field (beside
//!     `:exports`). Both readings keep it fingerprinted. The
//!     `constants_header` helper accepts either location, so every
//!     test pins the SUBSTANCE the plan does specify (presence, triple
//!     shape, sorting, sources, values) without gambling the file on
//!     the unpinned placement.
//!  2. NON-INTEGER PROFILE PARAMETERS. `use ... (identity_provider
//!     google)` passes a symbol where v0.2 constants are integers. The
//!     plan pins only the integer case (`sharing_limit 256 binds
//!     sharing_limit = 256`); whether `identity_provider` appears in
//!     the constants header (with a symbol value) or is skipped is not
//!     decided by either document. Header assertions are therefore
//!     presence/subset-based (never an exhaustive entry list), and the
//!     const/param collision test (01d) uses the integer parameter.
//!  3. W408 COVERAGE SEMANTICS. Plan section B's sentence alone ("each
//!     interface op not matched (suffix rule) by any property execute
//!     step or scenario `when` step") would mark todo's `query_tasks`
//!     COVERED -- create_then_read's second execute step names it
//!     exactly. But the design doc (binding) says twice that the
//!     flagship's `query_tasks` IS flagged uncovered, plan section C
//!     lists "W408 for `query_tasks`" among the golden changes, and
//!     oracle item 08 pins "`query_tasks` W408 present". The only
//!     readings satisfying all four documents make coverage mean
//!     EXERCISED THROUGH THE TRANSITION MACHINERY: an op with no
//!     transition is uncovered even when a step names it (query_tasks'
//!     lack of a behavior is exactly why create_then_read
//!     baseline-fails, per docs/change-study.md), and an op whose
//!     transition no step reaches under the trace machinery's suffix
//!     rule is uncovered too. Tests 06a/06d are constructed so both
//!     the plan-B-literal and the design-doc readings agree; 06b and
//!     08d pin the design-doc reading where they diverge. Consequence,
//!     deliberately NOT pinned: todo's `invite` op (reached by no step
//!     -- `invite_distinct` suffix-matches no transition, the same
//!     fact sharing_boundary's baseline failure records) may also earn
//!     a W408, so 08d asserts PRESENCE of the query_tasks entry, never
//!     a total count.
//!  4. W410 SCOPE. The plan warns on "a two-element pair whose gen
//!     symbol matches no declared actor", read literally over ALL
//!     generate pairs -- but plan section C has Stage 3 add `of user`
//!     only to the ACTOR pairs of todo.gym while expecting clean
//!     goldens, so task-style pairs (`task valid_task`) presumably do
//!     not warn. Neither reading is pinned for non-actor pairs: the
//!     W410 tests (07c/07d) use single-pair `generate (actor ...)`
//!     blocks, where both readings agree exactly.
//!  5. `of` CAPTURE SHAPE. Plan section A's "the captured pair becomes
//!     `(gen of actor-name)` three-element" is ambiguous about where
//!     the variable symbol sits. Not pinned here; item 07 pins the
//!     OBLIGATION-level contract the plan states precisely instead:
//!     the lowered obligation carries `(actor-of <name>)` immediately
//!     after `(generate ...)`.
//!  6. UNKNOWN `of` ACTOR ERROR CODE. The plan says "the existing
//!     E-class unresolved-reference error" without naming a code (the
//!     checker uses E202..E206 for different reference classes). 07b
//!     pins severity Error and that the message names the unknown
//!     actor, not a specific code.
//!
//! Diagnostic-code pins used below, from the design doc's own list:
//! E209 unresolved-constant, E210 invalid-constant-expression, E201
//! duplicates (const/const and const/param), W408 uncovered-operation,
//! W409 unexercised-transition, W410 unresolved-acceptance-actor.

use gymnast_rs::adequacy;
use gymnast_rs::diag::{Diagnostic, Severity};
use gymnast_rs::elaborate;
use gymnast_rs::ir::{Ir, IrNode};
use gymnast_rs::parser;
use gymnast_rs::sexpr::{self, canonical_serialize, Sexpr};
use gymnast_rs::verify;
use std::process::Command;

// ---------------------------------------------------------------------
// Helpers (not tests).
// ---------------------------------------------------------------------

/// Parse + elaborate, panicking (red) if the source does not parse --
/// pre-Stage-2, every spec using v0.2 syntax fails here, which is the
/// expected redness.
fn elab(src: &str) -> (Ir, Vec<Diagnostic>) {
    let (ast, parse_diags) = parser::parse(src);
    let file = ast.unwrap_or_else(|| {
        panic!(
            "spec failed to parse (expected red until the v0.2 parser \
             lands); parse diagnostics: {:?}",
            parse_diags
        )
    });
    elaborate::elaborate_with_parse_diags(&file, &parse_diags)
}

fn with_code<'a>(diags: &'a [Diagnostic], code: &str) -> Vec<&'a Diagnostic> {
    diags.iter().filter(|d| d.code == code).collect()
}

fn error_diags(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect()
}

/// The constants header's triple list, from either sanctioned location
/// (file-header ambiguity 1): a top-level `(constants (...))` entry of
/// the `(ir (...))` form, or a `:constants`/`constants` module field.
fn constants_header(ir: &Ir) -> Option<Vec<Sexpr>> {
    let ir_sx = ir.to_sexpr();
    let items = ir_sx.as_list()?.get(1)?.clone();
    if let Some(c) = items.assoc("constants") {
        return c.as_list().map(|l| l.to_vec());
    }
    let module = items.assoc("module")?;
    let fields = module.assoc("fields")?;
    for key in [":constants", "constants"] {
        if let Some(c) = fields.assoc(key) {
            return c.as_list().map(|l| l.to_vec());
        }
    }
    None
}

/// Canonical prints of the header triples, for set comparisons.
fn triple_prints(triples: &[Sexpr]) -> Vec<String> {
    triples.iter().map(|t| t.print()).collect()
}

/// Field lookup that works for both the crate's flat forms
/// (`(tag (k v) ...)`) and the nested bundle root
/// (`(verification-bundle ((k v) ...))`) -- the same idiom
/// `verify_oracle_test.rs` established.
fn field<'a>(v: &'a Sexpr, key: &str) -> Option<&'a Sexpr> {
    if let Some(found) = v.assoc(key) {
        return Some(found);
    }
    v.as_list()?.get(1)?.assoc(key)
}

/// The bundle's `coverage-diagnostics` entries; None when the field is
/// absent (pre-Stage-2), Some(vec) -- possibly empty -- once it exists.
fn coverage_diags(bundle: &Sexpr) -> Option<Vec<Sexpr>> {
    field(bundle, "coverage-diagnostics")
        .map(|v| v.as_list().map(|l| l.to_vec()).unwrap_or_default())
}

fn diag_entry_code(d: &Sexpr) -> &str {
    d.assoc("code").and_then(|c| c.as_str()).unwrap_or("")
}

fn diag_entry_message(d: &Sexpr) -> &str {
    d.assoc("message").and_then(|m| m.as_str()).unwrap_or("")
}

fn diag_entry_severity(d: &Sexpr) -> &str {
    d.assoc("severity").and_then(|s| s.as_sym()).unwrap_or("")
}

fn coverage_diags_with_code(bundle: &Sexpr, code: &str) -> Vec<Sexpr> {
    coverage_diags(bundle)
        .unwrap_or_default()
        .into_iter()
        .filter(|d| diag_entry_code(d) == code)
        .collect()
}

/// The bundle root's top-level field keys, in serialized order.
fn bundle_keys(bundle: &Sexpr) -> Vec<String> {
    bundle
        .as_list()
        .and_then(|outer| outer.get(1))
        .and_then(|inner| inner.as_list())
        .map(|pairs| {
            pairs
                .iter()
                .filter_map(|p| p.as_list())
                .filter_map(|p| p.first())
                .filter_map(|k| k.as_sym())
                .map(|k| k.to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn contains_sym(s: &Sexpr, name: &str) -> bool {
    match s {
        Sexpr::Sym(x) => x == name,
        Sexpr::List(items) => items.iter().any(|i| contains_sym(i, name)),
        _ => false,
    }
}

/// Replaces the VALUE of every two-element `(fingerprint _)` /
/// `(ir-fingerprint _)` pair, recursively. Used by 05b together with a
/// fingerprint-occurrence COUNT so masking can never hide a real
/// difference.
fn mask_fingerprints(s: &Sexpr) -> Sexpr {
    match s {
        Sexpr::List(items) => {
            if items.len() == 2 {
                if let Some(k) = items[0].as_sym() {
                    if k == "fingerprint" || k == "ir-fingerprint" {
                        return Sexpr::list(vec![
                            items[0].clone(),
                            Sexpr::Str("MASKED".to_string()),
                        ]);
                    }
                }
            }
            Sexpr::List(items.iter().map(mask_fingerprints).collect())
        }
        other => other.clone(),
    }
}

fn assert_partition_identical(a: &[IrNode], b: &[IrNode], partition: &str) {
    assert_eq!(
        a.len(),
        b.len(),
        "{} partition sizes differ between the const-spelled and \
         literal-spelled twins",
        partition
    );
    for (na, nb) in a.iter().zip(b.iter()) {
        assert_eq!(
            canonical_serialize(&na.to_sexpr()),
            canonical_serialize(&nb.to_sexpr()),
            "{} node {} must be byte-identical across the twins",
            partition,
            na.id
        );
    }
}

fn ps(text: &str) -> Sexpr {
    sexpr::parse(text).expect("parse expected-value sexpr")
}

fn todo_ir() -> Ir {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/todo.gym"))
        .expect("read ../examples/todo.gym");
    elab(&src).0
}

/// Runs the CLI binary on ../examples/todo.gym, asserting exit 0, and
/// returns stdout. stderr (warnings) is deliberately ignored.
fn todo_cli(subcommand: &str) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_gymnast-rs"))
        .args([
            subcommand,
            concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/todo.gym"),
        ])
        .output()
        .expect("run gymnast-rs");
    assert!(
        out.status.success(),
        "`gymnast-rs {} todo.gym` must exit 0; stderr:\n{}",
        subcommand,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("stdout is UTF-8")
}

// ---------------------------------------------------------------------
// Hand-built specs.
//
// TWIN_A (const-spelled) and TWIN_B (literal-spelled) differ ONLY in
// the three `const` declarations and the spelling of the integer
// positions. Substitution arithmetic, written out:
//   cap      = 7  -> requires `< cap` = `< 7`; fails-when `= cap` = `= 7`;
//                    invariant `<= cap - 1` = `<= 6` (7 - 1 = 6);
//                    scenario `(owner, cap)` = 7, `(owner, cap + 1)` = 8
//                    (7 + 1 = 8); concurrency `<= cap` = `<= 7`;
//                    under `virtual_users cap` = 500-style plain value 7.
//   dur      = 30 -> under `duration dur min` = `duration 30 min`,
//                    lowering to the pair `(duration (min 30))`.
//   max_name = 40 -> refinement `text (1..max_name)` = `text (1..40)`,
//                    lowering to `(text :min 1 :max 40)`.
// TWIN_B was validated against the current binary: `check` clean, IR
// `(diagnostics nil)`, verification summary (total 5) (passed 0)
// (failed 1) (skipped 3) (indeterminate 1) -- a nontrivial verdict
// spread (scenario failed, invariant indeterminate, concurrency /
// coverage / constraint skipped), so 05a's bundle equality is not
// vacuous. Both twins `use` the profile, so both headers carry the
// profile-bound constants and the twins isolate exactly the spec-const
// delta.
// ---------------------------------------------------------------------

const TWIN_A: &str = r#"
spec twin = v 0.1 owner product exports GadgetId, Gadget, gadget_service

use oddities/profiles/todo_standard @ 1.0
  (sharing_limit 256, identity_provider google)

const cap      = 7
const dur      = 30
const max_name = 40

application twin = (modules twin, default_acceptance production)

actor user = person (identity google_openid (issuer, subject))

mode UserId  = opaque text
mode ListId  = opaque text
mode Version = opaque int
mode Role    = enum (owner, editor, viewer)

mode GadgetId = opaque text

mode Gadget = struct (
  GadgetId id,
  text (1..max_name) name )

component twin_app = (
  responsibility "Track a bounded shelf of gadgets",
  provides gadget_service,
  uses (clock, id_source) )

interface gadget_service = for user (
  cmd add_gadget = (GadgetId id, text name) Gadget
      ! (forbidden, conflict) )

state twin_state = (
  of aggregate (Gadget),
  owner twin_app,
  durability durable,
  initial empty )

behavior add_gadget = on gadget_service.add_gadget (user, request) (
  reads (twin_state), writes twin_state;

  requires gadget_count (pre) < cap;
  ensures  post = insert_gadget (pre, request);
  fails forbidden
    when gadget_count (pre) = cap
    preserves all_state )

inv shelf_ceiling = on twin_state
  always for all Gadget g: gadget_count (g) <= cap - 1

constraint shelf_capacity = workload on gadget_service
  under (virtual_users cap, duration dur min)
  must lost_updates = 0

synthesis prototype = target ruby / rails (
  platform gymnast_reference_platform_v1,
  model small_code_model (class nano, temperature 0, max_attempts 3),
  attempts 3 )

acceptance production = of twin_app (
  scenario fill_the_shelf = (
    given owner = seeded_owner;
    when add_distinct (owner, cap); then succeeds;
    when add_distinct (owner, cap + 1); then fails_with forbidden ),

  concurrency shelf_race = (actors 7, schedule adversarial)
    must active_gadget_count <= cap,

  coverage (every_invariant, boundaries),

  execution (clock virtual, randomness seeded,
             network controlled, timezone "UTC") )
"#;

const TWIN_B: &str = r#"
spec twin = v 0.1 owner product exports GadgetId, Gadget, gadget_service

use oddities/profiles/todo_standard @ 1.0
  (sharing_limit 256, identity_provider google)

application twin = (modules twin, default_acceptance production)

actor user = person (identity google_openid (issuer, subject))

mode UserId  = opaque text
mode ListId  = opaque text
mode Version = opaque int
mode Role    = enum (owner, editor, viewer)

mode GadgetId = opaque text

mode Gadget = struct (
  GadgetId id,
  text (1..40) name )

component twin_app = (
  responsibility "Track a bounded shelf of gadgets",
  provides gadget_service,
  uses (clock, id_source) )

interface gadget_service = for user (
  cmd add_gadget = (GadgetId id, text name) Gadget
      ! (forbidden, conflict) )

state twin_state = (
  of aggregate (Gadget),
  owner twin_app,
  durability durable,
  initial empty )

behavior add_gadget = on gadget_service.add_gadget (user, request) (
  reads (twin_state), writes twin_state;

  requires gadget_count (pre) < 7;
  ensures  post = insert_gadget (pre, request);
  fails forbidden
    when gadget_count (pre) = 7
    preserves all_state )

inv shelf_ceiling = on twin_state
  always for all Gadget g: gadget_count (g) <= 6

constraint shelf_capacity = workload on gadget_service
  under (virtual_users 7, duration 30 min)
  must lost_updates = 0

synthesis prototype = target ruby / rails (
  platform gymnast_reference_platform_v1,
  model small_code_model (class nano, temperature 0, max_attempts 3),
  attempts 3 )

acceptance production = of twin_app (
  scenario fill_the_shelf = (
    given owner = seeded_owner;
    when add_distinct (owner, 7); then succeeds;
    when add_distinct (owner, 8); then fails_with forbidden ),

  concurrency shelf_race = (actors 7, schedule adversarial)
    must active_gadget_count <= 7,

  coverage (every_invariant, boundaries),

  execution (clock virtual, randomness seeded,
             network controlled, timezone "UTC") )
"#;

/// Item 04's spec: no `const` declarations at all -- every constant
/// reference resolves through the `use` clause's parameters (design
/// feature 1's probe-5 unification). Its literal twin (`sharing_limit`
/// replaced by `256` at the three reference sites) was validated
/// against the current binary; the expected clause shapes in 04a are
/// copied verbatim from that run's `ir` output.
const PARAMREF: &str = r#"
spec paramref = v 0.1 owner product exports Shelf, shelf_service

use oddities/profiles/todo_standard @ 1.0
  (sharing_limit 256, identity_provider google)

application paramref = (modules paramref, default_acceptance production)

actor member = person (identity google_openid (issuer, subject))

mode UserId  = opaque text
mode ListId  = opaque text
mode Version = opaque int
mode Role    = enum (owner, editor, viewer)

mode Shelf = struct (
  ListId id,
  Version version )

component shelf_app = (
  responsibility "Profile parameter reference probe",
  provides shelf_service,
  uses (clock) )

interface shelf_service = for member (
  cmd share_shelf = (ListId list, UserId principal) Shelf
      ! (forbidden) )

state shelf_state = (
  of aggregate (Shelf),
  owner shelf_app,
  durability durable,
  initial empty )

behavior share_shelf = on shelf_service.share_shelf (member, request) (
  reads (shelf_state), writes shelf_state;

  requires member_count (pre, request.list) < sharing_limit;
  ensures  post = add_member (pre, request);
  fails forbidden
    when member_count (pre, request.list) = sharing_limit
    preserves all_state )

inv shelf_sharing_cap = on shelf_state
  always for all Shelf s: member_count (s) <= sharing_limit
"#;

/// Shared pre-v0.2 skeleton for the coverage and actor-binding probes
/// (items 06/07): two ops, two behaviors, one property exercising op_a
/// through a BARE step name against the slash-qualified transition
/// `covspec/behavior/do_a`'s operation `widget_service/op_a` -- the
/// suffix rule in action. `{ACCEPTANCE}` is substituted per test.
/// Validated with the current binary (check-clean, no warnings; the
/// probe_a property obligation PASSES, confirming the step really does
/// reach the transition).
fn cov_spec(acceptance: &str) -> String {
    format!(
        r#"
spec covspec = v 0.1 owner product exports Widget, widget_service

application covspec = (modules covspec, default_acceptance production)

actor operator = person (identity basic_openid (issuer, subject))

mode WidgetId = opaque text

mode Widget = struct (
  WidgetId id,
  text (1..40) name )

component cov_app = (
  responsibility "Coverage teeth probe",
  provides widget_service,
  uses (clock, id_source) )

interface widget_service = for operator (
  cmd op_a = (WidgetId id, text name) Widget
      ! (forbidden),
  cmd op_b = (WidgetId id, text name) Widget
      ! (forbidden) )

state cov_state = (
  of aggregate (Widget),
  owner cov_app,
  durability durable,
  initial empty )

behavior do_a = on widget_service.op_a (operator, request) (
  reads (cov_state), writes cov_state;

  requires slot_free (pre, request);
  ensures  post = put_a (pre, request) )

behavior do_b = on widget_service.op_b (operator, request) (
  reads (cov_state), writes cov_state;

  requires slot_free (pre, request);
  ensures  post = put_b (pre, request) )

acceptance production = of cov_app (
{acceptance}
  execution (clock virtual, randomness seeded,
             network controlled, timezone "UTC") )
"#,
        acceptance = acceptance
    )
}

const PROBE_A: &str = r#"  property probe_a =
    generate (actor operator, widget valid_widget)
    execute op_a (actor, widget)
    must stored (result, widget),
"#;

/// Item 06c's spec: op_c is NAMED by an acceptance step but has no
/// behavior at all -- the flagship `query_tasks` shape (file-header
/// ambiguity 3). Validated with the current binary: check-clean
/// (probe_c's obligation fails with no-matching-transition, which is
/// exactly the "cannot be exercised" fact W408 must surface).
const COV_NO_BEHAVIOR: &str = r#"
spec covspec = v 0.1 owner product exports Widget, widget_service

application covspec = (modules covspec, default_acceptance production)

actor operator = person (identity basic_openid (issuer, subject))

mode WidgetId = opaque text

mode Widget = struct (
  WidgetId id,
  text (1..40) name )

component cov_app = (
  responsibility "Coverage teeth probe",
  provides widget_service,
  uses (clock, id_source) )

interface widget_service = for operator (
  cmd op_a = (WidgetId id, text name) Widget
      ! (forbidden),
  cmd op_c = (WidgetId id, text name) Widget
      ! (forbidden) )

state cov_state = (
  of aggregate (Widget),
  owner cov_app,
  durability durable,
  initial empty )

behavior do_a = on widget_service.op_a (operator, request) (
  reads (cov_state), writes cov_state;

  requires slot_free (pre, request);
  ensures  post = put_a (pre, request) )

acceptance production = of cov_app (
  property probe_a =
    generate (actor operator, widget valid_widget)
    execute op_a (actor, widget)
    must stored (result, widget),

  property probe_c =
    generate (actor operator, widget valid_widget)
    execute op_c (actor, widget)
    must stored (result, widget),

  coverage (every_operation),

  execution (clock virtual, randomness seeded,
             network controlled, timezone "UTC") )
"#;

/// Item 07's skeleton: one op, one behavior, and a SINGLE-PAIR
/// `generate` block (file-header ambiguity 4: with only the actor pair
/// present, every reading of W410's scope agrees on the expected
/// count). `{GENERATE}` is substituted per test. The two pre-v0.2
/// variants (`actor stranger`, `actor operator`) were validated with
/// the current binary: check-clean, zero diagnostics.
fn actor_spec(generate: &str) -> String {
    format!(
        r#"
spec covspec = v 0.1 owner product exports Widget, widget_service

application covspec = (modules covspec, default_acceptance production)

actor operator = person (identity basic_openid (issuer, subject))

mode WidgetId = opaque text

mode Widget = struct (
  WidgetId id,
  text (1..40) name )

component cov_app = (
  responsibility "Acceptance actor binding probe",
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

  requires slot_free (pre, request);
  ensures  post = put_a (pre, request) )

acceptance production = of cov_app (
  property probe_a =
    generate ({generate})
    execute op_a (actor)
    must stored (result),

  execution (clock virtual, randomness seeded,
             network controlled, timezone "UTC") )
"#,
        generate = generate
    )
}

// ---------------------------------------------------------------------
// Item 01: const parsing.
// ---------------------------------------------------------------------

/// 01: `const` declarations are accepted -- the parse produces no
/// error diagnostics, elaboration produces none, and the binding lands
/// in the constants header as `(cap 7 spec)`.
#[test]
fn oracle_01a_const_declaration_accepted() {
    let (ast, parse_diags) = parser::parse(TWIN_A);
    assert!(
        ast.is_some(),
        "a spec with const declarations must parse; diagnostics: {:?}",
        parse_diags
    );
    assert!(
        error_diags(&parse_diags).is_empty(),
        "const declarations must not produce parse errors: {:?}",
        parse_diags
    );
    let (ir, all_diags) = elab(TWIN_A);
    assert!(
        error_diags(&all_diags).is_empty(),
        "TWIN_A must elaborate without errors: {:?}",
        all_diags
    );
    let header = constants_header(&ir).expect("constants header present");
    assert!(
        triple_prints(&header).contains(&"(cap 7 spec)".to_string()),
        "constants header must carry (cap 7 spec); got {:?}",
        triple_prints(&header)
    );
}

/// 01: a non-integer RHS is a parse-time E210
/// (`invalid-constant-expression`) -- both the string-literal and the
/// bare-word spellings.
#[test]
fn oracle_01b_const_non_integer_rhs_is_e210() {
    for bad in [
        "spec badconst = v 0.1 owner product exports Widget\n\
         const cap = \"seven\"\n\
         mode Widget = opaque text\n",
        "spec badconst = v 0.1 owner product exports Widget\n\
         const cap = seven\n\
         mode Widget = opaque text\n",
    ] {
        let (_ast, parse_diags) = parser::parse(bad);
        let e210 = with_code(&parse_diags, "E210");
        assert!(
            !e210.is_empty(),
            "non-integer const RHS must be a parse-time E210; got {:?}",
            parse_diags
        );
        assert!(
            e210.iter().all(|d| d.severity == Severity::Error),
            "E210 is an error"
        );
    }
}

/// 01: a duplicate `const` name is E201, the standard duplicate shape.
#[test]
fn oracle_01c_duplicate_const_is_e201() {
    let src = "spec dupspec = v 0.1 owner product exports Widget\n\
               const cap = 7\n\
               const cap = 9\n\
               mode Widget = opaque text\n";
    let (_ir, all_diags) = elab(src);
    let e201 = with_code(&all_diags, "E201");
    assert!(
        !e201.is_empty(),
        "duplicate const must be E201; got {:?}",
        all_diags
    );
    assert!(
        e201.iter()
            .any(|d| d.severity == Severity::Error && d.message.contains("cap")),
        "the E201 must be an error naming 'cap': {:?}",
        e201
    );
}

/// 01: a spec-level `const` colliding with a profile parameter of the
/// same name is E201 -- the `use` clause's parameter pairs are real
/// constant bindings (design feature 1), so the collision is a
/// duplicate declaration like any other. Uses the INTEGER parameter
/// (file-header ambiguity 2).
#[test]
fn oracle_01d_const_profile_param_collision_is_e201() {
    let src = "spec collide = v 0.1 owner product exports UserId\n\
               use oddities/profiles/todo_standard @ 1.0\n\
               (sharing_limit 256, identity_provider google)\n\
               const sharing_limit = 300\n\
               mode UserId  = opaque text\n\
               mode ListId  = opaque text\n\
               mode Version = opaque int\n\
               mode Role    = enum (owner, editor, viewer)\n";
    let (_ir, all_diags) = elab(src);
    let e201 = with_code(&all_diags, "E201");
    assert!(
        e201.iter()
            .any(|d| d.severity == Severity::Error && d.message.contains("sharing_limit")),
        "const/profile-param collision must be an E201 error naming \
         'sharing_limit'; got {:?}",
        all_diags
    );
}

// ---------------------------------------------------------------------
// Item 02: substitution.
// ---------------------------------------------------------------------

/// 02: the const-spelled twin and the literal-spelled twin elaborate to
/// IRs whose nodes are IDENTICAL -- every partition, node by node, byte
/// by byte -- and whose module fields agree apart from the constants
/// header. Neither spec produces any diagnostic (so "the IR may differ
/// ONLY by the constants header and the new diagnostics" degenerates to
/// "only the constants header" here). See the TWIN_A comment for the
/// substitution arithmetic at each of the six position kinds (requires,
/// fails-when, invariant, scenario `when`, under, refinement).
#[test]
fn oracle_02a_substitution_yields_identical_nodes() {
    let (a, a_diags) = elab(TWIN_A);
    let (b, b_diags) = elab(TWIN_B);
    assert!(
        a_diags.is_empty() && b_diags.is_empty(),
        "both twins must elaborate diagnostic-free; A: {:?} B: {:?}",
        a_diags,
        b_diags
    );
    assert!(a.diagnostics.is_empty() && b.diagnostics.is_empty());
    assert_eq!(a.schema, b.schema);
    assert_eq!(a.module_name, b.module_name);
    assert_partition_identical(&a.design, &b.design, "design");
    assert_partition_identical(&a.transitions, &b.transitions, "transitions");
    assert_partition_identical(&a.obligations, &b.obligations, "obligations");
    assert_partition_identical(&a.synthesis, &b.synthesis, "synthesis");
    // Module fields, minus any constants-header entry, must agree.
    let mf = |ir: &Ir| {
        ir.module_fields
            .iter()
            .filter(|(k, _)| !k.contains("constants"))
            .map(|(k, v)| format!("{} {}", k, v.print()))
            .collect::<Vec<_>>()
    };
    assert_eq!(mf(&a), mf(&b), "non-constants module fields must agree");
}

/// 02: the constants header's presence, triple shape, sorting, and both
/// sources. TWIN_A's header carries the three spec consts (source sym
/// `spec`) alongside whatever the shared `use` clause binds; TWIN_B's
/// header is EXACTLY TWIN_A's minus the three spec consts (both specs
/// carry the identical `use` clause, so the profile-bound entries are
/// pinned equal by set difference without deciding file-header
/// ambiguity 2).
#[test]
fn oracle_02b_constants_header_shape_sorting_sources() {
    let (a, _) = elab(TWIN_A);
    let (b, _) = elab(TWIN_B);
    let ha = constants_header(&a).expect("TWIN_A constants header");
    let hb = constants_header(&b).expect("TWIN_B constants header");

    // Shape: every entry is a (name value source) triple; source is
    // the sym `spec` or a profile-name string.
    for t in ha.iter().chain(hb.iter()) {
        let items = t.as_list().expect("triple is a list");
        assert_eq!(
            items.len(),
            3,
            "triple has exactly 3 elements: {}",
            t.print()
        );
        assert!(
            items[0].as_sym().is_some(),
            "name is a symbol: {}",
            t.print()
        );
        let source_ok = items[2] == Sexpr::sym("spec") || items[2].as_str().is_some();
        assert!(source_ok, "source is sym spec or a string: {}", t.print());
    }

    // Sorting: names strictly ascending in both headers.
    for h in [&ha, &hb] {
        let names: Vec<&str> = h
            .iter()
            .filter_map(|t| t.as_list())
            .filter_map(|t| t.first())
            .filter_map(|n| n.as_sym())
            .collect();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(names, sorted, "header names must be sorted and unique");
    }

    // Both sources present in A: the three spec triples with their
    // exact values, and the profile-bound integer parameter.
    let a_set: std::collections::BTreeSet<String> = triple_prints(&ha).into_iter().collect();
    let b_set: std::collections::BTreeSet<String> = triple_prints(&hb).into_iter().collect();
    for spec_triple in ["(cap 7 spec)", "(dur 30 spec)", "(max_name 40 spec)"] {
        assert!(
            a_set.contains(spec_triple),
            "TWIN_A header must carry {}; got {:?}",
            spec_triple,
            a_set
        );
    }
    assert!(
        a_set.contains("(sharing_limit 256 \"oddities/profiles/todo_standard\")"),
        "TWIN_A header must carry the profile-bound sharing_limit; got {:?}",
        a_set
    );
    let expected_b: std::collections::BTreeSet<String> = a_set
        .iter()
        .filter(|t| !["(cap 7 spec)", "(dur 30 spec)", "(max_name 40 spec)"].contains(&t.as_str()))
        .cloned()
        .collect();
    assert_eq!(
        b_set, expected_b,
        "TWIN_B's header must be exactly TWIN_A's minus the three spec consts"
    );
}

// ---------------------------------------------------------------------
// Item 03: E209 unresolved-constant.
// ---------------------------------------------------------------------

/// 03: an explicitly const-shaped `(+ nosuch 1)` in a predicate names
/// no declared constant -> exactly one E209. The spec's OTHER bare
/// atoms in predicate positions (pre, request, slot_free, put_a, ...)
/// remain abstract exactly as today -- pinned by the EXACTLY-ONE count.
#[test]
fn oracle_03a_offset_form_with_unknown_constant_is_e209() {
    let src = actor_spec("actor operator").replace(
        "requires slot_free (pre, request);",
        "requires slot_count (pre) < nosuch + 1;",
    );
    let (_ir, all_diags) = elab(&src);
    let e209 = with_code(&all_diags, "E209");
    assert_eq!(
        e209.len(),
        1,
        "exactly one E209 for the one const-shaped form; got {:?}",
        all_diags
    );
    assert!(e209[0].severity == Severity::Error);
    assert!(
        e209[0].message.contains("nosuch"),
        "the E209 must name the unresolved constant: {}",
        e209[0].message
    );
}

/// 03: a refinement bound IDENT is a position where only constants are
/// legal -- `text (1..nope)` with no `nope` declared -> exactly one
/// E209 naming it.
#[test]
fn oracle_03b_refinement_bound_ident_unknown_is_e209() {
    let src = actor_spec("actor operator").replace("text (1..40) name", "text (1..nope) name");
    let (_ir, all_diags) = elab(&src);
    let e209 = with_code(&all_diags, "E209");
    assert_eq!(
        e209.len(),
        1,
        "exactly one E209 for the refinement bound; got {:?}",
        all_diags
    );
    assert!(e209[0].severity == Severity::Error);
    assert!(
        e209[0].message.contains("nope"),
        "the E209 must name the unresolved constant: {}",
        e209[0].message
    );
}

/// 03: a workload `under` value IDENT is likewise constants-only --
/// `virtual_users ghost` with no `ghost` declared -> exactly one E209.
/// (This spec parses under the CURRENT grammar -- `ghost` lexes as a
/// word value -- and today elaborates diagnostic-free, validated
/// against the current binary; the test is red purely because E209
/// does not exist yet.)
#[test]
fn oracle_03c_under_value_ident_unknown_is_e209() {
    let src = r#"
spec underspec = v 0.1 owner product exports Widget, widget_service

application underspec = (modules underspec, default_acceptance production)

actor operator = person (identity basic_openid (issuer, subject))

mode WidgetId = opaque text

mode Widget = struct (
  WidgetId id,
  text (1..40) name )

component cov_app = (
  responsibility "Under ident probe",
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

  requires slot_free (pre, request);
  ensures  post = put_a (pre, request) )

constraint cap_load = workload on widget_service
  under (virtual_users ghost, duration 30 min)
  must lost_updates = 0
"#;
    let (_ir, all_diags) = elab(src);
    let e209 = with_code(&all_diags, "E209");
    assert_eq!(
        e209.len(),
        1,
        "exactly one E209 for the under value; got {:?}",
        all_diags
    );
    assert!(e209[0].severity == Severity::Error);
    assert!(
        e209[0].message.contains("ghost"),
        "the E209 must name the unresolved constant: {}",
        e209[0].message
    );
}

// ---------------------------------------------------------------------
// Item 04: profile parameters bind.
// ---------------------------------------------------------------------

/// 04: every `sharing_limit` reference in PARAMREF elaborates to Int
/// 256. Expected clause shapes are copied VERBATIM from the current
/// binary's `ir` output for the literal twin (sharing_limit -> 256 at
/// the three sites), so this pins byte-level substitution equivalence:
///   requires  -> (requires (< (member_count pre request/list) 256))
///   fails     -> (fails forbidden :when (= (member_count pre
///                 request/list) 256) :preserves all_state)
///   invariant -> (forall ((s Shelf)) (<= (member_count s) 256))
#[test]
fn oracle_04a_profile_param_references_substitute_to_256() {
    let (ir, all_diags) = elab(PARAMREF);
    assert!(
        error_diags(&all_diags).is_empty(),
        "PARAMREF must elaborate without errors: {:?}",
        all_diags
    );

    assert_eq!(ir.transitions.len(), 1, "one behavior node");
    let behavior = &ir.transitions[0];
    let head = |c: &Sexpr, h: &str| {
        c.as_list()
            .and_then(|l| l.first())
            .and_then(|s| s.as_sym())
            .map(|s| s == h)
            .unwrap_or(false)
    };
    let requires: Vec<&Sexpr> = behavior
        .clauses
        .iter()
        .filter(|c| head(c, "requires"))
        .collect();
    assert_eq!(requires.len(), 1);
    assert_eq!(
        *requires[0],
        ps("(requires (< (member_count pre request/list) 256))")
    );
    let fails: Vec<&Sexpr> = behavior
        .clauses
        .iter()
        .filter(|c| head(c, "fails"))
        .collect();
    assert_eq!(fails.len(), 1);
    assert_eq!(
        *fails[0],
        ps("(fails forbidden :when (= (member_count pre request/list) 256) :preserves all_state)")
    );

    let inv = ir
        .obligations
        .iter()
        .find(|n| n.kind == "invariant")
        .expect("invariant node");
    assert_eq!(
        inv.field(":always"),
        Some(&ps("(forall ((s Shelf)) (<= (member_count s) 256))"))
    );

    // Totality: no sharing_limit symbol survives in the substituted
    // nodes (the import node's :arguments record is provenance and is
    // deliberately not scanned).
    assert!(
        !contains_sym(&behavior.to_sexpr(), "sharing_limit"),
        "behavior must carry no unsubstituted sharing_limit"
    );
    assert!(
        !contains_sym(&inv.to_sexpr(), "sharing_limit"),
        "invariant must carry no unsubstituted sharing_limit"
    );
}

/// 04: the constants header records the profile binding with the
/// profile name string as its source, exactly as the plan spells it.
#[test]
fn oracle_04b_profile_param_constants_header_entry() {
    let (ir, _) = elab(PARAMREF);
    let header = constants_header(&ir).expect("constants header present");
    assert!(
        triple_prints(&header)
            .contains(&"(sharing_limit 256 \"oddities/profiles/todo_standard\")".to_string()),
        "header must carry (sharing_limit 256 \"oddities/profiles/todo_standard\"); got {:?}",
        triple_prints(&header)
    );
}

// ---------------------------------------------------------------------
// Item 05: substitution preserves semantics END TO END.
// ---------------------------------------------------------------------

/// 05: the twins' verification bundles are byte-identical -- INCLUDING
/// the bundle fingerprint. Derivation of which fragments may differ:
/// the bundle's fields are schema / obligations / results / summary /
/// coverage / environment-diagnostics / transition-diagnostics /
/// (coverage-diagnostics) / diagnostics / source-diagnostics /
/// fingerprint. Obligations, results, coverage, and both diagnostic
/// folds are computed from the IR NODES, which 02a pins identical;
/// source-diagnostics carries the IR's own diagnostics, empty for both
/// twins (02a); the bundle embeds neither the IR fingerprint nor the
/// constants header anywhere (verified against the current binary:
/// TWIN_B's bundle contains exactly ONE "fnv1a64:" occurrence -- its
/// own trailing fingerprint, computed over the bundle's content).
/// Hence ZERO fragments may differ, and the pin is full byte equality.
#[test]
fn oracle_05a_twin_verify_bundles_byte_identical() {
    let (a, a_diags) = elab(TWIN_A);
    let (b, b_diags) = elab(TWIN_B);
    assert!(
        a_diags.is_empty() && b_diags.is_empty(),
        "both twins must elaborate diagnostic-free before the bundles \
         are compared; A: {:?} B: {:?}",
        a_diags,
        b_diags
    );
    let bundle_a = canonical_serialize(&verify::compile_verification(&a));
    let bundle_b = canonical_serialize(&verify::compile_verification(&b));
    assert_eq!(
        bundle_a, bundle_b,
        "substitution must not move a single verification byte"
    );
    // Non-vacuity: the shared bundle carries the twins' real verdict
    // spread (validated against the current binary for TWIN_B):
    // 5 obligations -- scenario fill_the_shelf failed (its when-steps
    // match no transition), invariant shelf_ceiling indeterminate
    // (forall is undecidable for the closed evaluator), concurrency +
    // coverage + constraint skipped.
    let summary = verify::bundle_summary(&verify::compile_verification(&b)).expect("summary");
    assert_eq!(
        (
            summary.total,
            summary.passed,
            summary.failed,
            summary.skipped,
            summary.indeterminate
        ),
        (5, 0, 1, 3, 1),
        "the twins' shared verdict spread"
    );
}

/// 05: the twins' adequacy campaigns are identical except the
/// subject's IR fingerprint. Derivation: `campaign-result` carries
/// exactly TWO fingerprint-bearing fragments -- `(subject ((module ..)
/// (ir-fingerprint ..)))` and the trailing `(fingerprint ..)` computed
/// over the whole result (mutant-result / blind-spot forms carry
/// none; asserted below by counting "fnv1a64:" occurrences). The
/// constants header participates in the IR fingerprint, so the
/// subjects differ; everything else -- every mutant verdict, kill
/// count, pass flag -- must be byte-identical, pinned by masking
/// exactly those two values and comparing whole serializations.
///
/// The five mutants target the TWIN spec's own nodes (the standard
/// todo mutants would all be inapplicable here, and an inapplicable
/// mutant's result carries nothing spec-dependent -- the comparison
/// would be vacuous). All five APPLY (asserted via the campaign's
/// `inapplicable` count), so every mutant verdict really is computed
/// from the twins' verification results.
#[test]
fn oracle_05b_twin_adequacy_identical_except_subject_fingerprint() {
    let (a, a_diags) = elab(TWIN_A);
    let (b, b_diags) = elab(TWIN_B);
    assert!(
        a_diags.is_empty() && b_diags.is_empty(),
        "both twins must elaborate diagnostic-free before the campaigns \
         are compared; A: {:?} B: {:?}",
        a_diags,
        b_diags
    );
    let mutants = vec![
        adequacy::Mutant::new(
            "t1",
            "weaken-precondition",
            "add_gadget accepts requests with its preconditions dropped",
            adequacy::Mutation::WeakenPrecondition {
                behavior_name: "add_gadget".to_string(),
            },
        ),
        adequacy::Mutant::new(
            "t2",
            "remove-invariant",
            "the shelf_ceiling invariant is removed entirely",
            adequacy::Mutation::RemoveInvariant {
                invariant_name: "shelf_ceiling".to_string(),
            },
        ),
        adequacy::Mutant::new(
            "t3",
            "weaken-limit",
            "shelf_ceiling's cap is weakened from 6 to 512",
            adequacy::Mutation::WeakenLimit {
                invariant_name: "shelf_ceiling".to_string(),
                new_limit: 512,
            },
        ),
        adequacy::Mutant::new(
            "t4",
            "remove-failure-mode",
            "add_gadget's declared failure modes are dropped",
            adequacy::Mutation::RemoveFailureMode {
                behavior_name: "add_gadget".to_string(),
            },
        ),
        adequacy::Mutant::new(
            "t5",
            "skip-state-write",
            "add_gadget acknowledges without writing to twin_state",
            adequacy::Mutation::SkipStateWrite {
                behavior_name: "add_gadget".to_string(),
            },
        ),
    ];
    let camp_a = adequacy::run_campaign(&a, &mutants);
    let camp_b = adequacy::run_campaign(&b, &mutants);
    let inapplicable = field(&camp_b, "inapplicable").and_then(|v| v.as_int());
    assert_eq!(
        inapplicable,
        Some(0),
        "non-vacuity: every twin-targeted mutant must apply"
    );

    let print_a = canonical_serialize(&camp_a);
    let print_b = canonical_serialize(&camp_b);
    assert_eq!(
        print_a.matches("fnv1a64:").count(),
        2,
        "campaign A carries exactly the two known fingerprint fragments"
    );
    assert_eq!(
        print_b.matches("fnv1a64:").count(),
        2,
        "campaign B carries exactly the two known fingerprint fragments"
    );

    // The subject binding is real: each campaign names its own IR.
    let subject_fp = |c: &Sexpr| {
        field(c, "subject")
            .and_then(|s| s.assoc("ir-fingerprint"))
            .and_then(|f| f.as_str())
            .map(|f| f.to_string())
            .expect("subject ir-fingerprint")
    };
    assert_eq!(subject_fp(&camp_a), a.fingerprint);
    assert_eq!(subject_fp(&camp_b), b.fingerprint);
    assert_ne!(
        a.fingerprint, b.fingerprint,
        "the constants header participates in the IR fingerprint"
    );
    assert_ne!(print_a, print_b);

    let masked_a = canonical_serialize(&mask_fingerprints(&camp_a));
    let masked_b = canonical_serialize(&mask_fingerprints(&camp_b));
    assert_eq!(
        masked_a, masked_b,
        "campaign outcomes must be identical except the subject fingerprint"
    );
}

// ---------------------------------------------------------------------
// Item 06: coverage teeth (W408 / W409).
// ---------------------------------------------------------------------

/// 06: coverage(every_operation) with one unexercised op -> exactly one
/// W408 naming it -- and the SUFFIX-covered op earns none. In this
/// spec both readings of file-header ambiguity 3 agree: op_a has a
/// behavior AND probe_a's bare `op_a` step reaches its transition
/// (operation `widget_service/op_a`, matched by the suffix rule -- the
/// current binary shows probe_a's obligation PASSING), while op_b has
/// a behavior no step reaches and no step naming it. Warnings, not
/// errors: a gap is information, not invalidity.
#[test]
fn oracle_06a_uncovered_operation_exactly_one_w408() {
    let src = cov_spec(&format!("{}\n  coverage (every_operation),\n", PROBE_A));
    let (ir, _) = elab(&src);
    let bundle = verify::compile_verification(&ir);
    let w408 = coverage_diags_with_code(&bundle, "W408");
    assert_eq!(
        w408.len(),
        1,
        "exactly one uncovered operation; coverage-diagnostics: {:?}",
        coverage_diags(&bundle)
    );
    assert_eq!(diag_entry_severity(&w408[0]), "warning");
    assert!(
        diag_entry_message(&w408[0]).contains("op_b"),
        "the W408 names the uncovered op: {}",
        diag_entry_message(&w408[0])
    );
    assert!(
        diag_entry_message(&w408[0]).contains("production"),
        "the W408 names the acceptance node: {}",
        diag_entry_message(&w408[0])
    );
    assert!(
        !diag_entry_message(&w408[0]).contains("op_a"),
        "the suffix-covered op earns no W408"
    );
    // every_transition is not listed, so no W409 -- declared intent
    // only is checked, even though do_b's transition is unexercised.
    assert!(coverage_diags_with_code(&bundle, "W409").is_empty());
}

/// 06 (file-header ambiguity 3, the discriminating pin): an op NAMED
/// by an execute step but backed by NO behavior cannot be exercised
/// through the transition machinery -- the flagship `query_tasks`
/// shape -- and must be flagged W408. Derivation: the design doc says
/// "the flagship's query_tasks" is flagged, plan section C regenerates
/// the todo goldens WITH "W408 for query_tasks", and oracle item 08
/// pins it, while create_then_read's `query_tasks (actor, task.list)`
/// step names the op exactly; therefore being named by a step is NOT
/// coverage -- being exercised is. Here probe_c names op_c, op_c has
/// no behavior, and its obligation fails with no-matching-transition
/// (verified against the current binary): exactly one W408, naming
/// op_c, none naming op_a.
#[test]
fn oracle_06b_step_named_op_without_behavior_is_still_uncovered() {
    let (ir, _) = elab(COV_NO_BEHAVIOR);
    let bundle = verify::compile_verification(&ir);
    let w408 = coverage_diags_with_code(&bundle, "W408");
    assert_eq!(
        w408.len(),
        1,
        "exactly one uncovered operation (op_c); coverage-diagnostics: {:?}",
        coverage_diags(&bundle)
    );
    assert!(
        diag_entry_message(&w408[0]).contains("op_c"),
        "the W408 names op_c: {}",
        diag_entry_message(&w408[0])
    );
    assert!(!diag_entry_message(&w408[0]).contains("op_a"));
}

/// 06: with the coverage clause ABSENT, no coverage check runs at all
/// -- no W408 despite the same uncovered ops -- and the bundle still
/// carries an (empty) coverage-diagnostics field: the field is
/// structural, the checks are declared-intent-gated.
#[test]
fn oracle_06c_absent_coverage_clause_produces_no_checks() {
    let src = cov_spec(PROBE_A);
    let (ir, _) = elab(&src);
    let bundle = verify::compile_verification(&ir);
    let cds = coverage_diags(&bundle).expect("coverage-diagnostics field present");
    assert!(
        cds.is_empty(),
        "no coverage clause -> no coverage diagnostics; got {:?}",
        cds
    );
}

/// 06: the every_transition analog -- W409 per behavior no acceptance
/// step reaches. do_a's transition is reached by probe_a's step; do_b's
/// is reached by nothing -> exactly one W409 naming do_b. And with
/// every_operation NOT listed, no W408 despite op_b being uncovered.
#[test]
fn oracle_06d_unexercised_transition_exactly_one_w409() {
    let src = cov_spec(&format!("{}\n  coverage (every_transition),\n", PROBE_A));
    let (ir, _) = elab(&src);
    let bundle = verify::compile_verification(&ir);
    let w409 = coverage_diags_with_code(&bundle, "W409");
    assert_eq!(
        w409.len(),
        1,
        "exactly one unexercised transition; coverage-diagnostics: {:?}",
        coverage_diags(&bundle)
    );
    assert_eq!(diag_entry_severity(&w409[0]), "warning");
    assert!(
        diag_entry_message(&w409[0]).contains("do_b"),
        "the W409 names the unexercised behavior: {}",
        diag_entry_message(&w409[0])
    );
    assert!(
        diag_entry_message(&w409[0]).contains("production"),
        "the W409 names the acceptance node: {}",
        diag_entry_message(&w409[0])
    );
    assert!(!diag_entry_message(&w409[0]).contains("do_a"));
    assert!(
        coverage_diags_with_code(&bundle, "W408").is_empty(),
        "every_operation is not listed, so no W408"
    );
}

/// 06: the bundle's pinned field order -- coverage-diagnostics lands
/// after transition-diagnostics, before diagnostics (plan section B's
/// explicit order, extending the phase-7 order by one field).
#[test]
fn oracle_06e_bundle_field_order_carries_coverage_diagnostics() {
    let src = cov_spec(&format!("{}\n  coverage (every_operation),\n", PROBE_A));
    let (ir, _) = elab(&src);
    let bundle = verify::compile_verification(&ir);
    assert_eq!(
        bundle_keys(&bundle),
        vec![
            "schema",
            "obligations",
            "results",
            "summary",
            "coverage",
            "environment-diagnostics",
            "transition-diagnostics",
            "coverage-diagnostics",
            "diagnostics",
            "source-diagnostics",
            "fingerprint",
        ],
        "bundle field order"
    );
}

// ---------------------------------------------------------------------
// Item 07: `of` binding.
// ---------------------------------------------------------------------

/// 07: `generate (actor ed of operator)` resolves against the declared
/// actor and the lowered obligation carries `(actor-of operator)`
/// IMMEDIATELY AFTER the `(generate ...)` pair. The resolved form
/// produces no W410 and no errors (the free-symbol warning is for the
/// unbound style only).
#[test]
fn oracle_07a_of_binding_lowers_actor_of() {
    let src = actor_spec("actor ed of operator");
    let (ir, all_diags) = elab(&src);
    assert!(
        error_diags(&all_diags).is_empty(),
        "resolved of-binding must not error: {:?}",
        all_diags
    );
    assert!(
        with_code(&all_diags, "W410").is_empty(),
        "an of-bound pair never warns W410"
    );

    let obligations = verify::lower_all_obligations(&ir);
    let prop = obligations
        .iter()
        .find(|o| o.assoc("kind").and_then(|k| k.as_sym()) == Some("property"))
        .expect("property obligation");
    let items = prop.as_list().expect("obligation is a list");
    let generate_index = items
        .iter()
        .position(|it| {
            it.as_list()
                .and_then(|l| l.first())
                .and_then(|h| h.as_sym())
                == Some("generate")
        })
        .expect("obligation carries a (generate ...) pair");
    let after = items
        .get(generate_index + 1)
        .expect("a field after (generate ...)")
        .as_list()
        .expect("(actor-of ...) pair");
    assert_eq!(after.len(), 2, "(actor-of name) is a two-element pair");
    assert_eq!(after[0].as_sym(), Some("actor-of"));
    assert_eq!(after[1], Sexpr::sym("operator"));
}

/// 07: an unknown actor after `of` is a closed-world ERROR (the plan
/// leaves the exact code to the existing unresolved-reference class --
/// file-header ambiguity 6 -- so the pin is severity + subject).
#[test]
fn oracle_07b_of_unknown_actor_is_an_error() {
    let src = actor_spec("actor ed of ghost_actor");
    let (_ir, all_diags) = elab(&src);
    assert!(
        error_diags(&all_diags)
            .iter()
            .any(|d| d.message.contains("ghost_actor")),
        "an unknown of-actor must be an error naming it; got {:?}",
        all_diags
    );
}

/// 07: a two-element pair whose generator symbol matches no declared
/// actor warns W410 (the current free-symbol style keeps working,
/// visibly) -- exactly one, naming the symbol, and no error.
#[test]
fn oracle_07c_bare_unknown_generator_warns_w410() {
    let src = actor_spec("actor stranger");
    let (_ir, all_diags) = elab(&src);
    assert!(
        error_diags(&all_diags).is_empty(),
        "W410 is a warning; existing specs keep working: {:?}",
        all_diags
    );
    let w410 = with_code(&all_diags, "W410");
    assert_eq!(
        w410.len(),
        1,
        "exactly one W410 for the one unbound pair; got {:?}",
        all_diags
    );
    assert!(w410[0].severity == Severity::Warning);
    assert!(
        w410[0].message.contains("stranger"),
        "the W410 names the generator symbol: {}",
        w410[0].message
    );
}

/// 07: a two-element pair whose generator symbol IS a declared actor
/// is silent. Vacuously green until W410 exists; it must STAY green
/// after Stage 2 -- the warning is for symbols the closed world does
/// not know.
#[test]
fn oracle_07d_bare_declared_actor_generator_is_silent() {
    let src = actor_spec("actor operator");
    let (_ir, all_diags) = elab(&src);
    assert!(
        with_code(&all_diags, "W410").is_empty(),
        "a declared-actor generator must not warn: {:?}",
        all_diags
    );
    assert!(error_diags(&all_diags).is_empty());
}

// ---------------------------------------------------------------------
// Item 08: flagship pins.
// ---------------------------------------------------------------------

/// 08: after Stage 3, `256` appears in todo.gym EXACTLY ONCE -- the
/// `use ... (sharing_limit 256, ...)` clause. Derivation from the
/// CURRENT file, which carries SIX occurrences of the substring "256"
/// (the use clause; invite_user's requires `< 256` and fails-when
/// `= 256`; the sharing_limit invariant's `<= 256`; the scenario's
/// `invite_distinct (owner, 256)`; the concurrency `<= 256`) plus one
/// "257" (the scenario's boundary probe): plan section C rewrites the
/// five non-use sites to `sharing_limit` / `sharing_limit + 1`, so the
/// boundary probe's 257 (256 + 1) disappears with them.
#[test]
fn oracle_08a_todo_gym_carries_exactly_one_256() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/todo.gym"))
        .expect("read ../examples/todo.gym");
    assert_eq!(
        src.matches("256").count(),
        1,
        "todo.gym must carry 256 in exactly one place (the use clause)"
    );
    assert!(
        !src.contains("257"),
        "the scenario boundary probe must be spelled sharing_limit + 1"
    );
}

/// 08: THE SEMANTIC INVARIANT, verification half -- todo.gym's summary
/// is (total 9) (passed 1) (failed 2) (skipped 4) (indeterminate 2),
/// and this must hold BOTH before and after the corpus update
/// (substitution preserves semantics; this test is green at Stage 1
/// and must never go red). Derivation of the 9 (current binary):
/// 2 properties + 1 scenario + 1 concurrency + 1 fault + 1 coverage
/// obligation from the acceptance node, + 2 invariants + 1 constraint;
/// passed 1 = viewer_cannot_mutate; failed 2 = create_then_read
/// (query_tasks step matches no transition) and sharing_boundary
/// (invite_distinct matches no transition); skipped 4 = concurrency,
/// fault, coverage, constraint; indeterminate 2 = both invariants
/// (forall bodies are undecidable for the closed evaluator).
#[test]
fn oracle_08b_todo_verify_summary_semantic_invariant() {
    let ir = todo_ir();
    let bundle = verify::compile_verification(&ir);
    let summary = verify::bundle_summary(&bundle).expect("summary");
    assert_eq!(summary.total, 9);
    assert_eq!(summary.passed, 1);
    assert_eq!(summary.failed, 2);
    assert_eq!(summary.skipped, 4);
    assert_eq!(summary.indeterminate, 2);
}

/// 08: THE SEMANTIC INVARIANT, adequacy half -- the standard five-
/// mutant campaign over todo.gym stays (total 5) (killed 0)
/// (survived 5) (inapplicable 0) (pass nil): all five mutants applied
/// and surviving, because property/scenario must-assertions are still
/// unevaluated -- the verifier's honestly-reported blind spot. Green
/// at Stage 1; must hold identically over the const-spelled corpus.
#[test]
fn oracle_08c_todo_adequacy_semantic_invariant() {
    let ir = todo_ir();
    let campaign = adequacy::run_campaign(&ir, &adequacy::standard_todo_mutants());
    let int_field = |key: &str| {
        field(&campaign, key)
            .and_then(|v| v.as_int())
            .unwrap_or_else(|| panic!("campaign field {}", key))
    };
    assert_eq!(int_field("total"), 5);
    assert_eq!(int_field("killed"), 0);
    assert_eq!(int_field("survived"), 5);
    assert_eq!(int_field("inapplicable"), 0);
    assert_eq!(
        field(&campaign, "pass").map(|p| p.print()),
        Some("nil".to_string()),
        "the campaign must honestly fail"
    );
}

/// 08: `query_tasks` is now honestly flagged uncovered -- todo.gym's
/// coverage clause lists every_operation, create_then_read's execute
/// step names query_tasks, but no behavior backs it, so it cannot be
/// exercised (file-header ambiguity 3; the design doc's flagship
/// consequence). PRESENCE is pinned, never the bundle's total W408
/// count (see the ambiguity note: `invite` may legitimately earn one
/// too, since invite_distinct suffix-matches no transition).
#[test]
fn oracle_08d_todo_query_tasks_w408_present() {
    let ir = todo_ir();
    let bundle = verify::compile_verification(&ir);
    let w408 = coverage_diags_with_code(&bundle, "W408");
    assert!(
        w408.iter()
            .any(|d| diag_entry_message(d).contains("query_tasks")),
        "todo's bundle must carry a W408 naming query_tasks; \
         coverage-diagnostics: {:?}",
        coverage_diags(&bundle)
    );
}

/// 08: all seven todo goldens match the committed fixtures byte for
/// byte, through the real CLI. Green at Stage 1 (the fixtures pin
/// today's behavior), red across Stage 2 (the new IR header, W408
/// entries, coverage-diagnostics field, and actor-of obligations land
/// in the artifacts), green again once Stage 3 regenerates the
/// fixtures exactly once. Together with 08b/08c this is the phase's
/// closing argument: the bytes may move, the verdicts may not.
#[test]
fn oracle_08e_todo_goldens_match_fixtures() {
    assert_eq!(
        todo_cli("ir"),
        include_str!("fixtures/todo-ir.sexpr"),
        "todo-ir.sexpr"
    );
    assert_eq!(
        todo_cli("plan"),
        include_str!("fixtures/todo-plan.sexpr"),
        "todo-plan.sexpr"
    );
    assert_eq!(
        todo_cli("prompts"),
        include_str!("fixtures/todo-prompts.sexpr"),
        "todo-prompts.sexpr"
    );
    assert_eq!(
        todo_cli("verify"),
        include_str!("fixtures/todo-verify.sexpr"),
        "todo-verify.sexpr"
    );
    assert_eq!(
        todo_cli("adequacy"),
        include_str!("fixtures/todo-adequacy.sexpr"),
        "todo-adequacy.sexpr"
    );

    // The two compile-tree artifacts.
    let out = std::env::temp_dir().join(format!("gymnast-v02-oracle-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let status = Command::new(env!("CARGO_BIN_EXE_gymnast-rs"))
        .args([
            "compile",
            concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/todo.gym"),
            out.to_str().unwrap(),
        ])
        .status()
        .expect("run compile");
    assert!(status.success(), "compile of todo.gym must exit 0");
    assert_eq!(
        std::fs::read_to_string(out.join("results.sexpr")).expect("results.sexpr"),
        include_str!("fixtures/todo-results.sexpr"),
        "todo-results.sexpr"
    );
    assert_eq!(
        std::fs::read_to_string(out.join("evidence-bundle.sexpr")).expect("evidence-bundle.sexpr"),
        include_str!("fixtures/todo-bundle.sexpr"),
        "todo-bundle.sexpr"
    );
    let _ = std::fs::remove_dir_all(&out);
}
