# frozen_string_literal: true

module GymnastPlatform
  module TestDoubles
    class ImmediateTransactions < Adapters::Transactions
      def capability_name = :transactions

      def within(_scope_key, &block)
        block.call
      end

      def read_committed(_scope_key, &block)
        block.call
      end

      def serializable(_scope_key, &block)
        block.call
      end
    end
  end
end
