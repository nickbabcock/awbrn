# `delete-unit`: voluntary unit removal

This revision implements `delete-unit-v1` for AWBW `2026-07-10`.

The command is legal during the active player's unit-action phase for a ready,
owned unit on the board. It provides no funds, resources, or other
compensation. Cargo units are not eligible because they have no board position.

If the unit occupies a property with incomplete capture progress, deletion
restores that progress to 20. The event order is `capture-changed` first when
needed, then `unit-removed` with reason `delete`. If this removes the owner's
last unit, normal rout, property, team, and match-completion consequences
follow the removal.

Deleting a loaded transport also removes its cargo, because cargo has no board
position to fall back to. Each carried unit gets its own `unit-removed` with
reason `carrier-lost`, in slot order, after the transport's own removal. This
matches the loss of a transport in combat.

Deletion is not a unit strike and generates no power charge. Removal event
projection uses the recipient's before/after visibility; a hidden enemy unit
is not revealed by a deletion event.
