# Join: `move-join`

Status: normative for the `move-join` command — same-kind merge, combined
HP/fuel/ammo, and overflow refund — under AWBW ruleset revision `2026-07-10`, as
feature `join-v1`. Movement is the shared prefix of `semantics/movement.md`;
capture interruption when the moving unit leaves a property is
`semantics/capture-reset.md`; visual HP is defined by `semantics/combat.md`. The
relevant events are `unit-moved`, `units-joined`, and `funds-changed` in
`schema/event.schema.json`.

## Scope

`move-join` moves a unit onto an allied unit of the **same kind** and merges the
two into one. The moving unit is consumed; the stationary target survives with
combined resources. This feature covers commander-neutral joining for every unit
kind under the AWBW profile.

Out of scope in this revision, deferred rather than guessed:

- Commander effects of any kind; joining here is commander-neutral, mirroring
  `combat-neutral-v1`. In particular, the overflow refund uses the profile's
  base `cost` and no commander build-cost modifier.
- Joining units that carry cargo. The command is rejected in that case (below);
  transferring cargo across a join is not a modeled behavior in this revision.

Fog is no longer excluded. The trap that suppresses the merge is inherited from
`semantics/movement.md`, and feature `fog-observation-v1` (`semantics/fog.md`,
`model/observation.md`) projects both the merge and the trap. The join target is
an allied unit, which `model/observation.md` always discloses to the actor, so
no hidden fact can change this command's validation. A `join-v1` fixture MAY set
`settings.fog = true`; the earlier requirement that it be false is withdrawn.

`move-join` consumes no random token.

## Join terms

For pre-state `S`, acting player `p`, command `{ player, unit, path, target }`,
moving unit `m = unit`, and target unit `t = target`:

- `visual-hp(hp) = ceiling(hp / 10)` (`semantics/combat.md`). A unit is at
  *full visual HP* when `visual-hp(hp) = 10`.
- `kind(u)`, `max-fuel(u)`, `max-ammo(u)`, and `cost(u)` are read from
  `units.json` for the unit's kind.
- `m` *carries cargo* when some living unit's `location` is
  `{ type: "cargo", transport: m, ... }`; likewise for `t`. Cargo is represented
  only by a cargo unit's location (`model/state.md`).
- `m` and `t` are *allied* when both are owned by players on one team
  (`semantics/movement.md`). `join-v1` fixtures are single-team, so allied means
  co-owned in practice.

All derived values are read from the authoritative pre-state and the state-bound
`Γ`; execution does not recompute them against another state.

## Validation and precedence

The moving unit `m` travels the shared movement prefix of
`semantics/movement.md` and ends on `t`'s board position. Malformed paths fail
`command.schema.json` first. Otherwise `validate` returns exactly one violation,
extending the shared movement order with family-specific target checks in the
`semantics/movement.md` family slot:

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
INVALID_TARGET   (target is not an allied on-board unit of the same kind)
INVALID_TARGET   (path destination is not the target's board position)
INVALID_TARGET   (target is already at full visual HP)
INVALID_TARGET   (the moving unit or the target carries cargo)
```

- `INVALID_TARGET` (`target: target`) when `t` is unresolved, is not allied, is
  not on the board, or `kind(t) != kind(m)`. Joining requires an identical kind;
  a different allied kind is a load case (`semantics/transport.md`), not a join.
- `INVALID_TARGET` (`target: destination`) when `destination(path)` is not `t`'s
  board position; a join must end on the target's tile.
- `INVALID_TARGET` (`target: target`) when `visual-hp(t.hp) = 10`. A full unit
  cannot receive a join, because the merge would waste the moving unit for at
  most a refund the game does not grant against a full target. This HP gate is
  **asymmetric**: it constrains only the stationary target `t`, never the moving
  unit `m`. A full-HP `m` may legally join a hurt `t` (the excess simply
  overflows to a funds refund), whereas a hurt `m` may not join a full-HP `t`.
  Swapping which of two units moves therefore changes legality, not merely the
  arithmetic.
- `INVALID_TARGET` (`target: target` or `target: unit`) when `t` or `m` carries
  cargo. A loaded transport may neither be joined into nor be the mover.
- As with `move-load`, the target legitimately occupies the destination, so the
  shared occupancy scan licenses `t` at `p_k` and `DESTINATION_OCCUPIED` does not
  arise for a well-formed join. The scan still rejects any occupant of an
  intermediate `p_i`, `i < k`.

A well-formed join never inspects `t`'s action state: a unit may be joined into
whether it is `ready` or `spent`.

Validation is pure, mutates nothing, and requests no random token.

## Execution

Execution applies the movement prefix, then merges, atomically. Let `A` be the
actual path (equal to the intended path when no fog trap intervenes; a trap
suppresses the merge entirely per `semantics/movement.md`). In order:

1. If `A` has at least two positions and `m` was its origin tile's current
   capturer, reset that capture and emit `capture-changed`
   (`semantics/capture-reset.md`).
2. Move `m` along `A`, subtracting `path-cost(Γ, m, A)` from its fuel; emit
   `unit-moved`. Let `f_m` be `m`'s fuel **after** this subtraction — the merge
   uses the post-move fuel, so movement cost reduces the fuel contributed to the
   survivor.
3. Compute the merge against `t` and the moved `m`:
   - `combined-vhp = visual-hp(t.hp) + visual-hp(m.hp)`.
   - `t.hp = min(combined-vhp, 10) * 10`. The survivor's HP is always a whole
     number of visual bars; the merge may **raise** total exact HP because each
     unit's fractional bar rounds up before summing (this "generated" HP is the
     documented AWBW behavior).
   - `t.fuel = min(f_m + t.fuel, max-fuel(t))`.
   - `t.ammo = min(m.ammo + t.ammo, max-ammo(t))`.
   - `t.action = spent`, even when the target was `ready` before the join.
   - `t`'s owner, position, and concealment are unchanged.
4. Remove `m` and emit `units-joined { source: m, target: t }`. This event
   entails `m`'s removal; no separate `unit-removed` is emitted. `m` never rests
   on `t`'s tile — the transient two-units-on-one-tile configuration exists only
   within this atomic transition and is never an exposed state, mirroring how a
   lethal strike's `to_hp: 0` is an event value, not a state value
   (`semantics/combat.md`).
5. If `combined-vhp > 10`, refund the overflow: let
   `refund = (cost(t) / 10) * (combined-vhp - 10)`, set
   `S.players[p].funds += refund`, and emit
   `funds-changed { player: p, from, to, reason: "unit-join" }`. Because every
   profile `cost` is a whole multiple of `1000`, `cost(t) / 10` is an exact
   integer. When `combined-vhp <= 10` there is no refund and no `funds-changed`.

The state remains in `unit-action`; joining introduces no victory checkpoint.

## Event ordering

| # | Event | Key fields | Emitted when |
| --- | --- | --- | --- |
| 1 | `capture-changed` | `position`, `from`, `to: 0` | only when `A` had ≥2 positions and `m` was capturing its origin |
| 2 | `unit-moved` | `unit: m`, `from`, `to`, `path: A`, `fuel_spent` | always (a join always steps onto the target's tile) |
| 3 | `units-joined` | `source: m`, `target: t` | always |
| 4 | `funds-changed` | `player: p`, `from`, `to`, `reason: "unit-join"` | only when `combined-vhp > 10` |

The survivor's combined HP, fuel, and ammo are asserted in the post-state, not
restated in `units-joined`, which carries only the two identities. The refund is
its own economy fact, consistent with production keeping `unit-created` and
`funds-changed` distinct (`semantics/production.md`).

## Evidence

Corroborated implementation:

- WarsWorld's move handler (`src/shared/match-logic/events/handlers/move.ts`)
  treats a path ending on a same-`type` allied unit as a join, rejecting a
  full-`visualHP` target and either unit having a loaded unit. Its apply step
  drains movement fuel first, then sets the survivor's fuel to
  `min(mover.fuel + target.fuel, initialFuel)`, sums visual HP, refunds
  `(buildCost / 10) * (newVisualHP - 10)` funds when the sum exceeds ten bars,
  sets HP to `min(newVisualHP, 10) * 10`, sums ammo, and removes the mover.
- AWBW Replay Player's `JoinUnitAction` removes the joining unit, overwrites the
  joined unit with the merged snapshot, and applies the replay's post-join funds,
  confirming the mover is consumed and the survivor keeps its identity.

Documentation-only:

- AWBW Wiki: two allied units of the same type may be joined by moving one onto
  the other; the result's HP is capped at ten bars and HP over the cap is
  refunded as funds.

Known conflict / deferral:

- WarsWorld's `setAmmo` does not clamp to the maximum, so its join can leave a
  survivor above `max-ammo`. The AWBW profile clamps ammo at `max-ammo(t)`,
  matching the fuel clamp and the impossibility of holding more than a unit's
  maximum; WarsWorld is not evidence for the unclamped sub-rule.
- Joining into a `ready` target spends the survivor; it cannot act after the
  merge. Cargo transfer across a join and commander refunds are excluded rather
  than guessed. A fog-trap join is not excluded:
  `semantics/movement.md` already fixes its authoritative behavior — the merge
  is suppressed and the actor stops short — and `model/observation.md` fixes
  what each recipient observes.
