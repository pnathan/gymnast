# frozen_string_literal: true

module GymnastPlatform
  module TestDoubles
    class SimpleLifecycle < Adapters::Lifecycle
      def initialize
        @running = false
        @shutdown_hooks = []
      end

      def capability_name = :lifecycle

      def start(dependencies: [])
        @running = true
        self
      end

      def stop(timeout_seconds: 30)
        @shutdown_hooks.each(&:call)
        @running = false
        self
      end

      def healthy?
        @running
      end

      def on_shutdown(&block)
        @shutdown_hooks << block
        self
      end

      def reset!
        @running = false
        @shutdown_hooks.clear
      end
    end
  end
end
