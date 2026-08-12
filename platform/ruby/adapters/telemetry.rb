# frozen_string_literal: true

module GymnastPlatform
  module Adapters
    class Telemetry
      BufferOverflow = Class.new(GymnastPlatform::Error)

      def capability_name = :telemetry

      def log(level, message, **fields)
        raise NotImplementedError
      end

      def trace(operation, **fields, &block)
        raise NotImplementedError
      end

      def metric(name, value, **tags)
        raise NotImplementedError
      end
    end
  end
end
