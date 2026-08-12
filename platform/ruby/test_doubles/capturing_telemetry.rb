# frozen_string_literal: true

module GymnastPlatform
  module TestDoubles
    class CapturingTelemetry < Adapters::Telemetry
      Entry = Struct.new(:level, :message, :fields, :timestamp,
        keyword_init: true)

      def initialize
        @entries = []
        @metrics = []
      end

      def capability_name = :telemetry

      def log(level, message, **fields)
        @entries << Entry.new(
          level: level, message: message, fields: fields,
          timestamp: Time.now
        )
        nil
      end

      def trace(operation, **fields, &block)
        log(:trace, "start #{operation}", **fields)
        result = block.call
        log(:trace, "end #{operation}", **fields)
        result
      end

      def metric(name, value, **tags)
        @metrics << { name: name, value: value, tags: tags }
        nil
      end

      def entries(level: nil)
        return @entries unless level
        @entries.select { |e| e.level == level }
      end

      def metrics
        @metrics
      end

      def reset!
        @entries.clear
        @metrics.clear
      end
    end
  end
end
