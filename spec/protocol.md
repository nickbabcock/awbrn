# AWVM conformance protocol

Version `0.1.0` is a newline-delimited JSON request/response protocol. Each
non-empty input line is one request and produces exactly one output line. A
process MUST preserve request order and MUST write no non-JSON data to stdout.

Every adapter MUST support a `capabilities` request:

```json
{"protocol_version":"0.1.0","request_id":"capabilities","operation":"capabilities"}
```

A conformance feature ID is a dot-separated path of one or more segments
matching `[a-z0-9]+(-[a-z0-9]+)*`. Its first segment MUST be a feature ID listed
in the `features` array of the addressed ruleset revision's manifest; any
further segments narrow that feature to a command family and scenario. IDs are
compared segment-wise, never as substrings.

The response lists the feature IDs an adapter claims. Listing an ID claims its
entire subtree: every case at or below that node. An adapter that implements a
feature completely SHOULD advertise the bare manifest ID; one that implements
part of a feature MAY advertise deeper nodes instead.

```json
{"protocol_version":"0.1.0","request_id":"capabilities","status":"ok","features":["movement-v1.move-wait.plain"]}
```

A runner MUST execute a case when the case's `feature` equals an advertised ID
or descends from one — that is, when it is that ID followed by `.` and at least
one further segment. Cases matching no advertised subtree are reported as
skipped, not passed or failed. So an adapter advertising `elimination-v1` runs
`elimination-v1.resign.cascade`, while one advertising only
`elimination-v1.resign` skips `elimination-v1.rout.crash-reselects-successor`.

The execution operation is `execute`:

```json
{"protocol_version":"0.1.0","request_id":"example/step-1","operation":"execute","ruleset":{"id":"awbw","revision":"2026-07-10"},"state":{},"command":{},"random":[]}
```

`state`, `command`, and `random` conform to their schemas under `schema/`.
`ruleset` MUST equal `state.ruleset`. The implementation receives no expected
fixture values.

An accepted response contains the complete resulting state, ordered semantic
events, and the number of random tokens consumed:

```json
{"protocol_version":"0.1.0","request_id":"example/step-1","status":"accepted","state":{},"events":[],"random_consumed":0}
```

`execute` is a trusted conformance and reducer interface, not a client-facing
fog boundary. Its complete state, authoritative unit IDs, random inputs, and
authoritative events MUST NOT be sent to an untrusted player client. A hosting
service exposes the recipient-specific results of `observe` and
`observe-events` instead and resolves client enemy references as specified by
`model/observation.md`.

A rules rejection uses `status: "rejected"` and contains `violation` plus
`random_consumed: 0`. A transport, decoding, unsupported-feature, or internal
failure uses `status: "error"`, a stable `code`, and a diagnostic `message`.
Protocol errors are not AWVM violations and do not establish conformance.

## Observation operations

An adapter that advertises `fog-observation-v1`, or any feature whose fixtures
assert observations, MUST support two further operations. Both are pure: they
return no state, consume no random token, and MUST NOT be affected by the order
in which a runner issues them.

`observe` projects one authoritative state for one recipient:

```json
{"protocol_version":"0.1.0","request_id":"example/obs-1","operation":"observe","ruleset":{"id":"awbw","revision":"2026-07-10"},"state":{},"recipient":"p1"}
```

```json
{"protocol_version":"0.1.0","request_id":"example/obs-1","status":"ok","observation":{}}
```

`observe-events` projects one transition for one recipient. It receives both
endpoints and the authoritative event array, because projection is defined
against pre- and post-command visibility:

```json
{"protocol_version":"0.1.0","request_id":"example/obs-2","operation":"observe-events","ruleset":{"id":"awbw","revision":"2026-07-10"},"state":{},"next_state":{},"events":[],"recipient":"p1"}
```

```json
{"protocol_version":"0.1.0","request_id":"example/obs-2","status":"ok","observed_events":[]}
```

`observation` conforms to `schema/observation.schema.json` and each member of
`observed_events` to `schema/observed-event.schema.json`. `recipient` MUST name
a player in `state`; a recipient that does not is a protocol error, not a
violation. `state`, `next_state`, and `events` are supplied by the runner from
the case and from the adapter's own accepted `execute` response — a runner MUST
NOT pass fixture `expect` values to either operation.

## Case playback with observations

A sequence case's `initial_observations` is checked with one `observe` per
listed recipient against `initial_state`, before the first step. A step's
`observations` is checked the same way against that step's resulting state,
which for a rejected step is the unchanged current state. A step's
`observed_events` is checked with one `observe-events` per listed recipient,
passing the state the step began in, the state the adapter returned, and the
events the adapter returned.

An equivalence case runs each side independently through `execute`, then applies
its declared assertion after every corresponding step:
`equal-observations` compares `observe` on both sides for the case's
`recipient`; `equal-observations-and-events` additionally compares
`observe-events`. Empty step arrays compare the initial observations only. The
comparison is between the two sides, not against any literal in the case, so an
equivalence case asserts noninterference without fixing what is observed.

Consumers compare JSON structurally as defined in `README.md`. Diagnostics may
be written to stderr and are not part of the protocol.
