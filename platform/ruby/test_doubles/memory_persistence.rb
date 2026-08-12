# frozen_string_literal: true

module GymnastPlatform
  module TestDoubles
    class MemoryPersistence < Adapters::Persistence
      def initialize
        @collections = {}
      end

      def capability_name = :persistence

      def get(collection, id)
        col = @collections[collection] || {}
        col.fetch(id) { raise NotFound, "#{collection}/#{id}" }
      end

      def put(collection, id, record)
        @collections[collection] ||= {}
        @collections[collection][id] = record
        record
      end

      def delete(collection, id)
        col = @collections[collection] || {}
        col.delete(id) || raise(NotFound, "#{collection}/#{id}")
      end

      def query(collection, predicate)
        col = @collections[collection] || {}
        col.values.select(&predicate)
      end

      def migrate(_migrations)
        nil
      end

      def reset!
        @collections.clear
      end

      def snapshot
        @collections.transform_values(&:dup)
      end
    end
  end
end
