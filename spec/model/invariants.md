# Authoritative state invariants

Every accepted state and every result state MUST satisfy:

- board width and height are positive;
- the tile grid has exactly `height` rows;
- every tile row has exactly `width` cells;
- every tile's `terrain` denotes a kind defined by the selected ruleset profile;
- player IDs are unique and `active_player` names an existing player;
- unit IDs are unique;
- every unit owner names an existing player;
- every on-board unit position is in bounds;
- no two on-board units occupy the same position;
- living unit exact HP is in `[1,100]` integer points;
- fuel and ammo are nonnegative and do not exceed their effective maxima; and
- references such as cargo/carrier relationships are internally consistent.

Feature-specific invariants extend this list. An implementation MUST reject an
invalid input state distinctly from rejecting a legal-state command.

The complete state-level relational invariants, including teams, turn order,
tile traits, commanders, cargo, and outcomes, are defined in `state.md`.
