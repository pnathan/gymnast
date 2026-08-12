# frozen_string_literal: true

module GymnastPlatform
  module Adapters
    class Persistence
      ConnectionLost = Class.new(GymnastPlatform::Error)
      ConstraintViolation = Class.new(GymnastPlatform::Error)
      NotFound = Class.new(GymnastPlatform::Error)

      def capability_name = :persistence

      def get(collection, id)
        raise NotImplementedError
      end

      def put(collection, id, record)
        raise NotImplementedError
      end

      def delete(collection, id)
        raise NotImplementedError
      end

      def query(collection, predicate)
        raise NotImplementedError
      end

      def migrate(migrations)
        raise NotImplementedError
      end
    end
  end
end
