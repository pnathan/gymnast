# frozen_string_literal: true

# Deterministic test doubles for all platform kit capabilities.
# These provide in-memory, reproducible implementations suitable
# for acceptance testing and verification obligations.

require_relative "../gymnast_platform"
require_relative "memory_persistence"
require_relative "memory_repository"
require_relative "stub_identity"
require_relative "immediate_transactions"
require_relative "virtual_clock"
require_relative "sequential_id_source"
require_relative "rack_http"
require_relative "capturing_telemetry"
require_relative "simple_lifecycle"

module GymnastPlatform
  module TestDoubles
    def self.configure_registry(registry = GymnastPlatform.registry)
      registry.register(:identity, StubIdentity.new)
      registry.register(:persistence, MemoryPersistence.new)
      registry.register(:repository, MemoryRepository.new)
      registry.register(:transactions, ImmediateTransactions.new)
      registry.register(:clock, VirtualClock.new)
      registry.register(:id_source, SequentialIdSource.new)
      registry.register(:http, RackHttp.new)
      registry.register(:telemetry, CapturingTelemetry.new)
      registry.register(:lifecycle, SimpleLifecycle.new)
      registry.register(:durable_store, MemoryPersistence.new)
      registry
    end
  end
end
