# Capture and `move-capture`

Status: normative for the `move-capture` command, property capture progress,
completion and ownership transfer, and decisive HQ-capture victory under AWBW
ruleset revision `2026-07-10`, as feature `capture-v1`. Interruption and reset
of an in-progress capture are defined by `semantics/capture-reset.md` and are
not restated here; this document defines the *attempt* and its *completion*,
the sequence that `capture-reset.md` deliberately left to "the later
capture/economy milestone."

## Scope

`move-capture` moves a capture-capable unit along the shared movement prefix and
then attempts to capture the property at the path destination. This feature
covers commander-neutral capture arithmetic, city/base/airport/port ownership
transfer, and the command-immediate `hq-capture` victory.

Out of scope in this revision, deferred rather than guessed:

- The base `capture-v1` fixtures are commander-neutral. Revisioned capture
  multipliers and instant capture are defined by
  `model/commander-profiles.md` and claimed only by narrower
  `commander-effective-values-v1.<commander>.<state>` fixtures.
- `capture-limit` victory. The checkpoint position is fixed by
  `model/phases.md`, but this feature does not drive a match to `finished` by
  property count.
- `lab-capture` victory and any elimination that depends on lab possession.
- The elimination *cascade* — disposing of an eliminated player's remaining units
  and properties. Under `capture-v1` alone, an `hq-capture` conformance fixture
  MUST place the losing player in a state where the captured HQ is that player's
  only property and the player owns no unit, so elimination has no cascade to
  resolve. Feature `elimination-v1` (`semantics/elimination.md`) specifies the
  cascade and lifts that restriction as a strict superset of this feature;
  simultaneous multi-player elimination precedence remains deferred there,
  exactly as `model/phases.md` leaves it.

Capture consumes no random token.

## Eligibility and terms

- The `capture` capability is closed-world: only `infantry` and `mech` may
  perform `move-capture` (`unit-capabilities.json`). Capability MUST NOT be
  inferred from any other statistic.
- A *capturable property* is a tile whose terrain kind carries the `capturable`
  trait: `city`, `base`, `airport`, `port`, `hq`, `com-tower`, and `lab`.
- `visual-hp(hp) = ceiling(hp / 10)`, the same display-HP function used by
  `semantics/combat.md`. A full-health foot soldier has `visual-hp = 10`.
- `points(d)` is the destination property's stored `capture_points`, always in
  `[1,20]` (`model/state.md`). `20` means no capture is in progress; a value
  below `20` means the acting unit is already this tile's current capturer, per
  `semantics/capture-reset.md`.
- Two players are *hostile* when they are on different teams. A property is
  capturable by unit `u` only when its `owner` is `null` or a player hostile to
  `u`'s owner.

## Validation and precedence

`move-capture` carries `player`, `unit`, and `path`; it names no separate
target, because the captured property is `destination(path)`. Malformed paths
fail `command.schema.json` first. Otherwise `validate(R, S, C)` returns exactly
one violation, extending the shared movement order of `semantics/movement.md`
with capture family-specific checks:

```text
AUTHORITY_REQUIRED
MATCH_FINISHED
WRONG_PHASE
NOT_ACTIVE_PLAYER
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
ACTION_NOT_SUPPORTED          (unit kind lacks the capture capability)
INVALID_TARGET                (destination is not a property capturable by this unit)
DESTINATION_OCCUPIED
```

- `ACTION_NOT_SUPPORTED` (`action: "capture"`) when the acting unit's kind is not
  in the `capture` capability set. This is a unit fact and outranks the
  target check.
- `INVALID_TARGET` (`target: destination`) when the destination terrain is not a
  `capturable` property, or when its `owner` is the acting player or any player
  on the acting player's team. A friendly or already-owned property cannot be
  captured.
- Capture eligibility does not require `visible-position` at the destination.
  Terrain is map data, and `move-capture` names no enemy unit identifier. A unit
  can move onto and capture a property that was fogged before the command.
- `DESTINATION_OCCUPIED` is still evaluated last, because a capturable
  destination may be blocked by another unit; capture does not license an
  occupied destination.

Validation never mutates state and requests no random token. The captured
property is not disclosed through any rejection payload beyond the coordinate
the command already contains.

## Execution

Execution is atomic. It first applies the shared movement prefix of
`semantics/movement.md`, which moves the unit to `destination(path)`, subtracts
fuel, and — when the actual path has at least two positions — resets any capture
the unit had in progress at its origin and emits that `capture-changed` first,
per `semantics/capture-reset.md`. A one-position `move-capture` continues a
capture already in progress on the same tile and performs no origin reset.

The movement prefix emits one `unit-moved` and entails the unit's `ready`-to-
`spent` transition; no separate `unit-action-changed` event is emitted, exactly
as for `move-wait`. A fog trap on the way to the property suppresses the capture
follow-up entirely, per `semantics/movement.md`; the unit does not reach the
property and captures nothing.

When the unit reaches the property, let `d = destination(path)`, let
`before = points(d)`, and let `strength = effective-capture(Γ, u,
visual-hp(u.hp))`; then `after = before - strength`. The HP used is the acting
unit's health after movement, which movement does not change.

### Partial capture (`after > 0`)

1. Set `capture_points(d) = after`.
2. Emit `capture-changed { position: d, from: before, to: after }`.

The unit is now `spent`, remains the tile's current capturer, and the state
stays in `unit-action`. Because `visual-hp` is in `[1,10]` and `before` is in
`[1,20]`, a stored partial result is always in `[1,19]`, satisfying the tile
invariant.

### Completion (`after <= 0`)

Completion transfers ownership and restores the property. In order:

1. Emit `capture-changed { position: d, from: before, to: 0 }`. The stored value
   never becomes `0`; `0` is the completion fact carried only by the event.
2. Set `d.owner = u.owner` and emit
   `tile-owner-changed { position: d, from: previous-owner, to: u.owner }`.
3. Set `capture_points(d) = 20` and emit
   `capture-changed { position: d, from: 0, to: 20 }`. Restoration is required:
   a property owned by the capturing player MUST NOT store a below-`20` value.

The unit is `spent` and remains on the property. If `d` is not an HQ, execution
stops here and the state stays in `unit-action`.

### Decisive HQ capture

`d` is an HQ exactly when its terrain kind carries the `capture-defeats-owner`
trait (in the AWBW profile, `hq`). When completion transfers an HQ away from a
non-`null` previous owner `p`, execution continues immediately, before returning
to `unit-action`:

4. Set `S.players[p].status = eliminated` and emit
   `player-status-changed { player: p, from: active, to: eliminated, reason:
   "hq-capture" }`.
5. If every player on `p`'s team is now eliminated, mark the team eliminated and
   emit `team-eliminated { team: p.team, reason: "hq-capture" }`.
6. Evaluate the command-immediate victory checkpoint. When exactly one team
   still has an active player, that team wins: set `S.match.status = finished`,
   `S.match.outcome = { type: "victory", winners: [winning-team], reason:
   "hq-capture" }`, and `S.turn.phase = finished`. Emit
   `match-completed { outcome }`. Execution stops; no turn hook, successor
   selection, or later event occurs (`model/phases.md`).

Steps 4 through 6 are exactly steps 1 through 3 of the shared procedure in
`semantics/elimination.md`, invoked with cause `hq-capture` and `u.owner` as
beneficiary. Under feature `elimination-v1` that procedure continues past step 6
with the cascade whenever two or more teams still survive; under `capture-v1`
alone the required fixture state leaves the cascade empty, so the two features
agree on every `capture-v1` case.

This ordering satisfies the completion contract reserved by
`semantics/capture-reset.md`: decrement to zero, owner transfer, restoration to
`20`, elimination, then victory. Reset (returning an *interrupted* property to
`20`) is a different transition and MUST NOT be substituted for this sequence.

### Lab substitution

Feature `lab-capture-v1` applies only when the authoritative board contains no
`hq` terrain. On such a map, a completed capture of an enemy-owned `lab`
eliminates its previous owner exactly when that player owns no other Lab after
the ownership transfer. Capturing one of several Labs is nonlethal. On a board
containing any HQ, Lab ownership changes never eliminate a player.

A decisive Lab uses the shared elimination cascade with cause `lab-capture`,
the capturing player as beneficiary, and the captured Lab as the trigger
position. Unlike an HQ, the Lab has no elimination replacement and remains a
Lab.

### Capture limit

Feature `capture-limit-v1` applies after every completed ownership transfer
when `settings.capture_limit` is non-null. Count only tiles owned by the
capturing player whose terrain carries `counts-toward-capture-limit`. In this
profile those are Cities, Bases, Airports, Ports, and HQs. Labs and Com Towers
do not count.

When the completed capture itself is of a counting terrain and the new total is
at least the limit, finish the match immediately with the capturing player's
team as the sole winner and reason `capture-limit`. Do not eliminate other
players or transfer their remaining assets. This checkpoint precedes HQ and
Lab elimination when the same capture could satisfy both conditions.

## Event ordering

For a capture that completes an enemy HQ and ends the match, the full ordered
stream is:

| # | Event | Emitted when |
| --- | --- | --- |
| 1 | `capture-changed` (origin reset) | only when the actual path has ≥2 positions and the unit was capturing its origin |
| 2 | `unit-moved` | always (movement prefix; entails `ready`→`spent`) |
| 3 | `capture-changed` (`before`→`0`) | on completion; partial captures instead emit a single `before`→`after` here and stop |
| 4 | `tile-owner-changed` | on completion |
| 5 | `capture-changed` (`0`→`20`) | on completion |
| 6 | `player-status-changed` (`reason: "hq-capture"`) | only on HQ completion with a non-`null` previous owner |
| 7 | `team-eliminated` | only when that player's whole team is now eliminated |
| 8 | `match-completed` | only when exactly one team remains active |

A partial capture emits rows 1–2 then a single terminal `capture-changed`
(`before`→`after`). A non-HQ completion emits rows 1–5 and stops. Row 3's `to`
is `0`, permitted by `schema/event.schema.json` even though `0` is never a
stored `capture_points` value.

## Victory checkpoints

`hq-capture`, `lab-capture`, and `capture-limit` are command-immediate
checkpoints. After the three capture facts, check capture limit first; if it
does not finish the match, check decisive HQ/Lab ownership loss and run the
shared elimination cascade. A capture-limit finish appends only
`match-completed`; a decisive Lab appends the same elimination event family as
an HQ, with reason `lab-capture`.

## Evidence

Corroborated implementation:

- WarsWorld's capture (`src/shared/match-logic/events/handlers/ability.ts`,
  `willCaptureTile`) subtracts the capturing unit's visual HP from the property's
  points, defaulting to `20`, and completes at `<= 0`. Its 1.5× and instant
  variants are commander effects and are excluded here.
- WarsWorld eliminates the previous owner on HQ capture
  (`eliminationReason: "hq-or-labs-captured"`), corroborating that a decisive HQ
  capture ends the owner's participation.
- The same handler checks capture limit before HQ/Lab loss, treats a Lab as
  lethal only when its previous owner has no HQ and no other owned Lab remains,
  and labels the two outcomes separately as property-goal and HQ/Lab capture.

Documentation-only:

- AWBW capture rules: only foot soldiers capture; a property has 20 capture
  points; a capturing unit reduces points by its displayed HP each turn;
  capturing an enemy HQ wins the game.
- AWBW Wiki `Lab`, `Properties`, and `Advance Wars Overview`: Labs substitute
  for HQs only on maps without HQs, every owned Lab must be lost, and Capture
  Limit counts HQs, Cities, Bases, Airports, and Ports while excluding Labs and
  Com Towers.

Confirmed model behavior:

- `AWBWXmlReplayParser` and AWBW Replay Player restore an interrupted property to
  `20` and preserve partial progress while the same capturer remains, as already
  cited by `semantics/capture-reset.md`.

The shared elimination cascade is specified by `semantics/elimination.md`.
