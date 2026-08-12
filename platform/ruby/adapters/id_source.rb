# frozen_string_literal: true

module GymnastPlatform
  module Adapters
    class IdSource
      EntropyExhausted = Class.new(GymnastPlatform::Error)

      def capability_name = :id_source

      def generate
        raise NotImplementedError
      end

      def generate_for(scope)
        raise NotImplementedError
      end
    end
  end
end
