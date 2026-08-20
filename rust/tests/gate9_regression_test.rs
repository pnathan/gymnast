//! Regression tests for the phase-9 Opus gate's findings (new file, not
//! a frozen oracle):
//!
//!  1. BLOCKER — the campaign result is bound to its subject, and a
//!     mutant whose target is absent is INAPPLICABLE: never a survivor,
//!     never a blind spot. The gate's reproduction: `adequacy` over
//!     bi-ingest.gym emitted output byte-identical (fingerprint
//!     included) to the committed todo golden, fabricating five blind
//!     spots about mutations that never applied.
//!  2. MAJOR — `adequacy` refuses specs whose verification bundle
//!     carries error diagnostics (E601 makes the first-match baseline
//!     lookup unsound), matching the `verify` contract.
//!  3. MAJOR — `degraded-only` has campaign-level teeth.
//!  4. MINOR — criticality discriminates: a non-critical survivor
//!     neither blocks `pass` nor becomes a blind spot.
//!  5. MINOR — the baseline is NOT accumulated across mutants: two
//!     mutants killing via the same obligation id are both killed.

use gymnast_rs::adequacy::{run_campaign, run_mutant, Mutant, Mutation};
use gymnast_rs::ir::{Ir, IrNode};
use gymnast_rs::sexpr::{self, Sexpr};
use gymnast_rs::verify::{lower_all_obligations, verify_obligation};
use gymnast_rs::{elaborate, parser};
use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

fn ps(text: &str) -> Sexpr {
    sexpr::parse(text).unwrap_or_else(|e| panic!("parse {:?}: {}", text, e))
}

fn nested_field<'a>(v: &'a Sexpr, key: &str) -> &'a Sexpr {
    v.as_list()
        .and_then(|items| items.get(1))
        .and_then(|inner| inner.assoc(key))
        .unwrap_or_else(|| panic!("no nested field {:?} in {}", key, v.print()))
}

fn baseline_results(ir: &Ir) -> Vec<Sexpr> {
    lower_all_obligations(ir)
        .iter()
        .map(|o| verify_obligation(ir, o))
        .collect()
}

/// Mirrors the frozen oracle's synthetic degraded IR: invariant
/// `(< count 10)` passes at baseline (the grounded failing requires
/// keeps count at Int 0); dropping the requires lets the write turn
/// count into a list, so the invariant becomes indeterminate.
fn degraded_ir() -> Ir {
    let state = IrNode::new(
        "syn/state/count".to_string(),
        "state",
        "count".to_string(),
        vec![(":initial".to_string(), Sexpr::Int(0))],
        vec![],
    );
    let behavior = IrNode::new(
        "syn/behavior/bump".to_string(),
        "behavior",
        "bump".to_string(),
        vec![
            (":on".to_string(), ps("(svc/bump actor input)")),
            (":writes".to_string(), ps("(count)")),
        ],
        vec![ps("(requires (= 1 2))")],
    );
    let invariant = IrNode::new(
        "syn/invariant/cap".to_string(),
        "invariant",
        "cap".to_string(),
        vec![
            (":always".to_string(), ps("(< count 10)")),
            (":scope".to_string(), Sexpr::sym("count")),
        ],
        vec![],
    );
    Ir::new(
        "gymnast.ir/0.1".to_string(),
        "syn".to_string(),
        vec![],
        vec![state],
        vec![behavior],
        vec![invariant],
        vec![],
        vec![],
    )
}

/// Invariant `(< count 10)` over Int 0 with no behaviors: baseline
/// `passed`; `WeakenLimit -1` makes the initial check a grounded Fails,
/// so the mutant is KILLED with that obligation detecting.
fn killable_ir() -> Ir {
    let state = IrNode::new(
        "syn/state/count".to_string(),
        "state",
        "count".to_string(),
        vec![(":initial".to_string(), Sexpr::Int(0))],
        vec![],
    );
    let invariant = IrNode::new(
        "syn/invariant/cap".to_string(),
        "invariant",
        "cap".to_string(),
        vec![
            (":always".to_string(), ps("(< count 10)")),
            (":scope".to_string(), Sexpr::sym("count")),
        ],
        vec![],
    );
    Ir::new(
        "gymnast.ir/0.1".to_string(),
        "syn".to_string(),
        vec![],
        vec![state],
        vec![],
        vec![invariant],
        vec![],
        vec![],
    )
}

fn mutant(id: &str, class: &str, mutation: Mutation, critical: bool) -> Mutant {
    Mutant {
        id: id.to_string(),
        class: class.to_string(),
        description: format!("{} ({})", id, class),
        mutation,
        critical,
    }
}

// ---------------------------------------------------------------------
// Finding 1: subject binding and inapplicability honesty.
// ---------------------------------------------------------------------

#[test]
fn gate1_missing_target_is_inapplicable_never_a_survivor() {
    let ir = killable_ir();
    let baseline = baseline_results(&ir);
    let m = mutant(
        "x1",
        "weaken-precondition",
        Mutation::WeakenPrecondition {
            behavior_name: "no_such_behavior".to_string(),
        },
        true,
    );
    let result = run_mutant(&ir, &baseline, &m);
    assert!(!result.applied, "missing target must report inapplicable");
    assert!(!result.killed);
    assert!(result.detecting_obligations.is_empty());
}

#[test]
fn gate1_campaign_over_foreign_spec_is_bound_and_fabricates_nothing() {
    // The gate's reproduction, at the library level: the todo mutant
    // set over the bi-ingest IR. Every target is absent — the honest
    // shape is 5 inapplicable, 0 survivors, 0 blind spots, pass nil
    // (the campaign could not test those defect classes), with the
    // subject naming bi_ingest and a fingerprint that can never equal
    // the todo golden's.
    let src = fs::read_to_string("../examples/bi-ingest.gym").expect("read bi-ingest.gym");
    let (ast, parse_diags) = parser::parse(&src);
    let file = ast.expect("parse bi-ingest.gym");
    let (ir, _diags) = elaborate::elaborate_with_parse_diags(&file, &parse_diags);

    let campaign = run_campaign(&ir, &gymnast_rs::adequacy::standard_todo_mutants());
    assert_eq!(nested_field(&campaign, "inapplicable"), &Sexpr::Int(5));
    assert_eq!(nested_field(&campaign, "survived"), &Sexpr::Int(0));
    assert_eq!(nested_field(&campaign, "killed"), &Sexpr::Int(0));
    assert_eq!(nested_field(&campaign, "critical-survived"), &Sexpr::Int(0));
    assert_eq!(
        nested_field(&campaign, "blind-spots"),
        &Sexpr::List(vec![]),
        "an untested defect class is not a blind spot"
    );
    assert_eq!(
        nested_field(&campaign, "pass"),
        &Sexpr::List(vec![]),
        "inapplicable critical mutants must not read as pass"
    );
    let subject = nested_field(&campaign, "subject");
    assert_eq!(
        subject.assoc("module").and_then(|m| m.as_str()),
        Some("bi_ingest")
    );
    assert_eq!(
        subject.assoc("ir-fingerprint").and_then(|f| f.as_str()),
        Some(ir.fingerprint.as_str())
    );

    // And the whole artifact — fingerprint included — differs from the
    // committed todo golden byte-for-byte.
    let golden = fs::read_to_string("tests/fixtures/todo-adequacy.sexpr").expect("golden");
    assert_ne!(sexpr::canonical_serialize(&campaign), golden);
}

// ---------------------------------------------------------------------
// Finding 2: adequacy refuses an unsound baseline.
// ---------------------------------------------------------------------

static TEMP_SEQ: AtomicU32 = AtomicU32::new(0);

#[test]
fn gate2_adequacy_exits_nonzero_on_bundle_error_diagnostics() {
    // Duplicate property → E601 in the verification bundle → the
    // first-match baseline lookup would be unsound → adequacy must
    // refuse (verify already does), emitting no campaign at all.
    let src = fs::read_to_string("../examples/todo.gym").expect("read todo.gym");
    let block = "  property viewer_cannot_mutate =\n    generate (actor authenticated_viewer, task valid_task)\n    execute create_task (actor, task)\n    must fails_with forbidden,\n";
    assert!(src.contains(block), "todo.gym block moved; update test");
    let doubled = src.replace(block, &format!("{}\n{}", block, block));
    let path = std::env::temp_dir().join(format!(
        "gate9-dup-property-{}-{}.gym",
        std::process::id(),
        TEMP_SEQ.fetch_add(1, Ordering::SeqCst)
    ));
    fs::write(&path, doubled).expect("write temp spec");

    let out = Command::new(env!("CARGO_BIN_EXE_gymnast-rs"))
        .args(["adequacy", path.to_str().expect("utf-8 path")])
        .output()
        .expect("run adequacy");
    let _ = fs::remove_file(&path);

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!out.status.success(), "unsound baseline must be refused");
    assert!(
        stderr.contains("E601"),
        "stderr must name the error: {}",
        stderr
    );
    assert!(
        !stdout.contains("campaign-result"),
        "no campaign may be emitted over an unsound baseline"
    );
}

// ---------------------------------------------------------------------
// Finding 3: degraded-only has campaign-level teeth.
// ---------------------------------------------------------------------

#[test]
fn gate3_campaign_counts_degraded_only() {
    let ir = degraded_ir();
    let m = mutant(
        "syn-d1",
        "weaken-precondition",
        Mutation::WeakenPrecondition {
            behavior_name: "bump".to_string(),
        },
        true,
    );
    let campaign = run_campaign(&ir, &[m]);
    assert_eq!(nested_field(&campaign, "degraded-only"), &Sexpr::Int(1));
    assert_eq!(nested_field(&campaign, "survived"), &Sexpr::Int(1));
    assert_eq!(nested_field(&campaign, "killed"), &Sexpr::Int(0));
    assert_eq!(nested_field(&campaign, "pass"), &Sexpr::List(vec![]));
}

// ---------------------------------------------------------------------
// Finding 4: criticality discriminates.
// ---------------------------------------------------------------------

#[test]
fn gate4_non_critical_survivor_neither_blocks_pass_nor_blinds() {
    let ir = degraded_ir();
    let m = mutant(
        "nc1",
        "weaken-precondition",
        Mutation::WeakenPrecondition {
            behavior_name: "bump".to_string(),
        },
        false, // non-critical
    );
    let campaign = run_campaign(&ir, &[m]);
    assert_eq!(nested_field(&campaign, "survived"), &Sexpr::Int(1));
    assert_eq!(nested_field(&campaign, "critical-survived"), &Sexpr::Int(0));
    assert_eq!(
        nested_field(&campaign, "blind-spots"),
        &Sexpr::List(vec![]),
        "blind spots are CRITICAL survivors only"
    );
    assert_eq!(
        nested_field(&campaign, "pass"),
        &Sexpr::sym("t"),
        "a non-critical survivor must not block pass"
    );
}

// ---------------------------------------------------------------------
// Finding 5: the baseline is not accumulated across mutants.
// ---------------------------------------------------------------------

#[test]
fn gate5_two_mutants_killing_via_same_obligation_are_both_killed() {
    let ir = killable_ir();
    let make = |id: &str| {
        mutant(
            id,
            "weaken-limit",
            Mutation::WeakenLimit {
                invariant_name: "cap".to_string(),
                new_limit: -1,
            },
            true,
        )
    };
    let campaign = run_campaign(&ir, &[make("k1"), make("k2")]);
    assert_eq!(
        nested_field(&campaign, "killed"),
        &Sexpr::Int(2),
        "mutant k2 must diff against the ORIGINAL baseline, not one \
         polluted by k1's failure"
    );
    assert_eq!(nested_field(&campaign, "survived"), &Sexpr::Int(0));
    assert_eq!(nested_field(&campaign, "pass"), &Sexpr::sym("t"));
}
