# Capture interruption and reset

Status: normative for interruption/reset triggers and authoritative event
ordering under AWBW ruleset revision `2026-07-10`. Capture attempts,
completion, ownership transfer, and capture victory remain in the later
capture/economy milestone.

## Representation and derived capturer

A capturable tile always stores `capture_points` in `[1,20]`. `20` means no
capture is in progress. A value below `20` means one on-board capture-capable
unit at that position is the **current capturer**. Its identity is derived from
tile position and is not stored on either record.

For every tile with `capture_points < 20`, `check-invariants` MUST require
exactly one on-board unit at that position, owned by a player other than the
tile owner (with `null` treated as no owner), whose kind has the `capture`
capability. Cargo cannot be a current capturer.

## Persistence condition

Capture progress persists across a transition if and only if all of these are
true after the transition:

1. the same tile is still the same capturable property;
2. its owner has not changed;
3. the pre-transition current capturer still exists;
4. that same unit remains on-board at that tile; and
5. the unit remains eligible to be its current capturer.

Action-state changes, turn boundaries, HP changes that do not destroy the unit,
resupply, repairs, and joining do not change the stored progress. HP and join
results affect only the capture amount applied by a future `move-capture`.

If any persistence condition becomes false without completing the capture, the
transition MUST set the tile's `capture_points` to `20` exactly once. Reset is
not conditional on the old value's amount and consumes no randomness.

## Command triggers

The following transitions reset an in-progress capture:

- the current capturer actually moves away from the tile, including movement
  preceding wait, attack, capture elsewhere, load, join elsewhere, supply,
  repair, concealment, launch, or explode;
- the current capturer becomes cargo;
- the current capturer is destroyed, deleted, or otherwise removed from current
  state;
- the current capturer's owner changes or the unit loses capture eligibility;
- the tile ceases to be the same capturable property; or
- the tile owner changes for a reason other than this capturer completing its
  capture.

An intended move that is fog-trapped before leaving `p_0` does not reset
progress: its actual path contains only `p_0`, so the capturer remains. If it
enters at least one tile before being trapped, progress resets.

A one-position `move-wait` does not reset progress. It spends the unit while
leaving the current capture amount unchanged. Continuing capture requires a
later `move-capture` command; waiting is not an implicit capture attempt.

## Join and damage

Joining units does not modify the property's current `capture_points` and emits
no `capture-changed` event. In a valid state, another unit may join into the
current capturer; the capturer is the stationary surviving target, remains on
the property, and retains the existing progress unchanged. The arriving
source's HP may change the surviving unit's HP, but that matters only when a
later capture command computes new progress.

Likewise, nonlethal damage does not modify `capture_points` and emits no
`capture-changed` event. The capturer's reduced HP changes only the amount
subtracted by a future capture command. Lethal damage is different solely
because it removes the current capturer; that removal interrupts and resets the
capture.

A joining source that moves away from a different in-progress capture resets
that origin because it left the property, not because joining intrinsically
changes capture progress.

These rules make the “persists through join” AWBW behavior precise without
moving capture state onto a unit.

## Authoritative event order

A reset emits exactly one event:

```json
{
  "type": "capture-changed",
  "position": [4, 7],
  "from": 10,
  "to": 20
}
```

The event is emitted immediately after the reset mutation and before the event
that makes the capturer's absence/location change observable.

Normative order by transition family:

```text
move away:             capture-changed, unit-moved, follow-up events
move then fog trap:    capture-changed, unit-moved, movement-trapped
load without movement: capture-changed, unit-loaded
join into capturer:    units-joined (no capture-changed)
join after move away:  capture-changed, unit-moved, units-joined
nonlethal damage:      unit-damaged (no capture-changed)
delete/removal:        capture-changed, unit-removed
lethal damage:         unit-damaged, capture-changed, unit-removed
tile replacement:      capture-changed, then the tile-change event
```

For a movement command, reset is based on the **actual** path, not the intended
path. It occurs only when that path has at least two positions. Fuel, position,
and action-state changes remain part of the subsequent `unit-moved` fact.

For lethal damage, `unit-damaged` first records the cause and zero resulting
HP. The reset then records the interrupted property fact, and `unit-removed`
records removal from current state. Non-damage removals start with
`capture-changed` because the removal itself is the interruption cause.
`units-joined` entails removal of its source unit, so no redundant
`unit-removed` event follows it. A join emits `capture-changed` only when its
movement prefix caused the source to leave a separate capture in progress.

If one atomic effect interrupts multiple captures, tiles are reset and events
emitted in canonical board order `(y,x)` before continuing to later effect
events. A valid pre-state has at most one current capturer per tile.

Completion is not a reset. A capture-completion specification will define the
order among decrement to zero, owner transfer, restoration to `20`, elimination,
and victory events; implementations MUST NOT apply this reset rule as a
substitute for that sequence.

## Evidence

Documentation-only:

- AWBW capture rules establish that interruption restores a property's full
  capture value and that progress continues while the capturing foot soldier
  remains.

Confirmed replay/model behavior:

- AWBW replay records expose property capture state before and after movement,
  removal, and join transitions.
- `AWBWXmlReplayParser` reconstructs an incomplete property as `20` when the
  unit above changes, and preserves partial progress when the same unit kind
  and owner remain; this supports persistence through join but cannot alone
  distinguish unit identity.

Corroborated implementation:

- AWBW Replay Player's `MoveUnitAction.SetupAndUpdate` restores the building to
  `20` before applying the movement result.
- WarsWorld clears a moving unit's capture progress before changing its board
  position.
