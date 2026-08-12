# frozen_string_literal: true

module GymnastPlatform
  module TestDoubles
    class MemoryRepository < Adapters::Repository
      def initialize
        @stores = {}
      end

      def capability_name = :repository

      def find(aggregate_type, id)
        store = @stores[aggregate_type] || {}
        store.fetch(id) { raise NotFound, "#{aggregate_type}/#{id}" }
      end

      def find_all(aggregate_type, ids)
        store = @stores[aggregate_type] || {}
        ids.map { |id| store[id] }.compact
      end

      def save(aggregate_type, id, record, expected_version: nil)
        @stores[aggregate_type] ||= {}
        existing = @stores[aggregate_type][id]
        if expected_version && existing
          actual = existing[:version] || existing["version"]
          if actual && actual != expected_version
            raise VersionConflict,
              "#{aggregate_type}/#{id}: expected #{expected_version}, got #{actual}"
          end
        end
        @stores[aggregate_type][id] = record
        record
      end

      def query(aggregate_type, &predicate)
        store = @stores[aggregate_type] || {}
        store.values.select(&predicate)
      end

      def load_aggregate(aggregate_type, scope_key)
        store = @stores[aggregate_type] || {}
        store.select { |k, _| k.start_with?(scope_key.to_s) }
      end

      def reset!
        @stores.clear
      end
    end
  end
end
