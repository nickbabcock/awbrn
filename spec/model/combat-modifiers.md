# Combat modifier algebra

Commander combat behavior is ruleset data. Core combat semantics MUST call the
operators in this document and MUST NOT branch on commander display names.

## Context and applicability

The combat context contains the active commander slot for each player, power
state, communication-tower count, weather, whether the strike is a counter,
the selected weapon's fire mode, both unit kinds and capabilities, and both
tiles. It also contains each combatant owner's current funds and owned-property
count. In a tag game only the active slot's commander contributes day-to-day
and power rules. An inactive commander contributes no combat rule.

A rule applies when every populated member of `when` matches. Array members are
OR alternatives; different predicate members are AND conditions. An empty
predicate matches every combatant. `counterattack: true` matches counters only
and `false` matches initial strikes only. Terrain predicates refer to the tile
occupied by the unit whose effective operator is being evaluated.
`property: true` matches when that tile has the ruleset terrain trait
`capturable`; `false` matches every other tile.

## Closed effects

The schema closes the common algebra over additive attack and defense percent,
luck-domain replacement, additive or multiplicative terrain stars, tower bonus
multiplication, and a rational counterattack multiplier. A profile requiring
different behavior MUST add a named, revisioned operator and normative
semantics before using it.

The contextual attack operators are:

- `attack-add-funds-divide`: add
  `floor(current owner funds / divisor)`;
- `attack-add-owned-properties-multiply`: add the owner's count of tiles with
  the `capturable` trait multiplied by `value`;
- `attack-add-terrain-stars-multiply`: add the combatant tile's base terrain
  stars multiplied by `value`.

All three read the immutable combat context captured before either strike of an
engagement. In particular, they do not read a terrain-star value modified by an
earlier rule. This lets a profile explicitly compose a terrain-derived attack
addition with a separate `terrain-stars-multiply` defense effect. Each division
is floored at the operator and the resulting integer is added immediately.

`enemy-terrain-stars-add` is an attacker-side contextual effect. After the
defender's own terrain-star rules have run, apply the attacker's matching
day-to-day and active-power additions in array order, then clamp the defender's
effective stars to a minimum of zero. This ordering makes the defender's
profile responsible for constructing its terrain value and the attacker
responsible for reducing the completed value.

Rules are evaluated in array order. The effective operators use this order:

1. Start attack and defense at 100, terrain stars at the terrain table value,
   good luck at `[0,9]`, and bad luck at `[0,0]`.
2. Apply matching day-to-day effects in listed order.
3. If COP or SCOP is active, add the generic power attack and defense bonus,
   then apply that power state's matching rules in listed order.
4. Add `10 * tower_count` to attack. A tower multiplier changes this bonus,
   not the attack accumulated earlier. Tower defense is zero unless a named
   rule creates it; its multiplier is evaluated the same way.
5. Apply counter effects. For each rational counter multiplier, replace attack
   with `floor(attack * numerator / denominator)` immediately.
6. Clamp attack and defense to a minimum of zero. Clamp effective terrain stars
   to a minimum of zero. Luck replacement domains are inclusive and MUST have
   `minimum <= maximum` (a relational constraint beyond the JSON Schema).

Thus the named queries are:

```text
effective-attack(context, attacker, defender) -> integer percent
effective-defense(context, defender, attacker) -> integer percent
effective-good-luck(context, unit) -> inclusive integer domain
effective-bad-luck(context, unit) -> inclusive integer domain
effective-counter(context, attacker, defender) -> ordered counter effects
counter-first(context, defender, attacker) -> boolean
```

`counter-first` is an engagement-ordering effect, not a scalar. When an
otherwise eligible direct counter has this effect, resolve the defender's
counter before the initiating strike. The defender uses its full pre-engagement
HP. If that strike removes the acting attacker, the initiating strike does not
occur and its luck tokens are not consumed. Otherwise the initiating attacker
uses its reduced exact HP. Combat roles remain stable: the defender's first
strike is a counter and the initiating attacker's second strike is initial.

`commander-combat.json` is complete for the AWBW commander roster. Revision
`2026-07-10` claims `combat-neutral-v1` and the commander-specific feature
paths named in its manifest. A commander profile MAY be encoded before it is advertised,
but it MUST NOT be claimed as conformant until an executable fixture covers
that commander and applicable power state.
