# frozen_string_literal: true

module GymnastPlatform
  module TestDoubles
    class VirtualClock < Adapters::Clock
      def initialize(start_time: Time.utc(2025, 1, 1))
        @current = start_time
        @monotonic = 0.0
      end

      def capability_name = :clock

      def now_utc
        @current
      end

      def monotonic_now
        @monotonic
      end

      def advance(seconds)
        @current += seconds
        @monotonic += seconds
        self
      end

      def set(time)
        @current = time
        self
      end
    end
  end
end
