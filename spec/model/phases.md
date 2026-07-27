# Phase and turn loop

This document defines the control loop for an active match. It fixes when a
player may issue commands, how the next active player is selected, when a day
advances, where automatic effects run, and when terminal results are checked.
Feature specifications define the state changes inside the named hooks below.

Phases are semantic control state, not UI screens. The closed phases are:

```text
turn-start unit-action turn-end finished
```

## Stable and automatic phases

`unit-action` is the only stable decision phase. A caller may retain a state in
this phase indefinitely without semantic time passing. Unit, production,
power, tag, and end-turn commands are validated only in this phase unless their
command specification explicitly says otherwise.

`turn-start` and `turn-end` are automatic phases. They admit no player command
and MUST run to either `unit-action` or `finished` without requesting a choice.
They may consume explicit random tokens. An implementation MAY expose their
intermediate states for tracing, but MUST NOT return one as a state awaiting a
player command.

`finished` is terminal. It MUST coincide with `match.status = finished`; no
gameplay command is legal and no automatic turn effect runs from it.

## Turn order

`turn.order` is the stable cyclic order chosen when the match begins.
`turn.position` indexes that array and `turn.active_player` MUST equal
`turn.order[turn.position]`.

Inactive players remain in the stable order so that eliminating a player does
not renumber historical turn positions. When selecting a successor, the loop
scans cyclically after the current position and skips every player whose status
is not `active`.

A **round boundary** occurs when successor selection wraps past the final array
index to an earlier index. The one-based `turn.day` increments exactly once at
that boundary. Skipping inactive players does not itself increment the day; a
scan that wraps does.

If no active successor exists, the match MUST finish during `turn-end`; the
loop MUST NOT construct another active turn.

## Match initialization

Starting a match constructs the immutable settings and stable turn order, sets
`day = 1`, selects the first active position, sets `active_player` from that
position, and enters `turn-start`.

Starting funds and predeployed board/unit state are initialization inputs, not
turn-start effects. The first player receives ordinary day-one start hooks,
including income. Random input required for the first turn is supplied to this
initial automatic advance.

## Command loop

An ordinary atomic action has this control flow:

```text
unit-action --gameplay command--> unit-action | finished
```

The command validates against the pre-command state, executes atomically, emits
ordered events, checks immediate victory conditions introduced by that command,
and either returns to `unit-action` or enters `finished`. It does not run turn
start/end hooks.

`end-turn` and `tag` are boundary commands:

```text
unit-action --end-turn--> turn-end
unit-action --tag-------> turn-end
turn-end ----------------> finished | turn-start
turn-start --------------> finished | unit-action
```

For AWBW, `tag` first swaps the active and backup commander, then follows the
same boundary loop as `end-turn`; it does not grant another action phase. The
commander specification must define the exact power-state changes caused by the
swap.

## Ordered `turn-end` hooks

Starting from the state produced by the accepted boundary command, execute:

1. Set `turn.phase` to `turn-end`.
2. Commit boundary-command effects, including a requested tag swap.
3. Clear any transient `moved` action state. A boundary command is invalid if
   it would abandon an incomplete non-atomic action that its command
   specification requires to be resolved.
4. Run ruleset `end-of-player-turn` effects in their published order.
5. Evaluate immediate elimination and victory conditions caused by those
   effects. If an outcome exists, finish the match.
6. Select the next active player by cyclic scan.
7. If the scan crosses a round boundary, perform the round-boundary procedure
   below. If it finishes the match, stop.
8. Set `position` and `active_player` to the successor and set `phase` to
   `turn-start`.

An implementation MUST NOT reactivate the successor's units, grant income, or
apply upkeep during `turn-end`; those are `turn-start` effects.

### Round boundary

At a round boundary:

1. Evaluate end-of-day limits against `turn.day`, the day that just completed.
   In AWBW, a configured day limit is resolved after the final active player's
   turn on that numbered day, before any turn of the following day begins.
2. Run any other ruleset `end-of-day` effects in their published order.
3. If a terminal outcome exists, set it and enter `finished` without changing
   `turn.day`.
4. Otherwise, increment `turn.day` by one and continue to successor setup.

Consequently, when day 20 is the limit, the state does not begin a day-21
`unit-action` phase, and the canonical finished state records `day = 20`.

## Ordered `turn-start` hooks

For the selected active player, execute:

1. Set `turn.phase` to `turn-start`.
2. Expire that player's effects whose lifetime is “until the start of the
   player's next turn,” including an active CO power where applicable.
3. Resolve weather expiry and any start-of-turn weather selection. Random
   weather MUST consume explicit random input here, never ambient randomness.
4. Grant income from properties that produce funds.
5. Apply automatic resupply from owned properties, adjacent APCs, and loaded
   supply-capable transports, in the order fixed by the supply specification.
6. Deduct start-of-turn fuel upkeep. AWBW automatic resupply occurs before this
   deduction.
7. Remove air/sea units that the fuel rules say crash or sink; remove their
   cargo according to the transport specification.
8. Apply paid property repairs in the deterministic order fixed by the repair
   specification, deducting funds and refilling resources as specified.
9. Evaluate elimination and victory caused by upkeep or other automatic
   effects. If an outcome exists, finish the match.
10. Normalize action state for every unit owned by the active player: eligible
    units become `ready`; a unit carrying a next-turn immobilization becomes
    `spent`, consuming that turn.
    Other players' units MUST NOT become ready.
11. Run remaining ruleset `start-of-player-turn` effects in their published
    order, checking terminal conditions after any effect capable of producing
    one.
12. Set `turn.phase` to `unit-action`.

Steps 5 through 8 fix only the cross-feature ordering needed by the turn loop.
The supply, fuel, transport, repair, and funds specifications must define target
eligibility, iteration order, rounding, partial payment, event order, and
simultaneous-versus-sequential behavior before these hooks are executable.

## Victory checkpoints

A condition is checked at the earliest checkpoint designated by its feature:

- command-immediate conditions, such as capture-limit or decisive HQ capture,
  are checked within that command before returning to `unit-action`;
- rout or elimination caused by an automatic hook is checked directly after
  that hook;
- resignation and timeout are external semantic commands and are checked as
  part of their execution; and
- day-limit is checked only at the round boundary.

Once an outcome is established, the reducer MUST atomically set
`match.status = finished`, set `match.outcome`, and set
`turn.phase = finished`. It MUST stop the loop immediately: no later automatic
hook, successor selection, income, upkeep, or random consumption occurs.

If several outcomes become true at the same checkpoint, the applicable feature
specification MUST define precedence. Until that precedence is specified, such
a case cannot be a normative conformance fixture.

## Atomicity and events

The semantic `execute` call that accepts a boundary command MUST run the entire
automatic loop through `unit-action` or `finished` and return the final state.
This prevents callers from choosing whether automatic effects occur. Events
MUST nevertheless expose the ordered phase transitions and state changes so a
replay or observer can reproduce the boundary.

Random tokens are consumed only when the corresponding hook is reached. A
terminal result before a random hook consumes no token for that hook. Missing,
extra, or out-of-domain random input is an execution error, not an alternative
game outcome.

## Evidence and unresolved details

Documentation-only evidence currently establishes:

- player turns proceed sequentially and the day advances when play wraps to the
  first position;
- income, resupply, and property repair occur at the start of a player's turn;
- AWBW automatic supply precedes air/sea fuel upkeep;
- tag swaps commanders and immediately ends the turn without a second turn;
- temporary powers and weather generally last until the same player's next
  turn; and
- day-limit resolution occurs after the last player's turn on the configured
  day.

This document intentionally leaves feature-local ordering unresolved where the
available documentation is insufficient. Replay-backed or controlled evidence
is still required for: random-weather token mapping and override precedence;
income versus paid repair
in edge cases; crash/cargo event ordering; and team/multi-player rout
precedence.
