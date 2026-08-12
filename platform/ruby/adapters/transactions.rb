# frozen_string_literal: true

module GymnastPlatform
  module Adapters
    class Transactions
      Deadlock = Class.new(GymnastPlatform::Error)
      Timeout = Class.new(GymnastPlatform::Error)
      Rollback = Class.new(GymnastPlatform::Error)

      def capability_name = :transactions

      def within(scope_key, &block)
        raise NotImplementedError
      end

      def read_committed(scope_key, &block)
        raise NotImplementedError
      end

      def serializable(scope_key, &block)
        raise NotImplementedError
      end
    end
  end
end
