# frozen_string_literal: true

module GymnastPlatform
  module TestDoubles
    class RackHttp < Adapters::Http
      def initialize
        @routes = {}
      end

      def capability_name = :http

      def route(method, path, &handler)
        @routes[[method.to_s.upcase, path]] = handler
        self
      end

      def start(port: 0)
        @started = true
        self
      end

      def stop
        @started = false
        self
      end

      def call(method, path, params: {}, body: nil, headers: {})
        handler = @routes.fetch([method.to_s.upcase, path]) do
          return Response.new(status: 404, body: "not found", headers: {})
        end
        request = Request.new(
          method: method.to_s.upcase, path: path,
          params: params, body: body, headers: headers
        )
        handler.call(request)
      end

      def started? = @started

      def reset!
        @routes.clear
        @started = false
      end
    end
  end
end
