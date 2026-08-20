# Shared domain definitions for cross-application communication

Status: DESIGN (proposed phase 10+). Nothing here is implemented; this
document records the considered design so the eventual execution plan
can be derived from it the way phases 3–9 were.

## Problem

Two Gymnast applications that communicate — todo emitting
`task_created`, a BI ingest service journaling it — need a shared
vocabulary: the event's shape, the mode definitions it carries, the
error set both sides agree on. Today there are exactly two ways to
share definitions, and both fail the communication case:

1. **Copy the declarations into both specs.** Semantic IDs are rooted
   at the module name (`module/kind/name`), so byte-identical
   `mode AnalyticsEvent` declarations in two specs produce DIFFERENT
   semantic ids (`todo/type/AnalyticsEvent` vs
   `bi_ingest/type/AnalyticsEvent`) and different fingerprints. The
   compiler cannot even express the question "do these two
   applications agree on this type?", let alone answer it. Drift
   between the copies is silent.
2. **Built-in profiles.** `use oddities/profiles/todo_standard @ 1.0`
   generates declarations deterministically and records
   `:profile-source` provenance — the right shape — but profiles are
   compiled into `profile.rs`. Users cannot define one, and a profile
   is spec-infrastructure (Cursor, Page), not domain vocabulary.

Meanwhile `emits task_created exactly_once_logically` extracts into
the transition's `emissions` and resolves to NOTHING. The emission
vocabulary — precisely the words two services use to talk to each
other — is the one part of a spec that is closed-world-checked
nowhere. That is not an accident to paper over; it is the missing
compilation unit.

## Proposal: the `domain` compilation unit

A domain is a `.gym` file whose only legal declarations are modes,
error sets, and (phase B) events — no actors, components, interfaces,
state, behaviors, invariants, constraints, synthesis, or acceptance.
It is pure vocabulary:

```
domain analytics = v 1.0
  exports EventId, SourceId, AnalyticsEvent, EventKind

mode EventId  = opaque text
mode SourceId = opaque text
mode EventKind = enum (track, identify, page_view, metric)

mode AnalyticsEvent = struct (
  EventId id,
  SourceId source,
  EventKind kind,
  text (..65536) payload )

event task_created  = carries AnalyticsEvent   # phase B
event task_trashed  = carries AnalyticsEvent
```

An application imports it the way it imports a profile today:

```
use domain analytics @ 1.0
```

### Resolution (deterministic, closed-world)

- Domains resolve from an explicit registry, exactly as profiles do
  today: a `domains/` directory sibling to the spec (path =
  `domains/<name>.gym`), no search paths, no network, no environment.
  The file's raw bytes participate in the importer's fingerprint
  chain, so identical inputs still produce byte-identical IR.
- The domain file is elaborated ONCE, by itself, to a domain IR
  containing only type/error/event nodes whose semantic ids are rooted
  at the DOMAIN name: `analytics/type/AnalyticsEvent`. The domain IR
  gets its own fingerprint over its fingerprint-free form (the
  standing discipline).
- Importing splices the domain's nodes into the application IR
  verbatim — same ids, same fingerprints, plus a `:domain-source`
  field (the `:profile-source` pattern) and an entry in a new IR
  header field: `(domains ((analytics "1.0" "fnv1a64:...") ...))`.

### The guarantee this buys

Two applications can communicate over a domain iff they import the
same `(name, version)` AND the recorded domain fingerprints are equal.
Because the spliced nodes carry domain-rooted ids, this is not a
convention — it is a checkable identity:

```
gymnast compat build/todo/ir.sexpr build/bi-ingest/ir.sexpr
# -> per shared domain: name, version, fingerprint, MATCH | MISMATCH
# exit 1 on any mismatch
```

Same name+version resolving to different fingerprints (someone edited
`domains/analytics.gym` without bumping the version) is a hard error
at elaboration — E207 `domain-version-fingerprint-conflict` — in any
build that can see both. Version pins are EXACT: no ranges, no
"compatible" structural typing, no duck-typed shape matching.
Fingerprint equality is the only compatibility relation; everything
weaker reintroduces silent drift with extra steps.

## Phase B: events and cross-application flows

With `event` declarations in domains, the dangling ends of today's
pipeline tie together:

- A behavior's `emits task_created ...` must resolve to an imported or
  locally declared event — new warning `W407 unresolved-emission`
  (the W406 pattern), upgraded to an error once the corpus is clean.
- A consuming application declares the other half:

  ```
  behavior ingest_event = on ingest_service.ingest (producer, request) (
    consumes analytics.task_created, ... )
  ```

- Flows can then cross application boundaries in the IR: the emitter
  records `(emits analytics/event/task_created)`, the consumer records
  `(consumes analytics/event/task_created)` — the same node id on both
  sides, which is the point.

## Phase C (far goal): cross-service verification

Once both sides name the same event node, the transition calculus can
check the contract seam: the emitter's post-state event (shape from
the domain) is a legal input to the consumer's transition — a
bounded-trace producer/consumer equivalence obligation, verified with
the same tri-state honesty rules as everything else (`indeterminate`
when the closed evaluator cannot decide, never a vacuous pass).

## Invariants preserved (why this fits the architecture)

- **No model authority.** Domains are elaborated, fingerprinted input;
  models see them only as compiled prompt projections (the type
  reference section), never author or modify them. The firewall's
  no-added-assumptions rule already covers candidates that invent
  types not in the projection.
- **Determinism.** Registry resolution by pinned path + version; the
  domain file's bytes are inside the fingerprint chain; splice order
  is the domain file's declaration order. Identical inputs →
  byte-identical IR, unchanged.
- **Closed world.** A domain that declares anything but vocabulary is
  an elaboration error. An import that does not resolve is an error.
  Exported-name collisions between domains, or between a domain and
  the spec, are E201 duplicates — no shadowing, no merge.
- **Honest evidence.** `compat` output and the IR `domains` header are
  fingerprint-carrying artifacts like every other; the evidence bundle
  can carry the domain table so promotion can (later) require
  wire-compat proof between deployable pairs.

## What this deliberately does NOT do

- No structural/duck compatibility, no version ranges, no "minor
  version" leniency: fingerprint equality or nothing.
- No remote/package-manager fetching. A domain is a file in the repo.
- No shared BEHAVIOR. Domains are nouns (modes, errors, events), never
  verbs — shared behavior is what components and interfaces are for,
  and sharing it would blur ownership and authority boundaries.
- No implicit imports: a spec that uses `analytics.task_created`
  without `use domain analytics @ 1.0` is a closed-world error.

## Suggested execution shape (when scheduled)

Committed-oracle process as phases 4–9. Phase A alone is a coherent
increment: `domain` unit parsing + validation, registry resolution,
domain IR + fingerprint, splice with `:domain-source` + IR `domains`
header, E207, `compat` subcommand, worked example (`domains/
analytics.gym` shared by `examples/todo.gym` and
`examples/bi-ingest.gym`), goldens updated once. Phase B (events,
W407, `consumes`) and phase C (seam verification) are separately
gateable increments.
