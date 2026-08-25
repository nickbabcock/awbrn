# AWVM

AWVM is an executable specification for deterministic, turn-based strategy
games. It defines the canonical game state, commands, random inputs, state
transitions, events, and player-specific observations needed for independent
implementations to agree on the result of a turn.

The first ruleset is the Advance Wars By Web (AWBW) variant. The specification
is currently a draft: its versioned artifacts are usable, but they may change
before the first stable release.

AWVM is not a game client, server, replay format, renderer, or AI. Those systems
can use AWVM as a shared semantic boundary without adopting a particular engine
or storage model.

## What is included

- `schema/` defines the JSON interchange formats.
- `model/` defines shared state, command, event, observation, and arithmetic
  concepts.
- `semantics/` defines command validation and transition behavior.
- `rulesets/` contains versioned ruleset data and the features supported by
  each revision.
- `fixtures/` contains normative conformance cases.
- `protocol.md` defines the JSON Lines protocol used to run an implementation.
- `crates/awvm` is the Rust reference implementation.

The current ruleset is `awbw/2026-07-10`. Its manifest is
`rulesets/awbw/2026-07-10/manifest.json`. The revision is a behavior label, not
a claim about an AWBW deployment date.

## Rust interface

The reference implementation exposes three operations:

```text
execute(state, command, random) -> execution | error
observe(visibility, state, recipient) -> observation | error
observe-events(visibility, state, next-state, events, recipient)
  -> observed-events | error
```

`execute` performs command validation and reduction atomically. A rejected
command does not mutate the input state and consumes no random tokens. Accepted
commands return a complete new state, ordered events, and the number of random
tokens consumed.

A host that builds a match rather than loading one also needs the opening that
`model/phases.md` describes, which no command reaches:

```text
begin-match(state) -> execution | error
```

It takes the initialized state — settings, board, roster, predeployed units,
starting funds, `day = 1`, phase `turn-start` — runs the first player's day-one
start hooks, day-one income included, and returns the state they act from.
The JSON Lines protocol below carries no such operation, so match
initialization is not part of the conformance corpus; what the corpus checks is
the hooks themselves, which the opening and every turn boundary share.

The main Rust types and functions are:

```rust
use awvm::{
    semantic::{observe, observe_events, AwbwVisibility, State},
    transition::{begin_match, execute, Command, Execution},
};
```

Within this repository, run the reference implementation with:

```sh
cargo test -p awvm
cargo run -p awvm --bin awvm-jsonl
```

The crate is intentionally independent of Bevy, ECS, rendering, replay
adapters, and the rest of the AWBRN application.

## JSON Lines protocol

`awvm-jsonl` reads one request per line from standard input and writes one
response per line to standard output. Start by requesting its capabilities:

```json
{"protocol_version":"0.1.0","request_id":"capabilities","operation":"capabilities"}
```

To execute the smallest movement fixture:

```sh
jq -c '{
  protocol_version: "0.1.0",
  request_id: (.id + "/" + .steps[0].id),
  operation: "execute",
  ruleset,
  state: .initial_state,
  command: .steps[0].command,
  random: .steps[0].random
}' spec/fixtures/movement/infantry-plain-move.json \
  | cargo run -q -p awvm --bin awvm-jsonl
```

See `protocol.md` for execute, observation, error, and capability envelopes.

## Conformance

An implementation conforms to a specific ruleset revision and advertised
feature set. For every applicable fixture it must produce the specified:

- acceptance or stable violation;
- complete resulting state;
- ordered authoritative events;
- random-token consumption; and
- recipient observations and observed events, when asserted.

Build the bundled adapter and run the full fixture corpus with:

```sh
cargo build -p awvm --bins
target/debug/awvm-conformance target/debug/awvm-jsonl spec/fixtures
```

The harness discovers fixtures recursively, asks the adapter which feature
paths it implements, runs every supported case, and reports the first
structural JSON difference. A capability claims its entire feature subtree, as
defined in `protocol.md`.

Validate the schemas and ruleset tables with:

```sh
node spec/tools/validate-json.mjs
node spec/tools/validate-ruleset.mjs
```

## Integrating another implementation

An implementation may use any language or internal architecture. At the AWVM
boundary it must:

1. accept canonical state, commands, and explicit random tokens;
2. avoid clocks, ambient randomness, network reads, and other hidden inputs;
3. return canonical state and ordered semantic events;
4. project state and events separately for each recipient; and
5. advertise only the feature paths it implements completely.

Engine entities, database keys, replay action payloads, animation state, and UI
selection do not belong in canonical AWVM values. Convert them in an adapter at
the edge of the host application.

## Normative artifacts

The schemas, versioned ruleset data, semantic documents, and fixtures are
normative. Examples and implementation details are not. If normative artifacts
disagree, the disagreement is a specification defect; no artifact silently
overrides another.

Object member order is insignificant. Array order is significant. States,
events, and observations are compared as JSON values rather than source text.

Versioning and compatibility rules are defined in `versioning.md`.
