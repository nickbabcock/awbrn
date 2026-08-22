# Elimination, the cascade, and `resign`

Status: normative for player elimination, the elimination cascade over an
eliminated player's units and properties, team elimination, the rout and
resignation victory checkpoints, and the `resign` command under AWBW ruleset
revision `2026-07-10`, as feature `elimination-v1`.

This document owns the shared procedure that `semantics/capture.md`,
`semantics/turn.md`, and `semantics/turn-hooks.md` each deferred. Those
documents describe *when* a player stops participating; this one describes
*what happens* when they do. Where an earlier feature specified a bounded
prefix of the procedure, that prefix is restated here in full and this document
governs.

## Scope

An **elimination** removes one player from further participation, disposes of
everything they own, and may end the match. This feature covers:

- the elimination procedure itself, parameterized by cause and beneficiary;
- the **cascade**: removal of the eliminated player's units and disposition of
  their properties, including demotion of a captured or abandoned HQ;
- **team elimination** and the single-remaining-team victory checkpoint;
- the `resign` command; and
- the **rout** checkpoint, including a rout that strikes the incoming active
  player during their own `turn-start`.

The causes in scope are `hq-capture` (from `semantics/capture.md`), `rout`, and
`resignation`. Out of scope in this revision, deferred rather than guessed:

- **`timeout`.** Timeout is a hosting-service clock/boot policy AWVM does not
  model. The `timed-out` player status and the `timeout` victory reason stay
  reserved for imported or server-projected terminal state. The procedure below
  remains parameterized so an adapter can apply the same cascade, and the
  adapter contract below fixes what that adapter MUST produce. What stays out of
  scope is the clock itself — when a timeout fires — not the shape of its
  result.
- **`lab-capture` and `capture-limit`.** Both are capture-driven checkpoints
  whose own conditions are unspecified. Once either fires it invokes exactly the
  procedure below, but neither is reachable in this feature.
- **Day-limit outcomes.** The checkpoint *timing* is fixed by
  `semantics/turn.md`; naming winning or tied teams at a day limit still needs
  AWBW's end-of-game scoring rule.
- **The zero-active-team terminal state.** When the single-remaining-team
  checkpoint is in force, a match cannot ordinarily reach zero active teams; the
  only route is a simultaneous elimination this feature already excludes. An
  `elimination-v1` fixture MUST NOT reach it, and no `draw` outcome is a
  conformance claim here.
- **Simultaneous elimination.** Two or more players reaching an elimination
  condition at one checkpoint is order-sensitive, because the first elimination
  can finish the match and pre-empt the second. `model/phases.md` forbids such a
  case from being normative until its precedence is evidenced. An
  `elimination-v1` fixture MUST reach each checkpoint with at most one player
  eliminable.
- **Commander effects of any kind**, mirroring `combat-neutral-v1`. In
  particular, what becomes of an eliminated player's active power, accumulated
  power charge, or commander-sourced weather is a commander concern; this
  feature leaves `power_state`, `power_charge`, and `power_uses` untouched on the
  eliminated player's record and requires fixtures with no active power.

Fog is no longer excluded. The cascade is a large board change, and feature
`fog-observation-v1` (`semantics/fog.md`, `model/observation.md`) projects it
per fact rather than wholesale: each removed unit takes the removal rule, so a
recipient is told `unit-removed` only where they can see the tile empty and
`unit-disappeared` otherwise; each demoted or transferred property takes the
tile rule, so a change at a position the recipient cannot see is not disclosed;
and `player-status-changed`, `team-eliminated`, and `match-completed` are public
to everyone. An `elimination-v1` fixture MAY set `settings.fog = true`; the
earlier requirement that it be false is withdrawn.

An eliminated player's units and properties stop granting vision at the moment
the cascade removes them, so a teammate's observation legitimately contracts
during the transition. `semantics/fog.md` requires that a not-yet-removed unit
of an eliminated player still grants vision, which fixes the pre-state side of
that contraction.

Elimination itself consumes no random token. A `resign` that continues into a
random-weather boundary may consume weather outcomes through
`semantics/turn-hooks.md`.

## Terms

For state `S`:

- A player is **participating** when `S.players[p].status` is `active`. The
  other three statuses (`eliminated`, `resigned`, `timed-out`) all mean the
  player has left; only `active` participates in turn order
  (`model/state.md`).
- A team is **surviving** when at least one of its member players is
  participating. `S.teams[t].status` MUST be `active` exactly when `t` is
  surviving.
- `units(S, p)` is the set of living units whose `owner` is `p`, board and cargo
  alike. Cargo owner equals transport owner (`model/state.md`), so a transport
  and its cargo are always in the same player's set.
- `tiles(S, p)` is the set of board tiles whose `owner` is `p`.
- A **demotable** property is a tile whose terrain kind declares
  `elimination_replacement` in `terrain.json`. In the AWBW profile the only such
  kind is `hq`, whose replacement is `city`. Demotion is what strips the
  `capture-defeats-owner` trait from a property that has served its purpose, so
  a demoted HQ can never eliminate its new owner.
- The **cause** of an elimination is one of `rout`, `hq-capture`, or
  `resignation`; a host adapter adds `timeout` under the adapter contract
  below. It is not stored on the player record — `model/state.md` keeps cause
  in history — and travels in the `player-status-changed`, `team-eliminated`,
  and `match-completed` events. Only the first of those is
  always emitted: a player eliminated while a teammate plays on produces
  neither of the other two, so `player-status-changed` is the one fact that
  carries every elimination's cause.
- The **beneficiary** of an elimination is the player who inherits the
  eliminated player's properties, or `null`. It is the capturing player when the
  cause is `hq-capture`, and `null` for every other cause.

## The elimination procedure

`eliminate(S, p, cause, beneficiary)` is invoked from a checkpoint that has
already established that `p` must leave. It is not a command and is never
invoked speculatively: a caller that invokes it MUST have determined that `p` is
participating in the pre-checkpoint state.

The steps run in this exact order.

### 1. Player status

Set `S.players[p].status` to `resigned` when the cause is `resignation`, to
`timed-out` when an adapter supplies the cause `timeout`, and to `eliminated`
for every other cause. Emit

```json
{
  "type": "player-status-changed",
  "player": "blue",
  "from": "active",
  "to": "eliminated",
  "reason": "rout"
}
```

`reason` is the cause, written as its reason identifier. The status
distinguishes a voluntary departure from a defeat and the reason records which
defeat, because the status alone cannot: `resigned` and `eliminated` are
equivalent for every rule in this revision — neither participates, both make
their team non-surviving, and both receive the identical cascade — and
`eliminated` covers `rout` and `hq-capture` alike.

The reason is carried here, and not left to the two later events, because
neither is guaranteed to follow. `team-eliminated` fires only when the player's
whole team stopped participating and `match-completed` only when the match
ended, so in a team match a consumer recording a per-player result would
otherwise have no cause to record. When a later event does follow, its reason
is the same value; the events agree by construction.

### 2. Team elimination

If no member of `p`'s team is still participating, set that team's `status` to
`eliminated` and emit

```json
{ "type": "team-eliminated", "team": "blue-team", "reason": "hq-capture" }
```

A team with another participating member emits nothing here and stays `active`.
The eliminated player's team membership is never rewritten.

### 3. Victory checkpoint

If exactly one team is now surviving, that team wins. Atomically set
`S.match.status = finished`, set

```json
{ "type": "victory", "winners": ["red-team"], "reason": "<cause>" }
```

as `S.match.outcome`, set `S.turn.phase = finished`, and emit
`match-completed { outcome }`. That single event entails the phase transition to
`finished`, exactly as `unit-moved` entails a `ready`-to-`spent` transition
(`model/events.md`); no separate `phase-changed` is emitted, whatever phase the
match was in when the checkpoint fired.

The victory reason is the cause of the elimination that produced the single
surviving team, mapped identically: `rout` to `rout`, `hq-capture` to
`hq-capture`, `resignation` to `resignation`. `winners` names teams, not
players, so a surviving alliance is recorded whole.

Execution **stops here**. Per `model/phases.md`, once an outcome is established
no later automatic hook, successor selection, income, or random consumption
occurs — and the cascade is one of those later effects. A match that ends on
this elimination therefore leaves the loser's units on the board and their
properties owned, exactly as they stood. This is not an omission: the board is
frozen at the winning instant, and AWBW likewise only redistributes a departed
player's holdings when there is a game left to play them in.

### 4. The cascade

Reached only when two or more teams survive. Every step below is skipped
entirely by a match that finished in step 3.

#### 4a. Units

Remove every unit in `units(S, p)`, in ascending unit-ID order. For each unit
`u`, in order:

1. If `u` is the current capturer of a tile with `capture_points < 20`, set that
   tile's `capture_points` to `20` and emit
   `capture-changed { position, from, to: 20 }` first, per the `delete/removal`
   row of `semantics/capture-reset.md`.
2. Emit `unit-removed { unit: u, reason: "elimination" }` and remove `u`.

A transport and its cargo are both owned by `p` and are both in the set, so
cargo needs no separate rule and no `carrier-lost` reason: each unit is removed
once, on its own turn in the ascending-ID pass, whether or not its transport was
removed earlier in the same pass. Ascending unit ID — not board order — is the
ordering key, because cargo has no board position.

#### 4b. Properties

Let `T` be `tiles(S, p)` extended, when the cause is `hq-capture`, with the
trigger HQ tile. That tile is already owned by the beneficiary at this point,
having been transferred by `semantics/capture.md` before the checkpoint, so it
is not in `tiles(S, p)`; it is nonetheless the very property whose demotion the
AWBW rule calls for, and it is included here for exactly that reason.

Process `T` in canonical board order — ascending `y`, then ascending `x` — as
`semantics/capture-reset.md` requires of any effect touching several tiles. For
each tile `d`, in order:

1. **Reset an interrupted capture.** If `capture_points(d) < 20`, set it to `20`
   and emit `capture-changed { position: d, from, to: 20 }`. Both remaining
   steps break the persistence conditions of `semantics/capture-reset.md` — a
   demotion makes `d` a different capturable property, and a disposition changes
   its owner — and a reset precedes the tile-change event that caused it.
2. **Demote.** If `d` is demotable, set its `terrain` to the terrain kind's
   `elimination_replacement` and emit
   `tile-terrain-changed { position: d, from, to, reason: "elimination" }`.
3. **Dispose.** If `d.owner` differs from `beneficiary`, set `d.owner` to
   `beneficiary` and emit
   `tile-owner-changed { position: d, from: p, to: beneficiary }`. When the
   beneficiary is `null` the property becomes neutral. The trigger HQ of an
   `hq-capture` already has the beneficiary as its owner and therefore emits
   nothing in this step; it still emits its demotion in step 2.

The two dispositions are the whole strategic point of the distinction. An HQ
capture hands the loser's economy to the capturing player; every other cause
returns it to neutral, which is why a defender facing an inevitable HQ capture
resigns first.

#### 4c. What the cascade does not touch

The eliminated player's `funds`, commander slots, `power_state`, `power_charge`,
`power_uses`, and position in `S.turn.order` are all unchanged. Turn order is
stable by `model/phases.md` so that eliminating a player never renumbers
historical positions; successor selection skips the player because their status
is no longer `active`, not because they left the array. `S.match.draw_offers`
likewise keeps whatever it held; draw semantics are a separate feature.

## Checkpoints

`model/phases.md` fixes where each condition is evaluated. This feature places
its three causes accordingly.

| Cause | Checkpoint | Invoked from |
| --- | --- | --- |
| `hq-capture` | command-immediate, within the capturing command | `semantics/capture.md` |
| `resignation` | within `resign`'s execution, before its boundary loop | this document |
| `rout` | command-immediate for a command-caused removal; directly after the automatic hook for a hook-caused removal | this document |

`timeout` has no row: it is not evaluated at any AWVM checkpoint. A host adapter
decides it on its own clock and applies the procedure, as the adapter contract
below fixes.

### The rout condition

A player is routed when a transition removes their **last** living unit. The
condition is triggered by the removal, not by the state predicate: after any
transition that removed at least one unit, a player `p` is routed exactly when
`units(S, p)` was non-empty immediately before that transition and is empty
immediately after.

Phrasing the condition on the transition rather than the state is what makes it
correct at the start of a match. A player who owns no unit on day one has lost
nothing and is not routed; the same player is routed the moment their first and
only unit is destroyed. A state predicate would eliminate them before they ever
built.

`move-join` can never rout its owner: the join's target survives, so the owner's
unit count falls to at least one. The removals that can rout an owner in the
currently specified features are the crash removals of
`semantics/turn-hooks.md` and the destruction of a unit at zero HP by
`semantics/combat.md`.

### Rout during `turn-start`

A crash at `model/phases.md` turn-start step 7 removes units belonging to the
player whose turn is opening. Step 9 is that hook's checkpoint, so the rout is
evaluated there, and the incoming active player can be eliminated during their
own `turn-start`.

If the elimination does not finish the match, the turn cannot open for a
non-participating player. The boundary loop instead resumes: select a successor
by continuing the cyclic scan from the eliminated player's position, apply the
round-boundary procedure if that scan wraps, and run the full `turn-start` hook
sequence for the new successor from step 1. The eliminated player receives no
action normalization and no `phase-changed` into `unit-action`. They keep
whatever income step 4 already granted them, because income precedes upkeep in
the fixed `model/phases.md` order and no step un-grants it.

The resumption does not re-enter `turn-end`. `turn.phase` is already
`turn-start` and turn-start step 1 sets it to `turn-start` again, which is not an
actual mutation, so no `phase-changed` is emitted for the hand-off: the events
are `turn-selected` for the new successor followed by that successor's hooks.
Only an actual phase mutation emits `phase-changed` (`model/events.md`), and
successor re-selection inside `turn-start` performs none.

Each resumption is an ordinary successor selection and can itself cross a round
boundary, so a single boundary command may emit more than one `day-advanced`.
The scan always terminates: every pass either finds a participating successor or
eliminates a player, and an elimination that leaves one surviving team finishes
the match in step 3 of the procedure.

## `resign`

`resign` carries `type` and `player` and no other operand
(`schema/command.schema.json`). It is a player command and a **boundary**
command: the resigning player leaves and, if the match continues, play advances
to the next participating player exactly as `end-turn` advances it.

### Validation and precedence

`validate(R, S, C)` returns exactly one violation using the common player-command
prefix of `model/violations.md` and nothing further:

```text
AUTHORITY_REQUIRED
MATCH_FINISHED
WRONG_PHASE
NOT_ACTIVE_PLAYER
```

- `AUTHORITY_REQUIRED` (`authority: "player"`) when the submission is not player
  authority. Host-service timeout handling is outside the command surface.
- `MATCH_FINISHED` when `S.match.status` is `finished`.
- `WRONG_PHASE` (`expected: "unit-action"`) when `S.turn.phase` is not
  `unit-action`.
- `NOT_ACTIVE_PLAYER` (`player: e`) when `e` is not `S.turn.active_player`.

AWBW permits resignation during the resigning player's own turn, and every
resignation observed in the replay corpus carries the same next-turn payload an
ordinary turn end carries. Out-of-turn resignation is consequently rejected by
`NOT_ACTIVE_PLAYER` rather than specified; if AWBW is later shown to accept it,
that is a new behavior revision, not a reinterpretation of this one.

Validation never mutates state and requests no random token.

### Execution

Execution is atomic and, like `end-turn`, runs the entire automatic loop through
the successor's `unit-action` or through `finished`. Let `e` be the resigning
player.

1. Set `S.turn.phase = turn-end` and emit
   `phase-changed { player: e, from: unit-action, to: turn-end }`.
2. Run `eliminate(S, e, "resignation", null)`. If it finished the match, stop;
   the phase set in step 1 is superseded by `finished` inside the procedure.
3. Otherwise continue the `turn-end` and `turn-start` reduction of
   `semantics/turn.md` from its step 3 (successor selection), with the
   `turn-hooks.md` hooks in their fixed positions. `e` is no longer selectable,
   so the scan skips them.

Placing the elimination inside `turn-end` and before successor selection is what
makes the resigning player unselectable by their own resignation and lets the
scan wrap correctly. The resigning player's units are gone and their properties
are neutral before the successor's income is computed, so a successor who owned
none of those properties gains nothing from the resignation — neutral property
produces no income for anyone.

## The `timeout` adapter contract

AWVM does not model a clock. *When* a player's time expires is a
hosting-service policy — bank size, boot delay, grace on disconnect — and no
AWVM command, checkpoint, or random token expresses it. `resign`'s
`AUTHORITY_REQUIRED` says as much from the command side: host-service timeout
handling is outside the command surface, and this document adds no `time-out`
command to reach it.

What *is* specified is the shape of the result. An **adapter** is the host-side
component that decides a player has timed out; this section fixes what it MUST
then produce, so that a booted player leaves the same shaped state and event
stream as a routed or resigned one, and a consumer recording match results needs
no separate code path for the cause. It is a clarification of the existing
procedure and introduces no new behavior, event type, or vocabulary.

### The adapter runs the same procedure

An adapter that has determined player `p` timed out invokes

```text
eliminate(S, p, "timeout", null)
```

exactly as specified above, with one substitution: step 1 sets
`S.players[p].status` to `timed-out` rather than `eliminated`. Every other step
is unchanged, and the beneficiary is `null` as it is for every cause but
`hq-capture`.

1. **Status.** Emit
   `player-status-changed { player: p, from: "active", to: "timed-out", reason:
   "timeout" }`.
2. **Team elimination.** Emit `team-eliminated { team, reason: "timeout" }` when
   no member of `p`'s team is still participating, and nothing otherwise.
3. **Victory checkpoint.** If exactly one team now survives, that team wins with
   the cause mapped identically to every other cause:
   `{ "type": "victory", "winners": ["red-team"], "reason": "timeout" }`.
   `timeout` is already a member of the `victory` reason vocabulary
   (`rulesets/awbw/2026-07-10/reasons.json`), so a 1v1 timeout produces exactly
   that outcome. Execution stops here on the same terms as any other cause: the
   loser's units stay on the board and their properties stay owned, frozen at
   the winning instant.
4. **The cascade.** Reached only when two or more teams survive, and run in
   full — an interrupted capture reset to `20`, every unit removed in ascending
   unit-ID order, then every property in board order reset, a demotable HQ
   demoted to its `elimination_replacement`, and every property disposed. With a
   `null` beneficiary the disposition is neutral throughout. A timeout is not a
   capture and enriches nobody: no `tile-owner-changed` names a player as the
   new owner.

The event ordering table below applies to a timeout unchanged, with row 1
carrying `reason: "timeout"`.

### `timed-out` is not a distinct standing

`timed-out`, `resigned`, and `eliminated` are equivalent for every rule in this
revision, extending the equivalence step 1 states for the first two. None
participates in turn order, each makes its team non-surviving, each is skipped
by successor selection, and each receives the identical cascade. No rule
branches on which of the three a player carries.

The three statuses are worth keeping distinct only because they record *how* a
seat's run ended for a reader, and even that is a coarse record: `eliminated`
covers `rout`, `hq-capture`, and `lab-capture` alike. A consumer that needs the
cause reads the `reason` of `player-status-changed` (step 1), never the status.

### The boundary

A timeout normally strikes the player holding the turn, and the boundary must
then be finished — otherwise the match sits in `unit-action` with a
non-participating player as `S.turn.active_player`, a position successor
selection would never have produced, since it skips every player whose status is
not `active` (`model/phases.md`). Nothing would advance the match from there.

- **`p` is `S.turn.active_player`.** The adapter applies exactly the `resign`
  execution above, substituting the cause: set `S.turn.phase = turn-end` and
  emit `phase-changed { player: p, from: unit-action, to: turn-end }`; run
  `eliminate(S, p, "timeout", null)`; and, if the match continues, resume the
  `semantics/turn.md` reduction from successor selection with the
  `turn-hooks.md` hooks in their fixed positions. `p` is no longer selectable,
  so the scan skips them. This mirrors AWBW, where a booted player's turn ends
  and play passes on exactly as a resignation passes it.
- **`p` is not the active player.** `S.turn` is untouched. No `phase-changed`,
  `turn-selected`, or `day-advanced` is emitted, and the procedure's own events
  are the whole transition. A host whose clock can expire off-turn takes this
  branch; one that only boots on-turn never does.

A timeout that eliminates the active player during their own `turn-start` — a
host policy that boots on the boundary rather than during `unit-action` — takes
the rout-during-`turn-start` resumption above verbatim, since that section is
written on the elimination, not on the cause.

### Conformance

The clock is not conformance-testable, so no `elimination-v1` fixture exercises
timeout and an implementation claiming `elimination-v1` is not required to ship
an adapter. This contract fixes admissibility rather than conformance: a state
carrying `timed-out`, or a `timeout` victory outcome, is a valid AWVM state; the
cascade that produced it is the cascade specified here; and a consumer may read
a timeout elimination with the same rules it reads a rout. Should a future
revision bring the clock inside AWVM, it adds the checkpoint that fires this
procedure and does not change the procedure.

## Event ordering

The procedure's events, in order, for an elimination that does **not** end the
match:

| # | Event | Emitted when |
| --- | --- | --- |
| 1 | `player-status-changed` (`reason: <cause>`) | always |
| 2 | `team-eliminated` | only when the player's whole team stopped participating |
| 3 | `capture-changed` (to `20`) | per removed unit that was a current capturer, immediately before its removal |
| 4 | `unit-removed` (`reason: "elimination"`) | once per owned unit, ascending unit ID, interleaved with row 3 |
| 5 | `capture-changed` (to `20`) | per disposed tile with an interrupted capture |
| 6 | `tile-terrain-changed` (`reason: "elimination"`) | per demotable tile |
| 7 | `tile-owner-changed` | per tile whose owner actually changes; rows 5–7 repeat per tile in board order |

An elimination that **does** end the match emits rows 1 and 2, then
`match-completed`, and nothing else.

The unit pass (rows 3–4) completes before the property pass (rows 5–7) begins.
The two passes use different orders — ascending unit ID, then canonical board
order — and interleaving them would make neither order observable.

## Integration with existing features

`elimination-v1` supersedes the bounded prefixes its predecessors specified. An
implementation claiming this feature MUST apply the full procedure at every
checkpoint below; an implementation claiming only the earlier feature keeps that
feature's narrower fixtures valid, because those fixtures are authored in states
where the cascade is empty.

**`semantics/capture.md`.** Its decisive-HQ-capture steps 4 through 6 are
exactly steps 1 through 3 of the procedure, with cause `hq-capture` and the
capturing unit's owner as beneficiary. Under `elimination-v1` the cascade
follows when the match continues; the capture's own three events
(`capture-changed`, `tile-owner-changed`, `capture-changed`) still precede the
whole procedure. Its scope restriction — that an `hq-capture` fixture place the
loser in a state with no other property and no unit — is lifted.

**`semantics/turn-hooks.md`.** Its step 7 crash removal may now leave the active
player with no units. The rout checkpoint at step 9 runs the procedure with
cause `rout` and beneficiary `null`, and, when the match continues, the boundary
loop resumes as described above. Its scope restriction — that a crash fixture
leave the active player at least one surviving unit — is lifted.

**`semantics/turn.md`.** Its deferred elimination-driven finish is this
document's step 3. Its deferred no-successor finish is unreachable while the
single-surviving-team checkpoint holds: a scan that finds no participating
successor requires zero surviving teams, and the elimination that removed the
second-to-last team would have finished the match first. The `model/phases.md`
no-successor branch therefore stays a structural guard rather than a reachable
outcome, and remains excluded together with the zero-active-team state. The
day-limit outcome stays deferred for want of a scoring rule.

## Victory checkpoints

`rout`, `hq-capture`, and `resignation` are the terminal outcomes this feature
can produce, each named by the cause of the elimination that left one surviving
team. `lab-capture`, `capture-limit`, `day-limit`, and every `draw` are other
features' checkpoints and are not reachable here. `timeout` is reachable only
through the adapter contract above: no command and no checkpoint in this
document produces it, and no fixture claims it.

## Evidence

Documentation-only (AWBW Wiki):

- Rout: "The most basic form of victory is the Rout, and occurs when a player
  loses or deletes their last unit on the map." This fixes the trigger as the
  loss of the last unit — a transition, not a standing state — and names the
  `rout` outcome.
- Non-capture cascade: "If the game has 2 or more players still on the map, the
  defeated player's properties will turn neutral, with their Headquarters
  turning into a neutral city." This fixes both the neutral disposition, the HQ
  demotion, and the condition that the cascade runs only when the match
  continues.
- HQ-capture cascade: "In games with more than 2 players where the game does not
  end right away, capturing the HQ will automatically transfer ownership of that
  player's properties to the capturing player, with the HQ itself turning into a
  city." This fixes the beneficiary disposition and confirms the demotion
  happens on both paths.
- Resignation: a player may resign during their own turn and is then "eliminated
  from the game as if they were routed", which is why resignation shares the
  neutral disposition. The wiki records the strategic corollary directly — "if
  HQ capture is inevitable, a defender can resign to deny the attacker their
  properties" — which is only meaningful if the two dispositions differ.

Confirmed replay:

- Every resignation in the local replay corpus is a `Resign` action carrying a
  `NextTurn` payload structurally identical to an `End` action's — successor ID,
  successor funds, weather, supplied and repaired lists — establishing that
  resignation is a boundary command that runs the successor's ordinary
  start-of-turn hooks. Four consecutive resignations in replay `1362397` each
  advance to the next player; the last carries a `GameOver` instead.
- `GameOver` payloads name winners and losers as player lists resolved from
  teams: replay `1403019` ends with "Team (comagoosie and TrashCompactor93) won
  the game!" and two winner IDs, and replay `1468032` with three. This
  corroborates the team-valued `winners` array of `model/state.md` and the
  single-surviving-team checkpoint.
- No replay in the corpus ends by HQ capture or rout, and AWBW's `Eliminated`
  action records only the departing player and an optional `GameOver`. The
  cascade is therefore not directly observable in the replay stream, and its
  ordering below the level fixed above is specification, not observation.

Corroborated implementation:

- WarsWorld's `eliminatePlayerByCapture`
  (`src/shared/match-logic/events/handlers/ability.ts`) reassigns every tile the
  eliminated player owns to the capturing unit's owner and then removes every
  one of their units, corroborating both halves of the `hq-capture` cascade. It
  does not demote the HQ; the wiki does, and the wiki is the AWBW-specific
  source.
- WarsWorld raises `all-units-destroyed` when a delete would remove the owner's
  last unit (`handlers/delete.ts`), `all-defender-units-destroyed` and
  `all-attacker-units-destroyed` when an attack would
  (`handlers/attack.ts`), and `all-units-crashed` when start-of-turn fuel would
  (`handlers/passTurn.ts`), corroborating that the rout condition is evaluated
  per removing transition and applies to attacker and defender alike.
- WarsWorld's `passTurn` loop, on eliminating the incoming player by crash,
  continues its turn loop to select another successor rather than opening the
  turn, corroborating the `turn-start` resumption above.
- AWBW Replay Player treats an eliminated player as removed from successor
  selection and orders the end-game listing by elimination time, but performs no
  cascade of its own.

Known deferral:

- `lab-capture`, `capture-limit`, day-limit outcome naming, the zero-active-team
  state, simultaneous elimination precedence, and the fate of an eliminated
  player's power charge and commander-sourced weather all require evidence this
  corpus does not contain, and are excluded rather than guessed. WarsWorld's own
  source marks the last of these `TODO what happens with olaf snow and other CO
  powers?`.
- The `timeout` **clock** is deferred for a different reason: it is a hosting
  policy, not an AWBW rule, and no corpus can supply it. Its *result* is not
  deferred. The wiki's account of resignation — a departing player is
  "eliminated from the game as if they were routed" — and the non-capture
  cascade it fixes apply to a booted player on the same terms, which is what the
  adapter contract states. No replay in the corpus ends by timeout, so its
  boundary handling is specified by analogy with the observed `Resign` payload
  rather than observed directly.
