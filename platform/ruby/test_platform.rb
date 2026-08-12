# frozen_string_literal: true

# Self-contained platform kit smoke test.
# Run: ruby platform/ruby/test_platform.rb

require_relative "test_doubles/all"

failures = []

def assert(desc, &block)
  if block.call
    print "."
  else
    $stderr.puts "\nFAIL: #{desc}"
    failures << desc
  end
rescue => e
  $stderr.puts "\nERROR: #{desc}: #{e.message}"
  failures << desc
end

# Registry
registry = GymnastPlatform::Registry.new
GymnastPlatform::TestDoubles.configure_registry(registry)

assert("registry has all capabilities") {
  required = %i[identity persistence repository transactions
    clock id_source http telemetry lifecycle durable_store]
  required.all? { |c| registry.available_capabilities.include?(c) }
}

assert("resolve unknown capability raises") {
  begin
    registry.resolve(:nonexistent)
    false
  rescue GymnastPlatform::CapabilityError
    true
  end
}

# Identity
identity = registry.resolve(:identity)
principal = GymnastPlatform::Adapters::Identity::Principal.new(
  id: "user-1", provider: "test", claims: {}
)
identity.register_principal("token-1", principal)

assert("identity validates known token") {
  identity.validate_token("token-1") == "token-1"
}

assert("identity rejects unknown token") {
  begin
    identity.validate_token("bad")
    false
  rescue GymnastPlatform::Adapters::Identity::Unauthenticated
    true
  end
}

assert("identity extracts principal") {
  identity.extract_principal("token-1").id == "user-1"
}

# Persistence
store = registry.resolve(:persistence)
store.put(:items, "id-1", { title: "test" })

assert("persistence get after put") {
  store.get(:items, "id-1")[:title] == "test"
}

assert("persistence not found") {
  begin
    store.get(:items, "missing")
    false
  rescue GymnastPlatform::Adapters::Persistence::NotFound
    true
  end
}

assert("persistence query") {
  results = store.query(:items, ->(r) { r[:title] == "test" })
  results.length == 1
}

# Repository
repo = registry.resolve(:repository)
repo.save(:tasks, "t-1", { id: "t-1", title: "task", version: 1 })

assert("repository find") {
  repo.find(:tasks, "t-1")[:title] == "task"
}

assert("repository version conflict") {
  begin
    repo.save(:tasks, "t-1", { version: 2 }, expected_version: 99)
    false
  rescue GymnastPlatform::Adapters::Repository::VersionConflict
    true
  end
}

# Clock
clock = registry.resolve(:clock)
assert("virtual clock starts at epoch") {
  clock.now_utc.year == 2025
}

clock.advance(60)
assert("virtual clock advances") {
  clock.monotonic_now == 60.0
}

# ID source
ids = registry.resolve(:id_source)
assert("sequential ids are deterministic") {
  a = ids.generate
  b = ids.generate
  a == "id-1" && b == "id-2"
}

# Telemetry
tel = registry.resolve(:telemetry)
tel.log(:info, "test message", component: "test")
assert("telemetry captures entries") {
  tel.entries(level: :info).length == 1
}

# HTTP
http = registry.resolve(:http)
http.route("GET", "/health") { |_req|
  GymnastPlatform::Adapters::Http::Response.new(
    status: 200, body: "ok", headers: {}
  )
}

assert("http routes requests") {
  http.call("GET", "/health").status == 200
}

assert("http 404 for unknown routes") {
  http.call("GET", "/missing").status == 404
}

# Lifecycle
lc = registry.resolve(:lifecycle)
lc.start
assert("lifecycle reports healthy") { lc.healthy? }
lc.stop
assert("lifecycle reports not healthy after stop") { !lc.healthy? }

# Summary
puts
if failures.empty?
  puts "#{__FILE__}: all assertions passed"
  exit 0
else
  puts "#{__FILE__}: #{failures.length} failures"
  failures.each { |f| puts "  - #{f}" }
  exit 1
end
