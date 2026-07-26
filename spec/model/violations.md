# Violations and failure classes

Status: normative envelope and `move-wait` precedence for specification 0.1.0.

## Failure classes

The semantic layers are distinct:

1. **Malformed input** fails decoding or `command.schema.json`. It is a
   `malformed-command` API error, not a violation from `validate`.
2. **Invalid authoritative state** is reported by `check-invariants(R,S)` (or
   as an `invalid-state` API failure before validation). A command MUST NOT be
   used to repair such a state.
3. **Authorization failure** means the supplied authority principal cannot
   submit the intent. It is checked before state-dependent command rejection
   and MUST not reveal hidden state. `AUTHORITY_REQUIRED` is the stable code.
4. **Command rejection** is the single primary stable violation returned by
   `validate(R,S,C)`. It changes no state, emits no event, and consumes no
   random token.
5. **Execution error** means a state-bound validated command cannot execute,
   including stale state binding or missing, wrong-type, or out-of-domain
   random input. It is not a violation and execution is atomic.

Violation objects have a stable `code` plus only the payload licensed for that
code. They contain no prose. Coordinates use canonical `[x,y]`; identifiers
are canonical IDs. Human-readable messages are adapter-owned and MUST key off
the code rather than enter conformance values.

## Stable codes

The closed schema reserves common command-family codes while defining complete
payloads for the initial movement slice:

| Code | Payload | Meaning |
| --- | --- | --- |
| `AUTHORITY_REQUIRED` | `authority` | required authority was absent |
| `MATCH_FINISHED` | none | gameplay is terminal |
| `WRONG_PHASE` | `expected`, `actual` | command is unavailable in this phase |
| `NOT_ACTIVE_PLAYER` | `player` | actor is not the active player |
| `UNIT_NOT_FOUND` | `unit` | referenced acting unit does not exist |
| `UNIT_NOT_OWNED` | `unit`, `player` | acting unit is not owned by actor |
| `UNIT_NOT_ON_BOARD` | `unit` | acting unit is cargo |
| `UNIT_ALREADY_ACTED` | `unit` | acting unit is not `ready` |
| `PATH_ORIGIN_MISMATCH` | `expected`, `actual` | first path position differs from unit position |
| `PATH_NON_ADJACENT` | `index`, `from`, `to` | step ending at `index` is not orthogonal |
| `PATH_REPEATED_POSITION` | `index`, `position`, `first_index` | path revisits a position |
| `PATH_OUT_OF_BOUNDS` | `index`, `position` | path position is outside the board |
| `TERRAIN_IMPASSABLE` | `index`, `position` | mover cannot enter or finish on the terrain |
| `PATH_OCCUPIED` | `index`, `position` | a disclosed intermediate obstruction blocks the path |
| `INSUFFICIENT_MOVEMENT` | `required`, `available` | path exceeds effective move allowance |
| `INSUFFICIENT_FUEL` | `required`, `available` | path exceeds available fuel |
| `DESTINATION_OCCUPIED` | `position` | disclosed destination occupancy forbids the action |
| `INVALID_TARGET` | optional `target` | target is absent or inapplicable |
| `TARGET_OUT_OF_RANGE` | optional `target` | target is outside effective range |
| `ACTION_NOT_SUPPORTED` | `action` | unit lacks the requested capability |
| `INSUFFICIENT_FUNDS` | `required`, `available` | actor cannot pay the cost |
| `INSUFFICIENT_POWER` | `required`, `available` | commander cannot pay the charge |
| `UNIT_LIMIT_REACHED` | `current`, `limit` | actor already owns at least the configured maximum number of units |
Reserved codes are not permission to implement an otherwise unspecified
feature. Feature documents may narrow payloads and establish precedence before
their cases become normative. Adding or changing a code requires a
specification minor version.

## Primary-violation precedence

Validation returns exactly one violation. Checks within a command family occur
in the listed order and stop at the first failure. This prevents implementation
iteration order, optimization, or hidden information from changing observable
rejection.

All player gameplay commands use this common prefix:

```text
authorization
MATCH_FINISHED
WRONG_PHASE
NOT_ACTIVE_PLAYER
```

Movement commands then use:

```text
UNIT_NOT_FOUND
UNIT_NOT_OWNED
UNIT_NOT_ON_BOARD
UNIT_ALREADY_ACTED
PATH_ORIGIN_MISMATCH
PATH_NON_ADJACENT
PATH_REPEATED_POSITION
PATH_OUT_OF_BOUNDS
TERRAIN_IMPASSABLE
PATH_OCCUPIED
INSUFFICIENT_MOVEMENT
INSUFFICIENT_FUEL
family-specific target/action checks
DESTINATION_OCCUPIED
```

Path checks scan indices from zero upward and report the lowest failing index.
Costs are evaluated only after structural, bounds, terrain, and disclosed
occupancy checks. Under fog, an occupancy fact the actor cannot observe MUST
not produce `PATH_OCCUPIED` or `DESTINATION_OCCUPIED`; if the intent is
otherwise legal, validation succeeds and execution applies the specified trap
transition.

For `move-wait`, “family-specific” is empty, making the sequence above its
complete precedence. Other movement-action families inherit the common
sequence but are non-executable until their feature specification inserts and
orders target/capability checks.

For `combat-neutral-v1`, `move-attack` inserts these checks after fuel and
before destination occupancy:

```text
ACTION_NOT_SUPPORTED        (acting unit has no attack-capable weapon)
INVALID_TARGET              (target is not an existing enemy board unit)
TARGET_OUT_OF_RANGE         (selected fire mode cannot reach the target)
INVALID_TARGET              (neither ammo nor unlimited has the matchup)
```

Ammo shortage is not itself a violation when an unlimited entry
exists: weapon selection falls back to unlimited. Indirect fire after a
non-zero move rejects as `ACTION_NOT_SUPPORTED` with action `move-and-fire`.
These checks disclose target identity only in the fog-disabled neutral slice.

`resign` uses the common prefix and adds nothing: it names no unit, tile, or
target, so `NOT_ACTIVE_PLAYER` is its last check
(`semantics/elimination.md`). `end-turn` is the same shape
(`semantics/turn.md`). `tag` extends that prefix with
`ACTION_NOT_SUPPORTED { action: "tag" }` when tag mode is disabled
(`semantics/tag.md`).

Non-movement families use the common prefix, then resolve the acting subject,
ownership/capability, target, spatial constraints, and resource cost in that
order. Lifecycle/system commands instead check required external authority,
terminal applicability, referenced player/team, and command-specific
conditions. Exact per-family lists are reserved for their feature milestones.
