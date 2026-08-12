# frozen_string_literal: true

module GymnastPlatform
  module TestDoubles
    class SequentialIdSource < Adapters::IdSource
      def initialize(prefix: "id")
        @prefix = prefix
        @counter = 0
      end

      def capability_name = :id_source

      def generate
        @counter += 1
        "#{@prefix}-#{@counter}"
      end

      def generate_for(scope)
        @counter += 1
        "#{@prefix}-#{scope}-#{@counter}"
      end

      def reset!
        @counter = 0
      end
    end
  end
end
