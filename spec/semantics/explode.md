# `move-explode`: Black Bomb self-destruct

This revision implements `move-explode-v1` for AWBW `2026-07-10`.

The mover must be an owned Black Bomb. The ordinary movement validator resolves
the submitted path; a hidden blocker truncates movement and suppresses the
explosion. Once movement completes, every other on-board unit within Manhattan
radius 3 takes 50 exact HP of nonlethal damage, with a minimum resulting HP of
1. Allied and enemy units are included; cargo units are excluded. The Black
Bomb is then removed.

Events are ordered as movement events, `area-strike-resolved`, affected
`unit-damaged` events in ascending stable unit-ID order, and `unit-removed` for
the Black Bomb. Explosion damage is not a unit strike and generates no power
charge. If the owner has no units remaining, normal rout and match-completion
handling follows the removal.

Recipient projection applies before/after visibility to each unit consequence;
hidden units do not become observable merely because they were in the blast.
