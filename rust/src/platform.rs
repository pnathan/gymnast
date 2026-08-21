//! Platform kit registry.
//!
//! Ported from `src/platform.lisp` (behavioral intent only, not byte
//! output). A platform kit is a versioned collection of capability
//! adapters that form the trusted runtime boundary for synthesized
//! applications. Generated code must cross capability interfaces for all
//! external effects; direct stdlib access is a synthesis prohibition.
//!
//! Follows the static-registry pattern used by `crate::profile`.
//!
//! Diagnostic error code: `E404` (`undeclared-capability`), the next free
//! code in the plan-stage `E4xx` range after `E401`/`E402`/`E403` used in
//! `crate::plan`.
//!
//! Surface spelling: capability NAMES are the registry's lookup keys, so
//! they must match the capability vocabulary the planner emits verbatim
//! (`crate::plan` hardcodes `id-source` and `durable-store` into the node
//! template, and `tests/fixtures/todo-plan.sexpr` pins them). Those names
//! stay hyphenated. Guarantee and failure-mode symbols are projected text
//! rather than lookup keys, so they follow the crate's underscored
//! convention (`utc_wall_time`, `token_validation`).

use crate::diag::diag_sexpr;
use crate::plan::PlanNode;
use crate::sexpr::Sexpr;

/// A single capability adapter: a named, versioned unit of the platform
/// kit's trusted boundary, with characterized guarantees and declared
/// failure modes that generated code must handle.
#[derive(Debug, Clone, PartialEq)]
pub struct Capability {
    /// Capability name (e.g. "identity", "persistence").
    pub name: &'static str,
    /// Capability version (e.g. "1.0").
    pub version: &'static str,
    /// Guarantees this capability provides.
    pub guarantees: &'static [&'static str],
    /// Failure modes generated code must handle.
    pub failure_modes: &'static [&'static str],
}

/// A versioned platform kit: a named collection of capabilities targeting
/// one platform language.
#[derive(Debug, Clone, PartialEq)]
pub struct PlatformKit {
    /// Kit name (e.g. "gymnast-ruby-platform-v1").
    pub name: &'static str,
    /// Kit version (e.g. "1.0").
    pub version: &'static str,
    /// Target platform language (e.g. "ruby").
    pub target: &'static str,
    /// Capabilities provided by this kit.
    pub capabilities: &'static [Capability],
}

/// Reference platform kit: `gymnast-ruby-platform-v1` version "1.0" for
/// target "ruby", carrying all ten capabilities from the Lamedh source
/// (`src/platform.lisp`) verbatim.
static RUBY_PLATFORM_CAPABILITIES: &[Capability] = &[
    Capability {
        name: "identity",
        version: "1.0",
        guarantees: &[
            "token_validation",
            "session_binding",
            "principal_extraction",
        ],
        failure_modes: &["unauthenticated", "token_expired", "provider_unavailable"],
    },
    Capability {
        name: "persistence",
        version: "1.0",
        guarantees: &["durable_commit", "read_after_write"],
        failure_modes: &["connection_lost", "constraint_violation", "not_found"],
    },
    Capability {
        name: "repository",
        version: "1.0",
        guarantees: &["typed_queries", "aggregate_loading", "optimistic_locking"],
        failure_modes: &["not_found", "version_conflict", "connection_lost"],
    },
    Capability {
        name: "transactions",
        version: "1.0",
        guarantees: &[
            "atomic_boundaries",
            "rollback_on_error",
            "serializable_per_scope",
        ],
        failure_modes: &["deadlock", "timeout", "rollback"],
    },
    Capability {
        name: "clock",
        version: "1.0",
        guarantees: &["monotonic", "utc_wall_time", "virtual_in_tests"],
        failure_modes: &["drift_beyond_tolerance"],
    },
    Capability {
        name: "id-source",
        version: "1.0",
        guarantees: &["globally_unique", "collision_resistant", "sortable"],
        failure_modes: &["entropy_exhausted"],
    },
    Capability {
        name: "http",
        version: "1.0",
        guarantees: &["request_routing", "content_negotiation", "error_mapping"],
        failure_modes: &["bad_request", "method_not_allowed", "internal_error"],
    },
    Capability {
        name: "telemetry",
        version: "1.0",
        guarantees: &["structured_logging", "request_tracing", "metric_emission"],
        failure_modes: &["buffer_overflow"],
    },
    Capability {
        name: "lifecycle",
        version: "1.0",
        guarantees: &["graceful_shutdown", "health_check", "dependency_ordering"],
        failure_modes: &["startup_failure", "shutdown_timeout"],
    },
    Capability {
        name: "durable-store",
        version: "1.0",
        guarantees: &["durable_commit", "read_after_write", "schema_migration"],
        failure_modes: &["connection_lost", "constraint_violation"],
    },
];

static RUBY_PLATFORM_KIT: PlatformKit = PlatformKit {
    name: "gymnast-ruby-platform-v1",
    version: "1.0",
    target: "ruby",
    capabilities: RUBY_PLATFORM_CAPABILITIES,
};

/// All registered platform kits. Extend this list to register additional
/// kits; mirrors Lamedh's property-table registration.
static REGISTRY: &[&PlatformKit] = &[&RUBY_PLATFORM_KIT];

/// Look up the capabilities provided for a target language. Mirrors
/// `gymnast-platform-capabilities-for-target`. Returns `None` for an
/// unregistered target.
///
/// If more than one kit registers for the same target, the last one in
/// `REGISTRY` order wins, matching Lamedh's "last kit registered for a
/// target language wins" property-table semantics.
pub fn capabilities_for_target(target_language: &str) -> Option<&'static [Capability]> {
    REGISTRY
        .iter()
        .rev()
        .find(|kit| kit.target == target_language)
        .map(|kit| kit.capabilities)
}

/// Look up a platform kit by (name, version). Mirrors
/// `gymnast-lookup-platform-kit`. Returns `None` if not found.
pub fn lookup_kit(name: &str, version: &str) -> Option<&'static PlatformKit> {
    REGISTRY
        .iter()
        .find(|kit| kit.name == name && kit.version == version)
        .copied()
}

/// Validate one plan node's declared capabilities against a platform kit,
/// producing an error diagnostic for each declared capability the kit
/// does not provide. Mirrors `gymnast-validate-node-capabilities`.
pub fn validate_node_capabilities(node: &PlanNode, kit: &PlatformKit) -> Vec<Sexpr> {
    node.capabilities
        .iter()
        .filter(|cap| !kit.capabilities.iter().any(|k| &k.name == cap))
        .map(|cap| {
            diag_sexpr(
                "error",
                "E404",
                (0, 0),
                format!(
                    "capability not provided by platform kit: {} (node {})",
                    cap, node.id
                ),
            )
        })
        .collect()
}

/// Validate every plan node's declared capabilities against a platform
/// kit. Mirrors `gymnast-validate-plan-capabilities`.
pub fn validate_plan_capabilities(nodes: &[PlanNode], kit: &PlatformKit) -> Vec<Sexpr> {
    nodes
        .iter()
        .flat_map(|node| validate_node_capabilities(node, kit))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_capabilities_registered() {
        assert_eq!(RUBY_PLATFORM_CAPABILITIES.len(), 10);
    }
}
