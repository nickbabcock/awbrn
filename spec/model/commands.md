# Canonical commands

A command is a complete intent submitted by an authority. JSON member order is
irrelevant; array order is significant. The normative syntax is
`schema/command.schema.json`.

Command identifiers are stable kebab-case and player commands contain `player`.
A command carries no timestamp, client sequence number, UI selection, animation
hint, calculated cost, or claimed outcome. Match-service operations such as
draw negotiation, timeout adjudication, and cancellation are outside this
gameplay command surface.

Schema acceptance means only that an intent is well shaped. It does not imply
that its references resolve or that it is legal in the current state.

## Paths and atomic actions

Every `move-*` command contains a path including its origin and destination. A
one-position path means the unit acts without moving. The path is submitted
intent, not a trusted result.

Movement and its follow-up are atomic. The core command families are:

| Command | Semantic intent |
| --- | --- |
| `move-wait` | Move, then spend the unit without another action. |
| `move-attack` | Move, then attack a unit or destructible tile target. |
| `move-capture` | Move, then capture the destination property. |
| `move-load` | Move the unit into a transport. |
| `move-join` | Move and merge into an allied unit. |
| `move-supply` | Move and manually supply eligible adjacent units. |
| `move-repair` | Move and use a targeted repair capability. |
| `move-hide` | Move and enter the unit's concealment mode. |
| `move-reveal` | Move and leave the unit's concealment mode. |
| `move-launch` | Move a capturing-capable unit onto a silo and launch it. |
| `move-explode` | Move and execute the unit's self-destruct action. |
| `delete-unit` | Remove an eligible owned unit without compensation. |

`move-attack.target` is a tagged union. A unit target uses a stable numeric
`UnitId`; a tile target uses a coordinate and is legal only for ruleset-declared
destructible tile state such as an AWBW pipe seam. The attack specification must
also require any positional relationship needed at validation/execution time.
Cargo units cannot be targeted merely because their IDs exist.

The numeric enemy `UnitId` is an internal canonical reference. Recipient
observations do not expose it. A client-facing adapter resolves a visible
position-scoped enemy reference before it constructs the command. For an atomic
`move-attack`, that resolution uses the pre-command observation. Movement in the
command cannot supply a new enemy reference for the same command.

`move-load.transport` and `move-join.target` are stable numeric `UnitId`s. Their final
path position is validated against the referenced unit's board position.

`move-launch.target` is the missile impact coordinate. The command does not
list affected units; execution derives them from authoritative state. Under
AWBW revision `2026-07-10`, an Infantry or Mech must complete movement on a
ready silo and the target must be in bounds.

`move-explode` is the Black Bomb self-destruct command. Its affected units are
derived from authoritative state after resolving the submitted movement path.

## Transport commands

`unload` names a transport, cargo unit, and destination. It has no transport
path: the transport is already at its authoritative location. AWBW's free
unload behavior may permit this command even after the transport has spent its
ordinary action, but the transport specification must define cargo action
state, adjacency, terrain, occupancy, and repeated unload legality.

Loading remains `move-load` because the cargo unit moves into the transport.
Cargo order is represented by the resulting location slot, which execution
chooses as the lowest available legal slot; clients do not choose a slot.

## Production and powers

`produce-unit` names the producing tile by coordinate and the requested unit
kind. Cost, capability, ownership, bans, lab requirements, occupancy, and unit
ID allocation are derived during validation/execution. The command MUST NOT
supply a new unit ID.

`activate-power.level` is `cop` or `scop`. It always applies to the player's
active commander; a client cannot activate an inactive tag slot directly.

`tag` swaps commander slots and ends the turn. `end-turn` ends it without a
swap. Both run the complete automatic boundary closure defined by
`phases.md`.

## Match lifecycle

`resign` is a player command. Its elimination and outcome consequences are
ruleset transitions, specified by `semantics/elimination.md`: it is a boundary
command that eliminates the resigning player and then runs the same automatic
closure as `end-turn`.

Draw negotiation, timeout adjudication, and match cancellation belong to the
hosting service. Terminal draw and cancellation outcomes may still be imported
or represented as match state, but AWVM defines no commands that produce them.

## Feature status

This document and schema reserve the canonical syntax so adapters and later
semantics use one vocabulary. A command is executable only when its feature
specification defines:

- applicability and violation precedence;
- all state changes and event order;
- `Γ` operators and rounding;
- random-token requests, if any;
- victory checkpoints; and
- fog-safe validation/observation behavior.

The manifest's `features` array is the authoritative list of executable slices;
it currently covers `move-wait` (`semantics/movement.md`), `move-capture`
(`semantics/capture.md`), `produce-unit` (`semantics/production.md`), `move-load`
and `unload` (`semantics/transport.md`), `move-join` (`semantics/join.md`),
`move-supply` (`semantics/supply.md`), `move-repair` (`semantics/repair.md`),
`move-hide` and `move-reveal` (`semantics/concealment.md`), scalar-only
`activate-power` (`semantics/powers.md`), the `end-turn` boundary
(`semantics/turn.md`), `resign` (`semantics/elimination.md`), `delete-unit`
(`semantics/delete.md`), and unit plus
destructible-tile `move-attack` targets (`semantics/combat.md`), `move-launch`
(`semantics/launch.md`), and `move-explode` (`semantics/explode.md`). The
remaining commands are
syntax-level commitments, not claims that their transitions are settled.

## AWBW replay adapter correspondence

AWBW replay actions are transition evidence and adapter inputs, not the
canonical command wire format. They frequently contain post-action snapshots,
funds, visibility-targeted payloads, generated IDs, and next-turn results that
a player neither chooses nor may know beforehand.

The initial adapter correspondence is:

| AWBW replay action | Canonical intent | Notes |
| --- | --- | --- |
| `Move` | `move-wait` | Replay path/result must be converted to submitted intent. |
| `Fire` | `move-attack` with unit target | Combat payload becomes evidence/events, not command input. |
| `AttackSeam` | `move-attack` with tile target | Targets destructible tile state. |
| `Capt` | `move-capture` | Building, vision, and income fields are results. |
| `Build` | `produce-unit` | Replay-generated unit ID is not accepted from the command. |
| `Join` | `move-join` | New funds and resultant unit are derived results. |
| `Load` | `move-load` | Loaded/transport IDs identify intent after adapter resolution. |
| `Unload` | `unload` | Destination is derived from the resulting unit snapshot. |
| `Supply` | `move-supply` | Supplied-unit lists are derived results. |
| `Repair` | `move-repair` | Funds and repaired snapshot are derived results. |
| `Hide` / `Unhide` | `move-hide` / `move-reveal` | Replay naming is adapter vocabulary. |
| `Power` | `activate-power` | Effect payload must be reproduced by `Γ` operators. |
| `End` | `end-turn` | `updatedInfo` represents automatic boundary closure. |
| `Tag` | `tag` | Also represents boundary closure. |
| `Delete` | `delete-unit` | Hidden recipient payload is observation data. |
| `Resign` | `resign` | Next-turn/game-over payloads are transition results. |

The replay format does not distinguish every canonical intent directly. It is
outcome-oriented and recipient-targeted, and its HP snapshots are display
precision rather than authoritative exact HP. An adapter MUST therefore report
when a replay lacks enough information for an exact conformance pre-state or
random-token reconstruction; it MUST NOT fill gaps by treating replay output as
player input.
