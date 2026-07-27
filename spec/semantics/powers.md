# Commander power activation and instant effects

Status: normative for `activate-power` under AWBW ruleset revision
`2026-07-10`, as the deliberately narrow feature paths
`commander-power-v1.adder.cop`, `commander-power-v1.adder.scop`,
`commander-power-v1.andy.cop`, `commander-power-v1.andy.scop`,
`commander-power-v1.colin.cop`, `commander-power-v1.colin.scop`,
`commander-power-v1.hachi.cop`, `commander-power-v1.hachi.scop`,
`commander-power-v1.jugger.cop`, `commander-power-v1.jugger.scop`,
`commander-power-v1.kindle.cop`, `commander-power-v1.kindle.scop`,
`commander-power-v1.lash.cop`, `commander-power-v1.lash.scop`,
`commander-power-v1.nell.cop`, `commander-power-v1.nell.scop`,
`commander-power-v1.max.cop`, `commander-power-v1.max.scop`,
`commander-power-v1.koal.cop`, `commander-power-v1.koal.scop`,
`commander-power-v1.flak.cop`, `commander-power-v1.flak.scop`,
`commander-power-v1.grimm.cop`, `commander-power-v1.grimm.scop`,
`commander-power-v1.grit.cop`, `commander-power-v1.grit.scop`,
`commander-power-v1.jake.cop`, `commander-power-v1.jake.scop`,
`commander-power-v1.javier.cop`, `commander-power-v1.javier.scop`,
`commander-power-v1.kanbei.cop`, `commander-power-v1.kanbei.scop`,
`commander-power-v1.eagle.cop`, `commander-power-v1.eagle.scop`,
`commander-power-v1.jess.cop`, `commander-power-v1.jess.scop`,
`commander-power-v1.hawke.cop`, `commander-power-v1.hawke.scop`,
`commander-power-v1.olaf.cop`, `commander-power-v1.olaf.scop`,
`commander-power-v1.sasha.cop`, `commander-power-v1.sasha.scop`,
`commander-power-v1.drake.cop`, `commander-power-v1.drake.scop`,
`commander-power-v1.rachel.cop`, `commander-power-v1.rachel.scop`,
`commander-power-v1.sami.cop`, `commander-power-v1.sami.scop`,
`commander-power-v1.sonja.cop`, `commander-power-v1.sonja.scop`,
`commander-power-v1.sensei.cop`,
`commander-power-v1.sensei.scop`, `commander-power-v1.sturm.cop`,
`commander-power-v1.sturm.scop`, and `commander-power-v1.von-bolt.scop`. The
revisioned activation data is
`rulesets/awbw/2026-07-10/commander-powers.json`; combat and effective-value
modifiers remain in their respective commander tables.

## Scope

The advertised profiles are Adder, whose COP costs two stars and SCOP costs
five stars; Andy's three-star COP and six-star SCOP; Colin's two-star COP and
six-star SCOP;
Hachi's three-star COP and five-star SCOP; Jugger's three-star COP and
seven-star SCOP; Kindle's three-star COP and six-star SCOP; Lash's four-star
COP and seven-star SCOP; Nell's three-star COP and six-star SCOP; Koal's
three-star COP and five-star SCOP; Max's three-star COP and six-star SCOP;
Flak's three-star COP and six-star SCOP; Grimm's three-star COP and six-star
SCOP; Grit's three-star COP and six-star SCOP; Jake's three-star COP and
six-star SCOP; Javier's three-star COP and six-star SCOP; Kanbei's four-star
COP and seven-star SCOP; Eagle's three-star COP and nine-star SCOP; Jess's
three-star COP and
six-star SCOP; Hawke's
five-star COP and nine-star SCOP; Olaf's three-star COP and
seven-star SCOP; Sasha's two-star COP and six-star SCOP; Drake's four-star
COP and seven-star SCOP; Rachel's three-star COP and six-star SCOP; Sensei's
two-star COP and six-star SCOP; Sami's three-star COP and eight-star SCOP;
Sonja's three-star COP and five-star SCOP;
Sturm's six-star COP and ten-star SCOP; and Von Bolt's ten-star SCOP.
Activation selects the already specified scalar combat and
effective-value modifiers, executes any typed instant effects, and expires at
the start of that player's next turn.

The power table is complete for the AWBW commander roster. Its instant-effect
algebra contains
`heal-visual-hp`, `heal-exact-hp`, `damage-exact-hp`, `set-weather`,
`drain-current-fuel-ratio`, `fire-area-strikes`,
`reduce-power-charge-by-funds-ratio`, `refresh-unit-action`,
`resupply-units`, `spawn-units-on-owned-properties`,
`fire-targeted-area-strike`, `fire-immobilizing-area-strike`, and
`multiply-funds-ratio`. Von Bolt intentionally has no COP in AWBW; his absent
COP level is unsupported rather than treated as a zero-effect power.

Neither activation nor expiry consumes randomness.

## Cost

One AWBW power star is 9,000 charge points. For a power with `stars = s` and
the active commander slot's pre-activation `power_uses = u`, the charge is:

```text
uses-for-cost = min(u, 10)
cost = 9000 * s * (5 + uses-for-cost) / 5
```

The division is exact for this profile. More generally the table's rational
scaling operator multiplies first and floors once. Cost uses the pre-activation
counter: successful activation subtracts that cost and then increments
`power_uses` by one. Rejection changes neither field.

## Validation and precedence

`activate-power` carries `{ player, level }`, where `level` is `cop` or `scop`.
It always addresses the player's one active commander slot. Validation uses:

```text
AUTHORITY_REQUIRED
MATCH_FINISHED
WRONG_PHASE
NOT_ACTIVE_PLAYER
ACTION_NOT_SUPPORTED
INSUFFICIENT_POWER
```

`ACTION_NOT_SUPPORTED { action: "activate-power" }` applies when powers are
disabled, a power is already active, or the active commander/level lacks a
complete activation profile. `INSUFFICIENT_POWER` reports the
computed `required` charge and the active slot's `available` charge.

## Execution

On success, atomically:

1. subtract the computed cost from the active commander slot's `power_charge`;
2. increment that slot's `power_uses`;
3. set the player `power_state` to the requested level and active
   `commander_slot`; and
4. emit `power-activated { player, commander, power }`; and
5. execute the profile's `instant_effects` in array order, appending each
   effect's events.

The state remains in `unit-action`. The event entails the private charge and
use-counter changes; no separate public meter event is emitted.

## `heal-visual-hp`

```json
{ "operator": "heal-visual-hp", "target": "owned-units", "amount": 2 }
```

`owned-units` selects every living unit whose owner is the activating player,
including cargo, and orders the selection by ascending unit ID. For each
selected unit independently:

```text
from = exact HP before this effect
visual = ceiling(from / 10)
to = min(10, visual + amount) * 10
```

If `to != from`, set the unit's exact HP to `to` and emit
`unit-repaired { unit, from_hp: from, to_hp: to, reason:
"commander-power" }`. If `to = from`, make no mutation and emit no event.
Consequently a unit at 55 exact HP reaches 80 under Andy's two-bar Hyper
Repair, a unit at 91 reaches 100, and a unit already at 100 emits no repair
fact. The operator never changes fuel, ammo, funds, action state, or location.

## `heal-exact-hp`

```json
{ "operator": "heal-exact-hp", "target": "owned-units", "amount": 10 }
```

`owned-units` has the same ownership, cargo inclusion, and ascending unit-ID
ordering as `heal-visual-hp`. For each selected unit independently:

```text
from = exact HP before this effect
to = min(100, from + amount)
```

This operator does not round `from` to visual HP. If `to != from`, set the
unit's exact HP to `to` and emit `unit-repaired { unit, from_hp: from, to_hp:
to, reason: "commander-power" }`; otherwise make no mutation and emit no event.
Hawke's COP uses `amount: 10`, and his SCOP uses `amount: 20`.

## `damage-exact-hp`

```json
{
  "operator": "damage-exact-hp",
  "target": "enemy-units",
  "amount": 10,
  "minimum_hp": 1
}
```

Let the activating player's team be `T`. `enemy-units` selects every living
unit, including cargo, whose owner's team is not `T`; units owned by another
player on `T` are not enemies. The selection is ordered by ascending unit ID.
`enemy-units-on-properties` instead selects only such enemy units with a board
location whose terrain has the ruleset `capturable` trait. Property ownership
is irrelevant; cargo and units on other terrain are excluded. The filtered
selection retains ascending unit-ID order.
For each selected unit independently:

```text
from = exact HP before this effect
to = max(minimum_hp, from - amount)
```

Subtraction is saturating before the lower clamp. If `to != from`, set the
unit's exact HP to `to` and emit `unit-damaged { unit, from_hp: from, to_hp:
to, reason: "commander-power" }`; otherwise make no mutation and emit no event.
Hawke's COP and SCOP use `minimum_hp: 1`, so neither power removes a unit or
triggers elimination. They use `amount: 10` and `amount: 20`, respectively.
Kindle's Urban Blight uses `enemy-units-on-properties`, `amount: 30`, and the
same one-HP floor.

Hawke profiles list healing before damage. The reducer therefore emits all
ascending-ID allied repair events before all ascending-ID enemy damage events.
Neither exact-HP operator changes fuel, ammo, funds, action state, concealment,
or location.

## `drain-current-fuel-ratio`

```json
{
  "operator": "drain-current-fuel-ratio",
  "target": "enemy-units",
  "numerator": 1,
  "denominator": 2
}
```

`enemy-units` uses the same enemy-team selection, cargo inclusion, and
ascending unit-ID ordering as `damage-exact-hp`. For each selected unit:

```text
drained = floor(fuel_before * numerator / denominator)
fuel_after = fuel_before - drained
```

Multiplication precedes one floor. If the result changes fuel, emit
`unit-resourced { unit, fuel_before, fuel_after, ammo_before, ammo_after,
reason: "commander-power" }`, with both ammo fields unchanged. A zero computed
drain emits nothing. Drake uses one half, so fuel 99 becomes 50, fuel 2 becomes
1, and fuel 1 remains 1. The effect cannot remove a unit immediately; ordinary
start-of-turn fuel upkeep and crash rules remain separate.

Tsunami applies fuel drain before ten exact HP of nonlethal damage. Typhoon
applies fuel drain, then twenty exact HP of nonlethal damage, then rain through
Drake's next turn. Each effect completes its ascending-ID event sequence before
the next begins. Allied players are excluded from all three effects.

## `fire-area-strikes`

Rachel's Covering Fire profile is:

```json
{
  "operator": "fire-area-strikes",
  "target": "all-board-units",
  "radius": 2,
  "damage": 30,
  "minimum_hp": 1,
  "selection_policies": ["infantry-hp", "unit-value", "unit-hp"],
  "friendly_contribution": "subtract"
}
```

Target selection reads the state at effect start and calculates every center
before any strike deals damage. Cargo units neither contribute to selection nor
take damage. Every board coordinate is a candidate center. A unit contributes
when its Manhattan distance from the candidate is at most `radius`. Units owned
by the activating player's team contribute negatively; every other unit
contributes positively.

Let `h = clamp(exact-hp, 1, 30)` and let `cost` be the unit owner's effective
build cost for that kind. The three policies score a unit as follows:

- `infantry-hp`: start with `h`. If the unit is Infantry or Mech and has more
  than ten exact HP, multiply by four; multiply by another two when it occupies
  a tile with an in-progress capture. This policy has no numeric secondary
  tiebreak.
- `unit-value`: use `2` when exact HP is below ten; otherwise use `h * cost`.
  Its secondary tiebreak is the sum of `exact-hp * cost` for enemy units only.
- `unit-hp`: use `h`. Its secondary tiebreak is the sum of uncapped exact HP
  for enemy units only.

For each policy, choose the greatest signed primary score, then its greatest
secondary score where applicable, then the smallest `y`, then the smallest
`x`. Arithmetic is exact signed integer arithmetic. The selected centers are
therefore deterministic even when every candidate has a nonpositive score.

Strikes execute in `selection_policies` order. Before each strike's damage,
emit `area-strike-resolved { strike, policy, center, radius, damage }`, where
`strike` is zero-based. Select all board units in the radius, including
friendly units, in ascending unit-ID order. For each:

```text
to = max(minimum_hp, exact-hp-before - damage)
```

Emit `unit-damaged` with reason `commander-power` only when HP changes.
Overlapping strikes apply cumulatively, but their already-selected centers do
not change. Covering Fire consequently fires INF, then COST, then HP missiles;
each deals thirty exact HP and cannot remove a unit.

## `fire-targeted-area-strike`

Sturm's COP profile is:

```json
{
  "operator": "fire-targeted-area-strike",
  "target": "enemy-unit-centers",
  "radius": 2,
  "damage": 40,
  "minimum_hp": 1,
  "selection_policy": "unit-value",
  "friendly_contribution": "subtract",
  "unit_value": "base-build-cost"
}
```

His SCOP uses the same profile with `damage: 80`. Cargo units neither
contribute to selection nor take damage. Candidate centers are exactly the
positions occupied by on-board units owned by another team. For each candidate,
select every on-board unit within Manhattan distance `radius` and sum:

```text
visual-hp = ceiling(exact-hp / 10)
value = visual-hp * base-build-cost(unit kind)
signed-value = -value for the activating team; +value for every other team
```

The base cost comes directly from `units.json`; commander cost modifiers do not
change meteor targeting. Choose the candidate with the greatest signed sum,
then the smallest `y`, then the smallest `x`. If no enemy unit is on the board,
the effect emits nothing. Otherwise emit `area-strike-resolved { strike: 0,
policy: "unit-value", center, radius, damage }`, then damage every on-board unit
in the radius in ascending unit-ID order:

```text
to = max(minimum_hp, exact-hp-before - damage)
```

Emit `unit-damaged` with reason `commander-power` only when HP changes. The
strike affects allied and enemy units alike, cannot remove a unit, and does not
affect cargo.

AWBW deliberately uses this one deterministic value policy for both powers.
The three randomly selected value/indirect/HP policies documented for cartridge
Meteor Strike are not AWBW behavior, so activation consumes no random token.

## `fire-immobilizing-area-strike`

Von Bolt's Ex Machina profile is:

```json
{
  "operator": "fire-immobilizing-area-strike",
  "target": "enemy-units",
  "radius": 2,
  "damage": 30,
  "minimum_hp": 1,
  "selection_policy": "unit-value",
  "friendly_contribution": "subtract",
  "unit_value": "base-build-cost",
  "duration": "through-target-next-turn"
}
```

Target selection reads the state at effect start. Cargo units neither
contribute nor take damage or immobilization. Every board coordinate is a
candidate center. Let `cost` be the base cost of the unit kind from
`units.json`, unaffected by commander build-cost modifiers. For each on-board
unit within Manhattan distance `radius`, calculate the primary contribution
used by the AWBW unit-value policy:

```text
contribution = 2                              when exact HP < 10
contribution = min(exact HP, 30) * base cost  otherwise
```

A unit owned by the activating player's team contributes negatively; a unit
owned by any other team contributes positively. Choose the greatest signed
sum. Ties choose the greatest sum of `exact HP * base cost` over enemy units in
the area, then the smallest `y`, then the smallest `x`. The selection is
deterministic even if every candidate has a nonpositive score.

Emit `area-strike-resolved { strike: 0, policy: "unit-value", center, radius,
damage }`. Then select only on-board units owned by another team within the
radius, ordered by ascending unit ID. Same-team units affected the target
calculation but are neither damaged nor immobilized. For each selected enemy:

1. set `hp` to `max(minimum_hp, hp - damage)` and, if changed, emit
   `unit-damaged` with reason `commander-power`;
2. unless its action is already `immobilized`, set it to `immobilized` and emit
   `unit-action-changed` with reason `commander-power`.

At that unit owner's next `turn-start`, action normalization changes
`immobilized` to `spent`, emits the corresponding `unit-action-changed`, and
does not make the unit ready. It therefore cannot act during that turn. At the
owner's following turn-start, ordinary `spent` normalization restores it to
`ready`. Reapplying the effect before the reserved turn does not add a second
reserved turn.

Ex Machina deals thirty exact HP with a one-HP floor, so it cannot remove a
unit or trigger elimination. Damage and action events are interleaved per
ascending target ID, with damage first. The operator consumes no randomness;
AWBW does not use Dual Strike's random HP/value selection.

## `set-weather`

```json
{
  "operator": "set-weather",
  "kind": "snow",
  "duration": "until-owner-next-turn"
}
```

`until-owner-next-turn` snapshots the number of currently active player
selections from immediately after the activating turn position through and
including the activating player's next position. In an ordinary three-player
turn order this is three. Inactive player positions are not counted. This value
is stored as `weather.remaining_turns`, whose source-independent decrement is
specified by `semantics/turn-hooks.md`.

The operator replaces both fields of any existing temporary override: set
`weather.kind` to `kind` and `weather.remaining_turns` to the computed count.
If either field changes, emit `weather-changed { from: kind-before, to: kind,
remaining_turns: count, reason: "commander-power" }`. If both already equal
the replacement values, emit nothing. A subsequent roster change does not
recompute the stored countdown.

Olaf's COP contains only this operator. His SCOP first applies
`damage-exact-hp` with `amount: 20` and a one-HP floor, then applies
`set-weather`; enemy damage events therefore precede the weather event.
At Olaf's next `turn-start`, `power-ended` is emitted at power-expiry step 2,
then weather-expiry step 3 decrements the final count to zero, restores the
fixed base weather from `settings.weather`, and emits `weather-changed` with
reason `expiry`.

## `multiply-funds-ratio`

```json
{
  "operator": "multiply-funds-ratio",
  "target": "activating-player",
  "numerator": 3,
  "denominator": 2
}
```

Read the activating player's funds at this effect's position in the ordered
`instant_effects` array, then calculate:

```text
to = floor(funds-before * numerator / denominator)
```

Multiplication is over unbounded mathematical integers and precedes the single
floor. `denominator` is positive. If `to` cannot be represented by the state's
nonnegative funds type, the pre-state is invalid for this activation and the
whole command is atomic: charge, `power_uses`, power state, funds, and events
all remain unchanged. If `to = funds-before`, emit nothing. Otherwise set the
activating player's funds to `to` and emit `funds-changed { player, from, to,
reason: "commander-power" }` immediately after the prior instant effect, or
immediately after `power-activated` when it is first.

Colin's Gold Rush uses `3/2`, multiplying only Colin's current funds by one and
one half and flooring once. It does not inspect or change allies' or enemies'
funds and consumes no randomness.

## `reduce-power-charge-by-funds-ratio`

```json
{
  "operator": "reduce-power-charge-by-funds-ratio",
  "target": "enemy-commander-slots",
  "funds_per_full_bar": 50000
}
```

Let `F` be the activating player's funds at effect execution. The operator
selects every commander slot of every player whose team differs from the
activator's team. Allied players are excluded. Target players are ordered by
ascending player ID and their commander slots by ascending index.

For a selected slot with commander `c` and current `power_uses = u`, its
`full-bar(c,u)` is the larger of its revisioned COP and SCOP star counts,
multiplied by the effective per-star charge at `u`:

```text
uses-for-cost = min(u, 10)
star-charge = 9000 * (5 + uses-for-cost) / 5
full-bar(c,u) = maximum-power-stars(c) * star-charge
reduction = floor(F * full-bar(c,u) / funds_per_full_bar)
to = max(0, power-charge-before - reduction)
```

Multiplication precedes the single floor. For each changed slot, mutate
`power_charge` and emit `power-charge-changed { player, commander_slot, from,
to, reason: "commander-power" }`. A zero charge or zero computed reduction
emits no event. A nonzero target charge whose commander lacks a complete power
profile is an invalid state rather than an assumed bar size.

Sasha's Market Crash uses `funds_per_full_bar: 50000`, equivalent to one
percent of each target's current full bar per 500 funds. It does not spend or
otherwise change Sasha's funds. Exact power charge and its changes are public
under `model/observation.md`.

## `gain-funds-from-visual-hp-damage`

```json
{
  "operator": "gain-funds-from-visual-hp-damage",
  "target": "enemy-unit",
  "numerator": 1,
  "denominator": 2,
  "unit_value": "effective-build-cost"
}
```

This is a strike effect rather than an activation-time instant effect. After
the strike's `unit-damaged` event, compute:

```text
visual-damage = visual-hp(from_hp) - visual-hp(to_hp)
target-value = effective-build-cost(target owner, target kind)
gain = floor(visual-damage * target-value * numerator / (10 * denominator))
```

Only damage to a unit owned by another team qualifies. The target's effective
build cost is resolved from the pre-command state, multiplication precedes one
floor, and exact damage that does not cross a visual-HP boundary yields zero.
If `gain > 0`, add it to the striker owner's funds and emit `funds-changed {
player, from, to, reason: "commander-power" }` immediately after the associated
`unit-damaged`. This applies equally to an initiating strike and a counter.

Sasha's War Bonds uses one half. The effect changes neither the target unit's
cost nor any power charge. Funds overflow is an invalid state.

## `refresh-unit-action`

```json
{
  "operator": "refresh-unit-action",
  "target": "owned-units",
  "exclude_unit_kinds": ["infantry", "mech"]
}
```

`owned-units` starts with every living unit owned by the activating player.
The revisioned `exclude_unit_kinds` predicate then removes matching kinds, and
the remaining selection is ordered by ascending unit ID. For each selected
unit whose action is `spent`, set its action to `ready` and emit
`unit-action-changed { unit, from: "spent", to: "ready", reason:
"commander-power" }`.

Already-`ready` units emit no event. A `moved` unit has a pending follow-up
choice and is not refreshed; activation does not discard an incomplete action.
The operator changes no HP, fuel, ammo, funds, concealment, or location.
Eagle's AWBW profile excludes `infantry` and `mech`, so every other owned kind
that has spent its action may act again, including a unit built earlier in the
same turn.

## `resupply-units`

```json
{ "operator": "resupply-units", "target": "owned-units" }
```

`owned-units` selects every living unit owned by the activating player,
including cargo, and orders the selection by ascending unit ID. Each selected
unit's fuel and ammo are independently set to the revisioned maxima in
`units.json`. When either value changes, emit:

```text
unit-resourced {
  unit,
  fuel_before, fuel_after,
  ammo_before, ammo_after,
  reason: "commander-power"
}
```

A unit already at both maxima emits no event. A unit kind with maximum ammo
zero may still emit when its fuel changes, with both ammo fields zero. The
operator does not alter HP, action state, concealment, location, or funds.

## `spawn-units-on-owned-properties`

Sensei's COP profile is:

```json
{
  "operator": "spawn-units-on-owned-properties",
  "target": "owned-properties",
  "property_kinds": ["city"],
  "unit_kind": "infantry",
  "hp": 90,
  "resources": "unit-maxima",
  "action": "ready",
  "concealment": "exposed",
  "occupied_tiles": "skip",
  "order": "y-then-x",
  "unit_limit": "settings"
}
```

The SCOP profile is identical except that `unit_kind` is `mech`. Candidates
are board tiles whose terrain table `property_kind` is listed in
`property_kinds` and whose tile owner is the activating player. Thus the AWBW
profile includes only owned cities: bases, airports, ports, HQs, labs,
communication towers, neutral cities, and other players' cities are not valid
spawn properties.

`y-then-x` scans rows from `y = 0` upward and positions within each row from
`x = 0` upward. `skip` excludes any candidate occupied by an on-board unit,
regardless of that unit's owner; cargo does not occupy a tile. When
`settings.unit_limit` is non-null, scanning stops as soon as the activating
player's count of living units, including cargo and units spawned earlier by
this effect, reaches that limit. A player already at the limit spawns nothing.

For each remaining position in scan order, allocate the numeric `UnitId` from
`next_unit_id`, incrementing it once per unit as specified by `model/state.md`.
If at least one unit would spawn, a missing, stale, or
overflowing counter makes the pre-state inadmissible and the whole command is
atomic. If no position remains, the counter is neither required nor changed.

Each unit starts with exact `hp = 90`, maximum fuel and ammo for `unit_kind`
from `units.json`, `action = ready`, `concealment = exposed`, and a board
location at its selected position. Unit bans, funds, and production-facility
eligibility do not apply to power-created units. After `power-activated`, emit
one `unit-created` per spawned unit in the same scan/identifier order. No
funds or action-change event accompanies creation.

At step 2 of that player's next `turn-start`, before weather expiry, income,
or any other hook, set `power_state` to `none` and emit
`power-ended { player, commander, power }`. Charge and `power_uses` do not
change on expiry. The active power therefore applies through intervening
players' turns but not to its owner's next-turn income or later hooks.

## Evidence

Corroborated implementation:

- WarsWorld's AWBW version properties use a 9,000-point base star, 20% cost
  growth per prior use capped at ten uses, and charge the pre-increment cost.
- WarsWorld's Adder AW2 profile assigns two stars to Sideslip and five to
  Sidewinder, with movement-only hooks.
- WarsWorld's Andy AW1/AW2/AWDS profiles assign three stars to Hyper Repair and
  heal every owned unit by two visual HP. Its shared unit `heal` operation
  rounds current exact HP up to visual HP before adding the requested bars.
- AWBW Replay Player's Colin profile and WarsWorld's AW2 Colin implementation
  agree that Gold Rush costs two stars and multiplies the activating player's
  current funds by one and one half.
- AWBW Replay Player's Hachi profile and WarsWorld's AW2 Hachi implementation
  agree that Barter costs three stars, Merchant Union costs five, both halve
  build costs, and Merchant Union additionally permits ground-unit production
  from owned cities.
- AWBW Replay Player's Jugger profile fixes his inclusive good-/bad-luck maxima
  at 29/14 day-to-day, 54/24 under three-star Overclock, and 94/44 under
  seven-star System Crash. WarsWorld corroborates the same domain sizes using
  exclusive-looking percentage descriptions.
- AWBW Replay Player and WarsWorld agree that Koal gains ten attack on roads
  day-to-day, another ten plus one movement under three-star Forced March, and
  another twenty plus two movement under five-star Trail of Woe.
- AWBW Replay Player and WarsWorld agree that Grimm has +30 attack/-20 defense
  day-to-day, gains another +20 attack under three-star Knuckleduster, and
  another +50 under six-star Haymaker after the shared power bonus is included.
- WarsWorld's Eagle AWDS profile assigns nine stars to Lightning Strike and
  marks every owned non-Infantry/non-Mech unit ready, including units built that
  turn.
- WarsWorld's Jess AWDS profile assigns three stars to Turbo Charge and
  resupplies every owned unit's fuel and ammo.
- WarsWorld's Hawke AW2/AWDS profile and AWBW Replay Player's `COs.json` agree
  that Black Wave costs five stars, heals owned units by 10 exact HP, and
  damages enemy units by 10 exact HP without killing them; Black Storm costs
  nine stars and uses 20 exact HP. WarsWorld additionally identifies these as
  the exceptional heals that do not round up to visual HP.
- AWBW Replay Player's `COs.json` and the AWBW Wiki CO kit data agree that
  Olaf's Blizzard costs three stars, Winter Fury costs seven total stars,
  both create snow for one day, and Winter Fury first damages enemy units by
  20 exact HP without killing them. The Wiki weather documentation defines one
  day as lasting until the source player's next turn.
- AWBW Replay Player's `COs.json`, WarsWorld's Sasha AWDS profile, and the AWBW
  Wiki agree that Market Crash costs two stars and removes ten percent of each
  enemy full bar per 5,000 of Sasha's current funds. The Wiki explicitly uses
  plural power bars, consistent with applying the effect to both tag slots.
- The same Sasha sources agree that six-star War Bonds grants funds equal to
  half the value of visual HP removed from enemy units. WarsWorld corroborates
  that the effect applies on attacks and counters and uses the damaged unit's
  effective build cost.
- The AWBW Wiki Drake and weather profiles establish Drake's four-/seven-star
  power costs, one-/two-bar nonlethal damage, half-fuel drain, and Typhoon rain
  lasting through his next turn. WarsWorld corroborates that odd fuel drains
  by the floor of half the current value before damage is applied.
- The AWBW Wiki Rachel profile documents the six-star Covering Fire order,
  simultaneous center calculation, cargo exclusion, signed friendly values,
  capped-HP scoring, policy-specific tiebreaks, top-left final tiebreak, and
  three-HP nonlethal radius-two damage. WarsWorld corroborates the missile
  order, Manhattan radius, sequential damage, and stable input calculations.
- AWBW Replay Player's `COs.json` and replay `unitAdd` model identify Sensei's
  two-/six-star powers, unoccupied-city selection, Infantry/Mech kinds, and
  nine-HP default-resource units. WarsWorld's Sensei AW2 profile corroborates
  ready action state, owned-city selection, configured unit-cap enforcement,
  and top-left row-major spawn order.
- The AWBW Wiki Sturm profile documents six-/ten-star power costs, four-/eight-
  HP radius-two damage, enemy-unit centers, signed visual-HP-times-base-cost
  scoring, cargo exclusion, and the top-left tiebreak. AWBW Replay Player
  corroborates the costs, damage, and replay missile coordinates. WarsWorld's
  cartridge implementation is retained only as disagreement evidence for the
  random three-policy behavior that AWBW does not use.
- The AWBW Wiki Von Bolt profile documents the ten-star cost, deterministic
  AWBW unit-value scoring and enemy-value tiebreak, same-team subtraction,
  cargo exclusion, top-left final tiebreak, three-HP nonlethal damage, and loss
  of the affected enemy units' next action. AWBW Replay Player corroborates the
  cost, damage, and stun replay field. WarsWorld corroborates radius two,
  enemy-only damage and immobilization, but its random HP/value selection is
  retained only as Dual Strike disagreement evidence.

Documentation-only evidence establishes that an activated power lasts until
the start of its owner's next turn. The shared turn-start reducer fixes power
expiry before weather expiry and explicit random-weather selection
(`semantics/turn-hooks.md`).
