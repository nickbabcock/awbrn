# Commander effective-value profiles

Commander effects are ruleset data. Core command and turn reducers MUST query
the operators in this document and MUST NOT branch on commander identifiers.
Only the active commander slot contributes. An active COP or SCOP selects the
matching state rules in addition to day-to-day rules.

Revision `awbw/2026-07-10` stores non-combat scalar hooks in
`commander-profiles.json`. Its status is `complete` for the AWBW commander
roster and the following closed effective-value queries. Commanders with no
noncombat scalar hook have an explicit empty profile.

```text
effective-move(context, unit, base) -> nonnegative integer
effective-movement-cost(context, unit, base-or-impassable) ->
  nonnegative integer or impassable
effective-vision(context, unit, base) -> nonnegative integer
reveals-concealing-terrain(context, unit) -> boolean
hides-hp(context, player) -> boolean
effective-income-per-property(context, player, base) -> nonnegative integer
effective-repair-bars(context, player, base) -> nonnegative integer
effective-upkeep(context, unit, base) -> nonnegative integer
effective-movement-weather(context, unit, weather) -> weather
effective-build-cost(context, player, base) -> nonnegative integer
commander-production-site(context, player, terrain, domain) -> boolean
effective-capture(context, unit, visual-hp) -> nonnegative integer
effective-attack-range(context, unit, base-maximum) -> nonnegative integer
```

Movement and vision rules match every populated predicate member and add their
integer value. Day-to-day additions apply first, followed by the active power
state's additions. Results clamp at zero. Movement-cost rules also apply
day-to-day first and the active power state second. The
`traversable-cost-set` operator replaces a finite base entry cost with its
nonnegative `value`; it never replaces an impassable `null`. A rule is skipped
when authoritative weather is listed in `except_weather_kinds`.

Income, repair, and upkeep fields are day-to-day additions; upkeep clamps at
zero. `ignores_snow_movement` and `ignores_rain_movement` substitute `clear`
only while looking up movement costs and do not change authoritative weather.
Drake's rain exception therefore does not prevent rain's vision penalty.
`rain_movement_as_snow` substitutes `snow` for the same lookup; Olaf combines
it with `ignores_snow_movement`, so his units treat snow as clear and rain as
snow without changing the match weather.
Weather substitution determines the base table entry before movement-cost
operators run, while `except_weather_kinds` still examines authoritative
weather. Sturm's day-to-day rule consequently sets every finite entry cost to
one in clear weather and rain, does nothing in snow, and never makes an
impassable terrain/class pair traversable.
Lash's Terrain Tactics and Prime Tactics apply the same finite-cost override
only in their respective COP and SCOP states. They likewise preserve
impassability and are disabled in authoritative snow.
Max's indirect units inherit a day-to-day maximum-range penalty of one. Max
Force adds one movement point and Max Blast adds two only to the revisioned
non-foot direct-combat unit set.

`reveals-concealing-terrain` selects the active power state's boolean, falling
back to the day-to-day value and then `false`. When true for a vision source,
terrain `vision_limit` does not restrict that source. The flag affects only
units owned by that source's player; it does not spread to allied players'
vision sources.

`hides_hp` is a day-to-day player flag. When true, recipient observations hide
the HP of that player's units from opponents. The player and the player's
allies still receive exact HP. The flag remains active during COP and SCOP.

Build-cost states select one rational multiplier: the active COP/SCOP value
replaces the day-to-day value when present, otherwise day-to-day is inherited.
The result is `floor(base * numerator / denominator)`. Production rules add
the matching terrain/domain pairs to the base terrain table's facilities.

Attack-range rules add to an indirect weapon's maximum range; they do not
change minimum range. Capture rules return
`floor(visual-hp * numerator / denominator)`, except an `instant` rule returns
20 capture points. All integer results clamp at zero.

Power activation and expiry are separate from these effective-value operators
and are defined by `semantics/powers.md` and `commander-powers.json`. The
manifest capability for every nonempty state MUST be backed by an executable
fixture.

## Evidence

The encoded values are `corroborated-implementation` from WarsWorld's
versioned commander definitions and pass-turn hooks, plus AWBW-specific
documentation where the profiles differ. Each advertised capability also has
an executable commander fixture.
