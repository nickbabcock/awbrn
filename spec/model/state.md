# Authoritative state model

An authoritative state is the complete semantic snapshot of one match at one
atomic instant. Given the ruleset, this snapshot contains every value needed by
`validate`, `execute`, `observe`, and `check-invariants`. It contains no UI
selection, animation, replay identifier, database key, wall-clock timer, or
external terrain encoding.

The normative machine shape is `schema/state.schema.json`; this document
defines meaning and cross-record invariants that JSON Schema cannot express.

## Identity and settings

`ruleset` identifies the immutable profile and revision used to interpret all
canonical identifiers. `settings` is an immutable match value conforming to
`settings.schema.json`. Settings live in state because commands and victory
checks depend on them; an implementation MAY intern them, but the semantic
input is the pair `(ruleset, settings)` recorded by the state.

Identifier domains are distinct even when their JSON representations match.
Implementations MUST NOT interchange player, team, commander, unit-kind,
terrain, teleporter, trait, reason, or ruleset IDs merely because those domains
use constrained strings on the wire. Unit instances are different: `UnitId` is
an opaque unsigned 32-bit integer. Its numeric value has no game meaning beyond
equality, stable ordering, and deterministic allocation.

`settings.lab_units` is the unique array of valid unit-kind IDs whose production
requires the producing player to own at least one Lab. It is not an on/off
switch. Unit bans, Lab-gated production, and commander-slot bans constrain
production and initial commander selection. They do not make an otherwise
well-formed snapshot invalid: imported or predeployed states may contain a
Lab-gated or banned unit, and commander changes are not yet specified. A match
creator MUST validate initial choices separately.

## Board and tiles

`board.tiles` is a row-major grid: `tiles[y][x]`. Each tile has a canonical
semantic `terrain` identifier. Mutable fields occur only when licensed by the
terrain kind's traits:

- `owner` is present on ownable terrain and is either a player ID or `null`;
- `capture_points` is present on capturable terrain and is in `[1,20]`;
- `silo` is present on a silo and is `ready` or `spent`;
- `destructible_hp` is present on a living destructible object and does not
  exceed that terrain profile's `destructible.maximum_hp`;
- `teleporter` names the association of a teleporter endpoint; and
- `trait_state` is a namespaced escape hatch for a ruleset-declared tile trait.

Unknown ad-hoc keys are forbidden. A ruleset revision defining `trait_state`
MUST define its key, value shape, initialization, and invariants. Fields not
licensed by a terrain trait MUST be absent.

The AWBW `teleporter` terrain uses contiguous zero-cost traversal rather than
linked endpoints, so its tiles MUST omit the optional `teleporter` association.
No living on-board unit may occupy an AWBW teleporter tile. Movement and unload
validation preserve this invariant by refusing those tiles as destinations.

Capture progress belongs to the tile, not the unit. It persists while the same
capturing foot soldier remains on the property (including when another unit
joins into it), and resets when the persistence conditions in
`semantics/capture-reset.md` fail. `capture_points < 20` asserts that exactly
one eligible on-board capturing unit is at that position; identity is derived
rather than stored.

## Players, teams, and commanders

Teams are stable records. A team has one or more member players. Team membership
is expressed once, by each player's `team`; `teams` contains team lifecycle
state and not a duplicate member list.

A player records funds, lifecycle `status`, and one or two commander slots.
Exactly one commander slot is `active`. Non-tag games have one slot; tag games
have two. Only the active commander supplies day-to-day effects and can have an
active power. Each slot owns its exact nonnegative `power_charge` and
`power_uses`; the applicable commander profile derives current COP/SCOP costs
and maximum charge. Charge MUST NOT exceed that derived maximum.

`power_state` records whether no power, COP, or SCOP is active. An active power
MUST name the active commander slot. Power lifetime and tag-switch effects are
defined by `semantics/powers.md` and `semantics/tag.md`.

Player status is `active`, `resigned`, `timed-out`, or `eliminated`. Only active
players participate in turn order. Elimination cause is history and belongs in
events, not current player state.

## Turn and weather

`turn` records the one-based day, active player, zero-based position in the
stable `order`, and phase. See `model/phases.md`. The active player MUST equal
`order[position]`. Turn order contains every player exactly once; successor
selection skips inactive players without mutating the stable order. How teams
affect ordering is fixed when a match is initialized and is not inferred from
array order.

Weather has a current canonical kind and a nonnegative `remaining_turns`.
`remaining_turns` counts player-turn boundaries after the current turn before
the override expires; `0` means no temporary override remains. This is a state
representation, not a claim about every weather source. Fixed settings are
represented by their current weather with zero remaining turns. Under the
random setting, zero means the next eligible selected player-turn consumes an
explicit weather outcome (`semantics/turn-hooks.md`). Olaf's power-created snow
and its owner-next-turn countdown are specified by `semantics/powers.md`.

## Units and locations

Living units are present in `units`; destroyed and joined-away units are absent
and survive only in events/history. A unit has a stable ID, kind, owner, exact
HP in exact integer points (`1..100`), fuel, ammo, action state,
concealment state, and exactly one location:

```json
{ "type": "board", "position": [4, 7] }
```

or:

```json
{ "type": "cargo", "transport": "u17", "slot": 0 }
```

`ready` means eligible for an ordinary action this turn, `moved` means movement
has been committed but a follow-up choice is pending, and `spent` means no
ordinary action remains. `immobilized` means a ruleset effect has reserved the
unit's next action normalization: at that owner's next `turn-start` it becomes
`spent`, consuming that turn, and at the following ordinary normalization it
becomes `ready`. This is intentionally not a boolean. Only units owned by the
active player may be `moved`; at most one unit may be `moved`; and that state is
valid only in `unit-action` phase.

Concealment is `exposed` or `hidden`. It is authoritative voluntary state, not
an observer's visibility result. Terrain concealment and fog visibility are
derived by `observe`.

Fuel and ammo MUST be within the effective maxima derived from unit kind and
ruleset effects. An ammo-less kind has exact ammo `0`.

## Unit identifier allocation

`next_unit_id` is an optional nonnegative integer recording the next identifier
a feature that spawns units will allocate. It exists so that unit creation is a
pure function of state: an allocated `UnitId` is the unsigned 32-bit integer
`next_unit_id`, and the spawning transition increments the counter. This
avoids host-generated identifiers, which would break determinism, and it never
reuses a departed unit's identifier, unlike deriving an identifier from the live
maximum.

When present, `next_unit_id` MUST exceed every live numeric unit ID, so the next
allocation and every larger one are guaranteed fresh. Allocation is
inadmissible when incrementing would exceed `4294967295`. States for features
that never spawn units MAY omit the field; a feature that allocates identifiers
(such as production) requires it and treats its absence as an inadmissible
pre-state rather than choosing an identifier by another rule.

## Cargo invariants

Cargo is represented only by a cargo unit's location; transports do not carry a
second list. For every cargo location:

- the transport exists, is not the same unit, and has a board or cargo location
  allowed by the ruleset;
- owner and transport owner are equal;
- the slot is below the transport's capacity and accepts the cargo kind;
- no two cargo units use the same `(transport, slot)` pair;
- occupied slots are dense from zero through `n-1`; and
- following transport references is acyclic and respects ruleset nesting.

AWBW currently has no nested transport capability, so AWBW cargo transports
MUST be on the board. Cargo units do not occupy board positions and do not
participate directly in board-position uniqueness.

## Match status and outcome

`match.status` is `active` or `finished`. An active match has no `outcome` and
records `draw_offers`, the unique IDs of active players currently consenting to
a draw. A finished match has exactly one outcome. The outcome is a single union:

- `victory` names one or more winning teams and a reason: `rout`, `hq-capture`,
  `lab-capture`, `capture-limit`, `day-limit`, `resignation`, or `timeout`;
- `draw` names zero or more tied teams and a reason: `day-limit`, `agreement`,
  or `no-contest`; or
- `cancelled` has a stable machine-readable reason.

Multiple winning/tied teams allow ruleset-defined alliances without rewriting
the result shape. The command/event work must settle exact lab victory,
  multi-player elimination, resignation, timeout, and tie transitions before
those reducers become normative.

## Additional cross-record invariants

In addition to `model/invariants.md`:

- IDs are unique within their category; team and player references resolve;
- every tile owner and unit owner resolves to a player;
- dimensions exactly match the tile grid;
- no two board units share a position;
- a finished match has no active turn phase and an active match does;
- winning and tied team IDs resolve and contain no duplicates; and
- all trait-, commander-, and unit-derived constraints above hold.

Schema validity is necessary but not sufficient. `check-invariants` MUST check
these relational and ruleset-derived conditions.

## Deferred evidence requirements

The representation deliberately permits values whose transition rules are not
yet settled. Before implementing reducers, obtain authoritative or replay-backed
evidence for: the exact integer power charge/cap formula; lab and
multi-team victory behavior; and capture completion/ownership-transfer ordering.
