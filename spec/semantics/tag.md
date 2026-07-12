# Tag switching and dual commander charge

Status: normative for tag commander switching and tag combat charge under AWBW
ruleset revision `2026-07-10`, as feature `tag-v1`. The boundary loop is
`semantics/turn.md`, ordinary power activation and expiry are
`semantics/powers.md`, and strike charge is `semantics/combat.md`.

## Scope

When `settings.tags = true`, each player has two commander slots and exactly one
is active. The two slots MAY contain the same commander kind. Only the active
slot contributes day-to-day, combat, effective-value, and active-power effects.
AWBW has no tag bonuses, Tag Break, or Dual Strike.

Each slot retains its own `power_charge` and `power_uses`. Activating a power
addresses only the active slot: it spends that slot's charge and increments only
that slot's use count. Filling both meters creates no additional command.

The commander swap itself consumes no random token. Its mandatory boundary
closure may consume weather outcomes when `settings.weather = "random"`
(`semantics/turn-hooks.md`).

## Tag combat charge

After each unit strike, first compute the ordinary dealt or received gain from
`semantics/combat.md`. If that player's `power_state` is not `none`, neither
commander slot gains charge. Otherwise:

1. add the full computed gain to the active slot;
2. add `floor(gain / 2)` to the inactive slot; and
3. clamp each independently to that commander's current scaled maximum.

Emit `power-charge-changed` once for each slot that actually changes, active
slot first and inactive slot second. The striker's slot events precede the
damaged owner's slot events, preserving the ordinary strike ordering. A full
slot does not prevent the other slot from gaining charge.

## `tag`

The command is `{ "type": "tag", "player": p }`. It is a boundary command, not
an ordinary unit action. Validation uses:

```text
AUTHORITY_REQUIRED
MATCH_FINISHED
WRONG_PHASE
NOT_ACTIVE_PLAYER
ACTION_NOT_SUPPORTED
```

`ACTION_NOT_SUPPORTED { action: "tag" }` applies when
`settings.tags = false`. A tag state with any player lacking exactly two
commander slots or with other than one active slot violates the authoritative
state invariants rather than producing a command violation.

Execution enters `turn-end`, swaps the acting player's active and inactive
slots, and then performs the same successor selection and turn-start closure as
`end-turn`. It never gives the acting player another action phase.

If the outgoing commander has an active COP or SCOP, switching ends it
immediately: clear `power_state` and emit `power-ended` before changing the slot
flags. Instantaneous effects already committed by the power are not undone;
only effects derived from the active `power_state` cease. Then toggle the two
slots and emit `commander-swapped`.

The boundary's leading event order is:

1. `phase-changed` from `unit-action` to `turn-end`;
2. optional `power-ended` for the outgoing active commander;
3. `commander-swapped { player, from_slot, to_slot }`; and
4. the ordinary `end-turn` successor and `turn-start` events.

## Evidence

Documentation-only from the supplied AWBW tag description:

- only the active commander supplies effects and abilities;
- either slot may contain the same commander;
- the inactive commander gains charge at half the active rate;
- neither slot gains charge while that player's power is active;
- power use changes only the active commander's future cost;
- switching is available at end of turn, ends an active power immediately, and
  does not grant a second turn; and
- AWBW has no Tag Breaks, Dual Strikes, or tag bonuses.
