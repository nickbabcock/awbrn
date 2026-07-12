# Production and `produce-unit`

Status: normative for the `produce-unit` command, funds deduction, deterministic
unit-identifier allocation, and new-unit initialization under AWBW ruleset
revision `2026-07-10`, as feature `production-v1`. The command envelope is
`schema/command.schema.json`; the created-unit fact is `unit-created` in
`schema/event.schema.json`; identifier allocation state is `next_unit_id`, defined
by `model/state.md`.

## Scope

`produce-unit` builds one new unit of a requested kind on a production facility
the acting player owns, paying its cost. This feature covers facility/domain
eligibility, unit bans, Lab-gated unit kinds, occupancy, cost, and deterministic
identifier and stat initialization.

Out of scope in this revision, deferred rather than guessed:

- The base `production-v1` fixtures are commander-neutral. Revisioned
  cost multipliers and extra facilities are defined by
  `model/commander-profiles.md` and claimed only by narrower
  `commander-effective-values-v1.<commander>.<state>` fixtures.
Production consumes no random token.

## Terms and derived values

For pre-state `S`, acting player `p`, target coordinate `q`, and requested kind
`k`:

- `tile(q)` is the terrain and tile state at `S.board.tiles[q.y][q.x]`.
- A *facility* is a tile whose terrain kind carries a `produces-<domain>` trait.
  In the AWBW profile: `base` → `produces-ground`, `airport` → `produces-air`,
  `port` → `produces-sea` (`terrain.json`).
- `domain(k)` is the requested kind's domain from `units.json` (`ground`, `air`,
  or `sea`). A facility produces `k` only when its `produces-<domain>` trait
  matches `domain(k)`.
- `cost(k)` is the effective build cost derived from the kind's base `cost` in
  `units.json` and `effective-build-cost`.
- `max-fuel(k)` and `max-ammo(k)` are the kind's `max_fuel` and `max_ammo` from
  `units.json`; a kind with no ammo has `max-ammo(k) = 0`.
- `funds(p)` is `S.players[p].funds`.
- `lab-gated(k)` is true exactly when `k` appears in `S.settings.lab_units`.
- `owns-lab(S,p)` is true when at least one `lab` tile in `S` has owner `p`.
  A Lab owned by another member of `p`'s team does not satisfy this predicate.
- `unit-count(S,p)` is the number of living units in `S.units` whose owner is
  `p`, including units on the board and in cargo. Teammate and opponent units
  do not contribute.

All derived values are read from the authoritative pre-state and the state-bound
`Γ`; execution does not recompute them against another state.

## Validation and precedence

`produce-unit` carries `player`, `position`, and `kind`, and MUST NOT carry a
unit ID (`model/commands.md`). Schema-malformed commands fail
`command.schema.json` first. Otherwise `validate(R, S, C)` returns exactly one
violation in this order:

```text
AUTHORITY_REQUIRED
MATCH_FINISHED
WRONG_PHASE
NOT_ACTIVE_PLAYER
INVALID_TARGET   (position is not a facility p owns that produces domain(k))
INVALID_TARGET   (kind k is unknown, banned, or Lab-gated without an owned Lab)
DESTINATION_OCCUPIED
UNIT_LIMIT_REACHED
INSUFFICIENT_FUNDS
```

- `INVALID_TARGET` with `target` equal to `position` when `q` is out of bounds,
  is not a facility, is not owned by `p`, or whose `produces-<domain>` trait does
  not match `domain(k)`. The production *site* is checked before the *kind* so
  that a bad site is reported deterministically when both are wrong.
- `INVALID_TARGET` with `target` equal to `k` when the requested kind is not a
  known unit kind, appears in `settings.unit_bans`, or both `lab-gated(k)` and
  `not owns-lab(S,p)`. An empty `settings.lab_units` array gates no kinds.
  Kinds absent from the array use ordinary production rules even on a map with
  no Labs. Unit bans remain authoritative when the player owns a Lab.
- Lab possession is read from the authoritative pre-state for every production
  command. Capturing a Lab immediately enables the listed kinds; losing the
  player's last Lab immediately disables them. Only exact player ownership
  counts, not team ownership.
- `DESTINATION_OCCUPIED` (`position: q`) when any living on-board unit already
  occupies `q`. A facility must be empty to build on.
- `UNIT_LIMIT_REACHED` (`current: unit-count(S,p)`, `limit`) when
  `settings.unit_limit = limit` and `unit-count(S,p) >= limit`. A null setting
  disables the check. The limit is owner-scoped rather than team-scoped and
  counts cargo because cargo remains a living owned unit.
- `INSUFFICIENT_FUNDS` (`required: cost(k)`, `available: funds(p)`) when
  `cost(k) > funds(p)`. Equality is affordable.

Validation is pure, mutates nothing, and requests no random token.

## Execution

Execution requires an admissible pre-state, which for production includes a
present `next_unit_id` (`model/state.md`). It is atomic and consumes no random
token. In order:

1. Deduct the cost: set `S.players[p].funds = funds(p) - cost(k)` and emit
   `funds-changed { player: p, from: funds(p), to: funds(p) - cost(k),
   reason: "unit-production" }`.
2. Allocate the identifier: let numeric `UnitId` `id` be `S.next_unit_id`, then
   set `S.next_unit_id = S.next_unit_id + 1`. Because the counter exceeds every
   live unit ID (`model/state.md`), `id` is fresh.
3. Create the unit and emit `unit-created { unit: id, kind: k, owner: p,
   position: q }`. The created unit's fields are fully determined:
   - `hp = 100`;
   - `fuel = max-fuel(k)` and `ammo = max-ammo(k)`;
   - `action = spent` — a produced unit cannot act on the turn it is built;
   - `concealment = exposed`;
   - `location = { type: "board", position: q }`.

The `spent` initial action state is entailed by `unit-created`; no separate
`unit-action-changed` event is emitted, mirroring how `move-wait`'s
`unit-moved` entails its action transition. The state remains in `unit-action`;
production introduces no victory checkpoint. The Lab rule constrains creation
only: predeployed or previously produced units remain legal when their owner
lacks or loses a Lab.

Commander-provided sites, including Hachi's Merchant Union city production,
change only site eligibility. They do not bypass unit bans or the Lab gate.

## Event ordering

| # | Event | Key fields |
| --- | --- | --- |
| 1 | `funds-changed` | `player: p`, `from`, `to`, `reason: "unit-production"` |
| 2 | `unit-created` | `unit: id`, `kind: k`, `owner: p`, `position: q` |

Payment precedes the unit's appearance. `unit-created` carries only identity,
kind, owner, and position; the produced unit's derived HP, fuel, ammo, action,
and concealment are not restated in the event because they are fixed by the
ruleset defaults above. There is no composite production event.

## Evidence

Corroborated implementation:

- WarsWorld's build handler
  (`src/shared/match-logic/events/handlers/build.ts`) rejects banned kinds, a
  reached unit cap, insufficient funds, an occupied tile, and a
  facility/domain mismatch, and creates a unit owned by the building player. Its
  Hachi facility exception is a commander effect and is excluded here.
- WarsWorld reads per-kind cost from unit constants; the profile's
  `units.json` `cost` values are the same canonical AWBW build costs.

Documentation-only:

- AWBW build rules: a factory/airport/port produces units of its matching
  domain, a built unit costs funds and is spent for the turn it is produced, and
  its facility must be unoccupied.

Known modeling choice:

- Deterministic identifier allocation without host state is modeled by
  `next_unit_id` (`model/state.md`) rather than by AWBW's opaque server IDs.
  Commander production modifiers are specified separately by
  `model/commander-profiles.md`.
