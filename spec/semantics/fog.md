# AWBW fog semantics

Status: normative for feature `fog-observation-v1` under AWBW ruleset revision
`2026-07-10`.

This document defines the profile operators that `observe` and `observe-events`
consume. It is the vision half of the observation boundary; the projection half
is [`model/observation.md`](../model/observation.md).

Every term used here — domain, terrain trait, sight radius, vision bonus,
vision limit — is resolved from revisioned ruleset tables. Core code MUST NOT
branch on display names, terrain spellings, or unit kind identifiers. An
implementation MUST reject a profile that omits a required semantic field
rather than infer the field from a name.

## Scope

This feature specifies:

- the three-valued **vision level** a team holds at each board position;
- the **sight radius** of a vision source, including weather and terrain;
- **`visible-position`**, the tile-level predicate; and
- **`visible-unit`**, the unit-level predicate, including terrain concealment
  and the voluntary concealment of a submerged Sub or hidden Stealth.

Out of scope in this revision, deferred rather than guessed:

- **Commander vision outside advertised profile paths.** The Sonja paths named
  in the manifest enable the revisioned `effective-vision` and
  `reveals-concealing-terrain` operators. Other commander effects that grant or
  remove sight remain deferred.
- **Remembered tile state.** A recipient sees a fogged tile's terrain but never
  a cached last-seen owner or capture progress. See `model/observation.md`.
- **Other terrain-adapter work.** Pipe seams are now represented directly and
  specified below, but other external orientation and artwork mappings remain
  deferred with the manifest's `terrain-adapter` pending table.

Neither observation operation consumes a random token, mutates state, or reads
history.

## Vision sources

Fix a ruleset `R`, authoritative state `S`, and team `t`. A *team member* is any
player `p` in `S.players` with `p.team = t`, **including inactive, resigned, and
eliminated players** while their units and properties remain in `S`. A finished
match does not alter this calculation.

Team `t` has exactly two kinds of vision source:

- **Property source** — every position `q` whose tile has an `owner` that is a
  team member. The tile kind is irrelevant: any owned property is a source.
  Unowned and enemy-owned properties provide no source.
- **Unit source** — every living unit owned by a team member whose `location`
  is `board`. Cargo provides no independent source, and contributes nothing
  even when its transport is a source.

No other state supplies vision. In particular, an enemy unit standing in the
open provides no vision to the team observing it.

## Sight radius

For a unit source `u` at position `p`:

```text
base  = units[kind(u)].vision
bonus = terrain(p).vision_bonus   if kind(u) is in unit_capabilities.elevated_vision
        0                          otherwise
rain  = -1 if S.weather.kind = "rain" else 0
sight(R,S,u) = max(1, base + m + bonus + rain)
```

`m = effective-vision(R,S,u,base) - base`. It is zero for the neutral profile;
commander paths may advertise revisioned nonzero values.

`vision_bonus` is present on exactly the terrains carrying the
`elevated-vision` trait; in this profile that is `mountain`, with bonus `3`.
`elevated_vision` lists the kinds eligible for that bonus; in this profile that
is `infantry` and `mech`. A kind absent from the list ignores the bonus
completely, so a Recon on a mountain has its ordinary radius of five.

`elevated_vision` and `capture` happen to hold the same two kinds in this
profile. They are independent relations and an implementation MUST NOT derive
either from the other.

The clamp is applied once, to the complete sum, and the floor is **one**, not
zero. Every on-board unit therefore always reveals at least its own tile and
the four orthogonally adjacent tiles. This floor is what makes the concealment
detector rule below a consequence rather than an extra case: an adjacent allied
unit necessarily also grants full vision level at the position it detects.

## Vision level

`vision-level(R,S,t,q)` takes one of three values, ordered

```text
none  <  air-only  <  full
```

If `S.settings.fog` is false, every in-bounds position is `full` and the rest of
this section does not apply.

Otherwise, a tile carrying `always-visible` is `full` for every team. The
revisioned `pipe` and `pipe-seam` terrain carry this trait. A seam therefore
exposes both its terrain and current `destructible_hp` even when no ordinary
source reaches it.

For every other tile, the level at `q` is the **maximum**, over every vision
source, of that source's contribution. Positions reached by no source are
`none`.

- A property source at `q` contributes `full` at `q`, and nothing anywhere else.
- A unit source `u` at `p` with `r = sight(R,S,u)` contributes, at every
  in-bounds `q` with `d = |q.x - p.x| + |q.y - p.y| <= r`:

  ```text
  full      if d <= vision-limit(terrain(q))
  air-only  otherwise
  ```

`vision-limit(terrain)` is that terrain's `vision_limit` when present and
unbounded otherwise. It is present on exactly the terrains carrying the
`conceals-in-fog` trait; in this profile that is `wood` and `reef`, both with
limit `1`. When `reveals-concealing-terrain(R,S,u)` is true, that source treats
every `vision-limit` as unbounded. The override is evaluated per source and
therefore does not transfer from Sonja's units to an allied player's units.
Sonja's vision-radius modifiers are likewise evaluated only for units she owns;
team vision pools the resulting contributions without transferring her
commander effects onto a teammate's sources.

Vision uses Manhattan distance and has no line-of-sight blocker. A source
contributes at `d = 0`, so a unit standing in woods holds `full` level on its
own tile — for its own team only.

The maximum is taken over sources, so one source seeing a woods tile from range
never downgrades another source standing beside it. A team either has an
adjacent source or it does not.

## Position visibility

```text
visible-position(R,S,t,q)  <=>  vision-level(R,S,t,q) = full
```

This is the tile-level predicate. `air-only` is deliberately **not** a visible
position: a distant woods tile stays fogged, and its owner, capture progress,
silo state, destructible HP, and trait state stay hidden, exactly as if no
source reached it. The `air-only` level exists solely to admit air units
through the unit predicate below, and is never itself exposed in an
observation.

## Unit visibility

`visible-unit(R,S,t,u)` for a living unit `u` in `S.units`, evaluated in order.
The first matching clause decides.

1. If `owner(u)` is a team member — **visible**. This covers own and allied
   units, on board or in cargo, hidden or exposed.
2. Otherwise, if `u.location.type` is `cargo` — **not visible**. Enemy cargo is
   never observable, even when its transport is fully visible and even when the
   recipient could count occupied slots by other means.
3. Otherwise let `q` be `u`'s board position. If the tile at `q` has an `owner`
   that is a team member — **visible**. An enemy standing on a property the
   team owns is detected regardless of concealment or terrain.
4. Otherwise, if `u.concealment` is `hidden` — visible **iff** some living
   on-board unit owned by a team member is orthogonally adjacent to `q`.
   Ordinary range vision, however long, is insufficient; a property source at a
   different position is insufficient. Any unit kind detects: there is no
   detector-compatibility relation in this profile.
5. Otherwise, if `S.settings.fog` is false — **visible**. Disabling fog reveals
   the complete map and every exposed on-board enemy unit, but does not cancel
   the voluntary concealment of a submerged Sub or hidden Stealth.
6. Otherwise `u.concealment` is `exposed`; let `L = vision-level(R,S,t,q)`.
   Visible **iff** `L = full`, or `L = air-only` and `units[kind(u)].domain` is
   `air`.

Clause 6 is the terrain-concealment rule: woods and reefs hide ground and sea
units from range, but never hide air units, and never hide anything from a
source one tile away.

Clause 4 dominates clauses 5 and 6 rather than composing with them. A hidden
Sub or Stealth remains concealed when map fog is disabled. A hidden Stealth is
an air unit, but a hidden air unit at range is still not visible — voluntary
concealment is checked before the domain exemption is reached.

### Consequences worth stating

- **Detection implies position visibility.** Because `sight` is at least one, an
  orthogonally adjacent unit source contributes `full` at `q` (its distance is
  `1`, and `vision_limit` is `1` on every terrain that has one). So clause 5
  never reveals a unit on a position the team cannot otherwise see. This is a
  theorem of the definitions above, not an extra requirement, and it fails
  under a zero floor — see Evidence.
- **Enemy presence never grants vision.** `vision-level` reads only team
  members' units and tiles' owners, so substituting any set of enemy units
  leaves it unchanged. This is the load-bearing lemma for the noninterference
  properties in `model/observation.md`.
- **Visibility and attack compatibility are separate.** `visible-unit` decides
  whether a unit may be named as a combat target and whether it appears in an
  observation. `semantics/combat.md` separately restricts which attackers can
  damage a detected submerged Sub or hidden Stealth.

## Worked examples

All examples use the `2026-07-10` profile with `settings.fog` true.

**Infantry on a mountain in rain.** `base = 2`, `bonus = 3` (infantry is
elevated-vision eligible, mountain carries `vision_bonus`), `rain = -1`:
`sight = max(1, 2 + 0 + 3 - 1) = 4`.

**Rocket in rain.** `base = 1`, `rain = -1`: `sight = max(1, 0) = 1`. The clamp
binds, and the rocket still sees its four neighbours.

**Woods at range.** A tank with `sight = 3` at `[0,0]`, a woods tile at `[3,0]`.
`d = 3 > vision_limit 1`, so the woods tile is `air-only`: the tile projects as
fogged, an enemy tank standing there is not visible, and an enemy bomber
standing there is.

**Woods adjacent.** The same tank at `[2,0]`. `d = 1 <= 1`, so `[3,0]` is
`full`: the tile projects as visible and the enemy tank on it is visible.

**Submerged Sub.** An enemy Sub with `concealment: "hidden"` at `[4,4]`, a
friendly Battleship with `sight = 2` at `[4,6]`. `d = 2`, so `[4,4]` is `full`,
but clause 5 applies and requires adjacency: the Sub is **not** visible. Moving
the Battleship to `[4,5]` makes it visible.

**Hidden Stealth over water.** An enemy Stealth with `concealment: "hidden"`
sitting on a reef the team sees only from range. Clause 5 is reached before the
air-domain exemption of clause 6, so the Stealth is not visible.

**Enemy on an owned property.** An enemy submerged Sub cannot reach one, but an
enemy hidden Stealth over a team-owned airport is visible by clause 4, with no
adjacent unit anywhere.

## Open question

- whether an eliminated player's not-yet-removed units grant vision, which this
  document currently asserts.
- the exact interaction between a property source and a submerged Sub, which
  cannot arise in this profile because no sea terrain is capturable; and
- commander and power vision modifiers beyond the advertised Sonja paths.

This revision makes no normative claim that the historical AWBW web UI displays
allied private resources. The team-private projection in `model/observation.md`
is the AWVM command-knowledge contract, not a rendering claim.
