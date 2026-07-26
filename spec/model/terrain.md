# Terrain model

A tile consists of a stable terrain kind and, where applicable, separate
mutable tile state. Presentation details such as orientation, artwork, and
animation are not semantic state.

## Terrain kind

A terrain kind defines only properties that can affect validation, transition,
or observation:

- `id`: canonical semantic identifier;
- `defense_stars`: base defensive value;
- `movement_costs`: cost by weather and movement class, where `null` means
  impassable;
- `property_kind`: property category, or `null`;
- `traits`: closed semantic capabilities or effects;
- `vision_bonus`: sight radius added to a unit standing here whose kind is
  listed in the profile's `elevated_vision` capability. Present exactly when the
  kind carries the `elevated-vision` trait;
- `vision_limit`: greatest source distance at which this terrain is fully
  revealed, beyond which it is revealed only for air units. Present exactly when
  the kind carries the `conceals-in-fog` trait;
- `destructible`: maximum HP, unit kind used for weapon/damage lookup, and the
  replacement terrain after destruction. Present exactly when the kind carries
  the `destructible` trait.

The `teleporter` trait marks AWBW's black transit tiles. They have zero defense
stars, no property kind or income, and a movement cost of zero for every
weather and movement class. They may occur consecutively, allowing a submitted
orthogonally-adjacent path to cross an arbitrarily long contiguous run without
spending movement points or fuel. A teleporter may be traversed but is never a
legal final unit location. Under fog it can never receive a visible vision
level, even from an adjacent or occupying vision source.

The two vision fields carry the magnitudes that the corresponding traits only
name. A trait without its field, or a field without its trait, is an invalid
profile; `tools/validate-ruleset.mjs` checks the correspondence. Their use is
defined by `semantics/fog.md`.

Movement cost is a function:

```text
movement-cost(terrain-kind, weather, movement-class) -> integer | impassable
```

Orientation MUST NOT affect this function.

## Mutable tile state

State that can change without replacing the underlying concept is stored on
the tile rather than encoded in its terrain identifier. Relevant fields are
introduced only by terrain traits. Examples include:

- property owner;
- capture points;
- destructible-object HP;
- missile-silo availability; and
- teleporter association (for rulesets that define linked endpoints; AWBW black
  tiles use contiguous traversal and have no association).

The owner is a player identifier or `null`; it is not a faction-colored terrain
kind. A property changing owner therefore does not change its terrain kind.
`destructible_hp` MUST be in `[1, destructible.maximum_hp]`. Reaching zero
removes the field and replaces the terrain with
`destructible.destruction_replacement`.

## External encodings

A ruleset adapter may map multiple external tile codes to the same terrain
kind and initial tile state. External orientation variants are intentionally
many-to-one. External codes and presentation metadata are not part of the
terrain-kind catalog.
