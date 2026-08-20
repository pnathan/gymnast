//! Content-addressed caching and incremental regeneration
//! (`docs/rust-port-plan-phase7.md`, section E). Ports `src/cache.lisp`'s
//! behavioral intent onto the Rust `Ir`/`Plan`/`PlanNode` contracts:
//! in-memory only (a persistent on-disk cache is a phase-8+ decision),
//! no globals (the Lamedh reference's `$gymnast-cache` special is
//! replaced by an explicit `CacheStore` value every caller threads
//! through), and NO CLOCK: every timestamp is caller-supplied opaque
//! data, never read from the environment, so two calls with the same
//! `(ir, plan, node, candidate, evidence, timestamp)` always produce the
//! same stored entry. No collection here is a `HashMap`/`HashSet` that
//! is ever iterated to produce output order — `HashSet` is used only for
//! O(1) membership tests, and every output list is built by walking a
//! `Vec` (`plan.nodes`, a caller-supplied `changed` slice, or a
//! `CacheStore`'s own insertion-ordered entries) — so two independent
//! runs over the same inputs agree byte-for-byte.
//!
//! Every loop here is a `for`/`while` over an already-bounded `Vec` or
//! `VecDeque` that only ever grows by consuming plan nodes it has not
//! already visited (`transitive_dependents`'s BFS visits each of
//! `plan.nodes.len()` ids at most once); no recursion, no panics on any
//! input this module's own public API can receive.

use crate::fingerprint;
use crate::ir::{resolve_ir_slice, Ir};
use crate::plan::{Plan, PlanNode};
use crate::prompt::dependency_slice;
use crate::sexpr::Sexpr;
use std::collections::{HashSet, VecDeque};

pub const CACHE_SCHEMA: &str = "gymnast.cache/0.1";

/// One stored synthesis result. `timestamp` is caller-supplied, opaque
/// data (a symbol or string) — the cache itself never reads a clock; a
/// caller that wants a real timestamp constructs it before calling
/// `cache_store_result`.
#[derive(Debug, Clone, PartialEq)]
pub struct CacheEntry {
    pub key: String,
    pub node_id: String,
    pub candidate: Sexpr,
    pub evidence: Sexpr,
    pub timestamp: Sexpr,
}

/// The in-memory cache: insertion-ordered entries, first (and only)
/// match wins on `lookup` by key, `store` replaces an existing entry
/// under the same key in place rather than growing the store — mirrors
/// `src/cache.lisp`'s `gymnast-cache-store`, minus the global.
#[derive(Debug, Clone, Default)]
pub struct CacheStore {
    entries: Vec<CacheEntry>,
}

impl CacheStore {
    pub fn new() -> Self {
        CacheStore {
            entries: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Replaces any existing entry whose `key` matches `entry.key`
    /// in place; otherwise appends. Never produces two stored entries
    /// under the same key.
    pub fn store(&mut self, entry: CacheEntry) {
        match self.entries.iter_mut().find(|e| e.key == entry.key) {
            Some(existing) => *existing = entry,
            None => self.entries.push(entry),
        }
    }

    pub fn lookup(&self, key: &str) -> Option<&CacheEntry> {
        self.entries.iter().find(|e| e.key == key)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn keys(&self) -> Vec<&str> {
        self.entries.iter().map(|e| e.key.as_str()).collect()
    }
}

// ---------------------------------------------------------------------
// Cache key construction.
// ---------------------------------------------------------------------

/// `(cache-key-material (node-fingerprint "...") (ir-slice-fingerprint
/// "...") (dependency-fingerprint "...") (recipe r) (model (...))
/// (capabilities (...)))` — FLAT form (plan section E: "flat form,
/// fingerprinted for the key"), unlike the nested `(tag ((k v) ...))`
/// shape `PlanNode`/`PromptPackage`/the verification bundle use for
/// their own fingerprinted contracts. `node-fingerprint`/
/// `ir-slice-fingerprint`/`dependency-fingerprint` are content hashes
/// (`Sexpr::Str`, like `PlanNode.fingerprint` itself); `recipe` is a
/// vocabulary term (`Sexpr::sym`, like `PlanNode.recipe`); `model` is
/// passed through verbatim (already an `Sexpr` on `PlanNode`);
/// `capabilities` is a vocabulary-term list (`Sexpr::sym` each, like
/// `PlanNode.capabilities`).
pub fn cache_key_material(ir: &Ir, plan: &Plan, node: &PlanNode) -> Sexpr {
    // ir-slice fingerprint: over `(ir-slice (<node.to_sexpr()> ...))` of
    // the node's RESOLVED input slice — unresolved inputs (W405) make
    // the slice smaller and are the caller's own concern to surface; the
    // key covers what was actually used, not what was declared.
    let (resolved, _warnings) = resolve_ir_slice(ir, &node.id, &node.inputs);
    let ir_slice_sexpr = Sexpr::list(vec![
        Sexpr::sym("ir-slice"),
        Sexpr::list(resolved.iter().map(|n| n.to_sexpr()).collect()),
    ]);
    let ir_slice_fingerprint = fingerprint::fingerprint(&ir_slice_sexpr);

    // dependency fingerprint: over the dependency slice
    // `((dep-id "fp") ...)`, built by `prompt.rs`'s OWN builder (never a
    // second copy) and fingerprinted directly, with no wrapping tag.
    let dep_pairs: Vec<Sexpr> = dependency_slice(plan, node)
        .into_iter()
        .map(|(id, fp)| Sexpr::list(vec![Sexpr::Str(id), Sexpr::Str(fp)]))
        .collect();
    let dependency_fingerprint = fingerprint::fingerprint(&Sexpr::list(dep_pairs));

    Sexpr::list(vec![
        Sexpr::sym("cache-key-material"),
        Sexpr::pair("node-fingerprint", Sexpr::Str(node.fingerprint.clone())),
        Sexpr::pair("ir-slice-fingerprint", Sexpr::Str(ir_slice_fingerprint)),
        Sexpr::pair("dependency-fingerprint", Sexpr::Str(dependency_fingerprint)),
        Sexpr::pair("recipe", Sexpr::sym(&node.recipe)),
        Sexpr::pair("model", node.model.clone()),
        Sexpr::pair(
            "capabilities",
            Sexpr::list(node.capabilities.iter().map(|c| Sexpr::sym(c)).collect()),
        ),
    ])
}

/// The fingerprint of `cache_key_material(ir, plan, node)` — identical
/// `(ir, plan, node)` content always yields an identical key, across any
/// number of independent pipeline runs (no clock, no hash-iteration
/// dependence anywhere in the material).
pub fn cache_key(ir: &Ir, plan: &Plan, node: &PlanNode) -> String {
    fingerprint::fingerprint(&cache_key_material(ir, plan, node))
}

/// Pure key equality — the ONLY thing that makes a stored entry valid
/// for `(ir, plan, node)`. Never inspects `candidate`/`evidence`/
/// `timestamp`.
pub fn entry_valid(ir: &Ir, plan: &Plan, node: &PlanNode, entry: &CacheEntry) -> bool {
    cache_key(ir, plan, node) == entry.key
}

// ---------------------------------------------------------------------
// Dependency closure.
// ---------------------------------------------------------------------

/// The plan nodes that directly depend on `node_id` (i.e. whose
/// `depends_on` names it), in plan (table) order.
pub fn node_dependents<'a>(plan: &'a Plan, node_id: &str) -> Vec<&'a PlanNode> {
    plan.nodes
        .iter()
        .filter(|n| n.depends_on.iter().any(|d| d == node_id))
        .collect()
}

/// The transitive dependency closure of `node_id`: the seed itself,
/// then every node reachable by repeatedly following `node_dependents`,
/// in BREADTH-FIRST discovery order (seed first, then each frontier's
/// direct dependents walked in plan/table order) — a `HashSet` guards
/// membership only, never iterated for output order, so this is
/// deterministic across runs. Bounded: each of `plan.nodes.len()` ids
/// enters the queue at most once.
pub fn transitive_dependents(plan: &Plan, node_id: &str) -> Vec<String> {
    let mut visited: HashSet<String> = HashSet::new();
    let mut result: Vec<String> = vec![node_id.to_string()];
    visited.insert(node_id.to_string());
    let mut queue: VecDeque<String> = VecDeque::new();
    queue.push_back(node_id.to_string());

    while let Some(current) = queue.pop_front() {
        for dep in node_dependents(plan, &current) {
            if visited.insert(dep.id.clone()) {
                result.push(dep.id.clone());
                queue.push_back(dep.id.clone());
            }
        }
    }
    result
}

/// The union of `transitive_dependents` over every id in `changed`,
/// first-seen order, deduplicated.
pub fn invalidated_nodes(plan: &Plan, changed: &[String]) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut result = Vec::new();
    for seed in changed {
        for id in transitive_dependents(plan, seed) {
            if seen.insert(id.clone()) {
                result.push(id);
            }
        }
    }
    result
}

// ---------------------------------------------------------------------
// Plan diffing.
// ---------------------------------------------------------------------

/// `true` when `node_id` is missing from EITHER plan's node set (added
/// or removed), or present in both with a differing `fingerprint`.
/// `false` when missing from both, or present in both with the same
/// fingerprint.
pub fn plan_node_changed(old: &Plan, new: &Plan, node_id: &str) -> bool {
    match (old.node(node_id), new.node(node_id)) {
        (None, None) => false,
        (None, Some(_)) | (Some(_), None) => true,
        (Some(a), Some(b)) => a.fingerprint != b.fingerprint,
    }
}

/// `(plan-diff (added (...)) (removed (...)) (modified (...))
/// (unchanged (...)) (affected-closure (...)))` — flat form, same as
/// the Lamedh reference's literal `(list 'plan-diff ...)` build. Id
/// lists: `added`/`modified`/`unchanged` in NEW-plan order, `removed`
/// in OLD-plan order. `affected-closure` is
/// `invalidated_nodes(new, changed)` where `changed = added ++ removed
/// ++ modified` (disjoint by construction: `added` and `removed` are
/// each in exactly one plan, `modified` in both) — over the NEW plan,
/// matching the Lamedh reference's `(gymnast-invalidated-nodes
/// new-plan changed)`.
pub fn diff_plans(old: &Plan, new: &Plan) -> Sexpr {
    let old_ids: Vec<String> = old.nodes.iter().map(|n| n.id.clone()).collect();
    let new_ids: Vec<String> = new.nodes.iter().map(|n| n.id.clone()).collect();
    let old_set: HashSet<&str> = old_ids.iter().map(|s| s.as_str()).collect();
    let new_set: HashSet<&str> = new_ids.iter().map(|s| s.as_str()).collect();

    let added: Vec<String> = new_ids
        .iter()
        .filter(|id| !old_set.contains(id.as_str()))
        .cloned()
        .collect();
    let removed: Vec<String> = old_ids
        .iter()
        .filter(|id| !new_set.contains(id.as_str()))
        .cloned()
        .collect();

    let mut modified: Vec<String> = Vec::new();
    let mut unchanged: Vec<String> = Vec::new();
    for id in &new_ids {
        if !old_set.contains(id.as_str()) {
            // Present only in `new` -- already counted in `added`, not
            // a candidate for modified/unchanged.
            continue;
        }
        if plan_node_changed(old, new, id) {
            modified.push(id.clone());
        } else {
            unchanged.push(id.clone());
        }
    }

    let mut changed: Vec<String> = Vec::with_capacity(added.len() + removed.len() + modified.len());
    changed.extend(added.iter().cloned());
    changed.extend(removed.iter().cloned());
    changed.extend(modified.iter().cloned());
    let affected_closure = invalidated_nodes(new, &changed);

    fn str_list(ids: Vec<String>) -> Sexpr {
        Sexpr::list(ids.into_iter().map(Sexpr::Str).collect())
    }

    Sexpr::list(vec![
        Sexpr::sym("plan-diff"),
        Sexpr::pair("added", str_list(added)),
        Sexpr::pair("removed", str_list(removed)),
        Sexpr::pair("modified", str_list(modified)),
        Sexpr::pair("unchanged", str_list(unchanged)),
        Sexpr::pair("affected-closure", str_list(affected_closure)),
    ])
}

// ---------------------------------------------------------------------
// Cache check / explain / store.
// ---------------------------------------------------------------------

/// `(cache-hit (node-id ...) (key ...) (candidate ...))` when `store`
/// holds a valid entry for `(ir, plan, node)`'s current key, else
/// `(cache-miss (node-id ...) (key ...))`.
pub fn cache_check_node(store: &CacheStore, ir: &Ir, plan: &Plan, node: &PlanNode) -> Sexpr {
    let key = cache_key(ir, plan, node);
    match store.lookup(&key) {
        Some(entry) if entry_valid(ir, plan, node, entry) => Sexpr::list(vec![
            Sexpr::sym("cache-hit"),
            Sexpr::pair("node-id", Sexpr::Str(node.id.clone())),
            Sexpr::pair("key", Sexpr::Str(key)),
            Sexpr::pair("candidate", entry.candidate.clone()),
        ]),
        _ => Sexpr::list(vec![
            Sexpr::sym("cache-miss"),
            Sexpr::pair("node-id", Sexpr::Str(node.id.clone())),
            Sexpr::pair("key", Sexpr::Str(key)),
        ]),
    }
}

/// `cache_check_node` mapped over every plan node, in plan (table)
/// order (ambiguity note: the plan's elided `cache_check_plan(...)`
/// signature is read by direct analogy with `cache_check_node`'s fully
/// given one, mirroring `src/cache.lisp`'s `gymnast-cache-check-plan
/// (ir plan)` with the store threaded in like every other section-E
/// function).
pub fn cache_check_plan(store: &CacheStore, ir: &Ir, plan: &Plan) -> Vec<Sexpr> {
    plan.nodes
        .iter()
        .map(|node| cache_check_node(store, ir, plan, node))
        .collect()
}

/// `(explanation (node-id ...) (status hit|miss) (reason
/// valid-entry|no-cache-entry|key-mismatch) (key ...)[ (stored-key
/// ...)])`.
///
/// Ambiguity note (`CacheStore` exposes lookup ONLY by key, and
/// `store()`'s only source for that key is `entry.key` itself, so an
/// entry found via `store.lookup(cache_key(...))` necessarily has
/// `entry.key == cache_key(...)` BY CONSTRUCTION — `entry_valid` is
/// trivially true whenever an entry is found this way): a STALE entry
/// for this same node id, stored under a now-superseded key, is
/// therefore only discoverable by scanning for a stored entry whose
/// `node_id` matches — the first such entry in the store's insertion
/// order, deterministic. `key-mismatch` names that reading: it requires
/// an entry that once matched this node id under a now-superseded key,
/// not merely "any key exists that isn't this one."
pub fn cache_explain_node(store: &CacheStore, ir: &Ir, plan: &Plan, node: &PlanNode) -> Sexpr {
    let key = cache_key(ir, plan, node);
    if store.lookup(&key).is_some() {
        return Sexpr::list(vec![
            Sexpr::sym("explanation"),
            Sexpr::pair("node-id", Sexpr::Str(node.id.clone())),
            Sexpr::pair("status", Sexpr::sym("hit")),
            Sexpr::pair("reason", Sexpr::sym("valid-entry")),
            Sexpr::pair("key", Sexpr::Str(key)),
        ]);
    }
    match store.entries.iter().find(|e| e.node_id == node.id) {
        Some(stale) => Sexpr::list(vec![
            Sexpr::sym("explanation"),
            Sexpr::pair("node-id", Sexpr::Str(node.id.clone())),
            Sexpr::pair("status", Sexpr::sym("miss")),
            Sexpr::pair("reason", Sexpr::sym("key-mismatch")),
            Sexpr::pair("key", Sexpr::Str(key)),
            Sexpr::pair("stored-key", Sexpr::Str(stale.key.clone())),
        ]),
        None => Sexpr::list(vec![
            Sexpr::sym("explanation"),
            Sexpr::pair("node-id", Sexpr::Str(node.id.clone())),
            Sexpr::pair("status", Sexpr::sym("miss")),
            Sexpr::pair("reason", Sexpr::sym("no-cache-entry")),
            Sexpr::pair("key", Sexpr::Str(key)),
        ]),
    }
}

/// Computes `(ir, plan, node)`'s current key and stores `candidate`/
/// `evidence`/`timestamp` under it — `timestamp` is caller-supplied
/// opaque data, never a clock read.
pub fn cache_store_result(
    store: &mut CacheStore,
    ir: &Ir,
    plan: &Plan,
    node: &PlanNode,
    candidate: Sexpr,
    evidence: Sexpr,
    timestamp: Sexpr,
) {
    let key = cache_key(ir, plan, node);
    store.store(CacheEntry {
        key,
        node_id: node.id.clone(),
        candidate,
        evidence,
        timestamp,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elaborate::elaborate;
    use crate::parser;
    use crate::plan::plan;

    fn todo_setup() -> (Ir, Plan) {
        let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/todo.gym"));
        let (ast, _) = parser::parse(src);
        let ir = elaborate(&ast.unwrap());
        let p = plan(&ir);
        (ir, p)
    }

    #[test]
    fn test_cache_key_stable_over_two_calls() {
        let (ir, p) = todo_setup();
        for node in &p.nodes {
            assert_eq!(cache_key(&ir, &p, node), cache_key(&ir, &p, node));
        }
    }

    #[test]
    fn test_store_new_is_empty_and_len_tracks_inserts() {
        let mut store = CacheStore::new();
        assert!(store.is_empty());
        store.store(CacheEntry {
            key: "k".to_string(),
            node_id: "n".to_string(),
            candidate: Sexpr::list(vec![]),
            evidence: Sexpr::list(vec![]),
            timestamp: Sexpr::sym("t0"),
        });
        assert_eq!(store.len(), 1);
        assert!(!store.is_empty());
    }

    #[test]
    fn test_node_dependents_empty_for_unknown_id_never_panics() {
        let (_ir, p) = todo_setup();
        assert!(node_dependents(&p, "todo/plan/does-not-exist").is_empty());
    }

    #[test]
    fn test_transitive_dependents_unknown_seed_is_just_itself() {
        let (_ir, p) = todo_setup();
        assert_eq!(
            transitive_dependents(&p, "todo/plan/does-not-exist"),
            vec!["todo/plan/does-not-exist".to_string()]
        );
    }
}
