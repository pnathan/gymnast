//! Tests-of-record for `cache.rs` (`docs/rust-port-plan-phase7.md`,
//! section E), plus the section-D bundle fingerprint/typed-summary/E601
//! additions to `verify.rs` and the section-F `RunResult`/`Attempt`
//! readback additions to `runner.rs` -- all three grouped here because the
//! plan's own Stage 3 groups sections E-F together and lists every one of
//! these items under this file's own oracle-test bullet list. Authored
//! from the plan ALONE, BEFORE `crate::cache` exists and before
//! `verify::bundle_summary`/`VerificationSummary`, the bundle's
//! `fingerprint`/`diagnostics` fields, or `runner::{Attempt,
//! RunResult}::from_sexpr`/`RunResult::node_fingerprint` exist (the
//! committed-oracle upgrade: Stage 1 commits this file to git before any
//! implementation stage runs). `src/cache.lisp` (read in full) was
//! consulted only for BEHAVIORAL INTENT; every Rust shape adaptation comes
//! from the phase-7 plan's explicit signatures.
//!
//! Implementation stages MUST NOT edit this file. If an assertion here
//! turns out to conflict with a considered implementation choice, the
//! implementer reports the conflict; only the integrator resolves it.
//!
//! This file will not compile until `crate::cache` exists and the
//! `verify.rs`/`runner.rs` additions above land -- that is expected at
//! this stage.
//!
//! RESOLVED AMBIGUITIES (plan text under-specifies these; the
//! contract-consistent reading taken here is noted at each site too):
//!
//!  1. `cache_check_plan`/`cache_explain_node`'s elided (`...`) parameter
//!     lists are read by direct analogy with `cache_check_node`'s FULLY
//!     given signature `(store: &CacheStore, ir: &Ir, plan: &Plan, node:
//!     &PlanNode) -> Sexpr` -- i.e. `cache_check_plan(store: &CacheStore,
//!     ir: &Ir, plan: &Plan) -> Vec<Sexpr>` (mapping `cache_check_node`
//!     over `plan.nodes`, mirroring `src/cache.lisp`'s
//!     `gymnast-cache-check-plan (ir plan)` with the store threaded in as
//!     every other section-E function threads it) and
//!     `cache_explain_node(store: &CacheStore, ir: &Ir, plan: &Plan, node:
//!     &PlanNode) -> Sexpr`, same parameter order as `cache_check_node`.
//!  2. `cache_explain_node`'s `key-mismatch` branch: `CacheStore` exposes
//!     lookup ONLY by key (`lookup(&self, key: &str)`), and `store()`'s
//!     only source for that key is `entry.key` itself -- so an entry found
//!     via `store.lookup(cache_key(ir, plan, node))` necessarily has
//!     `entry.key == cache_key(ir, plan, node)` BY CONSTRUCTION, making
//!     `entry_valid` (pure key equality) trivially true whenever an entry
//!     is found this way. A STALE entry (stored under an id's OLD key,
//!     now superseded because the node/plan/ir changed) is therefore only
//!     discoverable by scanning `store.keys()` and filtering by
//!     `entry.node_id`, not by looking up the current key at all -- the
//!     only way `explain`'s `stored-key` field could ever be populated
//!     with a value distinct from `key`. This is the reading exercised
//!     below: `key-mismatch` requires an entry that once matched this
//!     node id under a now-superseded key.
//!  3. `cache_key_material`'s six Sexpr field encodings follow the SAME
//!     id-vs-vocabulary-term convention `PlanNode::field_pairs` (already
//!     read in full) already establishes: `node-fingerprint`/
//!     `ir-slice-fingerprint`/`dependency-fingerprint` (content hashes,
//!     like `PlanNode.fingerprint` itself) as `Sexpr::Str`; `recipe`
//!     (a vocabulary term, like `PlanNode.recipe`) as `Sexpr::sym`;
//!     `capabilities` (a vocabulary-term list, like `PlanNode.capabilities`)
//!     as a list of `Sexpr::sym`; `model` passed through verbatim (it is
//!     already an `Sexpr` on `PlanNode`).

use gymnast_rs::cache::{
    cache_check_node, cache_check_plan, cache_explain_node, cache_key, cache_key_material,
    cache_store_result, diff_plans, entry_valid, invalidated_nodes, node_dependents,
    transitive_dependents, CacheEntry, CacheStore, CACHE_SCHEMA,
};
use gymnast_rs::diag::diag_sexpr;
use gymnast_rs::elaborate;
use gymnast_rs::fingerprint;
use gymnast_rs::ir::{resolve_ir_slice, Ir, IrNode};
use gymnast_rs::parser;
use gymnast_rs::plan::{plan, Plan, PlanNode};
use gymnast_rs::runner::{Attempt, AttemptStatus, RunResult, RunStatus};
use gymnast_rs::sexpr::Sexpr;
use gymnast_rs::verify::{bundle_summary, compile_verification, lower_all_obligations};
use std::fs;

// ---------------------------------------------------------------------
// Shared fixtures / helpers (not tests themselves).
// ---------------------------------------------------------------------

fn load_todo_ir_from_source(src: &str) -> Ir {
    let (ast, parse_diags) = parser::parse(src);
    let file = ast.expect("parse todo.gym (or a modified variant)");
    let (ir, _all_diags) = elaborate::elaborate_with_parse_diags(&file, &parse_diags);
    ir
}

fn load_todo_ir() -> Ir {
    let src = fs::read_to_string("../examples/todo.gym").expect("read ../examples/todo.gym");
    load_todo_ir_from_source(&src)
}

/// Re-parses todo.gym with the profile's `sharing_limit` argument changed
/// from 256 to 100 -- the ONLY textual change. This edits the
/// `todo/import/oddities/profiles/todo_standard` node's `:arguments`
/// field alone; the profile generator itself ignores its args (see
/// `rust/src/profile.rs`'s `generate_todo_standard`, read in full), so no
/// generated type shape changes, only that one field's literal value.
fn modified_sharing_limit_ir() -> Ir {
    let src = fs::read_to_string("../examples/todo.gym").expect("read ../examples/todo.gym");
    let needle = "(sharing_limit 256, identity_provider google)";
    assert!(
        src.contains(needle),
        "fixture drift: expected literal `{}` in todo.gym",
        needle
    );
    let modified = src.replacen(needle, "(sharing_limit 100, identity_provider google)", 1);
    load_todo_ir_from_source(&modified)
}

fn field<'a>(v: &'a Sexpr, key: &str) -> Option<&'a Sexpr> {
    if let Some(found) = v.assoc(key) {
        return Some(found);
    }
    v.as_list()
        .and_then(|items| items.get(1))
        .and_then(|inner| inner.assoc(key))
}

fn nil() -> Sexpr {
    Sexpr::list(vec![])
}

fn node_by_local<'a>(p: &'a Plan, local: &str) -> &'a PlanNode {
    let id = format!("todo/plan/{}", local);
    p.node(&id)
        .unwrap_or_else(|| panic!("plan must have node {}", id))
}

fn sample_candidate() -> Sexpr {
    Sexpr::list(vec![
        Sexpr::sym("candidate"),
        Sexpr::list(vec![Sexpr::pair("node-id", Sexpr::Str("x".to_string()))]),
    ])
}

fn expected_ir_slice_fingerprint(ir: &Ir, node: &PlanNode) -> String {
    let (resolved, _warnings) = resolve_ir_slice(ir, &node.id, &node.inputs);
    let slice_sexpr = Sexpr::list(vec![
        Sexpr::sym("ir-slice"),
        Sexpr::list(resolved.iter().map(|n| n.to_sexpr()).collect()),
    ]);
    fingerprint::fingerprint(&slice_sexpr)
}

fn expected_dependency_fingerprint(p: &Plan, node: &PlanNode) -> String {
    let pairs: Vec<Sexpr> = node
        .depends_on
        .iter()
        .map(|dep_id| {
            let fp = p
                .nodes
                .iter()
                .find(|n| &n.id == dep_id)
                .map(|n| n.fingerprint.clone())
                .unwrap_or_else(|| "missing".to_string());
            Sexpr::list(vec![Sexpr::Str(dep_id.clone()), Sexpr::Str(fp)])
        })
        .collect();
    fingerprint::fingerprint(&Sexpr::list(pairs))
}

// =======================================================================
// 1. Key determinism across two independent pipeline runs.
// =======================================================================

#[test]
fn oracle_01_cache_key_deterministic_across_independent_pipeline_runs() {
    let ir_a = load_todo_ir();
    let p_a = plan(&ir_a);
    let ir_b = load_todo_ir();
    let p_b = plan(&ir_b);
    assert_eq!(p_a.nodes.len(), p_b.nodes.len());
    for (na, nb) in p_a.nodes.iter().zip(p_b.nodes.iter()) {
        assert_eq!(na.id, nb.id);
        let ka = cache_key(&ir_a, &p_a, na);
        let kb = cache_key(&ir_b, &p_b, nb);
        assert_eq!(
            ka, kb,
            "{}: cache key must be identical across two independent pipeline runs",
            na.id
        );
    }
}

// =======================================================================
// cache_key_material shape: the six fields in order, each value pinned
// exactly (see file header ambiguity 3), and cache_key == fingerprint of
// the material.
// =======================================================================

#[test]
fn oracle_01b_cache_key_material_shape_and_values_over_every_plan_node() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    for node in &p.nodes {
        let material = cache_key_material(&ir, &p, node);
        assert_eq!(
            material
                .as_list()
                .and_then(|l| l.first())
                .and_then(|s| s.as_sym()),
            Some("cache-key-material")
        );
        assert_eq!(
            field(&material, "node-fingerprint").and_then(|s| s.as_str()),
            Some(node.fingerprint.as_str()),
            "{}: node-fingerprint",
            node.id
        );
        let expected_slice_fp = expected_ir_slice_fingerprint(&ir, node);
        assert_eq!(
            field(&material, "ir-slice-fingerprint").and_then(|s| s.as_str()),
            Some(expected_slice_fp.as_str()),
            "{}: ir-slice-fingerprint",
            node.id
        );
        let expected_dep_fp = expected_dependency_fingerprint(&p, node);
        assert_eq!(
            field(&material, "dependency-fingerprint").and_then(|s| s.as_str()),
            Some(expected_dep_fp.as_str()),
            "{}: dependency-fingerprint",
            node.id
        );
        assert_eq!(
            field(&material, "recipe").and_then(|s| s.as_sym()),
            Some(node.recipe.as_str()),
            "{}: recipe",
            node.id
        );
        assert_eq!(
            field(&material, "model"),
            Some(&node.model),
            "{}: model",
            node.id
        );
        let expected_caps = Sexpr::list(node.capabilities.iter().map(|c| Sexpr::sym(c)).collect());
        assert_eq!(
            field(&material, "capabilities"),
            Some(&expected_caps),
            "{}: capabilities",
            node.id
        );

        let expected_key = fingerprint::fingerprint(&material);
        assert_eq!(
            cache_key(&ir, &p, node),
            expected_key,
            "{}: cache_key must be the fingerprint of cache_key_material",
            node.id
        );
    }
}

// =======================================================================
// 2. Key sensitivity per material component (four tamper cases).
// =======================================================================

#[test]
fn oracle_02a_tamper_recipe_changes_key() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    let node = node_by_local(&p, "design-contracts");
    let mut tampered = node.clone();
    tampered.recipe = "tampered-recipe-v1".to_string();
    assert_ne!(
        cache_key(&ir, &p, node),
        cache_key(&ir, &p, &tampered),
        "changing recipe alone must move the cache key"
    );
}

#[test]
fn oracle_02b_tamper_model_changes_key() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    // A generative node so `model` is not the shared `none` sentinel.
    let node = node_by_local(&p, "transition-kernel");
    let mut tampered = node.clone();
    tampered.model = Sexpr::sym("tampered-model");
    assert_ne!(
        cache_key(&ir, &p, node),
        cache_key(&ir, &p, &tampered),
        "changing model alone must move the cache key"
    );
}

#[test]
fn oracle_02c_tamper_a_dependencys_fingerprint_changes_key() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    // transition-kernel depends on design-contracts; mutate ONLY
    // design-contracts' stored fingerprint inside a cloned plan, leaving
    // every plan-node's own contract fields (including transition-
    // kernel's) byte-identical.
    let mut tampered_plan = p.clone();
    {
        let dc = tampered_plan
            .nodes
            .iter_mut()
            .find(|n| n.id == "todo/plan/design-contracts")
            .unwrap();
        dc.fingerprint = "fnv1a64:0".to_string();
    }
    let tk = node_by_local(&p, "transition-kernel");
    assert_ne!(
        cache_key(&ir, &p, tk),
        cache_key(&ir, &tampered_plan, tk),
        "a dependency's fingerprint moving alone must move the dependent's cache key"
    );
}

#[test]
fn oracle_02d_tamper_the_ir_slice_changes_key_but_not_the_plan_node_fingerprint() {
    // The sharing_limit profile-argument-only spec edit changes the
    // `todo/import/.../todo_standard` node's `:arguments` field (and
    // hence its own content, and hence the IR's top-level fingerprint),
    // but NOT its id -- and `application-assembly` is the only plan node
    // whose `inputs` includes that import node (see build_plan_nodes'
    // kind set `{application, import, component, synthesis, constraint}`).
    let ir1 = load_todo_ir();
    let p1 = plan(&ir1);
    let ir2 = modified_sharing_limit_ir();
    let p2 = plan(&ir2);

    let aa1 = node_by_local(&p1, "application-assembly");
    let aa2 = node_by_local(&p2, "application-assembly");

    // The PlanNode's own contract embeds only ID STRINGS in `inputs`,
    // never resolved node content -- so its fingerprint does NOT move.
    assert_eq!(
        aa1.fingerprint, aa2.fingerprint,
        "PlanNode contracts reference inputs by id only; a resolved-content-only change \
         must not move the plan-node's own fingerprint"
    );

    // But cache_key's ir-slice-fingerprint IS computed over the node's
    // RESOLVED slice content (the actual import node, arguments and
    // all) -- so the cache key must move even though the plan-node
    // fingerprint did not: "the key covers what was actually used."
    assert_ne!(
        cache_key(&ir1, &p1, aa1),
        cache_key(&ir2, &p2, aa2),
        "a resolved ir-slice content change must move the cache key even when the plan-node \
         fingerprint itself does not"
    );
}

// =======================================================================
// 3. store/lookup/replace/clear/len.
// =======================================================================

fn make_entry(key: &str, node_id: &str, candidate: Sexpr) -> CacheEntry {
    CacheEntry {
        key: key.to_string(),
        node_id: node_id.to_string(),
        candidate,
        evidence: nil(),
        // The cache NEVER reads a clock (determinism); this is a caller-
        // supplied opaque symbol, never a real timestamp.
        timestamp: Sexpr::sym("epoch-1"),
    }
}

#[test]
fn oracle_03a_new_store_is_empty() {
    let store = CacheStore::new();
    assert_eq!(store.len(), 0);
    assert!(store.is_empty());
    assert!(store.keys().is_empty());
    assert_eq!(store.lookup("anything"), None);
}

#[test]
fn oracle_03b_store_and_lookup_two_distinct_keys() {
    let mut store = CacheStore::new();
    let e1 = make_entry("k1", "m/plan/a", sample_candidate());
    let e2 = make_entry("k2", "m/plan/b", sample_candidate());
    store.store(e1.clone());
    store.store(e2.clone());
    assert_eq!(store.len(), 2);
    assert!(!store.is_empty());
    assert_eq!(store.lookup("k1"), Some(&e1));
    assert_eq!(store.lookup("k2"), Some(&e2));
    let mut keys = store.keys();
    keys.sort();
    assert_eq!(keys, vec!["k1", "k2"]);
}

#[test]
fn oracle_03c_store_replaces_same_key_len_unchanged_lookup_returns_newest() {
    let mut store = CacheStore::new();
    let old = make_entry("k1", "m/plan/a", Sexpr::sym("old-candidate"));
    let new = make_entry("k1", "m/plan/a", Sexpr::sym("new-candidate"));
    store.store(old);
    assert_eq!(store.len(), 1);
    store.store(new.clone());
    assert_eq!(
        store.len(),
        1,
        "storing under an existing key must replace, not grow"
    );
    assert_eq!(store.lookup("k1"), Some(&new));
    assert_eq!(
        store.keys(),
        vec!["k1"],
        "no duplicate key entries after replace"
    );
}

#[test]
fn oracle_03d_clear_empties_the_store() {
    let mut store = CacheStore::new();
    store.store(make_entry("k1", "m/plan/a", sample_candidate()));
    store.store(make_entry("k2", "m/plan/b", sample_candidate()));
    assert_eq!(store.len(), 2);
    store.clear();
    assert_eq!(store.len(), 0);
    assert!(store.is_empty());
    assert_eq!(store.lookup("k1"), None);
    assert_eq!(store.lookup("k2"), None);
}

#[test]
fn oracle_03e_cache_schema_constant() {
    assert_eq!(CACHE_SCHEMA, "gymnast.cache/0.1");
}

// =======================================================================
// 4. entry_valid: pure key equality.
// =======================================================================

#[test]
fn oracle_04_entry_valid_is_pure_key_equality() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    let node = node_by_local(&p, "design-contracts");
    let key = cache_key(&ir, &p, node);

    let valid_entry = make_entry(&key, &node.id, sample_candidate());
    assert!(
        entry_valid(&ir, &p, node, &valid_entry),
        "an entry whose key matches the current cache_key must be valid"
    );

    let mut mismatched = valid_entry.clone();
    mismatched.key = "not-the-real-key".to_string();
    assert!(
        !entry_valid(&ir, &p, node, &mismatched),
        "an entry whose key does not match the current cache_key must be invalid, \
         REGARDLESS of node_id/candidate/evidence/timestamp content"
    );

    // Purity: candidate/evidence/timestamp content must not matter, only
    // the key.
    let mut different_payload = valid_entry;
    different_payload.candidate = Sexpr::sym("wildly-different-payload");
    different_payload.evidence = Sexpr::sym("also-different");
    assert!(
        entry_valid(&ir, &p, node, &different_payload),
        "entry_valid must depend on the key alone, not on candidate/evidence content"
    );
}

// =======================================================================
// 5. Transitive dependency closure per the fixed 8-node table, for seeds
//    design-contracts, transition-kernel, service-handlers,
//    acceptance-harness (derived and pinned below; includes-seed
//    asserted). Direct `node_dependents` for design-contracts is
//    the DERIVATION's starting fact (plan: "dependents of
//    design-contracts are {transition-kernel, authorization-policy,
//    persistence, interface-contracts}").
// =======================================================================

#[test]
fn oracle_05a_direct_dependents_of_design_contracts() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    let dependents = node_dependents(&p, "todo/plan/design-contracts");
    let ids: Vec<&str> = dependents.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            "todo/plan/transition-kernel",
            "todo/plan/authorization-policy",
            "todo/plan/persistence",
            "todo/plan/interface-contracts",
        ],
        "direct dependents of design-contracts, in plan (table) order"
    );
}

// DERIVATION (BFS over the 8-node table's `depends-on` edges, "seed
// first, then BFS in plan/table order" -- worked out fully against
// tests/fixtures/todo-plan.sexpr's depends-on lists):
//
// design-contracts   -> {transition-kernel, authorization-policy,
//                         persistence, interface-contracts} (direct)
//                    -> transitively EVERY other node (8 total, all of
//                         them, since transition-kernel's own dependents
//                         reach service-handlers/application-assembly,
//                         and service-handlers reaches acceptance-harness)
// transition-kernel  -> {authorization-policy, persistence,
//                         service-handlers, application-assembly} (direct)
//                    -> + acceptance-harness (via service-handlers) = 6
// service-handlers   -> {acceptance-harness, application-assembly} (direct)
//                    -> no further (3 total)
// acceptance-harness -> {application-assembly} (direct) -> no further
//                         (2 total)
#[test]
fn oracle_05b_transitive_closure_design_contracts_is_all_eight_nodes() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    let closure = transitive_dependents(&p, "todo/plan/design-contracts");
    assert_eq!(
        closure,
        vec![
            "todo/plan/design-contracts".to_string(),
            "todo/plan/transition-kernel".to_string(),
            "todo/plan/authorization-policy".to_string(),
            "todo/plan/persistence".to_string(),
            "todo/plan/interface-contracts".to_string(),
            "todo/plan/service-handlers".to_string(),
            "todo/plan/application-assembly".to_string(),
            "todo/plan/acceptance-harness".to_string(),
        ],
        "design-contracts is the root every other node transitively depends on"
    );
    assert!(
        closure.contains(&"todo/plan/design-contracts".to_string()),
        "closure must include the seed"
    );
    assert_eq!(closure.len(), 8);
}

#[test]
fn oracle_05c_transitive_closure_transition_kernel() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    let closure = transitive_dependents(&p, "todo/plan/transition-kernel");
    assert_eq!(
        closure,
        vec![
            "todo/plan/transition-kernel".to_string(),
            "todo/plan/authorization-policy".to_string(),
            "todo/plan/persistence".to_string(),
            "todo/plan/service-handlers".to_string(),
            "todo/plan/application-assembly".to_string(),
            "todo/plan/acceptance-harness".to_string(),
        ]
    );
    assert!(
        closure.contains(&"todo/plan/transition-kernel".to_string()),
        "closure must include the seed"
    );
    assert_eq!(closure.len(), 6);
    assert!(!closure.contains(&"todo/plan/design-contracts".to_string()));
    assert!(!closure.contains(&"todo/plan/interface-contracts".to_string()));
}

#[test]
fn oracle_05d_transitive_closure_service_handlers() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    let closure = transitive_dependents(&p, "todo/plan/service-handlers");
    assert_eq!(
        closure,
        vec![
            "todo/plan/service-handlers".to_string(),
            "todo/plan/acceptance-harness".to_string(),
            "todo/plan/application-assembly".to_string(),
        ]
    );
    assert!(
        closure.contains(&"todo/plan/service-handlers".to_string()),
        "closure must include the seed"
    );
    assert_eq!(closure.len(), 3);
}

#[test]
fn oracle_05e_transitive_closure_acceptance_harness() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    let closure = transitive_dependents(&p, "todo/plan/acceptance-harness");
    assert_eq!(
        closure,
        vec![
            "todo/plan/acceptance-harness".to_string(),
            "todo/plan/application-assembly".to_string(),
        ]
    );
    assert!(
        closure.contains(&"todo/plan/acceptance-harness".to_string()),
        "closure must include the seed"
    );
    assert_eq!(closure.len(), 2);
}

#[test]
fn oracle_05f_transitive_closure_leaf_node_is_just_itself() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    let closure = transitive_dependents(&p, "todo/plan/application-assembly");
    assert_eq!(
        closure,
        vec!["todo/plan/application-assembly".to_string()],
        "nothing depends on application-assembly; its closure is only itself"
    );
}

// DERIVATION: invalidated_nodes(["service-handlers", "transition-kernel"])
// unions transitive_dependents of each seed, first-seen order, deduped:
//   from service-handlers: [service-handlers, acceptance-harness,
//                            application-assembly]  (all new)
//   from transition-kernel: [transition-kernel, authorization-policy,
//                            persistence, service-handlers(dup),
//                            application-assembly(dup), acceptance-
//                            harness(dup)] -> only transition-kernel,
//                            authorization-policy, persistence are new
// union = [service-handlers, acceptance-harness, application-assembly,
//          transition-kernel, authorization-policy, persistence]  (6)
#[test]
fn oracle_05g_invalidated_nodes_unions_first_seen_order_deduped() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    let changed = vec![
        "todo/plan/service-handlers".to_string(),
        "todo/plan/transition-kernel".to_string(),
    ];
    let union = invalidated_nodes(&p, &changed);
    assert_eq!(
        union,
        vec![
            "todo/plan/service-handlers".to_string(),
            "todo/plan/acceptance-harness".to_string(),
            "todo/plan/application-assembly".to_string(),
            "todo/plan/transition-kernel".to_string(),
            "todo/plan/authorization-policy".to_string(),
            "todo/plan/persistence".to_string(),
        ]
    );
}

#[test]
fn oracle_05h_invalidated_nodes_empty_changed_is_empty() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    assert!(invalidated_nodes(&p, &[]).is_empty());
}

// =======================================================================
// 6. diff_plans: identical plans (all unchanged, empty closure) and a
//    plan from a modified spec (sharing_limit 256 -> 100).
// =======================================================================

#[test]
fn oracle_06a_diff_plans_identical_all_unchanged_empty_closure() {
    let ir_a = load_todo_ir();
    let p_a = plan(&ir_a);
    let ir_b = load_todo_ir();
    let p_b = plan(&ir_b);

    let diff = diff_plans(&p_a, &p_b);
    let list_of = |k: &str| -> Vec<Sexpr> {
        field(&diff, k)
            .and_then(|v| v.as_list())
            .unwrap_or_else(|| panic!("diff must carry `{}`", k))
            .to_vec()
    };
    assert!(list_of("added").is_empty());
    assert!(list_of("removed").is_empty());
    assert!(list_of("modified").is_empty());
    assert_eq!(list_of("unchanged").len(), 8);
    assert!(list_of("affected-closure").is_empty());
}

// DERIVATION (see oracle_02d above): the sharing_limit profile-argument
// edit changes ONLY the import node's own field content, never any id and
// never any PlanNode's `inputs`/`depends-on`/`target`/`model`/`may-write`/
// `capabilities`/`obligations`/`prohibitions` (all of which reference
// other nodes by ID STRING only) -- so plan_node_changed is false for
// EVERY one of the 8 nodes, even though the underlying IR (and hence
// Plan::ir_fingerprint) did change. diff_plans therefore reports every
// node unchanged and an empty affected-closure: a genuine, if easy-to-
// miss, consequence of the fixed table's id-only `inputs` design.
#[test]
fn oracle_06b_diff_plans_modified_spec_sharing_limit_all_unchanged() {
    let ir1 = load_todo_ir();
    let p1 = plan(&ir1);
    let ir2 = modified_sharing_limit_ir();
    let p2 = plan(&ir2);
    assert_ne!(
        ir1.fingerprint, ir2.fingerprint,
        "the IR itself must actually change (the import node's :arguments differ)"
    );

    let diff = diff_plans(&p1, &p2);
    let list_of = |k: &str| -> Vec<Sexpr> {
        field(&diff, k)
            .and_then(|v| v.as_list())
            .unwrap_or_else(|| panic!("diff must carry `{}`", k))
            .to_vec()
    };
    assert!(
        list_of("added").is_empty(),
        "no plan node is added by a profile-argument-only spec change"
    );
    assert!(
        list_of("removed").is_empty(),
        "no plan node is removed by a profile-argument-only spec change"
    );
    assert!(
        list_of("modified").is_empty(),
        "PlanNode contracts reference `inputs` by id only, never resolved content, so a \
         profile-argument-only change must not move any plan-node fingerprint"
    );
    assert_eq!(list_of("unchanged").len(), 8);
    assert!(
        list_of("affected-closure").is_empty(),
        "no changed node ids -> empty invalidation closure"
    );
}

// =======================================================================
// 7. cache_check hit-after-store, miss-before, explain reasons all three.
// =======================================================================

#[test]
fn oracle_07a_cache_check_node_miss_before_hit_after_store() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    let node = node_by_local(&p, "design-contracts");
    let store = CacheStore::new();

    let miss = cache_check_node(&store, &ir, &p, node);
    assert_eq!(
        miss.as_list()
            .and_then(|l| l.first())
            .and_then(|s| s.as_sym()),
        Some("cache-miss")
    );
    assert_eq!(
        field(&miss, "node-id").and_then(|s| s.as_str()),
        Some(node.id.as_str())
    );

    let mut store2 = CacheStore::new();
    let candidate = sample_candidate();
    cache_store_result(
        &mut store2,
        &ir,
        &p,
        node,
        candidate.clone(),
        nil(),
        Sexpr::sym("epoch-1"),
    );
    let hit = cache_check_node(&store2, &ir, &p, node);
    assert_eq!(
        hit.as_list()
            .and_then(|l| l.first())
            .and_then(|s| s.as_sym()),
        Some("cache-hit")
    );
    assert_eq!(field(&hit, "candidate"), Some(&candidate));
    assert_eq!(
        field(&hit, "key").and_then(|s| s.as_str()),
        Some(cache_key(&ir, &p, node).as_str())
    );
}

#[test]
fn oracle_07b_cache_check_plan_one_hit_rest_miss() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    let node = node_by_local(&p, "design-contracts");
    let mut store = CacheStore::new();
    cache_store_result(
        &mut store,
        &ir,
        &p,
        node,
        sample_candidate(),
        nil(),
        Sexpr::sym("epoch-1"),
    );

    let results = cache_check_plan(&store, &ir, &p);
    assert_eq!(results.len(), p.nodes.len());
    let hits = results
        .iter()
        .filter(|r| {
            r.as_list().and_then(|l| l.first()).and_then(|s| s.as_sym()) == Some("cache-hit")
        })
        .count();
    assert_eq!(hits, 1, "exactly the stored node should hit");
    let misses = results
        .iter()
        .filter(|r| {
            r.as_list().and_then(|l| l.first()).and_then(|s| s.as_sym()) == Some("cache-miss")
        })
        .count();
    assert_eq!(misses, p.nodes.len() - 1);
}

#[test]
fn oracle_07c_cache_explain_node_no_cache_entry() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    let node = node_by_local(&p, "design-contracts");
    let store = CacheStore::new();

    let explanation = cache_explain_node(&store, &ir, &p, node);
    assert_eq!(
        explanation
            .as_list()
            .and_then(|l| l.first())
            .and_then(|s| s.as_sym()),
        Some("explanation")
    );
    assert_eq!(
        field(&explanation, "node-id").and_then(|s| s.as_str()),
        Some(node.id.as_str())
    );
    assert_eq!(
        field(&explanation, "status").and_then(|s| s.as_sym()),
        Some("miss")
    );
    assert_eq!(
        field(&explanation, "reason").and_then(|s| s.as_sym()),
        Some("no-cache-entry")
    );
}

#[test]
fn oracle_07d_cache_explain_node_valid_entry() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    let node = node_by_local(&p, "design-contracts");
    let mut store = CacheStore::new();
    cache_store_result(
        &mut store,
        &ir,
        &p,
        node,
        sample_candidate(),
        nil(),
        Sexpr::sym("epoch-1"),
    );

    let explanation = cache_explain_node(&store, &ir, &p, node);
    assert_eq!(
        field(&explanation, "status").and_then(|s| s.as_sym()),
        Some("hit")
    );
    assert_eq!(
        field(&explanation, "reason").and_then(|s| s.as_sym()),
        Some("valid-entry")
    );
    assert_eq!(
        field(&explanation, "key").and_then(|s| s.as_str()),
        Some(cache_key(&ir, &p, node).as_str())
    );
}

// See file header ambiguity 2: key-mismatch is only reachable by storing
// an entry for a node id, then asking `explain` about that SAME node id
// after its current key has moved -- the stale entry is found by
// node-id, not by the (now different) current key.
#[test]
fn oracle_07e_cache_explain_node_key_mismatch_stale_entry_for_same_node_id() {
    let ir = load_todo_ir();
    let p = plan(&ir);
    let node = node_by_local(&p, "design-contracts");
    let mut store = CacheStore::new();
    let old_key = cache_key(&ir, &p, node);
    cache_store_result(
        &mut store,
        &ir,
        &p,
        node,
        sample_candidate(),
        nil(),
        Sexpr::sym("epoch-1"),
    );

    let mut tampered = node.clone();
    tampered.recipe = "tampered-recipe-v1".to_string();
    let new_key = cache_key(&ir, &p, &tampered);
    assert_ne!(old_key, new_key, "the tamper must actually move the key");

    let explanation = cache_explain_node(&store, &ir, &p, &tampered);
    assert_eq!(
        field(&explanation, "status").and_then(|s| s.as_sym()),
        Some("miss")
    );
    assert_eq!(
        field(&explanation, "reason").and_then(|s| s.as_sym()),
        Some("key-mismatch")
    );
    assert_eq!(
        field(&explanation, "key").and_then(|s| s.as_str()),
        Some(new_key.as_str())
    );
    assert_eq!(
        field(&explanation, "stored-key").and_then(|s| s.as_str()),
        Some(old_key.as_str())
    );
}

// =======================================================================
// 8. Bundle fingerprint recomputes over the fingerprint-free form (same
//    discipline as Ir/Plan/PlanNode/PromptPackage, all already read).
// =======================================================================

#[test]
fn oracle_08_bundle_fingerprint_recomputes_over_fingerprint_free_form() {
    let ir = load_todo_ir();
    let bundle = compile_verification(&ir);
    let fp = field(&bundle, "fingerprint")
        .and_then(|s| s.as_str())
        .expect("bundle must carry a fingerprint")
        .to_string();

    let outer = bundle.as_list().expect("bundle must be a list").to_vec();
    let mut inner = outer[1]
        .as_list()
        .expect("bundle nests one field list")
        .to_vec();
    let last = inner.pop().expect("bundle field list must be non-empty");
    assert_eq!(
        last.as_list()
            .and_then(|l| l.first())
            .and_then(|s| s.as_sym()),
        Some("fingerprint"),
        "fingerprint must be the LAST field, same as Ir/Plan/PlanNode/PromptPackage"
    );
    let stripped = Sexpr::list(vec![outer[0].clone(), Sexpr::list(inner)]);
    let expected = fingerprint::fingerprint(&stripped);
    assert_eq!(fp, expected);
}

#[test]
fn oracle_08b_bundle_fingerprint_deterministic_across_two_runs() {
    let ir1 = load_todo_ir();
    let ir2 = load_todo_ir();
    let b1 = compile_verification(&ir1);
    let b2 = compile_verification(&ir2);
    assert_eq!(
        field(&b1, "fingerprint").and_then(|s| s.as_str()),
        field(&b2, "fingerprint").and_then(|s| s.as_str())
    );
}

// =======================================================================
// 9. bundle_summary reads the freshly compiled bundle's summary,
//    including indeterminate (see evaluator3_oracle_test.rs's oracle_03c
//    for the full total/passed/failed/skipped/indeterminate derivation --
//    1/2/4/2 of 9 -- reused here verbatim; `todo-verify.sexpr` is
//    regenerated only at Stage 3 with these new semantics, so this test
//    exercises the accessor against a live bundle, not the not-yet-
//    regenerated golden file).
// =======================================================================

#[test]
fn oracle_09_bundle_summary_typed_accessor_matches_derived_totals() {
    let ir = load_todo_ir();
    let bundle = compile_verification(&ir);
    let summary = bundle_summary(&bundle).expect("bundle_summary must parse a well-formed bundle");
    assert_eq!(summary.total, 9);
    assert_eq!(summary.passed, 1);
    assert_eq!(summary.failed, 2);
    assert_eq!(summary.skipped, 4);
    assert_eq!(summary.indeterminate, 2);
}

#[test]
fn oracle_09b_bundle_summary_none_on_a_malformed_value() {
    assert_eq!(bundle_summary(&nil()), None);
    assert_eq!(bundle_summary(&Sexpr::sym("not-a-bundle")), None);
}

// =======================================================================
// 10. E601 fires for a duplicated property name, absent from todo.
// =======================================================================

fn property_clause(name: &str) -> Sexpr {
    Sexpr::list(vec![
        Sexpr::sym("property"),
        Sexpr::sym(name),
        Sexpr::sym(":generate"),
        nil(),
        Sexpr::sym(":execute"),
        nil(),
        Sexpr::sym(":must"),
        nil(),
    ])
}

#[test]
fn oracle_10a_e601_fires_once_for_a_duplicated_property_name() {
    let acc = IrNode::new(
        "hand/acceptance/dup".to_string(),
        "acceptance",
        "dup".to_string(),
        vec![(":subject".to_string(), Sexpr::sym("app"))],
        vec![property_clause("dup"), property_clause("dup")],
    );
    let ir = Ir::new(
        "gymnast.ir/0.1".to_string(),
        "hand".to_string(),
        vec![],
        vec![],
        vec![],
        vec![acc],
        vec![],
        vec![],
    );
    let obligations = lower_all_obligations(&ir);
    assert_eq!(
        obligations.len(),
        2,
        "both clauses must lower (duplication is not deduped away)"
    );

    let bundle = compile_verification(&ir);
    let diags = field(&bundle, "diagnostics")
        .and_then(|d| d.as_list())
        .expect("bundle must carry a `diagnostics` field once ids collide");
    let e601: Vec<&Sexpr> = diags
        .iter()
        .filter(|d| d.assoc("code").and_then(|c| c.as_str()) == Some("E601"))
        .collect();
    assert_eq!(
        e601.len(),
        1,
        "a second occurrence of a duplicated id adds exactly one E601, got {:?}",
        diags
    );
    assert_eq!(
        e601[0].assoc("severity").and_then(|s| s.as_sym()),
        Some("error")
    );
    let message = e601[0]
        .assoc("message")
        .and_then(|s| s.as_str())
        .unwrap_or("");
    assert!(
        message.contains("hand/acceptance/dup/property/dup"),
        "E601 message must name the duplicated obligation id, got: {}",
        message
    );
}

#[test]
fn oracle_10b_e601_absent_from_todo_gym() {
    let ir = load_todo_ir();
    let bundle = compile_verification(&ir);
    let diags = field(&bundle, "diagnostics").and_then(|d| d.as_list());
    let e601_count = diags
        .map(|ds| {
            ds.iter()
                .filter(|d| d.assoc("code").and_then(|c| c.as_str()) == Some("E601"))
                .count()
        })
        .unwrap_or(0);
    assert_eq!(
        e601_count, 0,
        "todo.gym's 9 obligation ids are all distinct; E601 must never fire for it"
    );
}

// =======================================================================
// 11. RunResult/Attempt round-trip plus strict-rejection cases.
// =======================================================================

fn sample_attempt() -> Attempt {
    Attempt {
        number: 2,
        prompt_fingerprint: "fnv1a64:111".to_string(),
        response_length: 42,
        response_fingerprint: "fnv1a64:222".to_string(),
        diagnostics: vec![diag_sexpr(
            "error",
            "E514",
            (0, 0),
            "bad output".to_string(),
        )],
        status: AttemptStatus::Rejected,
    }
}

fn sample_run_result_succeeded() -> RunResult {
    let accepted = Attempt {
        number: 1,
        prompt_fingerprint: "fnv1a64:1".to_string(),
        response_length: 10,
        response_fingerprint: "fnv1a64:2".to_string(),
        diagnostics: vec![],
        status: AttemptStatus::Accepted,
    };
    RunResult {
        node_id: "m/plan/transition-kernel".to_string(),
        node_fingerprint: "fnv1a64:333".to_string(),
        status: RunStatus::Succeeded,
        attempts: vec![sample_attempt(), accepted],
        candidate: Some(sample_candidate()),
    }
}

fn sample_run_result_exhausted() -> RunResult {
    RunResult {
        node_id: "m/plan/transition-kernel".to_string(),
        node_fingerprint: "fnv1a64:444".to_string(),
        status: RunStatus::Exhausted,
        attempts: vec![sample_attempt()],
        candidate: None,
    }
}

/// Injects a bogus, unrecognized `(bogus-field 1)` pair into the field
/// list of a `run-result`-or-`attempt`-shaped Sexpr (`(tag ((k v) ...))`).
fn inject_bogus_field(sexpr: &Sexpr) -> Sexpr {
    match sexpr {
        Sexpr::List(outer) if outer.len() == 2 => {
            let mut outer = outer.clone();
            if let Sexpr::List(inner) = &mut outer[1] {
                inner.push(Sexpr::pair("bogus-field", Sexpr::Int(1)));
            }
            Sexpr::List(outer)
        }
        other => other.clone(),
    }
}

/// Removes the `(key ...)` pair from a `(tag ((k v) ...))`-shaped Sexpr.
fn remove_field(sexpr: &Sexpr, key: &str) -> Sexpr {
    match sexpr {
        Sexpr::List(outer) if outer.len() == 2 => {
            let mut outer = outer.clone();
            if let Sexpr::List(inner) = &mut outer[1] {
                inner.retain(|pair| {
                    pair.as_list()
                        .and_then(|p| p.first())
                        .and_then(|s| s.as_sym())
                        != Some(key)
                });
            }
            Sexpr::List(outer)
        }
        other => other.clone(),
    }
}

#[test]
fn oracle_11a_attempt_round_trip() {
    let attempt = sample_attempt();
    let s = attempt.to_sexpr();
    assert_eq!(
        Attempt::from_sexpr(&s),
        Some(attempt),
        "Attempt::from_sexpr must invert Attempt::to_sexpr"
    );
}

#[test]
fn oracle_11b_attempt_strict_rejects_unknown_field() {
    let s = sample_attempt().to_sexpr();
    assert_eq!(Attempt::from_sexpr(&inject_bogus_field(&s)), None);
}

#[test]
fn oracle_11c_attempt_strict_rejects_missing_required_field() {
    let s = sample_attempt().to_sexpr();
    assert_eq!(Attempt::from_sexpr(&remove_field(&s, "status")), None);
}

#[test]
fn oracle_11d_run_result_round_trip_succeeded_with_candidate() {
    let rr = sample_run_result_succeeded();
    let s = rr.to_sexpr();
    assert_eq!(
        RunResult::from_sexpr(&s),
        Some(rr),
        "RunResult::from_sexpr must invert RunResult::to_sexpr (Succeeded, with candidate)"
    );
}

#[test]
fn oracle_11e_run_result_round_trip_exhausted_no_candidate() {
    let rr = sample_run_result_exhausted();
    let s = rr.to_sexpr();
    assert_eq!(
        RunResult::from_sexpr(&s),
        Some(rr),
        "RunResult::from_sexpr must invert RunResult::to_sexpr (Exhausted, no candidate)"
    );
}

#[test]
fn oracle_11f_run_result_node_fingerprint_serialized_immediately_after_node_id() {
    let s = sample_run_result_succeeded().to_sexpr();
    let printed = s.print();
    let pos_id = printed.find("node-id").expect("must print node-id");
    let pos_fp = printed
        .find("node-fingerprint")
        .expect("must print node-fingerprint");
    let pos_status = printed.find("(status").expect("must print status");
    assert!(
        pos_id < pos_fp && pos_fp < pos_status,
        "node-fingerprint must be serialized after node-id and before status, got: {}",
        printed
    );
}

#[test]
fn oracle_11g_run_result_strict_rejects_unknown_field() {
    let s = sample_run_result_succeeded().to_sexpr();
    assert_eq!(RunResult::from_sexpr(&inject_bogus_field(&s)), None);
}

#[test]
fn oracle_11h_run_result_strict_rejects_missing_node_fingerprint() {
    let s = sample_run_result_succeeded().to_sexpr();
    assert_eq!(
        RunResult::from_sexpr(&remove_field(&s, "node-fingerprint")),
        None,
        "readers require node_fingerprint; a run-result missing it must be rejected, not \
         defaulted"
    );
}

#[test]
fn oracle_11i_run_result_strict_rejects_missing_node_fingerprint_exhausted_variant() {
    let s = sample_run_result_exhausted().to_sexpr();
    assert_eq!(
        RunResult::from_sexpr(&remove_field(&s, "node-fingerprint")),
        None
    );
}

#[test]
fn oracle_11j_run_result_strict_rejects_wrong_type_status() {
    let s = sample_run_result_succeeded().to_sexpr();
    let tampered = match &s {
        Sexpr::List(outer) if outer.len() == 2 => {
            let mut outer = outer.clone();
            if let Sexpr::List(inner) = &mut outer[1] {
                for pair in inner.iter_mut() {
                    if let Sexpr::List(kv) = pair {
                        if kv.first().and_then(|s| s.as_sym()) == Some("status") {
                            kv[1] = Sexpr::Int(1); // wrong type: not a Sym
                        }
                    }
                }
            }
            Sexpr::List(outer)
        }
        other => other.clone(),
    };
    assert_eq!(RunResult::from_sexpr(&tampered), None);
}

#[test]
fn oracle_11k_run_result_from_sexpr_none_on_a_wholly_malformed_value() {
    assert_eq!(RunResult::from_sexpr(&nil()), None);
    assert_eq!(RunResult::from_sexpr(&Sexpr::sym("not-a-run-result")), None);
    assert_eq!(Attempt::from_sexpr(&nil()), None);
    assert_eq!(Attempt::from_sexpr(&Sexpr::sym("not-an-attempt")), None);
}
