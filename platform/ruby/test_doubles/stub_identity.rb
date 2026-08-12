# frozen_string_literal: true

module GymnastPlatform
  module TestDoubles
    class StubIdentity < Adapters::Identity
      def initialize
        @principals = {}
      end

      def capability_name = :identity

      def register_principal(token, principal)
        @principals[token] = principal
        self
      end

      def validate_token(token)
        @principals.fetch(token) do
          raise Unauthenticated, "unknown token"
        end
        token
      end

      def extract_principal(validated_token)
        @principals.fetch(validated_token) do
          raise Unauthenticated, "no principal for token"
        end
      end

      def bind_session(principal, session_id)
        { principal_id: principal.id, session_id: session_id }
      end

      def reset!
        @principals.clear
      end
    end
  end
end
