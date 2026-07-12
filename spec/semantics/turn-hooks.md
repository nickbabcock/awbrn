# Turn-start automatic hooks

Status: normative for the automatic `turn-start` hooks that `semantics/turn.md`
deliberately deferred — weather expiry, automatic resupply, fuel upkeep, air/sea
crash removal, and paid property repair — under AWBW ruleset revision
`2026-07-10`, as feature `turn-hooks-v1`. This document specifies the state
changes and events produced inside the `model/phases.md` `turn-start` hook
positions 3 and 5 through 8. The control loop that reaches these positions is
`semantics/turn.md`; the event envelope is `schema/event.schema.json`.

`turn-hooks-v1` is a strict superset of `turn-boundary-v1`: it reduces the same
accepted `end-turn` or `tag` boundary, but no longer
requires the deferred hooks to be no-ops. Where `turn-boundary-v1` demanded a
state in which weather, supply, fuel, crash, and repair changed nothing
(`semantics/turn.md` "Scope"), `turn-hooks-v1` gives those hooks executable
meaning. Validation, precedence, successor selection, day advancement, income,
action normalization, and the victory-checkpoint timing are unchanged from
`semantics/turn.md` and are not restated here.

## Scope

This feature makes the following `model/phases.md` `turn-start` steps
executable, in their fixed positions, for the selected active player `s`:

- step 3 — **weather expiry and explicit random-weather selection**;
- step 5 — **automatic resupply** from owned properties, adjacent owned APCs, and
  owned cruiser/carrier cargo;
- step 6 — **fuel upkeep** for air and sea units;
- step 7 — **crash removal** of air/sea units whose fuel is exhausted, and of
  their cargo; and
- step 8 — **paid property repair**.

Income (step 4/7 of the two documents) and action normalization (step 10) keep
the `semantics/turn.md` behavior, including conversion of a Von Bolt
`immobilized` action to `spent` for the selected player's turn. The hooks run
per category in the order above, not interleaved per unit: every
automatic-supply source is resolved before any fuel upkeep, every crash before
any repair.

The following remain **out of scope**, deferred rather than guessed exactly as
`model/phases.md`, `model/state.md`, and `semantics/turn.md` leave them:

- **Random-weather sampling policy.** The reducer consumes an already-resolved
  semantic outcome and does not interpret a seed, integer roll, probability, or
  PRNG state. A host may implement AWBW's distribution, including
  commander-dependent weighting, but that sampling policy is outside the
  transition contract.
- **Commander effects of any kind** — Eagle's air-fuel reduction, Rachel's
  extra repair bar, per-property income bonuses, and instant/1.5× modifiers.
  All hooks here are commander-neutral, mirroring `combat-neutral-v1`; the base
  `fuel_per_turn`, base `cost`, and a two-bar repair cap are used.
- **Power expiry** (`model/phases.md` turn-start step 2).
- **Elimination and victory caused by an automatic hook** (step 9). A unit that
  crashes may leave its owner with no units. Under `turn-hooks-v1` alone a crash
  fixture MUST leave the active player with at least one surviving unit, so no
  elimination is reached and no cargo-carrying transport is the player's last
  unit. Feature `elimination-v1` (`semantics/elimination.md`) specifies the rout
  checkpoint at step 9, the cascade, and the successor re-selection that follows
  when the incoming player is eliminated during their own `turn-start`, as a
  strict superset of this feature; it lifts that restriction.
- **Custom recipient relations.** The AWBW profile gives transport supply
  `targets: "owned-units"`, so an owned APC does not top off an adjacent
  teammate's unit. A custom ruleset may select `friendly-units` instead. Property
  and cargo supply are inherently same-owner in this profile.

Fog is no longer excluded. Feature `fog-observation-v1` (`semantics/fog.md`,
`model/observation.md`) projects every automatic event this feature emits:
`automatic-supply` and `automatic-repair` take the unit-fact rule, a crash takes
the removal rule, `funds-changed` stays team-private, and `weather-changed`
is public. A `turn-hooks-v1` fixture MAY set `settings.fog = true`; the earlier
requirement that it be false is withdrawn. Note that weather is a vision input —
rain reduces every sight radius by one — so a fixture that expires rain changes
what its recipients observe, which is exactly what the projection now specifies.

Every hook is deterministic for its complete input. A random-weather selection
consumes one explicit semantic token; all other hooks consume none.

## Terms and derived values

For the selected active player `s` and the authoritative pre-state after income
has been granted (`semantics/turn.md` step 7):

- `domain(u)` is the acting unit kind's `domain` in `units.json`: `ground`,
  `air`, or `sea`.
- A tile `t` **repairs** `u` when `t.owner = s`, `u` stands on `t`, and `t`'s
  terrain kind carries the trait `repairs-ground` for a ground unit,
  `repairs-air` for an air unit, or `repairs-sea` for a sea unit (`terrain.json`).
  In the AWBW profile that is: `city`, `base`, and `hq` repair ground; `airport`
  repairs air; `port` repairs sea. `com-tower` and `lab` repair nothing.
- The **repair set** `R(s)` is the set of living units owned by `s` on a tile
  that repairs them.
- `upkeep(u)` is `u`'s start-of-turn fuel consumption from `units.json`
  `fuel_per_turn`: the `hidden` value when `u` is a submerged sub or a hidden
  stealth (`concealment = hidden` and the kind declares a `hidden` upkeep),
  otherwise the `normal` value. Ground kinds have `normal = 0`. No AWBW kind has
  a positive `normal` upkeep off water except air kinds, so upkeep is nonzero
  only for air and sea units.
- The **resupplied set** `Q(s)` is the set of units owned by `s` that a supply
  source reaches this turn: every unit on a tile that repairs it, every unit
  orthogonally adjacent to an owned APC on the board, and every unit loaded as
  cargo in an owned cruiser or carrier. Membership in `Q(s)` is by geometry and
  ownership, independent of whether the unit's resources actually change.
- `visual-hp(hp) = ceiling(hp / 10)` (`semantics/combat.md`).
- `heal-cost(u) = cost(kind(u)) / 10`, the funds for one visual bar; every
  profile `cost` is a whole multiple of `1000`, so this is an exact integer
  (`semantics/repair.md`).

All derived values are read from the authoritative pre-state and the state-bound
`Γ`. The hooks do not recompute ownership or terrain partway through, but funds
consumed by repair are spent sequentially (below).

## Execution

The hooks execute in the fixed order below, atomically, between
`semantics/turn.md` step 6 (`phase-changed` into `turn-start`) and step 10
(action normalization). Income (step 7 of `semantics/turn.md`) is granted
immediately before the supply hook and after weather expiry.

### Step 3 — Weather expiry and selection

`weather.remaining_turns` counts the player-turn boundaries an active override
survives (`model/state.md`). At the selected player's `turn-start`:

1. If `weather.remaining_turns = 0`, weather is not a temporary override; do
   nothing and emit nothing.
2. Otherwise decrement `weather.remaining_turns` by one. If it is now above zero,
   the override persists with the same `kind`. If it reaches zero, the override
   has expired: set `weather.kind` to the fixed base weather
   `settings.weather`. When the setting is `random`, expiration instead selects
   `clear` for this player-turn and consumes no token; this is the forced clear
   turn after a temporary non-clear override.
   Emit `weather-changed { from: kind-before, to: kind-after,
   remaining_turns: remaining-after, reason: "expiry" }`.

`weather-changed` is emitted for the decrement itself
because it mutates authoritative weather state; a same-`kind` decrement still
changes `remaining_turns` and is therefore a fact.

When `weather.remaining_turns = 0` at the start of this step and
`settings.weather = "random"`, request exactly one token:

```json
{ "type": "weather-selection", "value": "clear" | "rain" | "snow" }
```

The token is the resolved semantic outcome, not a seed or numeric sample. Set
`weather.kind` to its value and leave `weather.remaining_turns = 0`. Emit
`random-outcome { kind: "weather-selection", outcome: value }`, followed by
`weather-changed { from, to: value, remaining_turns: 0,
reason: "random-weather" }` only when the kind actually changes. Selecting the
current kind consumes the token and emits `random-outcome` but no no-op
`weather-changed`.

This selection occurs once for every selected player-turn not occupied by a
temporary override or its forced-clear expiry turn. If a crash eliminates that
selected player and the automatic loop selects another, the next player may
request the next token in the same boundary execution. Missing, wrong-kind, or
out-of-domain weather input is an atomic execution error. Extra trailing tokens
are ignored and are not counted as consumed.

### Step 5 — Automatic resupply

Resupply tops off fuel and ammunition to the unit maxima (`units.json`,
`0` ammo for an ammo-less kind), for free, from three source categories resolved
in this fixed order. A unit is attributed to the first source that reaches it;
once at both maxima it is unchanged by any later source.

1. **Properties.** For each tile that repairs an owned unit, in row-major
   (`y`, then `x`) order, refill that unit's fuel and ammo. Emit
   `automatic-supply { source: [x, y], units: [unit] }` when the unit's fuel or
   ammo actually changed.
2. **APCs.** For each APC owned by `s` on the board, in ascending unit-ID order,
   refill the fuel and ammo of every orthogonally adjacent unit (Manhattan
   distance one) that satisfies the capability's `targets` relation and that a
   prior source has not already topped off. The AWBW relation is `owned-units`.
   Emit `automatic-supply { source: apc-id, units: [ids...] }` with the changed
   units ascending by ID, when at least one changed.
3. **Cargo transports.** For each cruiser or carrier owned by `s`, in ascending
   unit-ID order, refill the fuel and ammo of each of its cargo units. Emit
   `automatic-supply { source: transport-id, units: [ids...] }` with the changed
   cargo ascending by ID, when at least one changed.

`automatic-supply` names the source and the topped-off units but carries no
before/after values; the maxima are derivable from `units.json` and are asserted
in the end state, exactly as `units-joined` asserts merged resources without
restating them (`model/events.md`). A source that changes nothing (every reached
unit was already full) emits no event.

### Step 6 — Fuel upkeep

Fuel upkeep applies **only when the day being started is at least `2`**
(`turn.day >= 2` after any round-boundary increment); the initial day-one turns
consume no upkeep. When it applies, for each living air or sea unit owned by `s`
**not** in the resupplied set `Q(s)`, in ascending unit-ID order:

- set `u.fuel = max(u.fuel - upkeep(u), 0)`; and
- emit `unit-resourced { unit: u, fuel_before, fuel_after, ammo_before: a,
  ammo_after: a, reason: "fuel-upkeep" }` when `0 < fuel_after < fuel_before`.

A unit whose fuel reaches `0` here is not reported by `unit-resourced`; it
crashes in step 7 and is reported solely by its `unit-removed`, so a doomed
unit produces exactly one fact. Units in `Q(s)` were just refilled or are on a
repairing property; AWBW does not drain their fuel, so they are skipped rather
than drained-after-refill. Ground units and units with `upkeep(u) = 0` change
nothing and emit nothing. Ammo is never consumed by upkeep, so
`ammo_before = ammo_after`. Unlike automatic supply, upkeep has no dedicated
coarse event; the per-unit `unit-resourced` fact keeps the event stream complete
for a surviving unit's decrement, honoring the `semantics/turn.md` rule that
every state mutation emits its fact.

### Step 7 — Crash removal

After upkeep, any air or sea unit owned by `s` whose fuel is now `0` crashes. For
each such unit, in ascending unit-ID order:

1. Emit `unit-removed { unit: u, reason: "fuel-depleted" }` and remove `u`.
2. If `u` carried cargo, remove each cargo unit and emit
   `unit-removed { unit: cargo, reason: "carrier-lost" }` in ascending slot
   order, immediately after `u`'s removal. Cargo is lost with its transport
   (`model/state.md` cargo invariants; the transport's board slot is gone).

Only air and sea units crash; a ground unit at `0` fuel is stranded, never
removed. A unit in `Q(s)` never reaches `0` here because it was skipped by
upkeep. If a crash leaves `s` owning no units, the resulting rout is **not**
applied by this feature (see Scope); such a state is inadmissible as a
`turn-hooks-v1` fixture and is instead specified by `semantics/elimination.md`,
whose rout checkpoint sits at step 9, immediately after this hook.

### Step 8 — Paid property repair

Repair heals hit points for a funds cost; the fuel and ammo of every unit in
`R(s)` were already refilled in step 5. Funds are spent sequentially, so an
earlier unit's repair can exhaust funds needed by a later one. For each unit `u`
in `R(s)`, in ascending unit-ID order:

1. Let `vh = visual-hp(u.hp)` and `missing = 10 - vh`. If `missing = 0`, `u` is
   already at full visual HP: no heal, no cost, no event.
2. Otherwise let `bars = min(2, missing, floor(S.players[s].funds /
   heal-cost(u)))`. AWBW heals at most two visual bars per turn from a property.
   - If `bars = 0`, `s` cannot afford even one bar: `u.hp` is unchanged and no
     event is emitted. The free step-5 resupply still stands.
   - Otherwise set `S.players[s].funds -= bars * heal-cost(u)`, set
     `u.hp = min(vh + bars, 10) * 10`, and emit
     `automatic-repair { unit: u, position: [x, y], hp_restored: u.hp - hp-before,
     cost: bars * heal-cost(u) }`.

Healing rounds `u.hp` up to its current bar and then adds `bars` bars, so a
fractional-bar unit may gain more than `bars * 10` exact HP; this is the same
documented AWBW rounding used by `semantics/repair.md`. `automatic-repair`
carries the funds `cost` inline, so — unlike the Black Boat's generic
`unit-repaired`, which has no cost field and therefore pairs with a
`funds-changed` (`semantics/repair.md`) — property repair emits **no** separate
`funds-changed`; the running total is asserted in the end state and each unit's
charge is recorded by its `automatic-repair.cost`.

## Event ordering

The full `turn-start` event stream for a continuing boundary, extending the
`semantics/turn.md` table with the hook facts, is:

| # | Event | Emitted when |
| --- | --- | --- |
| 1 | `phase-changed` (`unit-action`→`turn-end`) | always |
| 2 | `day-advanced` | only on a round boundary that does not finish the match |
| 3 | `turn-selected` | always (play continues) |
| 4 | `phase-changed` (`turn-end`→`turn-start`) | always |
| 5 | `weather-changed` (`reason: "expiry"`) or `random-outcome` then optional `weather-changed` (`reason: "random-weather"`) | according to the step-3 expiry/selection branch |
| 6 | `funds-changed` (`reason: "turn-start-income"`) | only when income is positive |
| 7… | `automatic-supply` | once per supply source that changed ≥1 unit, properties (row-major) then APCs then cargo transports (ascending ID) |
| 8… | `unit-resourced` (`reason: "fuel-upkeep"`) | once per air/sea unit outside `Q(s)` that survives with less fuel (`0 < fuel_after`), ascending ID, only when `turn.day >= 2` |
| 9… | `unit-removed` (`reason: "fuel-depleted"` / `"carrier-lost"`) | once per crashed unit and each of its cargo, transport then cargo (slot order) |
| 10… | `automatic-repair` | once per `R(s)` unit that actually heals, ascending ID |
| 11… | `unit-action-changed` (`reason: "turn-start"`) | once per surviving unit owned by `s` whose action changes, ascending ID; `immobilized` becomes `spent`, otherwise non-ready becomes `ready` |
| 12 | `phase-changed` (`turn-start`→`unit-action`) | always |

Notes:

- Rows 5 through 10 are the hooks this feature adds; rows 1–4, 6, 11, and 12 are
  unchanged from `semantics/turn.md`. A crashed unit (row 9) is gone before
  action normalization (row 11) and never appears there.
- The category boundaries are strict: all `automatic-supply` precede all
  `unit-resourced` upkeep, which precede all `unit-removed` crashes, which
  precede all `automatic-repair`. This is the `model/phases.md` step 5→6→7→8
  order and makes the stream reproducible.
- A unit both adjacent to an owned APC and on a repairing property is attributed
  to the property (the earlier source); it is supplied once and, being in
  `Q(s)`, skips upkeep.

## Victory checkpoints

`turn-hooks-v1` introduces no reachable terminal outcome. Crash-driven
elimination and the rout checkpoint at `model/phases.md` turn-start step 9 are
deferred with the elimination model (see Scope), so no fixture may drive the
match to `finished` through a hook. The day-limit checkpoint timing is inherited
unchanged from `semantics/turn.md`.

## Evidence

Corroborated implementation:

- WarsWorld's `applyPassTurnEvent`
  (`src/shared/match-logic/events/handlers/passTurn.ts`) runs, for the next
  player, weather update, income, then per unit: `propertyRepairAndResupply` for
  a unit on an owned matching repair facility (which resupplies then heals),
  otherwise fuel drain and removal on `fuel <= 0` for non-`base` (air/sea)
  facilities, then `APCresupply` of owned neighbours. `getTurnFuelConsumption`
  returns `5` for airport units (`2` for copters, `8` for a hidden stealth) and
  `1` for port units (`5` for a hidden sub) — matching `units.json`
  `fuel_per_turn`. `propertyRepairAndResupply` heals
  `min(2, floor(funds / (buildCost/10)), 10 - visualHP)` bars and deducts
  `bars * buildCost/10`, matching the two-bar cap, the `cost/10` per-bar price,
  and the sequential affordability used here. Its Rachel `+1` bar is a commander
  effect and is excluded.
- The AWBW replay `End`/`NextTurn` action itself
  (`AWBW-Replay-Player/AWBWApp.Game/API/Replay/Actions/EndTurnAction.cs`) carries
  `nextFunds`, `supplied` (a set of unit IDs set to max fuel and ammo), and
  `repaired` (a map of unit ID to new HP, also set to max fuel and ammo),
  corroborating the `automatic-supply { units }` and
  `automatic-repair { unit, hp_restored }` shapes. Its fuel loop consumes
  `FuelUsagePerTurn` only when `NextDay > 1` and only for units **not** already
  in the supplied/repaired set, and removes an `Air`/`Lander`/`Sea` unit at
  `fuel <= 0` — corroborating the day-≥2 gate, the resupplied-set upkeep skip,
  and air/sea-only crash used here.

Documentation-only:

- AWBW Wiki: at the start of a player's turn, that player collects income and
  units on owned properties are repaired and resupplied; APCs resupply adjacent
  friendly units; air and sea units consume fuel each turn and are lost when it
  runs out; property repair restores up to two HP bars for a cost proportional
  to the unit's value; and temporary weather lasts until the same player's next
  turn.

Known deferral:

- Random-weather token mapping, commander-power weather (including multi-boundary
  snow), power expiry, Eagle's fuel reduction and other commander modifiers,
  allied (cross-owner) APC supply, and the crash/capture elimination cascade with
  its rout checkpoint all require additional evidence and dedicated
  specifications, and are excluded rather than guessed, consistent with
  `model/state.md`, `model/phases.md`, and `semantics/turn.md`.
