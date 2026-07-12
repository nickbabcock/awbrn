# Repair: `move-repair`

Status: normative for the `move-repair` command — the Black Boat's manual repair
of one adjacent friendly unit — under AWBW ruleset revision `2026-07-10`, as
feature `repair-v1`. Movement is the shared prefix of `semantics/movement.md`;
visual HP is defined by `semantics/combat.md`; the repair capability relation is
`model/unit-capabilities.md` and
`rulesets/awbw/2026-07-10/unit-capabilities.json`. The relevant events are
`unit-moved`, `unit-resourced`, `funds-changed`, and `unit-repaired` in
`schema/event.schema.json`.

## Scope

`move-repair` moves a repair-capable unit and then services one named adjacent
friendly unit: it always refills that unit's fuel and ammo for free, and — funds
permitting — heals it one visual HP bar for a funds cost. In the AWBW profile
only the **Black Boat** carries the manual `repair` capability
(`unit-capabilities.json`).

This is the manual, mid-turn repair action. The `start-of-turn` automatic
terrain/property repair (owned cities, bases, and HQ healing the units on them)
and its `automatic-repair` event are defined by `semantics/turn-hooks.md`
(feature `turn-hooks-v1`); `move-repair` never emits `automatic-repair`.

Out of scope in this revision, deferred rather than guessed:

- Commander effects of any kind; repair here is commander-neutral, mirroring
  `combat-neutral-v1`. The heal cost uses the profile's base `cost`.
- Any repair target beyond a single named adjacent friendly unit.

Fog is no longer excluded. Feature `fog-observation-v1` (`semantics/fog.md`,
`model/observation.md`) specifies the projection of this command's events, and
the named target is a friendly unit, which `model/observation.md` always
discloses to the actor. A `repair-v1` fixture MAY set `settings.fog = true`; the
earlier requirement that it be false is withdrawn.

`move-repair` consumes no random token.

## Repair terms

For pre-state `S`, acting player `p`, command `{ player, unit, path, target }`,
acting unit `u = unit`, and target unit `t = target`:

- `u` is *repair-capable* when `kind(u)` has a `repair` entry in
  `unit-capabilities.json`. In the AWBW profile that is exactly `black-boat`,
  whose entry declares `relation: "adjacent"`, `exact_hp: 10`,
  `cost_percent: 10`, and `also_refills: ["fuel", "ammo"]`.
- After movement `u` rests at `d = destination(path)`. `t` must be a living
  on-board unit, friendly to `p` (`semantics/movement.md`), other than `u`, whose
  board position is orthogonally adjacent to `d` (Manhattan distance one).
- `visual-hp(hp) = ceiling(hp / 10)` (`semantics/combat.md`).
- `heal-cost(t) = cost(t) / 10`, the funds for one visual bar (ten percent of the
  target kind's `cost` from `units.json`). Every profile `cost` is a whole
  multiple of `1000`, so this is an exact integer.

All derived values are read from the authoritative pre-state and the state-bound
`Γ`; adjacency is computed against `d`, the post-move position.

## Validation and precedence

Malformed paths fail `command.schema.json` first. Otherwise `validate` returns
exactly one violation, extending the shared movement order of
`semantics/movement.md` with family-specific checks in its family slot:

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
ACTION_NOT_SUPPORTED   (the acting unit's kind has no repair capability)
INVALID_TARGET         (target is not a living allied on-board unit, or is u)
TARGET_OUT_OF_RANGE    (target is not adjacent to the post-move position)
DESTINATION_OCCUPIED
```

- `ACTION_NOT_SUPPORTED` (`action: "move-repair"`) when `kind(u)` is not
  repair-capable. A capability fact about the unit, not a target error.
- `INVALID_TARGET` (`target: target`) when `t` is unresolved, is not friendly, is
  not on the board, or is `u` itself. A Black Boat cannot repair itself.
- `TARGET_OUT_OF_RANGE` (`target: target`) when the Manhattan distance from `t`'s
  board position to `d` is not exactly one.
- `DESTINATION_OCCUPIED` applies normally: `move-repair` licenses no destination
  occupant, and the target sits on a tile adjacent to `d`, not on `d`. The
  repair target is therefore never the licensed occupant of the destination,
  unlike `move-load`/`move-join`.

Crucially, insufficient funds is **not** a validation failure. A `move-repair`
against a repairable target is legal even when the player cannot afford the heal;
execution resupplies but skips the heal in that case (below). Validation is pure,
mutates nothing, and requests no random token.

## Execution

Execution applies the movement prefix, then services `t`, atomically. Because the
Black Boat cannot capture, the movement prefix never emits `capture-changed`. In
order:

1. Move `u` along the actual path, subtracting `path-cost(Γ, u, A)` from its
   fuel and setting `u`'s action to `spent`; emit `unit-moved`. A fog trap
   suppresses the repair follow-up exactly as for any `move-*` command.
2. **Resupply (always, free).** Set `t.fuel = max-fuel(t)` and
   `t.ammo = max-ammo(t)`. If either actually changes, emit
   `unit-resourced { unit: t, fuel_before, fuel_after, ammo_before, ammo_after,
   reason: "unit-repair" }`.
3. **Heal (conditional, paid).** Let `vh = visual-hp(t.hp)`.
   - If `vh = 10` the target is already at full visual HP: no heal, no cost, and
     no `funds-changed`/`unit-repaired` event.
   - Otherwise let `hp' = min(vh + 1, 10) * 10`. Healing rounds the target's HP
     up to its current bar and then adds one bar, so a fractional-bar target may
     gain more than ten exact HP; this is the documented AWBW heal rounding and
     matches the visual-HP model of `semantics/combat.md`. If
     `heal-cost(t) <= S.players[p].funds`, then set
     `S.players[p].funds -= heal-cost(t)` and emit
     `funds-changed { player: p, from, to, reason: "unit-repair" }`; set
     `t.hp = hp'` and emit `unit-repaired { unit: t, from_hp, to_hp: hp',
     reason: "unit-repair" }`.
   - If `heal-cost(t) > S.players[p].funds`, the player cannot afford the heal:
     `t.hp` is unchanged and no `funds-changed`/`unit-repaired` is emitted. The
     free resupply of step 2 still stands. This "resupply without repair when
     broke" behavior is an intentionally modeled AWBW quirk.

Only the acting Black Boat ends its turn; `t`'s action state is unchanged. The
state remains in `unit-action`; repair introduces no victory checkpoint.

## Event ordering

| # | Event | Key fields | Emitted when |
| --- | --- | --- | --- |
| 1 | `unit-moved` | `unit: u`, `from`, `to`, `path: A`, `fuel_spent` | always |
| 2 | `unit-resourced` | `unit: t`, fuel/ammo before-after, `reason: "unit-repair"` | only when the target's fuel or ammo actually changes |
| 3 | `funds-changed` | `player: p`, `from`, `to`, `reason: "unit-repair"` | only when a paid heal occurs |
| 4 | `unit-repaired` | `unit: t`, `from_hp`, `to_hp`, `reason: "unit-repair"` | only when a paid heal occurs |

Payment precedes the heal fact, mirroring production's `funds-changed` before
`unit-created` (`semantics/production.md`). Resupply precedes heal because AWBW
services fuel and ammo before restoring HP. Events 3 and 4 are emitted together
or not at all.

## Evidence

Corroborated implementation:

- WarsWorld's repair handler (`src/shared/match-logic/events/handlers/repair.ts`)
  requires the acting unit to be a Black Boat and the adjacent target to be owned
  by the same player, resupplies the target unconditionally, heals one visual bar
  for `buildCost / 10` only when the target is below full visual HP and the
  player can afford it, and otherwise (its tested case) resupplies without
  repairing.
- AWBW Replay Player's `RepairUnitAction` refills the repaired unit's ammo and
  fuel to maxima, sets its HP to the post-repair value, and applies the replay's
  post-repair funds, with an optional preceding move.

Documentation-only:

- AWBW Wiki "Black Boat": the Black Boat repairs and resupplies an adjacent
  friendly unit, restoring one HP bar for a cost proportional to the repaired
  unit's value while also refilling fuel and ammunition.

Now specified elsewhere:

- Start-of-turn terrain/property repair and its `automatic-repair` event are
  defined by `semantics/turn-hooks.md` (feature `turn-hooks-v1`), not by this
  manual command.

Known deferral:

- Black Boat repair of multiple or self targets and commander cost modifiers
  require additional specification and are excluded rather than guessed,
  consistent with `model/phases.md` and `semantics/turn.md`. Fog-safe repair is
  no longer among them: `model/observation.md` projects the heal through the
  unit-fact rule and keeps the funds movement team-private.
