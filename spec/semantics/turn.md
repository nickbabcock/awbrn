# Turn boundary and `end-turn`

Status: normative for the `end-turn` boundary command, income, active-player
selection, day advancement, and start-of-turn action normalization under AWBW
ruleset revision `2026-07-10`, as feature `turn-boundary-v1`. The control loop
this document reduces is defined structurally by `model/phases.md`; the event
envelope is `schema/event.schema.json`; the closed violation set is
`schema/violation.schema.json`.

## Scope

This document makes a bounded subset of the `model/phases.md` `turn-end` and
`turn-start` hooks executable. It covers the ordinary *continuing* boundary:
the active player ends the turn, a successor is selected, and the successor
opens an ordinary `unit-action` phase.

The following are deliberately **out of scope** in this revision and remain
deferred exactly as `model/phases.md` and `model/state.md` leave them:

- `tag` commander swapping and its power-expiry effect are specified by
  `semantics/tag.md` as feature `tag-v1`; this feature reduces `end-turn`.
- Commander-specific income (for example the Sasha per-property bonus) and any
  other commander effect; income here is commander-neutral, mirroring
  `combat-neutral-v1`.
- Weather selection, commander-power weather, automatic resupply, fuel upkeep,
  air/sea crash removal, and paid property repair (`model/phases.md` turn-start
  steps 3 and 5 through 8). Feature `turn-hooks-v1`
  (`semantics/turn-hooks.md`) now makes the deterministic subset of these hooks
  executable — weather expiry and explicit selection, resupply, upkeep, crash,
  and repair — as a strict superset of this feature. Power-created weather is
  specified by `semantics/powers.md`.
- Power expiry (`model/phases.md` turn-start step 2).
- Every match-finish path. The day-limit checkpoint *timing* is fixed below,
  but naming the winning or tied teams at a day limit needs AWBW's end-of-game
  scoring rule, which is not yet evidenced. No finish is a conformance claim
  here. Elimination-driven finishes are specified by feature `elimination-v1`
  (`semantics/elimination.md`), which also shows the `model/phases.md`
  no-successor branch to be unreachable while its single-surviving-team
  checkpoint holds.

A conformance fixture for `turn-boundary-v1` MUST therefore be authored in a
state where every deferred hook is a no-op: weather has no pending override
(`weather.remaining_turns = 0`), no player has an active power, the successor's
units incur no fuel upkeep and need no repair or resupply, and no boundary
crosses a day limit or eliminates a team. Under those conditions the reduction
is fully deterministic and consumes no random token.

## Terms and derived values

For pre-state `S` and the accepted boundary command's player `e`:

- `order = S.turn.order`, `pos = S.turn.position`, and
  `active(S) = S.turn.active_player`, with the invariant `active(S) = order[pos]`.
- A player is *selectable* when its `status` is `active`.
- `successor(S)` is the player found by scanning positions `pos+1, pos+2, ...`
  cyclically (modulo `|order|`) and returning the first selectable player. The
  acting player is included in the scan only after a full wrap.
- The scan *crosses a round boundary* when it advances past the final index of
  `order` before finding the successor, that is when the successor's index is
  less than or equal to `pos`.
- `income-tiles(S, p)` is the set of board tiles whose `owner` is `p` and whose
  terrain kind carries the `income` trait in `terrain.json`. In the AWBW profile
  those kinds are `city`, `base`, `airport`, `port`, and `hq`; `com-tower` and
  `lab` are ownable but carry no `income` trait and never contribute.
- `income(S, p) = |income-tiles(S, p)| * S.settings.income_per_property`.

`income(S, p)` is the whole definition of income for this ruleset, and the
boundary below is not its only caller: `model/phases.md` pays the same value to
the first player when a match is initialized, because their day-one turn-start
runs with no boundary before it. Day one is not a separate rule and MUST NOT be
given a separate amount.

All derived values are evaluated against the authoritative pre-state. The
reduction does not recompute terrain, ownership, or settings partway through.

## Validation and precedence

`end-turn` is well shaped per `schema/command.schema.json`: it carries `type`
and `player` and no path, target, or claimed result. Beyond that,
`validate(R, S, C)` returns exactly one structured violation using this order:

```text
AUTHORITY_REQUIRED
MATCH_FINISHED
WRONG_PHASE
NOT_ACTIVE_PLAYER
```

- `AUTHORITY_REQUIRED` (`authority: "player"`) when the command is not player
  authority. `end-turn` is a player command; a `match-authority` submission is
  rejected here.
- `MATCH_FINISHED` when `S.match.status` is `finished`.
- `WRONG_PHASE` (`expected: "unit-action"`) when `S.turn.phase` is not
  `unit-action`. The automatic phases admit no command.
- `NOT_ACTIVE_PLAYER` (`player: e`) when `e` is not `active(S)`.

This revision defines no command that leaves a persistent pending unit action,
so the `model/phases.md` step-3 rule against abandoning an unresolved non-atomic
action cannot be triggered and reserves no additional violation. Clearing
transient `moved` state is consequently a no-op.

Validation never mutates state and requests no random token.

## Execution

Execution of an accepted `end-turn` runs the automatic loop from `turn-end`
through either the successor's `unit-action` or a day-limit finish and returns
the final state; a caller cannot observe or skip an intermediate automatic
phase. The loop is atomic and consumes no random token in this feature.

Let `e = active(S)` and `s = successor(S)`. Within `turn-boundary-v1` a
selectable successor always exists (fixtures keep every player active), so the
loop always reaches `unit-action`.

### `turn-end`

1. Set `S.turn.phase = turn-end`.
2. Commit boundary-command effects. `end-turn` has none beyond the phase change;
   there is no commander swap and no transient action state to clear.
3. Select `s` by the cyclic scan above. Record whether the scan crossed a round
   boundary.
4. If the scan crossed a round boundary and `S.settings.day_limit` is a positive
   integer equal to `S.turn.day`, resolve `day-limit-v1` **without** incrementing
   `turn.day`, selecting the successor, or running turn-start hooks. Count every
   owned property for each active player, including HQs, Labs, and Com Towers.
   Let the leaders be the active players tied for the greatest count. If all
   leaders belong to one team, that team wins with reason `day-limit`; otherwise
   draw the distinct leading teams with reason `day-limit`. Team IDs are sorted.
   Otherwise, if the scan crossed a round boundary, increment `S.turn.day`.
5. Set `S.turn.position` to `s`'s index and `S.turn.active_player = s`.

### `turn-start`

6. Set `S.turn.phase = turn-start`.
7. Grant income: set `S.players[s].funds += income(S, s)`.
8. Normalize `s`'s unit action state: every unit owned by `s` whose action is
   not already `ready` becomes `ready`. Units owned by any other player are left
   unchanged. No unit owned by `s` is `spent` after this step in the AWBW
   profile, which defines no start-of-turn ineligibility.
9. Set `S.turn.phase = unit-action`.

The turn-start hooks (weather, resupply, fuel, crash, repair, power expiry)
occupy their `model/phases.md` positions between steps 6 and 9. Under
`turn-boundary-v1` the required no-op states leave them changing and emitting
nothing; feature `turn-hooks-v1` (`semantics/turn-hooks.md`) gives the
deterministic subset — weather expiry, resupply, upkeep, crash, and repair —
executable meaning in exactly those positions, keeping power expiry and weather
selection deferred.

## Event ordering

Every state mutation above emits its authoritative fact in transition order.
The events are a flat ordered array; the `phase-changed` events bracket each
stage, and each economy or action fact carries a stable `reason`. There is no
composite `end-turn` event and no nesting; the relation between the boundary and
its downstream facts is expressed by position within the phase brackets and by
`reason`, per `model/events.md`.

For a continuing boundary the ordered events are:

| # | Event | Key fields | Emitted when |
| --- | --- | --- | --- |
| 1 | `phase-changed` | `player: e`, `from: unit-action`, `to: turn-end` | always |
| 2 | `day-advanced` | `from: d`, `to: d+1` | only when the scan crossed a round boundary and no day limit finished the match |
| 3 | `turn-selected` | `player: s`, `position: index(s)` | always (play continues) |
| 4 | `phase-changed` | `player: s`, `from: turn-end`, `to: turn-start` | always |
| 5 | `funds-changed` | `player: s`, `from`, `to`, `reason: "turn-start-income"` | only when `income(S, s) > 0` |
| 6 | `unit-action-changed` | `unit`, `from`, `to: ready`, `reason: "turn-start"` | once per unit owned by `s` whose action actually changes, ascending by unit ID |
| 7 | `phase-changed` | `player: s`, `from: turn-start`, `to: unit-action` | always |

For a day-limit boundary, emit only the initial `phase-changed` into
`turn-end`, followed by `match-completed`. The stored day remains the configured
limit and the phase becomes `finished`.

Notes:

- Event 2 precedes event 3. Day advancement is detected by the same scan that
  selects the successor and is a `turn-end` fact, so it is emitted before the
  transition into `turn-start`. This matches `model/events.md`: successor
  selection and a wrap are `turn-end` facts, and entering the next automatic
  phase is a separate `phase-changed`.
- Event 5 is omitted, not emitted with `from` equal to `to`, when the successor
  owns no income tile. `funds-changed` records an actual change.
- Event 6 is deterministically ordered by ascending unit ID so the array is
  reproducible. A unit already `ready` at the boundary emits nothing.
- A `day-advanced` event's `to` is at least `2` per `schema/event.schema.json`,
  which is consistent because a wrap can only occur after day `1`.

## Victory checkpoints

Feature `day-limit-v1` evaluates its terminal checkpoint at the round boundary,
before any day increment, exactly as `model/phases.md` requires. Other
command-immediate, rout, elimination, resignation, and timeout checkpoints
belong to their respective transition features.

`resign` is the other boundary command that reduces through this document.
`semantics/elimination.md` defines it as an elimination followed by the
successor selection and `turn-start` sequence below, entered from its step 3.

## Evidence

Corroborated implementation:

- WarsWorld grants income to the *next* player at the pass-turn boundary
  (`src/shared/match-logic/events/handlers/passTurn.ts`:
  `nextTurnPlayer.data.funds += nextTurnPlayer.getFundsPerTurn()`), confirming
  that income is a start-of-successor-turn effect rather than an end-of-turn
  effect for the player who passed.
- Archived AWBW replays open on day one with the first player already holding
  one turn of income and every other player holding their starting funds alone,
  which is the initialization grant `model/phases.md` describes. Replay 1362397
  is worked through there.
- WarsWorld's `getFundsPerTurn`
  (`src/shared/wrappers/player-in-match.ts`) counts owned changeable tiles
  excluding `lab` and `commtower`, then multiplies by `fundsPerProperty`. The
  AWBW profile encodes the same exclusion structurally through the `income`
  terrain trait, so the count is over `income`-trait tiles the player owns.
  The Sasha per-property bonus in the same function is a commander effect and is
  out of scope here.

Documentation-only:

- AWBW Wiki: player turns proceed in the stable order, the day advances when play
  wraps to the first position, and income is collected at the start of a player's
  turn. `model/phases.md` records the same and additionally fixes that a
  configured day limit resolves after the final player's turn on that day.
- AWBW Wiki `Advance Wars Overview`: day-limit scoring counts every owned
  property, including Labs and Com Towers, and a tie for the greatest player
  count produces a draw.

Known deferral:

- Weather selection remains unresolved and is excluded rather than guessed,
  consistent with `model/state.md` and `model/phases.md`.
