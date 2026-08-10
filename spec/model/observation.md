# Recipient observation

Status: normative for feature `fog-observation-v1`. The vision operators this
document consumes are defined by [`semantics/fog.md`](../semantics/fog.md).

## Contract

For a ruleset `R`, authoritative state `S`, and player `p`, `observe(R,S,p)`
returns the canonical value described by `schema/observation.schema.json`. `p`
MUST name a player in `S`; any player may be a recipient, including one whose
status is `resigned`, `timed-out`, or `eliminated`, and including every player
of a `finished` match. Observation is pure: it consumes no random token, changes
no state, and reads no history or ambient cache.

`observe-events(R,S,S',E,p)` returns an ordered array conforming to
`schema/observed-event.schema.json`. It is a projection, not a filtered copy of
`E`. The two endpoints are sufficient for this revision: projections use
pre-command and post-command visibility; they do not expose an event-by-event
intermediate state. A future rule needing intermediate visibility MUST revise
the interface or add authoritative intermediate facts before becoming
normative.

Throughout, `t` is `p`'s team, `pre` is `observe(R,S,p)`, and `post` is
`observe(R,S',p)`.

## Canonical state projection

The observation contains:

- the ruleset reference and recipient;
- settings, complete immutable terrain layout, board dimensions, public teams,
  public turn and weather, and the public match result;
- every tile, row-major. `visibility` says whether its mutable fields are
  current. Terrain and teleporter topology are map data and are always present
  when applicable;
- players sorted by authoritative player order. `relation` is `self`, `ally`,
  or `opponent`. Funds are team-private. Exact power charge, power-use count,
  commander identity, active slot, active power and status are public. A client
  can derive the current COP and SCOP costs from the public commander state and
  the referenced ruleset;
- own and allied units, including cargo, plus enemy on-board units satisfying
  `visible-unit`. Each unit carries a tagged `ref`, never an untyped `id`.
  Friendly references sort first by ascending numeric unit ID, followed by
  enemy references sorted by position. No placeholder represents an omitted
  unit. Own and allied units use `{type:"friendly",unit:n}`, where `n` is their
  authoritative unit ID. An enemy unit instead uses
  `{type:"enemy",position:[x,y]}`, where `(x,y)` is its visible board position.
  Every other included unit field is exact, except that `hp` is `hidden` when
  the unit owner's active commander hides HP and the recipient is an opponent.
  A unit's owner and allies always receive its exact HP. Enemy cargo is omitted,
  even when its transport is visible. Own/allied cargo uses its authoritative
  transport and slot;
- active match draw offers only from the recipient's team, sorted by player ID.
  A finished outcome is public.

Settings remain public, but secrets are recipient-specific even when fog is
disabled. With fog disabled every mutable tile and every exposed on-board enemy
unit is visible; a submerged Sub or hidden Stealth still requires concealment
detection. Opponent funds, cargo and draw offers remain private.

### Unit references

An enemy reference `{type:"enemy",position:[x,y]}` is a locator, not a
persistent identity. It denotes the visible enemy occupying `(x,y)` in that
observation only. It changes when the unit moves, is absent while the unit is
hidden, and MAY later denote a different enemy that becomes visible at the
same position. A consumer MUST NOT use it to correlate units across
observations.

The tagged union makes friendly identity and enemy location structurally
disjoint. An enemy reference has no unit-ID member, so it cannot accidentally
carry or be interpreted as an authoritative ID. This requires no secret,
random value, or recipient history and preserves the purity of `observe`.

Canonical commands use numeric authoritative unit IDs inside the trusted
transition system. A client-facing adapter MUST NOT disclose enemy IDs or
accept a guessed authoritative enemy ID. When a client selects
`{type:"enemy",position:[x,y]}`, the adapter resolves the currently visible
enemy at `(x,y)` for that authenticated recipient and then constructs the
canonical command internally. A stale, hidden, absent, or ambiguous reference
resolves exactly like an invalid target.

### Tile projection

For each position `q`, `visibility` is `visible` exactly when
`visible-position(R,S,t,q)` holds, and `fogged` otherwise.

For a fogged tile, `owner`, `capture_points`, `silo`, `destructible_hp`, and
`trait_state` MUST be absent. For a visible tile, every applicable authoritative
mutable field MUST be present with its current value.

`terrain` and `teleporter` are always present, at every visibility, because they
are immutable map data supplied at match creation. The `air-only` vision level
of `semantics/fog.md` produces a `fogged` tile and is never itself represented:
a recipient distinguishes it only by observing an air unit standing on a fogged
tile.

The base map is supplied at match creation, while AWBW replay `discovered`
records are per-action reveal updates rather than evidence of a last-seen
authoritative value. Revision 0.1.0 therefore defines no remembered mutable tile
state and does not extend `S`. This decision is deliberately narrower than
claiming all AWBW clients display no cached presentation.

## Command knowledge and equivalence

Validation may use public fields, all self/allied unit and resource fields, and
the absence/presence of enemy units in this observation. An allied unit is
therefore always a disclosed blocker. It MUST NOT use an omitted enemy fact to
choose a different violation. Execution may discover the first hidden enemy
occupant at the documented movement-trap boundary
(`semantics/movement.md`).

Define `S1 ~=p S2` iff `observe(R,S1,p) = observe(R,S2,p)`. Implementations MUST
preserve these properties:

1. Substituting hidden enemies, including their IDs, kinds, statistics, action,
   concealment, cargo and count, does not alter the observation.
2. Ordering is computed after replacing enemy IDs with position-scoped
   references and never exposes an authoritative ID order or array index.
3. Validation of equal observed legal intents in equivalent states has the same
   public result until execution reaches a documented trap boundary.
4. Equivalent transitions have equal observed-event arrays whenever their
   visible consequences are equal.
5. Neither observation operation consumes randomness or mutates either state.

Property 1 is a consequence of `semantics/fog.md` rather than an independent
constraint: `vision-level` reads only team members' units and tile owners, so no
enemy unit contributes to it, and every enemy-derived member of the observation
is gated on `visible-unit`. Implementations that compute vision incrementally
MUST NOT let a removed or substituted enemy perturb a cached vision array.

## Event projection

Projection processes authoritative events in order. Each event contributes zero
or more observed elements; elements produced by one event remain adjacent and
retain their relative order. Events not covered by a rule below contribute
nothing.

**Projection is driven by authoritative events, not by a visibility diff.** A
unit that entered the recipient's vision because the *recipient's own* unit moved
generates no authoritative event about itself, and therefore no observed element.
The recipient learns of it from `post`, which lists it. Consumers MUST treat the
observation as authoritative for what exists and the observed events as
authoritative for what happened; neither is derivable from the other. AWBW's
replay records carry per-action `discovered` updates that serve the same purpose,
and `observe` subsumes them.

Two derived values are used throughout:

- `reason-of(e)` is the authoritative event's `reason` member when it has one,
  and otherwise the event's `type`. Observed `unit-changed`, `unit-removed`, and
  `tile-changed` all carry a `reason`, while several authoritative events do
  not; this rule fixes the substitute without inventing vocabulary.
- `snapshot(u)` is `u`'s member of `post`, which is the recipient-safe object
  `schema/observation.schema.json#/$defs/unit` describes.
- `observed-ref(u,q)` is `{type:"friendly",unit:u.id}` for an own or allied
  unit and `{type:"enemy",position:q}` for an enemy at visible position `q`.
  Where `q` is clear from context, this is abbreviated `observed-ref(u)`.

### The unit-fact rule

Most events assert something about one unit. Let `vis-pre(u)` mean `u` appears
in `pre.units` and `vis-post(u)` mean `u` appears in `post.units`. For an event
`e` asserting a fact about a surviving unit `u`, emit:

| `vis-pre(u)` | `vis-post(u)` | element |
| --- | --- | --- |
| yes | yes | `unit-changed{unit: observed-ref(u), state: snapshot(u), reason: reason-of(e)}` |
| no | yes | `unit-appeared{unit: snapshot(u), position: u's post board position}` |
| yes | no | `unit-disappeared{unit: observed-ref(u), position: u's pre board position}` |
| no | no | nothing |

This single rule is what makes concealment project correctly without a special
case. An enemy Sub that dives out of the recipient's detection range is
`vis-pre` and not `vis-post`, so it produces `unit-disappeared`; one that
surfaces inside detection range produces `unit-appeared` carrying a full
snapshot the recipient is now entitled to. A recipient who never had detection
sees nothing either way, and a teammate of the Sub's owner always takes the
first row.

The rule applies only to a unit still present in `S'`. A unit absent from `S'`
is reported by its removal event instead, so a lethal `unit-damaged` followed by
`unit-removed` yields one element, not two.

The two middle rows read a board position, which is always available when they
are reached: an enemy cargo unit can never become visible, and an own or allied
cargo unit is visible at both endpoints and so takes the first row. Load and
unload, the transitions that do cross between board and cargo, have their own
rules below.

**Appearance and disappearance are transition-level facts.** `vis-pre` and
`vis-post` are fixed for the whole transition, so a unit can never both appear
and disappear within one projection. A projection MUST therefore contain at most
one `unit-appeared` and at most one `unit-disappeared` per unit ID: the element
is emitted at the position of the first event that calls for it, and every later
event that would repeat it contributes nothing. A Sub that moves and then dives
beyond detection produces a single `unit-disappeared`, positioned where the move
event stood, and not one per authoritative event.

`unit-changed` is not deduplicated. Its `reason` distinguishes the facts, and a
recipient must be able to count them — two damage events in one combat exchange
are two observations even when the second snapshot is the only one that
survives.

`unit-appeared` carries a complete unit object because the recipient has no
prior value to update. `unit-changed` also carries a complete object rather than
a delta, so that a before/after numeric payload can never leak a hidden
endpoint.

Events projected by the unit-fact rule: `unit-action-changed`, `unit-damaged`
(when the unit survives), `unit-repaired`, `unit-resourced`,
`concealment-changed`, `automatic-repair`, and — once per listed unit, in the
event's array order — `automatic-supply`.

### Movement

`unit-moved{unit, from, to, path, fuel_spent}`:

- If `unit` is owned by a team member, emit
  `unit-moved{unit, from, to, path}` with the full actual path. `fuel_spent` is
  dropped: the recipient reads the resulting fuel from `post`.
- Otherwise, when neither `vis-pre(unit)` nor `vis-post(unit)`, emit nothing;
  hidden-to-hidden movement is invisible.
- Otherwise, when `vis-pre` and not `vis-post`, emit
  `unit-disappeared{unit, position: from}`.
- Otherwise, when `vis-post` and not `vis-pre`, emit
  `unit-appeared{unit: snapshot(unit), position: to}`.
- Otherwise both hold. Let `observable-at(q)` mean that `visible-unit` would
  hold for `unit` if it stood at `q`, evaluated in either `S` or `S'`. Let `R`
  be the ordered subsequence of path positions for which `observable-at` holds.
  Emit
  `unit-moved{unit: {type:"enemy",position:to}, from, to, path: R}`.

`R` always contains the origin and destination, because `vis-pre` and
`vis-post` hold there. It need not be contiguous: when a route crosses a
concealing woods or reef tile, or otherwise dips through fog, the positions
before and after the hidden run remain in `path` while the hidden positions are
omitted. `from` and `to` remain the endpoints the recipient genuinely observed.
A hidden prefix, a hidden suffix, hidden intermediate positions, and the fuel
delta are never disclosed.

`movement-trapped{unit, blocker, position}` becomes `movement-stopped{unit}` for
the actor's team only, and is omitted for every other recipient. It names
neither the blocker nor the blocked coordinate. The blocker's own team learns
nothing from this event; if the trapped mover became visible to them, the
`unit-moved` projection above already says so.

### Creation, removal, and transport

`unit-created{unit, kind, owner, position}`: emit
`unit-appeared{unit: snapshot(unit), position}` when the unit is owned by a team
member or `vis-post(unit)` holds, and nothing otherwise.

`unit-removed{unit, reason}` and the source side of `units-joined`:

- when `unit` is owned by a team member, emit
  `unit-removed{unit, reason: reason-of(e)}`;
- otherwise, when not `vis-pre(unit)`, emit nothing;
- otherwise, when `visible-position(R,S',t,q)` holds for the unit's pre-state
  board position `q`, emit `unit-removed{unit, reason: reason-of(e)}` — the
  recipient watched the tile empty and may be told why;
- otherwise emit `unit-disappeared{unit, position: q}`. The recipient knows only
  that it is gone.

`unit-damaged` with `to_hp: 0` is followed by the removal that
`semantics/combat.md` requires; the damage event itself contributes nothing
because the unit is absent from `S'`, and the removal event carries the fact.

`units-joined{source, target}` emits the source element above, then the target
element from the unit-fact rule, in that order.

`unit-loaded{unit, transport, slot}`: the cargo unit leaves the board. Team
members take the unit-fact rule's first row and receive `unit-changed` with a
cargo location. Any other recipient with `vis-pre(unit)` receives
`unit-disappeared` at the pre-state board position, because enemy cargo is never
visible; a recipient without `vis-pre` receives nothing.

`unit-unloaded{unit, transport, position}`: the reverse. Team members receive
`unit-changed`; any other recipient receives `unit-appeared` when
`vis-post(unit)` holds, and nothing otherwise.

### Tiles

`tile-owner-changed`, `tile-terrain-changed`, `capture-changed`,
`silo-changed`, and `destructible-damaged` each emit
`tile-changed{position, tile, reason: reason-of(e)}` when the position satisfies
`visible-position` in `S` or in `S'`, and nothing otherwise. `tile` is the
position's entry in `post`, so a tile that changed and then went dark carries a
`fogged` payload. A capture reset at a position the recipient cannot see is
therefore never disclosed.

### Team-private facts

`funds-changed{player, from, to, reason}` emits
`player-changed{player, state: post's entry for that player, reason}` when
`player` is a team member, and nothing otherwise. Opponent funds are absent from
the observation, so there is no payload that could be emitted.

`power-charge-changed{player, commander_slot, from, to, reason}` is global. It
emits one `player-changed` containing the post-state player entry to every
recipient. Opponent commander entries expose exact charge but continue to omit
funds only.

`draw-offer-changed{player, offered}` emits
`public-event{kind: "draw-offer-changed"}` when `player` is a team member, and
nothing otherwise. This follows the state projection, which restricts
`match.own_team_offers` to the recipient's team. Draw negotiation belongs to
the hosting service; this projection supports imported or server-owned offer
state without exposing a gameplay command.

`automatic-supply` and `automatic-repair` are **not** team-private. They assert
ordinary unit facts and take the unit-fact rule, so an opponent watching a
visible unit refuel observes the change. Only the accompanying `funds-changed`
is restricted. An earlier revision of this document listed them as team-private,
which contradicted the rule that every included unit's schema fields are exact;
that defect is resolved in favour of the unit-fact rule.

### Public facts

`area-strike-resolved` is copied unchanged to every recipient. Commander-power
missile centers, policies, radius, damage, strike index, and order are public
even when fog hides one or more affected units. Each subsequent
`unit-damaged` still takes the ordinary unit-fact projection, so the public
impact coordinate does not reveal a hidden unit.

`phase-changed`, `turn-selected`, `day-advanced`, `weather-changed`,
`power-activated`, `power-ended`, `commander-swapped`,
`player-status-changed`, `team-eliminated`, and `match-completed` each emit
`public-event{kind}` with `kind` equal to the authoritative event's `type`, for
every recipient.

`public-event` carries no payload by design. It is a signal that a public fact
changed; the recipient obtains every updated value from `post`, which is
authoritative for all of them. Two consecutive public events of the same kind
for different subjects therefore project to two identical elements, preserving
count and order without naming the subjects.

### Facts with no public envelope

`attack-resolved` and `random-outcome` are omitted for every recipient. An
attack's visible consequences reach the recipient through the damage, resource,
removal, and tile events that accompany it, each carrying the authoritative
`reason`. A random outcome that produced no visible consequence produces no
observed element, which is exactly the noninterference the luck model requires.

## Appearance is not creation

Appearance and disappearance are observations, not claims of creation or
destruction. `unit-appeared` may report a unit that has existed for many turns
and merely walked into vision; `unit-disappeared` may report a unit that is
alive, submerged, and adjacent to nothing. `unit-removed` is the only element
that asserts a unit ceased to exist, and it is emitted only to recipients
entitled to that fact.

Unit references may appear in these events only because the corresponding
reference is present in the recipient's pre- or post-observation. Friendly
references contain authoritative numeric IDs; enemy references are
position-scoped, contain no ID, and are taken from the applicable endpoint. An
implementation MUST NOT expose an authoritative enemy identifier.
