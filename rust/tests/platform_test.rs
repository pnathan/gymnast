// Oracle tests for the platform-kit registry (rust/src/platform.rs), written
// before the implementation per this project's committed-oracle discipline.
//
// Ported from src/platform.lisp (behavioral intent only, not byte output).
// The reference kit is `gymnast-ruby-platform-v1` version "1.0" for target
// "ruby", carrying all ten capabilities from the Lamedh source verbatim
// (guarantees and failure-modes symbols copied exactly, with Rust surface
// names are lookup keys and stay hyphenated to match the planner's
// capability vocabulary: `id-source`, `durable-store`).

use gymnast_rs::plan::PlanNode;
use gymnast_rs::platform;
use gymnast_rs::sexpr::Sexpr;

fn empty_node(id: &str, capabilities: Vec<String>) -> PlanNode {
    PlanNode::new(
        id.to_string(),
        "class",
        "recipe",
        vec![],
        vec![],
        Sexpr::sym("nil"),
        Sexpr::sym("nil"),
        vec![],
        capabilities,
        vec![],
        vec![],
    )
}

#[test]
fn oracle_01_capabilities_for_target_ruby_resolves() {
    let caps = platform::capabilities_for_target("ruby");
    assert!(caps.is_some());
    assert_eq!(caps.unwrap().len(), 10);
}

#[test]
fn oracle_02_capabilities_for_unknown_target_is_none() {
    assert!(platform::capabilities_for_target("cobol").is_none());
    assert!(platform::capabilities_for_target("").is_none());
}

#[test]
fn oracle_03_lookup_kit_resolves() {
    let kit = platform::lookup_kit("gymnast-ruby-platform-v1", "1.0");
    assert!(kit.is_some());
    let kit = kit.unwrap();
    assert_eq!(kit.name, "gymnast-ruby-platform-v1");
    assert_eq!(kit.version, "1.0");
    assert_eq!(kit.target, "ruby");
    assert_eq!(kit.capabilities.len(), 10);
}

#[test]
fn oracle_04_lookup_kit_unknown_name_or_version_is_none() {
    assert!(platform::lookup_kit("unknown-kit", "1.0").is_none());
    assert!(platform::lookup_kit("gymnast-ruby-platform-v1", "2.0").is_none());
}

fn find_cap<'a>(caps: &'a [platform::Capability], name: &str) -> &'a platform::Capability {
    caps.iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("capability {name} not found"))
}

#[test]
fn oracle_05_all_ten_capabilities_present() {
    let caps = platform::capabilities_for_target("ruby").unwrap();
    let names: Vec<&str> = caps.iter().map(|c| c.name).collect();
    let expected = [
        "identity",
        "persistence",
        "repository",
        "transactions",
        "clock",
        "id-source",
        "http",
        "telemetry",
        "lifecycle",
        "durable-store",
    ];
    for e in expected {
        assert!(names.contains(&e), "missing capability {e}");
    }
    assert_eq!(names.len(), 10);
}

#[test]
fn oracle_06_identity_capability_exact() {
    let caps = platform::capabilities_for_target("ruby").unwrap();
    let cap = find_cap(caps, "identity");
    assert_eq!(cap.version, "1.0");
    assert_eq!(
        cap.guarantees,
        [
            "token_validation",
            "session_binding",
            "principal_extraction"
        ]
    );
    assert_eq!(
        cap.failure_modes,
        ["unauthenticated", "token_expired", "provider_unavailable"]
    );
}

#[test]
fn oracle_07_persistence_capability_exact() {
    let caps = platform::capabilities_for_target("ruby").unwrap();
    let cap = find_cap(caps, "persistence");
    assert_eq!(cap.version, "1.0");
    assert_eq!(cap.guarantees, ["durable_commit", "read_after_write"]);
    assert_eq!(
        cap.failure_modes,
        ["connection_lost", "constraint_violation", "not_found"]
    );
}

#[test]
fn oracle_08_repository_capability_exact() {
    let caps = platform::capabilities_for_target("ruby").unwrap();
    let cap = find_cap(caps, "repository");
    assert_eq!(cap.version, "1.0");
    assert_eq!(
        cap.guarantees,
        ["typed_queries", "aggregate_loading", "optimistic_locking"]
    );
    assert_eq!(
        cap.failure_modes,
        ["not_found", "version_conflict", "connection_lost"]
    );
}

#[test]
fn oracle_09_transactions_capability_exact() {
    let caps = platform::capabilities_for_target("ruby").unwrap();
    let cap = find_cap(caps, "transactions");
    assert_eq!(cap.version, "1.0");
    assert_eq!(
        cap.guarantees,
        [
            "atomic_boundaries",
            "rollback_on_error",
            "serializable_per_scope"
        ]
    );
    assert_eq!(cap.failure_modes, ["deadlock", "timeout", "rollback"]);
}

#[test]
fn oracle_10_clock_capability_exact() {
    let caps = platform::capabilities_for_target("ruby").unwrap();
    let cap = find_cap(caps, "clock");
    assert_eq!(cap.version, "1.0");
    assert_eq!(
        cap.guarantees,
        ["monotonic", "utc_wall_time", "virtual_in_tests"]
    );
    assert_eq!(cap.failure_modes, ["drift_beyond_tolerance"]);
}

#[test]
fn oracle_11_id_source_capability_exact() {
    let caps = platform::capabilities_for_target("ruby").unwrap();
    let cap = find_cap(caps, "id-source");
    assert_eq!(cap.version, "1.0");
    assert_eq!(
        cap.guarantees,
        ["globally_unique", "collision_resistant", "sortable"]
    );
    assert_eq!(cap.failure_modes, ["entropy_exhausted"]);
}

#[test]
fn oracle_12_http_capability_exact() {
    let caps = platform::capabilities_for_target("ruby").unwrap();
    let cap = find_cap(caps, "http");
    assert_eq!(cap.version, "1.0");
    assert_eq!(
        cap.guarantees,
        ["request_routing", "content_negotiation", "error_mapping"]
    );
    assert_eq!(
        cap.failure_modes,
        ["bad_request", "method_not_allowed", "internal_error"]
    );
}

#[test]
fn oracle_13_telemetry_capability_exact() {
    let caps = platform::capabilities_for_target("ruby").unwrap();
    let cap = find_cap(caps, "telemetry");
    assert_eq!(cap.version, "1.0");
    assert_eq!(
        cap.guarantees,
        ["structured_logging", "request_tracing", "metric_emission"]
    );
    assert_eq!(cap.failure_modes, ["buffer_overflow"]);
}

#[test]
fn oracle_14_lifecycle_capability_exact() {
    let caps = platform::capabilities_for_target("ruby").unwrap();
    let cap = find_cap(caps, "lifecycle");
    assert_eq!(cap.version, "1.0");
    assert_eq!(
        cap.guarantees,
        ["graceful_shutdown", "health_check", "dependency_ordering"]
    );
    assert_eq!(cap.failure_modes, ["startup_failure", "shutdown_timeout"]);
}

#[test]
fn oracle_15_durable_store_capability_exact() {
    let caps = platform::capabilities_for_target("ruby").unwrap();
    let cap = find_cap(caps, "durable-store");
    assert_eq!(cap.version, "1.0");
    assert_eq!(
        cap.guarantees,
        ["durable_commit", "read_after_write", "schema_migration"]
    );
    assert_eq!(
        cap.failure_modes,
        ["connection_lost", "constraint_violation"]
    );
}

#[test]
fn oracle_16_node_with_unknown_capability_produces_one_error_diagnostic() {
    let kit = platform::lookup_kit("gymnast-ruby-platform-v1", "1.0").unwrap();
    let node = empty_node("m/plan/thing", vec!["quantum_teleport".to_string()]);
    let diags = platform::validate_node_capabilities(&node, kit);
    assert_eq!(diags.len(), 1);
}

#[test]
fn oracle_17_node_with_known_capabilities_produces_no_diagnostics() {
    let kit = platform::lookup_kit("gymnast-ruby-platform-v1", "1.0").unwrap();
    let node = empty_node(
        "m/plan/thing",
        vec!["identity".to_string(), "clock".to_string()],
    );
    let diags = platform::validate_node_capabilities(&node, kit);
    assert!(diags.is_empty());
}

#[test]
fn oracle_18_validate_plan_capabilities_aggregates_across_nodes() {
    let kit = platform::lookup_kit("gymnast-ruby-platform-v1", "1.0").unwrap();
    let nodes = vec![
        empty_node("m/plan/a", vec!["identity".to_string()]),
        empty_node("m/plan/b", vec!["bogus_cap".to_string()]),
        empty_node(
            "m/plan/c",
            vec!["bogus_cap".to_string(), "also_bogus".to_string()],
        ),
    ];
    let diags = platform::validate_plan_capabilities(&nodes, kit);
    assert_eq!(diags.len(), 3);
}

/// Regression guard for the naming mismatch caught in review: the registry's
/// capability names are LOOKUP KEYS, so every capability the planner actually
/// emits must resolve in the kit. Before this guard the registry spelled
/// `id_source`/`durable_store` while `crate::plan` emits `id-source`/
/// `durable-store`, so two of ten capabilities silently fell back to the
/// bare-name projection in every generated prompt. The earlier oracles all
/// used single-word names (`identity`, `clock`) and could not see it.
#[test]
fn oracle_19_every_planner_capability_resolves_in_the_ruby_kit() {
    let src = std::fs::read_to_string("../examples/todo.gym").expect("todo.gym");
    let (ast, _) = gymnast_rs::parser::parse(&src);
    let file = ast.expect("todo.gym parses");
    let ir = gymnast_rs::elaborate::elaborate(&file);
    let plan = gymnast_rs::plan::plan(&ir);

    let caps = platform::capabilities_for_target("ruby").expect("ruby kit");
    let known: Vec<&str> = caps.iter().map(|c| c.name).collect();

    let mut unresolved: Vec<String> = vec![];
    for node in &plan.nodes {
        for cap in &node.capabilities {
            if !known.contains(&cap.as_str()) {
                unresolved.push(format!("{} (node {})", cap, node.id));
            }
        }
    }
    assert!(
        unresolved.is_empty(),
        "planner emits capabilities the ruby kit does not define: {unresolved:?}"
    );
}
