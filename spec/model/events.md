# Authoritative events

Status: normative envelope for specification version 0.1.0. Only `move-wait`
and phase-boundary event behavior is currently executable; the other event
types reserve the facts needed by command and automatic-effect specifications.

## Purpose and ordering

An accepted execution returns an ordered finite sequence `E` of authoritative
events. An event records one semantic fact of the transition. It is not a
command, state patch, replay database record, log message, animation request,
or notification. Events contain no timestamps, prose, database identifiers,
or full unit/state snapshots.

Event order is normative. An event describes the state change that has already
occurred at that point in the transition. Rejection emits no events. Once
`match-completed` is emitted, execution stops and no later event is permitted.

The closed 0.1.0 union is defined by `schema/event.schema.json`:

| Family | Event types |
| --- | --- |
| control | `phase-changed`, `turn-selected`, `day-advanced` |
| movement/action | `unit-moved`, `movement-trapped`, `unit-action-changed`, `unit-created`, `unit-removed`, `unit-damaged`, `unit-repaired`, `unit-resourced`, `unit-loaded`, `unit-unloaded`, `units-joined`, `concealment-changed` |
| board/economy | `tile-owner-changed`, `tile-terrain-changed`, `capture-changed`, `silo-changed`, `destructible-damaged`, `funds-changed` |
| combat/powers | `attack-resolved`, `area-strike-resolved`, `power-activated`, `power-ended`, `power-charge-changed`, `commander-swapped` |
| automatic/random | `weather-changed`, `random-outcome`, `automatic-supply`, `automatic-repair` |
| lifecycle | `draw-offer-changed`, `player-status-changed`, `team-eliminated`, `match-completed` |

The inventory is deliberately fact-oriented. For example, combat may emit an
`attack-resolved` choice followed by damage, resource, removal, elimination,
and completion events; it does not emit an opaque combat result object.
Feature specifications fix which reserved events occur and their exact order.

## Exact-HP change facts

`unit-damaged` and `unit-repaired` are the two exact-HP change facts and are
mirror images: both carry `unit`, `from_hp`, `to_hp`, and `reason`. A repair's
`to_hp` is at least `1` because healing never removes a unit, whereas
`unit-damaged` alone may reach `to_hp: 0` (see `semantics/combat.md`).
`unit-repaired` records a player-commanded heal such as the Black Boat repair of
`semantics/repair.md`; `automatic-repair` remains reserved for the deferred
start-of-turn terrain/property repair hook (`model/phases.md`) and is never
emitted by a player command. Fuel and ammo restoration is a separate
`unit-resourced` fact, so a repair that both heals and refills emits both, and a
`units-joined` merge asserts the survivor's combined HP, fuel, ammo, and spent
action state without restating them in the event.

## Tile identity and ownership facts

`tile-owner-changed` records a change of a tile's `owner` while its terrain kind
stays the same. `tile-terrain-changed` records a change of the tile's `terrain`
identifier itself and carries a `reason`, because a tile's kind determines its
traits, defense, and income and therefore cannot be inferred from an ownership
fact. The two are independent: a transition that both re-kinds and re-owns a
tile emits `tile-terrain-changed` first and then `tile-owner-changed`, so every
consumer sees the tile's new identity before its new owner. The only transition
in this revision that re-kinds a tile is the elimination cascade's demotion of a
`capture-defeats-owner` property (`semantics/elimination.md`).

## Facts that declare a reason

Eleven event types carry a `reason`, a ruleset reason identifier naming why the
fact occurred: `unit-action-changed`, `unit-removed`, `unit-damaged`,
`unit-repaired`, `unit-resourced`, `tile-terrain-changed`, `funds-changed`,
`power-charge-changed`, `weather-changed`, `player-status-changed`, and
`team-eliminated`. Every other type omits it, and projection substitutes the
event's own `type` where an observed element needs one (`model/observation.md`).

An event declares a reason when its own payload cannot be re-derived from state
and the surrounding stream. `player-status-changed` is in the list for that
reason: a player eliminated while a teammate is still participating emits
neither `team-eliminated` nor `match-completed`, so without its own `reason` the
cause of that player's departure would leave no trace. Its value is the
elimination cause fixed by `semantics/elimination.md`, and agrees with the
`reason` of any `team-eliminated` or `match-completed` that follows in the same
transition.

## `move-wait`

An unobstructed accepted `move-wait` emits exactly one `unit-moved` event:

```json
{
  "type": "unit-moved",
  "unit": "u1",
  "from": [1, 2],
  "to": [3, 2],
  "path": [[1, 2], [2, 2], [3, 2]],
  "fuel_spent": 2
}
```

This event also entails the command's `ready` to `spent` action transition; a
separate `unit-action-changed` event is not emitted for ordinary `move-wait`.
A one-position wait has equal `from` and `to`, retains the one-position path,
and has the normatively computed `fuel_spent` (normally zero).

When movement interrupts capture, `capture-changed` precedes `unit-moved` as
defined by `semantics/capture-reset.md`.

Fog-trap movement emits the actual `unit-moved` prefix followed by
`movement-trapped`, as defined by the movement specification. `movement-trapped`
projects to `movement-stopped` for the actor's team and is omitted for every
other recipient (`model/observation.md`).

## `move-launch`

For AWBW revision `2026-07-10`, a launch emits movement events first, followed
by `area-strike-resolved`, then one `unit-damaged` event per affected board
unit in ascending stable unit-ID order, and finally `silo-changed`. The area
strike is radius 3 and deals 30 exact HP with a nonlethal floor of 1. It
includes allied and enemy board units, but not cargo, and grants no power
charge. Event projection applies the ordinary before/after visibility rules
to each unit fact, so a hidden unit's damage is not disclosed by the launch.

## `move-explode`

For AWBW revision `2026-07-10`, a Black Bomb explosion emits movement events,
one `area-strike-resolved` event for radius 3 and 50 exact HP damage, damage
events for other affected board units in ascending stable unit-ID order, and
then `unit-removed` for the bomb. Damage is nonlethal with a floor of 1 HP;
cargo is excluded. The explosion is not a unit strike and grants no power
charge. If the removal leaves the owner without units, the normal rout and
match-completion events follow.

## `delete-unit`

`delete-unit` removes a ready, owned, on-board unit without compensation. If it
is standing on an incomplete capture, `capture-changed` restores that property
to 20 before `unit-removed`. If the owner has no units remaining, the normal
rout and match-completion events follow. Deletion grants no power charge and
its removal is projected through the ordinary fog visibility rules.

## Phase boundaries

Every actual phase mutation emits `phase-changed` with `from`, `to`, and the
player whose turn control state is changing. `end-turn` therefore first emits
`unit-action` to `turn-end`. If play continues, successor selection emits
`turn-selected`; a wrap additionally emits `day-advanced`; entering the next
automatic phase emits `turn-end` to `turn-start`. Completion instead emits
`match-completed`, which entails the transition to `finished`; no separate
`phase-changed` accompanies it, so a command-immediate victory and a
boundary-loop victory produce the same terminal fact
(`semantics/elimination.md`).

Automatic-hook fact events occur between the surrounding phase events in the
same order as their state mutations. The precise ordering of hook-local events
is owned by each feature specification.

## Random outcomes

Random tokens themselves are execution input and are not events.
`random-outcome` records the semantic choice produced by a consumed token with
a ruleset-defined `kind` and closed scalar `outcome`. A feature-specific event
then records any resulting mutation. This makes normatively different random
transitions distinguishable without exposing an RNG, seed, or token index.

## Recipient projection

`E` is authoritative and may disclose hidden units, private funds, or concealed
effects. It MUST never be sent directly to a player. The separate pure function

```text
observe-events(R, S, S', E, player) -> E_player
```

may retain an event, redact fields, replace it with a less-specific observed
event, combine facts, or omit it. It preserves the relative order of retained
facts and consumes no randomness. Recipient events are not members of the
authoritative event schema unless unchanged.

Their closed schema is `schema/observed-event.schema.json` and the projection of
every event type in the table above is normative under feature
`fog-observation-v1`, defined by `model/observation.md` and
`semantics/fog.md`.

Adding an authoritative event type or changing a payload requires a
specification minor version. Implementations MUST reject unknown event types
rather than treating them as ignorable patches.
