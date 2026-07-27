# Supply: `move-supply`

Status: normative for the `move-supply` command — the APC's manual adjacent
resupply of fuel and ammunition — under AWBW ruleset revision `2026-07-10`, as
feature `supply-v1`. Movement is the shared prefix of `semantics/movement.md`;
capture interruption is `semantics/capture-reset.md`; the supply capability
relation is `model/unit-capabilities.md` and
`rulesets/awbw/2026-07-10/unit-capabilities.json`. The relevant events are
`unit-moved` and `unit-resourced` in `schema/event.schema.json`.

## Scope

`move-supply` moves a supply-capable unit and then refills the fuel and ammo of
every eligible adjacent unit to that unit's maxima. In the AWBW profile only the
**APC** carries a manually triggerable adjacent supply capability, and its
`targets: "owned-units"` relation restricts recipients to the APC's owner.

This feature is the *manual, mid-turn* supply action, distinct from the
`start-of-turn` automatic supply recorded by the same capability's `trigger`
field. The automatic start-of-turn supply, the Cruiser/Carrier `cargo`-relation
supply, and the corresponding `automatic-supply` event are defined by
`semantics/turn-hooks.md` (feature `turn-hooks-v1`); `move-supply` never emits
`automatic-supply`.

Out of scope in this revision, deferred rather than guessed:

- Commander effects of any kind; supply here is commander-neutral, mirroring
  `combat-neutral-v1`.
- Any resupply cost or HP effect. Manual supply is free and never changes HP.

Fog is no longer excluded. Feature `fog-observation-v1` (`semantics/fog.md`,
`model/observation.md`) specifies both the projection of this command's events
and the movement prefix's trap, and manual supply raises no command-knowledge
question of its own: the supply set contains only owned or friendly units, which
`model/observation.md` always discloses to the actor. A `supply-v1` fixture MAY
set `settings.fog = true`; the earlier requirement that it be false is
withdrawn.

`move-supply` consumes no random token.

## Supply terms

For pre-state `S`, acting player `p`, command `{ player, unit, path }`, and
acting unit `u = unit`:

- `u` is *supply-capable* when `kind(u)` has a `supply` entry with
  `relation: "adjacent"` in `unit-capabilities.json`. In the AWBW profile that
  is exactly `apc`. The Cruiser and Carrier entries use `relation: "cargo"` and
  are not triggerable by `move-supply`.
- After movement `u` rests at `d = destination(path)`. The *supply set* is the
  set of living on-board units, other than `u`, whose board position is
  orthogonally adjacent to `d` (Manhattan distance one) and whose owner satisfies
  the capability's `targets` relation. `owned-units` requires `owner(v) =
  owner(u)`; `friendly-units` allows any owner on `u`'s team
  (`semantics/movement.md`). The AWBW profile uses `owned-units`; the broader
  value is reserved for custom rulesets.
- A supplied unit's fuel becomes `max-fuel`, its ammo becomes `max-ammo`
  (`0` for an ammo-less kind), both read from `units.json`.

All derived values are read from the authoritative pre-state and the state-bound
`Γ`; the supply set is computed against `d`, the post-move position.

## Validation and precedence

The command carries no target. Malformed paths fail `command.schema.json` first.
Otherwise `validate` returns exactly one violation, extending the shared movement
order of `semantics/movement.md` with a single family-specific action check:

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
ACTION_NOT_SUPPORTED   (the acting unit's kind has no adjacent supply capability)
DESTINATION_OCCUPIED
```

- `ACTION_NOT_SUPPORTED` (`action: "move-supply"`) when `kind(u)` is not
  supply-capable as defined above. This is a capability fact about the unit, not
  a target error, so it uses the capability code rather than `INVALID_TARGET`.
- `DESTINATION_OCCUPIED` applies normally: unlike `move-load` and `move-join`,
  `move-supply` licenses no destination occupant, so a unit resting on `d`
  rejects the move at the shared destination check.
- A `move-supply` whose supply set is **empty** is still legal; it resupplies
  nobody and simply ends the APC's turn like a `move-wait`. A `supply-v1`
  conformance fixture SHOULD nevertheless supply at least one unit so the effect
  is observable.

Validation is pure, mutates nothing, and requests no random token.

## Execution

Execution applies the movement prefix, then resupplies, atomically. Because the
APC cannot capture, the movement prefix never emits `capture-changed`. In order:

1. Move `u` along the actual path, subtracting `path-cost(Γ, u, A)` from its
   fuel and setting `u`'s action to `spent`; emit `unit-moved`. The action
   transition is entailed by `unit-moved` (`semantics/movement.md`). A fog trap
   suppresses the supply follow-up exactly as for any `move-*` command.
2. For each unit `v` in the supply set, in ascending unit-ID order, set
   `v.fuel = max-fuel(v)` and `v.ammo = max-ammo(v)`. If either value actually
   changes, emit `unit-resourced { unit: v, fuel_before, fuel_after,
   ammo_before, ammo_after, reason: "unit-supply" }`. A unit already at both
   maxima is left unchanged and emits nothing, mirroring how `funds-changed` and
   `unit-action-changed` record only actual changes (`semantics/turn.md`).

Supplied units are **not** spent by being supplied; only the acting APC ends its
turn. The state remains in `unit-action`; supply introduces no victory
checkpoint.

## Event ordering

| # | Event | Key fields | Emitted when |
| --- | --- | --- | --- |
| 1 | `unit-moved` | `unit: u`, `from`, `to`, `path: A`, `fuel_spent` | always |
| 2… | `unit-resourced` | `unit: v`, fuel/ammo before-after, `reason: "unit-supply"` | once per supplied unit whose fuel or ammo actually changes, ascending by unit ID |

Deterministic ascending unit-ID ordering makes the `unit-resourced` array
reproducible. There is no composite supply event; each refilled unit is its own
fact.

## Evidence

Corroborated implementation:

- AWBW Replay Player's `SupplyUnitAction` sets each supplied unit's ammo and fuel
  to that unit's maxima and applies to a list of supplied unit IDs, with an
  optional preceding move; it changes no HP and charges nothing.

Documentation-only:

- AWBW Wiki "Units": the APC resupplies adjacent units owned by the same player
  with fuel and ammunition. AWBW additionally resupplies automatically at the
  start of the owner's turn; that automatic hook and the `automatic-supply`
  event are defined by `semantics/turn-hooks.md`.

Now specified elsewhere:

- Start-of-turn automatic supply — from owned properties, adjacent owned APCs,
  and Cruiser/Carrier `cargo`-relation transports — and the `automatic-supply`
  event are defined by `semantics/turn-hooks.md` (feature `turn-hooks-v1`), not
  by this manual command.
- Fog-safe supply projection is defined by `model/observation.md`. A supplied
  unit takes the unit-fact rule, so a recipient observes the refill exactly when
  the unit is visible to them; the accompanying funds movement, when a feature
  produces one, stays team-private.
