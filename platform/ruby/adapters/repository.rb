# frozen_string_literal: true

module GymnastPlatform
  module Adapters
    class Repository
      NotFound = Class.new(GymnastPlatform::Error)
      VersionConflict = Class.new(GymnastPlatform::Error)
      ConnectionLost = Class.new(GymnastPlatform::Error)

      def capability_name = :repository

      def find(aggregate_type, id)
        raise NotImplementedError
      end

      def find_all(aggregate_type, ids)
        raise NotImplementedError
      end

      def save(aggregate_type, id, record, expected_version: nil)
        raise NotImplementedError
      end

      def query(aggregate_type, &predicate)
        raise NotImplementedError
      end

      def load_aggregate(aggregate_type, scope_key)
        raise NotImplementedError
      end
    end
  end
end
