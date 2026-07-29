# `move-launch`: missile silo launch

This revision implements `move-launch-v1` for AWBW `2026-07-10`.

The command uses the ordinary movement validator. The mover must be an owned
Infantry or Mech, and its resolved destination must be a ready missile silo.
The impact coordinate must be in board bounds. A hidden blocker may truncate
the movement; that produces movement events only and does not spend the silo.

After movement, the silo missile affects every on-board unit within Manhattan
radius 2 (at most 13 tiles), including allied and enemy units and the launching
unit. It deals 30 exact HP (three visual bars), with
`to_hp = max(1, from_hp - 30)`, and never removes a unit. Cargo is outside the
blast area.

Events are ordered as follows: movement events, one `area-strike-resolved`,
`unit-damaged` for affected units in ascending stable unit-ID order, and then
`silo-changed` from `ready` to `spent`. Silo damage is not a unit strike and
therefore generates no power charge.

The area event and any silo/unit consequences are projected independently
through the observation boundary. A recipient receives unit damage only when
the unit is visible before or after the transition (or belongs to that
recipient's team); hidden units never appear merely because they were inside
the blast.
