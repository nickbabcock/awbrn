# Combat semantics: neutral slice

This revision normatively exposes only `combat-neutral-v1`. It fixes neutral
unit-versus-unit arithmetic and random-token consumption; commander-specific
combat, charge, destructible targets, fog event
projection, and elimination remain outside this feature.

## Weapon and command eligibility

Resolve Manhattan distance after movement. A direct weapon has range 1. An
indirect weapon uses the unit's minimum and maximum range and the attacker MUST
not have moved in the command. Selection tries the ammo matrix when ammo is
sufficient, then the unlimited matrix. The selected entry supplies base damage
and ammo cost. If neither entry exists, the attack is invalid.

Selection is matchup-specific, not merely based on whether the attacker has
ammo. For example, Tank has no `ammo` entry against Infantry, so it selects
`unlimited` against Infantry even with full ammo. Against Tank it selects
`ammo` when available and falls back to `unlimited` when empty.

The `weapon` member of `attack-resolved` is the stable mechanical selector
`ammo` or `unlimited`. It is not a localized display name and does not claim
that the unlimited weapon is conventionally a unit's secondary weapon.

## Damage

Let `visual-hp(hp) = ceiling(hp / 10)`. Given selected base damage `B`, attack
percent `A`, defense percent `D`, defender terrain stars `T`, good luck `G`,
bad luck `L`, and the attacker's exact HP `HA`:

```text
attack-factor = max(0, floor(B * A / 100) + G - L)
hp-factor     = attack-factor * HA
defense-term  = max(0, 200 - D - T * visual-hp(defender.hp))
damage        = floor(floor(hp-factor * defense-term / 100) / 100)
```

Damage is clamped to the defender's exact HP. All values are integers and the
two displayed floors are distinct rounding points.

Zero HP is an event value, not a state value. A lethal strike emits
`unit-damaged` with `to_hp: 0`, performs any capture reset, and removes the unit
before the complete post-state is exposed.

For every executed strike consume one token of type `combat-good-luck`, then
one token of type `combat-bad-luck`. A token value MUST lie in the applicable
inclusive domain and is the resulting `G` or `L`; no scaling is performed.
A counter's tokens are consumed only if a counterattack actually occurs.

## Counterattack and ordering

After the initial damage, a surviving defender counters only when its selected
weapon is direct, can target the attacker, and has sufficient ammo. Counter
damage uses its post-damage exact HP. Ammunition for each strike is deducted
when that strike begins.

For this slice events are ordered: movement events (when the path changes
position), attacker resource change if ammo was spent, `attack-resolved`,
defender `unit-damaged`, then defender removal if lethal.
For a counter, append its resource change, `attack-resolved`, attacker damage,
and attacker removal in the same order. Finally emit the acting unit's
`unit-action-changed`. Capture reset caused by damage follows
`capture-reset.md` immediately after the corresponding `unit-damaged` event
and before removal.

Ammo is deducted when its strike begins. The acting unit becomes `spent` even
when its target is removed. A removed defender does not select a counter weapon
and consumes no counter luck tokens.

When the defender's revisioned `counter-first` operator is true and that
defender is otherwise eligible to counter, reverse the two strike-resolution
blocks while preserving combat roles. Consume the defender's good- and
bad-luck tokens first. If the acting attacker survives, consume its two tokens
and resolve its initiating strike using its reduced exact HP. If it does not
survive, remove it immediately; do not consume its tokens, spend ammunition for
its unperformed strike, or emit `unit-action-changed` for the removed unit.

Team elimination, match completion, and fog-sensitive target compatibility are
outside `combat-neutral-v1`. Its fixtures MUST use exposed units and leave both
teams active after combat.

Recipient event projection is no longer outside this feature: `attack-resolved`
and `random-outcome` are omitted for every recipient, and the strike's visible
consequences reach each one through the damage, resource, removal, and tile
events that accompany it (`model/observation.md`). What remains fog-sensitive is
*targeting*, not projection, and is specified separately below. A
`combat-neutral-v1` fixture MUST keep every participant visible to both sides.
Setting `settings.fog = true` while satisfying that is permitted but gains
nothing, and the existing fixtures leave fog disabled.

## Fog-sensitive unit targeting

Feature `combat-visibility-v1` validates a unit target against the acting
player's team and the authoritative state before combat randomness is consumed.
The target MUST satisfy `visible-unit(R,S,team(player),target)` from
`semantics/fog.md`. With map fog disabled this admits every exposed on-board
unit, while a submerged Sub or hidden Stealth still requires concealment
detection. An unseen target is rejected as `INVALID_TARGET`, using the same
violation class as an absent or otherwise invalid unit target.

Visibility and attack compatibility are distinct. Any adjacent friendly unit
can detect a voluntarily concealed unit, but a detected submerged Sub can be
attacked only by a Sub or Cruiser, and a detected hidden Stealth can be attacked
only by a Fighter or Stealth. These compatibility restrictions continue to
apply to a unit whose `concealment` is `hidden` when fog is disabled.

The current reducer supports only the stationary path `[origin]`, so
pre-command and post-movement visibility are identical. A future combat-movement
increment MUST evaluate the target after resolving the movement path and hidden
occupancy, without allowing a target identifier to reveal an unseen unit.

## Destructible tile targets

Feature `combat-tile-target-v1` implements
`move-attack.target { type: "tile", position }` for a ruleset terrain carrying
the `destructible` trait and a live `destructible_hp` field. The target
position MUST be in bounds, unoccupied, visible to the acting player's team,
and within the attacker's ordinary effective direct or indirect range. All
other tile targets are rejected as `INVALID_TARGET`; a valid destructible
outside range is `TARGET_OUT_OF_RANGE`.

The terrain profile supplies `target_kind`, used for ordinary weapon selection
and base damage. AWBW's `pipe-seam` has 100 maximum HP, zero defense stars,
uses `neo-tank` as its target kind, and becomes `plain` at zero HP. Resolve one
initial strike with the attacker's effective commander attack modifier, the
seam's fixed defense of 100, and luck exactly zero. A seam does not counter and
tile attacks request and consume no random tokens.

Event order is:

1. `unit-resourced` when the selected weapon consumes ammunition;
2. `attack-resolved` with the tile target;
3. `destructible-damaged`;
4. on lethal damage, `tile-terrain-changed` with reason `combat`; and
5. `unit-action-changed` from `ready` to `spent`.

Nonlethal damage updates `destructible_hp`. Lethal damage removes that field
and installs the profile's `destruction_replacement`. Damage to a destructible
tile has no owner or unit value, grants no power charge or Sasha War Bonds
funds, performs no rout check, and cannot reset capture.

The revisioned pipe seam is an `always-visible` position under
`semantics/fog.md`, so its exact HP and both damage events project to every
recipient. The generic visibility check remains normative for future
destructible terrain without that trait.

## Combat power charge

Feature `combat-power-charge-v1` applies after each executed unit strike,
including a counter or counter-first strike. Let:

```text
visual-damage = visual-hp(from-hp) - visual-hp(to-hp)
target-value  = effective-build-cost(target owner, target kind)
dealt-gain    = floor(target-value * visual-damage / 20)
received-gain = floor(target-value * visual-damage / 10)
```

The striker gains `dealt-gain`; the damaged unit's owner gains
`received-gain`. Only visual HP bars lost by the directly struck unit count.
Destroyed cargo, silo damage, power damage, and other non-unit-strike damage do
not add charge.

A player with an active COP or SCOP gains no charge. Otherwise, in a non-tag
game add charge to the active commander slot and clamp it to that commander's
current scaled maximum charge. A zero gain or already-full meter emits no event.
Feature `tag-v1` extends this step in tag games by also charging the inactive
slot at half rate and clamping the two meters independently.

For each strike, charge changes follow `unit-damaged` and any contextual
strike-funds event, with the striker's change before the target owner's change.
Each mutation emits:

```text
power-charge-changed {
  player, commander_slot, from, to,
  reason: "combat" | "combat-counter"
}
```

These authoritative facts and exact charge are global information under
`model/observation.md`.

## Evidence

The weapon selection and base damage are backed by `weapons.json`. The formula,
visual HP use, counter post-damage HP, and demand-driven counter are
`corroborated-implementation` from AWBW Replay Player and WarsWorld. They need
controlled AWBW experiments before this profile can be marked complete.
Power charge arithmetic is `documentation-only` from the AWBW Wiki CO power
meter documentation pending replay-controlled confirmation.
