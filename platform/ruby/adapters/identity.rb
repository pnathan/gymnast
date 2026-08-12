# frozen_string_literal: true

module GymnastPlatform
  module Adapters
    class Identity
      Unauthenticated = Class.new(GymnastPlatform::Error)
      TokenExpired = Class.new(GymnastPlatform::Error)
      ProviderUnavailable = Class.new(GymnastPlatform::Error)

      Principal = Struct.new(:id, :provider, :claims, keyword_init: true)

      def capability_name = :identity

      def validate_token(token)
        raise NotImplementedError
      end

      def extract_principal(validated_token)
        raise NotImplementedError
      end

      def bind_session(principal, session_id)
        raise NotImplementedError
      end
    end
  end
end
