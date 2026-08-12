# frozen_string_literal: true

# Model provider adapter for gymnast synthesis.
#
# Provider-agnostic interface: implementations must respond to
# #generate(prompt_text, model_policy) and return a string
# containing a candidate S-expression.
#
# The runner never executes model output; it only parses it as data.

require "json"
require "net/http"
require "uri"

module GymnastPlatform
  module ModelProvider
    class Base
      attr_reader :model_id

      def generate(prompt_text, model_policy)
        raise NotImplementedError
      end

      def provider_name
        raise NotImplementedError
      end

      def identity
        { provider: provider_name, model: model_id }
      end
    end

    class ClaudeHaiku < Base
      DEFAULT_MODEL = "claude-haiku-4-5-20251001"

      def initialize(api_key: nil, model_id: DEFAULT_MODEL, max_tokens: 8192)
        @api_key = api_key || ENV.fetch("ANTHROPIC_API_KEY") {
          raise GymnastPlatform::ConfigurationError,
            "ANTHROPIC_API_KEY required for Claude provider"
        }
        @model_id = model_id
        @max_tokens = max_tokens
      end

      def provider_name = "anthropic"

      def generate(prompt_text, model_policy)
        temperature = extract_temperature(model_policy)
        uri = URI("https://api.anthropic.com/v1/messages")
        body = {
          model: @model_id,
          max_tokens: @max_tokens,
          temperature: temperature,
          messages: [{ role: "user", content: prompt_text }]
        }

        response = Net::HTTP.start(uri.host, uri.port, use_ssl: true) do |http|
          request = Net::HTTP::Post.new(uri)
          request["content-type"] = "application/json"
          request["x-api-key"] = @api_key
          request["anthropic-version"] = "2023-06-01"
          request.body = JSON.generate(body)
          http.request(request)
        end

        unless response.is_a?(Net::HTTPSuccess)
          raise GymnastPlatform::Error,
            "model request failed: #{response.code} #{response.body}"
        end

        result = JSON.parse(response.body)
        content = result.dig("content", 0, "text")
        extract_sexpr(content)
      end

      private

      def extract_temperature(model_policy)
        return 0 unless model_policy.is_a?(Array) || model_policy.is_a?(Hash)
        if model_policy.is_a?(Array)
          model_policy.each_cons(2) do |k, v|
            return v if k == :temperature
          end
        end
        0
      end

      def extract_sexpr(text)
        return text unless text
        match = text.match(/\(candidate\b.*\z/m)
        match ? match[0] : text
      end
    end

    class Stub < Base
      def initialize(responses: {})
        @responses = responses
        @model_id = "stub-model"
        @calls = []
      end

      def provider_name = "stub"

      def generate(prompt_text, model_policy)
        @calls << { prompt: prompt_text, policy: model_policy }
        node_id = extract_node_id(prompt_text)
        @responses.fetch(node_id) {
          @responses.fetch(:default, "(candidate (error \"no stub response\"))")
        }
      end

      def calls = @calls

      def register_response(node_id, response)
        @responses[node_id] = response
        self
      end

      private

      def extract_node_id(text)
        match = text.match(/Node:\s*(\S+)/)
        match ? match[1] : :default
      end
    end
  end
end
