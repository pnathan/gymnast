# frozen_string_literal: true

module GymnastPlatform
  module Adapters
    class Http
      BadRequest = Class.new(GymnastPlatform::Error)
      MethodNotAllowed = Class.new(GymnastPlatform::Error)
      InternalError = Class.new(GymnastPlatform::Error)

      Request = Struct.new(:method, :path, :params, :body, :headers,
        keyword_init: true)
      Response = Struct.new(:status, :body, :headers, keyword_init: true)

      def capability_name = :http

      def route(method, path, &handler)
        raise NotImplementedError
      end

      def start(port:)
        raise NotImplementedError
      end

      def stop
        raise NotImplementedError
      end
    end
  end
end
