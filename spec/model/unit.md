# Unit model

A unit kind defines immutable base statistics. A unit instance stores mutable
state such as owner, position, HP, fuel, ammo, action status, cargo, and
concealment mode.

Base statistics may be modified by the active ruleset context. Consequently,
`move`, `vision`, `cost`, and similar fields are inputs to effective-value
functions rather than cached values on unit instances.

## Range

`indirect_range` describes only indirect fire. `null` means the unit has no
indirect-fire range. It does not mean the unit cannot attack. Direct weapon
ranges belong to weapon data.

## Fuel consumption

`fuel_per_turn.normal` is consumed by ordinary start-of-turn upkeep.
`fuel_per_turn.hidden`, when present, replaces the normal value while a unit is
in its fuel-consuming concealment mode. A missing hidden value means that the
unit has no such mode, not that hidden upkeep is zero.

## Capabilities

Transport, supply, capture, concealment, and weapon behavior will reference
unit kinds through separate capability and weapon tables. They are not inferred
from display names, domains, or zero ammo.
