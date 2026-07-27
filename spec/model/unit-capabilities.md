# Unit capabilities

Capabilities are relations between unit kinds and actions. They are separate
from scalar unit statistics and weapon data.

The capability table uses closed-world semantics: if a unit kind is absent
from a capability section, it does not have that capability. Implementations
MUST NOT infer capabilities from domain, movement class, cost, name, or ammo.

## Capture

`capture` enumerates unit kinds that may perform the capture command. Property
eligibility and capture arithmetic are defined by capture semantics.

## Elevated vision

`elevated_vision` enumerates unit kinds that receive a terrain's `vision_bonus`
when standing on it. A kind absent from the list ignores the bonus entirely.
The magnitude lives on the terrain, not here, so one list serves every
elevated-vision terrain a profile defines. Sight arithmetic is defined by
`semantics/fog.md`.

This relation is independent of every other section. That the AWBW profile's
`elevated_vision` and `capture` lists happen to hold the same two kinds is a
coincidence of that profile, and an implementation MUST NOT derive either from
the other or from movement class.

## Transport

Each transport definition gives a capacity and an explicit set of cargo unit
kinds. Cargo eligibility MUST NOT be broadened based on unit domain.

Loading and unloading additionally require a board position valid for both the
transport and cargo operation. Those positional rules belong to transport
semantics, not this relation table.

Destroying a transport also destroys its cargo. Cargo is not an on-board unit:
it does not occupy a tile, act, provide vision, receive area effects, or count
as an independently targetable unit while loaded unless another rule explicitly
states otherwise.

## Supply and repair

Supply entries state their trigger, spatial relation, targets, and effects.
`targets: "owned-units"` restricts recipients to units with the supply source's
owner. `targets: "friendly-units"` also permits units owned by another player
on the source owner's team. Ruleset profiles select the relation; consumers
MUST NOT treat team membership alone as supply eligibility.
Repair is distinct from supply even when the same action also refills fuel and
ammo. Payment and exact-HP restoration are defined by repair semantics.

## Concealment and special actions

Concealment entries define named modes and their enter/exit commands. Visibility,
targeting, and fuel consequences are defined elsewhere.

Special actions name behavior that cannot be inferred from ordinary weapon
data. Their transition semantics require dedicated rules before conformance can
be claimed.
