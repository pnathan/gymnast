# frozen_string_literal: true

# Gymnast Ruby Platform Kit v1.0
#
# Trusted runtime boundary for synthesized applications.
# Generated code must cross these adapter interfaces for all
# external effects.  Direct stdlib access is prohibited by
# the synthesis contract.

module GymnastPlatform
  VERSION = "1.0"

  class Error < StandardError; end
  class CapabilityError < Error; end
  class ConfigurationError < Error; end

  class Registry
    def initialize
      @adapters = {}
    end

    def register(capability_name, adapter)
      unless adapter.respond_to?(:capability_name)
        raise ConfigurationError, "adapter must respond to :capability_name"
      end
      @adapters[capability_name.to_sym] = adapter
      self
    end

    def resolve(capability_name)
      @adapters.fetch(capability_name.to_sym) do
        raise CapabilityError, "undeclared capability: #{capability_name}"
      end
    end

    def available_capabilities
      @adapters.keys
    end

    def freeze_registry!
      @adapters.freeze
      self
    end
  end

  @registry = Registry.new

  def self.registry
    @registry
  end

  def self.register(capability_name, adapter)
    @registry.register(capability_name, adapter)
  end

  def self.resolve(capability_name)
    @registry.resolve(capability_name)
  end

  def self.configure
    yield @registry if block_given?
    self
  end
end

require_relative "adapters/identity"
require_relative "adapters/persistence"
require_relative "adapters/repository"
require_relative "adapters/transactions"
require_relative "adapters/clock"
require_relative "adapters/id_source"
require_relative "adapters/http"
require_relative "adapters/telemetry"
require_relative "adapters/lifecycle"
