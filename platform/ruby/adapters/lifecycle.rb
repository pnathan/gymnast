# frozen_string_literal: true

module GymnastPlatform
  module Adapters
    class Lifecycle
      StartupFailure = Class.new(GymnastPlatform::Error)
      ShutdownTimeout = Class.new(GymnastPlatform::Error)

      def capability_name = :lifecycle

      def start(dependencies:)
        raise NotImplementedError
      end

      def stop(timeout_seconds: 30)
        raise NotImplementedError
      end

      def healthy?
        raise NotImplementedError
      end

      def on_shutdown(&block)
        raise NotImplementedError
      end
    end
  end
end
