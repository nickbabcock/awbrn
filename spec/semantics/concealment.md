# Concealment: `move-hide` and `move-reveal`

Status: normative for the `move-hide` and `move-reveal` commands — entering and
leaving a concealment mode for the Sub and Stealth — under AWBW ruleset revision
`2026-07-10`, as feature `concealment-v1`. Movement is the shared prefix of
`semantics/movement.md`; the concealment capability relation is
`model/unit-capabilities.md` and
`rulesets/awbw/2026-07-10/unit-capabilities.json`. The relevant events are
`unit-moved` and `concealment-changed` in `schema/event.schema.json`.

## Scope

`move-hide` moves a concealment-capable unit and then conceals it;
`move-reveal` moves it and then exposes it. In the AWBW profile the
concealment-capable kinds are the **Sub** (mode `submerged`, entered by the
replay `Dive`/`Hide` and left by `Surface`/`Unhide`) and the **Stealth** (mode
`hidden`). Both canonical commands are *directional*: `move-hide` targets the
concealed state and `move-reveal` targets the exposed state.

This feature covers only the authoritative concealment **state transition** and
the action/fuel bookkeeping of the movement prefix. Deliberately out of scope in
this revision, deferred rather than guessed:

- **The elevated hidden fuel upkeep.** A concealed unit drains more fuel per turn
  (`units.json` `fuel_per_turn.hidden`: Sub `5` vs `1`, Stealth `8` vs `5`). That
  drain is a **turn-start** upkeep hook that `semantics/turn.md` defers; the
  `move-hide`/`move-reveal` command itself charges only movement fuel and never
  applies the upkeep.
- **Combat/targeting consequences.** Whether a concealed Sub or Stealth may be
  targeted, and by what, is a combat/fog matter that `semantics/combat.md`
  already excludes from `combat-neutral-v1`.
- Commander effects of any kind; concealment here is commander-neutral, mirroring
  `combat-neutral-v1`.

**Visibility is no longer out of scope.** What a recipient sees of a concealed
unit — the entire point of concealment in play — is specified by feature
`fog-observation-v1`: `semantics/fog.md` clause 5 of `visible-unit` requires an
orthogonally adjacent allied unit, or an allied-owned property at the concealed
unit's position, to detect it, and `model/observation.md` projects the resulting
`concealment-changed` event as `unit-changed`, `unit-appeared`, or
`unit-disappeared` according to the recipient's detection at each endpoint.
Sub/Stealth concealment is independent of the map Fog setting: disabling Fog
reveals the map and exposed units, but a concealed unit still requires
adjacency or an enemy-owned property to be detected. A `concealment-v1` fixture
MAY use either Fog setting; the existing command fixtures remain valid because
they assert authoritative state rather than recipient observations.

Neither command consumes a random token.

## Concealment representation

- A unit's `concealment` is the closed enum `exposed | hidden` (`model/state.md`,
  `schema/state.schema.json`). A single `hidden` value means "concealed";
  `move-hide` sets it and `move-reveal` clears it.
- The capability's per-kind `mode` (`submerged` for `sub`, `hidden` for
  `stealth`) is a presentation label derived from the unit's kind. It is **not**
  separately stored: a submerged Sub and a hidden Stealth share the single state
  value `hidden`, and `concealment-changed` reports the `exposed`/`hidden`
  transition, not the flavor.
- A unit is *concealment-capable* when `kind(u)` has a `concealment` entry in
  `unit-capabilities.json` — exactly `sub` and `stealth` in this profile. A valid
  state never marks a non-concealment-capable unit `hidden`
  (`check-invariants`).

## Validation and precedence

Both commands carry `{ player, unit, path }` and no target. Malformed paths fail
`command.schema.json` first. Otherwise `validate` returns exactly one violation,
extending the shared movement order of `semantics/movement.md` with a single
family-specific action check:

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
ACTION_NOT_SUPPORTED   (unit cannot enter/leave concealment as this command requires)
DESTINATION_OCCUPIED
```

- `ACTION_NOT_SUPPORTED` (`action: "move-hide"` or `action: "move-reveal"`) in
  either of two cases: the acting unit's kind is not concealment-capable, or the
  unit is already in the state the command targets — a `move-hide` on an
  already-`hidden` unit, or a `move-reveal` on an already-`exposed` unit. Both
  are redundant or impossible transitions and yield the same capability-shaped
  violation, which names the unavailable action rather than the reason. Because
  the canonical commands are directional, a no-op toggle is rejected rather than
  silently accepted.
- `DESTINATION_OCCUPIED` applies normally: concealment licenses no destination
  occupant. A one-position path (conceal or reveal in place) is legal and has
  cost zero, exactly like a one-position `move-wait`.

Validation is pure, mutates nothing, and requests no random token.

## Execution

Execution applies the movement prefix, then flips concealment, atomically.
Because neither the Sub nor the Stealth can capture, the movement prefix never
emits `capture-changed`. Let `A` be the actual path. In order:

1. Move `u` along `A`, subtracting `path-cost(Γ, u, A)` from its fuel and setting
   `u`'s action to `spent`; emit `unit-moved`. The action transition is entailed
   by `unit-moved` (`semantics/movement.md`). A fog trap suppresses the
   concealment follow-up exactly as for any `move-*` command.
2. Flip concealment and emit `concealment-changed { unit, from, to }`:
   - `move-hide` sets `u.concealment = hidden` and reports `from: exposed,
     to: hidden`.
   - `move-reveal` sets `u.concealment = exposed` and reports `from: hidden,
     to: exposed`.

No extra fuel is charged for the mode change itself; the elevated hidden upkeep
is a deferred turn-start effect (see Scope). The state remains in `unit-action`;
concealment introduces no victory checkpoint.

## Event ordering

| # | Event | Key fields | Emitted when |
| --- | --- | --- | --- |
| 1 | `unit-moved` | `unit: u`, `from`, `to`, `path: A`, `fuel_spent` | always (a one-position path still emits it, with zero fuel) |
| 2 | `concealment-changed` | `unit: u`, `from`, `to` | always |

There is no composite concealment event. `unit-moved` carries the position and
fuel fact and entails the action spend; `concealment-changed` carries only the
mode flip.

## Evidence

Corroborated implementation:

- WarsWorld's ability handler
  (`src/shared/match-logic/events/handlers/ability.ts`) resolves the Sub/Stealth
  hide as `unit.data.hidden = !unit.data.hidden`, a pure state flip with no
  immediate fuel charge; its `getTurnFuelConsumption`
  (`.../passTurn/consumeFuelAndCrash.ts`) applies the elevated hidden drain
  (Stealth `8`, Sub `5`) only at the pass-turn boundary, confirming the upkeep is
  a turn-start effect rather than part of the hide command.
- AWBW Replay Player's `HideUnitAction`/`UnhideUnitAction` set the Sub/Stealth
  dived flag true/false with an optional preceding move and no immediate fuel
  cost, and are distinct directional actions rather than one toggle — matching
  the directional `move-hide`/`move-reveal` split.

Documentation-only:

- AWBW Wiki "Sub" and "Stealth": diving/hiding conceals the unit from enemies
  (except adjacent ones) and raises its per-turn fuel consumption. The
  "except adjacent ones" clause is the detection rule now specified as clause 5
  of `visible-unit` in `semantics/fog.md`; the fuel consequence belongs to the
  deferred turn-start upkeep.

Known deferral:

- The elevated hidden fuel upkeep and concealed-target combat compatibility each
  require additional specification (`semantics/turn.md`, `semantics/combat.md`)
  and are excluded rather than guessed. Fog visibility of concealed units is no
  longer deferred; it is specified by `semantics/fog.md`. WarsWorld models
  concealment as an unconditional
  toggle; the AWBW profile's directional commands reject a redundant no-op
  instead, so WarsWorld is not evidence for accepting one.
