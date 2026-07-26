# Transport: `move-load` and `unload`

Status: normative for the `move-load` and `unload` commands, cargo slot
assignment, and the AWBW free-unload rule under AWBW ruleset revision
`2026-07-10`, as feature `transport-v1`. Movement is the shared prefix of
`semantics/movement.md`; capture interruption when a loading unit leaves a
property is `semantics/capture-reset.md`; the cargo representation and its
invariants are `model/state.md`. The relevant events are `unit-loaded` and
`unit-unloaded` in `schema/event.schema.json`.

## Scope

`move-load` moves a cargo-eligible unit onto an owned transport and loads it.
`unload` is a standalone action that places one loaded unit onto an adjacent
tile. This feature covers commander-neutral loading and unloading for the AWBW
transport table (`unit-capabilities.json`).

Out of scope in this revision, deferred rather than guessed:

- Commander effects of any kind; transport here is commander-neutral, mirroring
  `combat-neutral-v1`.
- The base-game unload rule (see the AWBW divergence below): AWVM's AWBW profile
  is normative only for the AWBW free-unload branch.

Fog introduces no hidden-occupancy branch for `unload`. Its destination must be
orthogonally adjacent to the owned on-board transport. Every unit contributes
at least one tile of sight, adjacent concealing terrain has `vision_limit: 1`,
and voluntarily hidden units are detected by any adjacent friendly unit
(`semantics/fog.md`). Therefore any unit occupying a legal unload destination
is disclosed before the command is submitted, and `DESTINATION_OCCUPIED` cannot
leak a hidden fact.

`move-load` targets an owned transport, which is always disclosed to the actor,
and both commands' events project under `model/observation.md` — an enemy's
`unit-loaded` becomes `unit-disappeared` for anyone who could see the cargo,
since enemy cargo is never observable, and an enemy's `unit-unloaded` becomes
`unit-appeared` for anyone who can see the drop tile. A `transport-v1` fixture
MAY set `settings.fog = true`.

Neither command consumes a random token.

## Transport terms

- The `transport` capability gives each transport kind a `capacity` and an
  explicit `cargo` kind set (`unit-capabilities.json`). Cargo eligibility MUST
  NOT be broadened by unit domain.
- Cargo is represented only by a cargo unit's `location`
  (`{ "type": "cargo", "transport": id, "slot": n }`); a transport carries no
  second list (`model/state.md`). Occupied slots are dense from `0` through
  `capacity-1`, and AWBW transports are always on the board (no nesting).
- A unit is *loadable* into transport `t` for player `p` when `t` is owned by
  `p`, is on the board, is a transport kind whose `cargo` set includes the cargo
  unit's kind, and has at least one free slot.

## `move-load`

The command is `{ player, unit, path, transport }`. The cargo `unit` travels the
shared movement prefix and ends on the transport's board position, then loads.

### Validation

Malformed paths fail `command.schema.json` first. Otherwise `validate` returns
one violation, extending the shared movement order of `semantics/movement.md`:

```text
AUTHORITY_REQUIRED
MATCH_FINISHED
WRONG_PHASE
NOT_ACTIVE_PLAYER
UNIT_NOT_FOUND
UNIT_NOT_OWNED
UNIT_NOT_ON_BOARD
UNIT_ALREADY_ACTED
PATH_ORIGIN_MISMATCH
PATH_NON_ADJACENT
PATH_REPEATED_POSITION
PATH_OUT_OF_BOUNDS
TERRAIN_IMPASSABLE
PATH_OCCUPIED
INSUFFICIENT_MOVEMENT
INSUFFICIENT_FUEL
INVALID_TARGET   (transport is not a loadable transport for this cargo kind)
INVALID_TARGET   (path destination is not the transport's board position)
```

- `INVALID_TARGET` (`target: transport`) when the referenced transport is not
  loadable per the definition above — unresolved, not owned, not on board, wrong
  kind for this cargo, or already full.
- `INVALID_TARGET` (`target: destination`) when `destination(path)` is not the
  transport's board position; a load must end on the transport.
- The shared occupancy scan applies to `p_1 .. p_(k-1)` as usual, but the
  transport occupying the destination `p_k` is the load's *licensed* occupant,
  so it does not raise `DESTINATION_OCCUPIED` (`semantics/movement.md`). Because
  a valid destination holds only the declared transport, `DESTINATION_OCCUPIED`
  does not arise for a well-formed load.

Loading does not inspect or require the transport's action state: a transport
may receive cargo whether it is `ready` or `spent`.

### Execution

Execution applies the movement prefix, then loads, atomically:

1. If the actual path has at least two positions and the cargo was its origin
   tile's current capturer, reset that capture and emit `capture-changed`
   (`semantics/capture-reset.md`).
2. Move the cargo along the actual path, subtracting fuel; emit `unit-moved`.
   This entails the cargo's `ready`-to-`spent` transition, so no separate
   `unit-action-changed` is emitted.
3. Set the cargo's `location` to `{ type: "cargo", transport, slot }`, where
   `slot` is the lowest free slot index, and emit
   `unit-loaded { unit, transport, slot }`.

The transport's own record is unchanged: it neither moves nor changes action
state. The state stays in `unit-action`. Because a valid load always steps onto
the transport tile from an adjacent or farther origin, `unit-moved` is always
emitted (a cargo unit cannot begin on its transport's tile).

Ordered events: optional `capture-changed`, then `unit-moved`, then
`unit-loaded`.

## `unload`

The command is `{ player, transport, cargo, destination }`. It carries no path:
the transport is already at its authoritative location, and this command does
not move it (`model/commands.md`).

### Validation

```text
AUTHORITY_REQUIRED
MATCH_FINISHED
WRONG_PHASE
NOT_ACTIVE_PLAYER
INVALID_TARGET      (transport is not an owned on-board transport)
INVALID_TARGET      (cargo is not currently loaded in that transport)
TARGET_OUT_OF_RANGE (destination is not orthogonally adjacent to the transport)
TERRAIN_IMPASSABLE  (the cargo kind cannot enter or stop on the destination terrain)
DESTINATION_OCCUPIED (destination holds a unit)
```

- `INVALID_TARGET` (`target: transport`) when the transport is unresolved, not
  owned by the player, or not on the board.
- `INVALID_TARGET` (`target: cargo`) when `cargo` is not a unit whose location is
  `{ type: "cargo", transport, ... }` for that transport.
- `TARGET_OUT_OF_RANGE` (`target: destination`) when the destination's Manhattan
  distance from the transport's board position is not exactly one.
- `TERRAIN_IMPASSABLE` (`position: destination`) when the destination terrain's
  base cost for the cargo kind's movement class is `null` under the current
  weather, or when it carries the `teleporter` trait. Teleporters are
  traversable at zero cost but cannot hold a unit, so cargo cannot be unloaded
  onto one.
- `DESTINATION_OCCUPIED` (`position: destination`) when any living on-board unit
  occupies the destination.

Crucially, `unload` does **not** check the transport's action state: it is legal
whether the transport is `ready` or `spent`. See the AWBW divergence below.

### Execution

Atomically:

1. Remove `cargo` from its slot and set its `location` to
   `{ type: "board", position: destination }`.
2. Compact the transport's remaining cargo so occupied slots stay dense from `0`:
   any cargo unit in a slot above the vacated one shifts down by one. Slot
   compaction changes only cargo `location.slot` values and emits no event; the
   result is asserted in state.
3. Set the unloaded unit's action to `spent`; it cannot act on the turn it is
   unloaded. Fuel and ammo are unchanged.
4. Emit `unit-unloaded { unit, transport, position }`, which entails the unit's
   transition to `spent`.

The transport's record — position, and importantly its action state — is
unchanged. The state stays in `unit-action`.

Ordered events: a single `unit-unloaded`.

## AWBW divergence: free unload and "boosting"

AWVM's AWBW profile intentionally models AWBW behavior, which differs from the
Game Boy/DS titles specifically in how unloading interacts with the transport's
turn.

In the base games, unloading is a *sub-action of the transport's move*: a
transport may unload only at the end of a movement, unloading ends the
transport's turn, and a movement trap or a hidden enemy on the drop tile cancels
the unload entirely.

In AWBW, unloading is a *standalone free action*. A transport may unload at any
point in its turn, including after it has already moved, and unloading does not
end the transport's turn. This is why the `unload` command is pathless, is
validated as a main action rather than a movement follow-up, and does not test
the transport's action state.

This free-unload rule is what enables the community technique known as
**"boosting"** (colloquially "boosties"). Because a unit can be loaded and then
unloaded without spending the transport, a player loads a unit into a transport
at the edge of the transport's movement range and immediately unloads it,
advancing the unit up to the transport's move beyond its own range; the tactic
chains across multiple transports. The boosted unit still arrives `spent` — the
gain is repositioning, not extra actions.

WarsWorld encodes the same fork as the `unloadOnlyAfterMove` version property:
`true` selects the base-game move-and-unload behavior, `false` selects AWBW's
standalone free unload. AWVM's AWBW profile is normative for the AWBW branch
only; the base-game branch is out of scope for this revision.

## Evidence

Documentation-only:

- AWBW Wiki, "Changes in AWBW" and "Glossary": in the original games unloading
  happens only at the end of movement and ends the unit's turn, and a trap or
  hidden enemy prevents the unload; in AWBW transports may unload at any point in
  their turn, even after moving, and unloading does not end the transport's turn,
  making it a free action. The boost technique loads a unit at the edge of a
  transport's range and immediately unloads it for extra reach, chainable across
  transports.

Corroborated implementation:

- WarsWorld separates `unloadWait` (a move sub-action, requiring
  `unloadOnlyAfterMove`) from `unloadNoWait` (a standalone main action, requiring
  `!unloadOnlyAfterMove`); both mark the unloaded unit not-ready
  (`isReady: false`) and neither spends the transport. Its
  `throwIfUnitCantBeUnloadedToTile` requires the destination terrain to admit the
  cargo's movement type.
- AWBW Replay Player's `UnloadUnitAction` moves the cargo out of the transport to
  an adjacent position and removes it from the transport's cargo list, with an
  optional preceding transport move.

Known deferral:

- The base-game `unloadOnlyAfterMove = true` branch requires additional
  specification and is excluded rather than guessed. Multi-slot cargo
  compaction is fixed by the dense-slot state invariant and covered by
  conformance. A hidden enemy on an unload destination is not a deferred branch
  because adjacent transport vision makes that state unobservable only in an
  invalid or nonconforming visibility model.
