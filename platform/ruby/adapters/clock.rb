# frozen_string_literal: true

module GymnastPlatform
  module Adapters
    class Clock
      DriftBeyondTolerance = Class.new(GymnastPlatform::Error)

      def capability_name = :clock

      def now_utc
        raise NotImplementedError
      end

      def monotonic_now
        raise NotImplementedError
      end

      def elapsed_since(monotonic_mark)
        monotonic_now - monotonic_mark
      end
    end
  end
end
