# Movement and `move-wait`

Status: normative for ordinary movement, authoritative fog-trap execution, and
recipient projection under AWBW ruleset revision `2026-07-10`. Recipient state
and event projection are defined by `fog.md` and `model/observation.md`.

## Scope

This document defines the shared movement prefix used by every `move-*`
command and completes `move-wait`. Other `move-*` commands use the same path
validation and movement reduction, but their follow-up actions remain
non-executable until their feature specifications define additional legality
and effects.

Coordinates are zero-based `[x,y]`, origin upper-left. A command path is the
actor's complete intended route, including its origin and intended destination.
It is not a request for the VM to find or optimize a route.

## Terms and derived values

For path `P = [p_0, ..., p_k]`, acting unit `u`, and pre-state `S`:

- `origin(P) = p_0`, `destination(P) = p_k`, and `steps(P) = k`.
- Positions are orthogonally adjacent exactly when their Manhattan distance is
  one.
- `class(u)` and base move allowance come from `units.json`.
- `terrain(p)` is the semantic terrain at `S.board.tiles[p.y][p.x]`.
- `movement-weather(Γ,u)` is the commander-aware weather used only for
  movement-cost lookup.
- `base-cost(Γ,u,p)` is the entry at
  `movement-costs[terrain(p)][movement-weather(Γ,u)][class(u)]`.
- `entry-cost(Γ,u,p)` is the effective movement cost after applying the closed
  movement-cost modifier algebra. `null` remains impassable: a modifier MUST
  NOT turn a `null` base cost into a finite cost unless a named ruleset operator
  explicitly permits it.
- `move-allowance(Γ,u)` is the effective movement-point allowance.
- For an actually traversed path `A = [p_0, ..., p_j]`,
  `path-cost(Γ,u,A) = sum(entry-cost(Γ,u,p_i), i=1..j)`. The origin costs zero.

The same `path-cost` is both movement points spent and fuel spent. Terrain and
weather therefore affect fuel: entering a cost-2 tile consumes two fuel. There
is no separate per-edge fuel formula in the AWBW profile.

All derived values are evaluated against the authoritative pre-state and the
same state-bound `Γ`. Movement does not recompute weather, commander effects,
terrain, or ownership partway through the path.

The AWBW commander algebra's `traversable-cost-set` operator replaces a finite
base cost and preserves `null`. Its weather exceptions inspect authoritative
weather even when another operator substituted weather for base-table lookup.
Sturm sets finite costs to one except in snow. The resulting entry cost is also
the fuel spent, and the same query is used by movement, join, capture, and
unload passability.

## Path form

A path MUST contain at least one position (enforced syntactically), begin at
the unit's board position, remain in bounds, and consist only of orthogonally
adjacent steps. No position may occur more than once. A one-position path is a
wait in place and has cost zero.

The prohibition on repeated positions makes paths canonical and prevents
cost-consuming loops. It applies even when the repeated route would remain
within movement and fuel allowances.

## Occupancy

Only living on-board units occupy tiles; cargo units do not occupy their
transport's tile independently. The acting unit is ignored at `p_0`.

For `move-wait`:

- a visible enemy or allied unit on any `p_i`, `i > 0`, blocks the path;
- an occupied intermediate position produces `PATH_OCCUPIED`;
- an occupied intended destination produces `DESTINATION_OCCUPIED`;
- a hidden enemy obstruction under fog is not a validation failure and is
  handled by the trap execution below; and
- a hidden allied unit is known through shared allied vision and is therefore
  treated as a disclosed obstruction, not a trap.

“Allied” means owned by any player on the actor's team. Join and load commands
may license their declared destination occupant, but never an intermediate
occupant; their feature specifications insert those destination checks after
the shared movement checks.

## Validation and precedence

Malformed paths fail `command.schema.json` before semantic validation.
`validate(R,S,C)` otherwise returns exactly one structured violation using this
order:

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
family-specific target/action checks
DESTINATION_OCCUPIED
```

Checks that inspect a path report the lowest failing index. Each category is
completed over the path before the next category begins. Thus a later
out-of-bounds position does not outrank an earlier non-adjacent step, and an
impassable tile does not outrank any out-of-bounds position.

`PATH_OCCUPIED` scans `p_1` through `p_(k-1)`. Destination occupancy is deferred
until after family-specific checks because load/join may license it. For
`move-wait`, the family-specific list is empty.

Movement and fuel compare the intended full-path cost against the effective
allowance and current fuel. Equality is legal. A one-position path is legal at
zero fuel. Hidden enemy occupancy is omitted from occupancy checks; validation
uses the terrain and cost of the full intended path and MUST NOT disclose the
obstruction through a rejection or payload.

## State-bound validated command

In addition to the binding required by `model/transition-system.md`, the
validated command binds:

- acting unit and its pre-state board position;
- intended path;
- effective entry cost for every non-origin path position;
- intended total path cost;
- effective move allowance; and
- every occupancy fact validation was permitted to observe.

Execution MUST reject a stale binding rather than silently recompute these
values against another state.

## Ordinary `move-wait` execution

When no hidden enemy traps the unit, the actual path equals the intended path.
Execution atomically:

1. when `k > 0`, resets capture progress associated with the acting unit and
   emits `capture-changed`, according to `capture-reset.md`;
2. sets the unit's board position to `p_k`;
3. subtracts `path-cost(Γ,u,P)` from fuel;
4. sets its action to `spent`; and
5. emits one `unit-moved` event containing the actual path and fuel spent.

For a one-position wait, position and fuel are unchanged, action becomes
`spent`, and `unit-moved` is still emitted with equal endpoints and zero fuel.
The action transition is entailed by `unit-moved`; no separate
`unit-action-changed` event is emitted.

Movement consumes no random token. The state remains in `unit-action` unless a
later command-specific immediate victory checkpoint finishes it; `move-wait`
itself introduces none.

Therefore an unobstructed move away from an in-progress capture emits
`capture-changed` before `unit-moved`. A one-position wait preserves progress.

## Hidden-occupancy trap execution

This section describes authoritative behavior; it does not define what any
recipient observes.

Execution scans the intended path from `p_1` onward against authoritative
on-board occupancy whenever an enemy occupant was not disclosed to the actor
during validation. Map fog is the usual cause, but a submerged Sub or hidden
Stealth can remain undisclosed even when map fog is disabled. Let `p_t` be the
lowest-index position occupied by such an enemy unit. The actual path is the
non-empty prefix
`A = [p_0, ..., p_(t-1)]`; the acting unit does not enter `p_t`.

Execution atomically:

1. if `A` contains at least two positions, resets the actor's capture progress
   and emits `capture-changed`;
2. moves the actor to `p_(t-1)` (which may equal `p_0`);
3. subtracts `path-cost(Γ,u,A)` from fuel; the blocked tile costs nothing;
4. sets the actor's action to `spent`;
5. suppresses the requested follow-up action, regardless of command family;
6. emits `unit-moved` for `A`; then
7. emits `movement-trapped` naming the actor, blocker, and blocked position.

No random token is consumed. The blocking enemy is unchanged. The trap does
not turn into an attack, join, load, or other follow-up. If `t = 1`, the
`unit-moved` event has a one-position actual path and zero fuel spent.

The authoritative trap event is secret-bearing. `observe-events` projects it
according to `model/observation.md`: only the actor's team receives
`movement-stopped`, with blocker and blocked position omitted.

## Terrain and teleporters

The AWBW `2026-07-10` movement table is the sole executable base-cost relation.
It provides clear/rain/snow costs for all eight movement classes. `null` means
impassable, including the profile's `teleporter` terrain for every class.

This teleporter rule follows the official terrain chart used by the candidate
table. The AWBW Wiki instead documents zero-cost traversal, contiguous
long-distance behavior, and a prohibition on ending there. That conflict is
recorded rather than resolved by inventing adjacency exceptions. No special
teleport edge exists in this revision; changing it requires stronger evidence,
a revised table, and explicit path syntax/semantics.

Predeployed units may occupy terrain their class cannot enter. Such a state is
not invalid merely for that reason. They may leave through an adjacent legal
tile because the origin has zero cost, but may not wait in place if a feature
requires the current tile itself to be enterable; `move-wait` does not impose
that extra requirement.

## Evidence

Documentation-only:

- AWBW official terrain chart: the profile's weather/class/terrain costs.
- AWBW Wiki “Units”: fuel spent equals movement points spent, including terrain
  and weather modifiers.
- AWBW Wiki “Fog of War”: a hidden enemy interrupts movement at first contact
  and the trapped unit loses all remaining actions.
- AWBW Wiki “AWBW Guide”: manually selected paths must fit movement and may not
  pass blocking terrain or enemy units.

Confirmed replay:

- bundled AWBW replays `1391406`, `1403019`, `1563018`, and `1578186` contain
  trapped move records with truncated recipient paths and resulting unit
  snapshots.

Corroborated implementation:

- WarsWorld's movement reducer scans the supplied path in order, rejects
  repeated positions, stops before the first enemy obstruction, marks a trap,
  suppresses the follow-up action, and charges only its resulting path.

Known conflict:

- WarsWorld currently charges fuel per traversed edge, while AWBW documentation
  states fuel equals movement points spent. The AWBW profile follows the AWBW
  documentation; WarsWorld is not evidence for that sub-rule.
- The official terrain chart and AWBW Wiki disagree on teleporter traversal.
  This revision retains the official-table `null` costs.
