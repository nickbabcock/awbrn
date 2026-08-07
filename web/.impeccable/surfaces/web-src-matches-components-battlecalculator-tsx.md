---
version: 1
slug: "web-src-matches-components-battlecalculator-tsx"
primary_target: "web/src/matches/components/BattleCalculator.tsx"
related_targets: ["crates/awvm/src/calculator.rs","crates/awbrn-wasm/src/calculator.rs","web/src/matches/components/battle_calculator.ts","web/src/engine/worker_module.ts"]
---

# Surface Brief: The Engagement That Is Not On The Board

Mode: **Operate**. It sits inside
[Unit Command on the Board](./web-src-matches-screens-matchactivepage-tsx.md)
and beside
[The Attack Half of a Move](./web-src-matches-components-unitactionmenu-tsx.md),
and inherits both. It is not a second forecast panel: those two briefs own the
forecast that rides on an order the player can give right now, and that anti-goal
still holds. This surface owns the question neither can answer.

## Job and audience

The same two players, plus a third posture: the one planning rather than acting.

- The **desk player** wants to compare three or four hypotheticals before
  spending anything — a unit they have not built, a tile they have not reached,
  a tower they have not taken.
- The **phone player** wants one matchup checked, fast, without leaving the
  match.
- Both already keep AWBW's damage calculator open in a second tab. That tab is
  the thing this surface exists to close.

## Outcome and proof

**Primary task:** configure an engagement that does not exist and read what it
costs both sides, in damage and in funds.

**Success:** a player answers "is this trade worth it" without leaving AWBRN and
without doing arithmetic.

**Proof it works:** the second tab stays closed.

## What the order menu cannot do

The `Fire` row forecasts only attacks the reducer will currently accept. Every
question a calculator is for is outside that set: a unit that is not on the
board, terrain it cannot reach, a commander who is not playing, a power that is
not charged, a treasury that does not exist yet. And the order row reports
percentages only, which cannot say whether 60% of a Mega Tank beats 90% of a
Recon.

## Selected direction

**Thesis: the premise is asked once, and every target is compared against it.**

One persistent strip carries everything that moves a damage figure — both
commanders and their power level, both treasuries, both property counts, both
com tower counts, the weather. Underneath it, one attacker, then a list of
targets that re-scores the instant any input changes. A per-row copy of an army
would make every row a form; the strip makes the whole column move at once, which
is the thing a player is actually trying to see.

```
┌ BATTLE CALCULATOR ──────────────────────── X ┐
│ WEATHER  [Clear] Rain Snow                   │
│ ATTACKING      OS │ DEFENDING            BM  │
│ [CO▾] D2D COP SCOP│ [CO▾] D2D COP SCOP       │
│ FUNDS PROPS TOWERS│ FUNDS PROPS TOWERS       │
├──────────────────────────────────────────────┤
│ ATTACKER [INF▾][10 HP▾][Plain▾]  1,000 FUNDS │
├ TARGET ───────────── DAMAGE   FUNDS      NET ┤
│ [INF][Infantry▾][10 HP▾][Plain▾]             │
│              DEAL  49 – 57%  490 – 570       │
│              TAKE  21 – 29%  210 – 290 +200 –│
│                                        +360  │
│ + ADD TARGET                                 │
└──────────────────────────────────────────────┘
```

**Settled, and not to be decided again:**

1. **Rules truth is AWVM's, through the reducer.** `awvm::calculator` lowers the
   request into an ordinary `State` — two players, a board, two units, real owned
   tiles for the tower and property counts — and calls the same
   `transition::forecast_unit_attack` a real order goes through. Nothing about
   weapon selection, commander algebra, terrain stars, the counter-first
   inversion or the range correlation is restated. A second combat model is what
   the product's fifth principle forbids, and a calculator is exactly where one
   would grow.
2. **`DEAL` and `TAKE`, exactly as the order menu says them.** The player is in
   the attacking seat in both places and the words must not differ between them.
3. **The counter range stays correlated with the attack range**, paired as the
   parent brief specifies: the good outcome is the top of Deal with the bottom of
   Take.
4. **Percentages uncapped**, for the parent brief's reason. **Funds are capped**,
   for the opposite one: overkill says how decisively an attack lands and says
   nothing about the trade, because a destroyed unit costs its owner what it was
   worth and not a coin more.
5. **Net is arithmetic, not a verdict.** It is signed on both ends and set in
   ink. No colour, no "good trade", no confidence language — the parent brief's
   anti-goal holds here, and judging the numbers is what the player came to do.
6. **Only the damage column is labelled.** The funds beside it are the same two
   facts on the same two lines in the same order; naming each fact twice on one
   row is noise.
7. **"No CO" is the ruleset's neutral commander, not an absent one.** An absent
   commander leaves the commander algebra entirely, and com towers and treasury
   effects live inside that algebra — a state with no commander reports a tower
   as worth nothing.
8. **A tower is a property.** The two counts bound each other; they never
   describe an army holding three towers and one building.
9. **An impossible pairing is reported, never hidden.** A player who asked what
   an Anti-Air does to a Battleship is owed "cannot reach this target".
10. **It commits nothing.** The action menu remains the only thing on the board
    that spends a unit.

**Focal moment:** typing a com tower count and watching every row in the column
move at once.

**Anti-reference:** the spreadsheet, and the settings form. This is the CO intel
readout doing the one job a player currently leaves the product for.

## Seeding

It opens carrying the real match — both COs and power state, funds, properties,
com towers, weather, and the armies' own colours on the sprites — and is a
scratchpad from that moment. A board that reports again mid-edit never reaches in
and rewrites a field. A figure withheld under fog seeds as zero rather than as a
guess: an invented number is one the player never chose to believe.

The attacking seat is the viewer's, or the acting army's when the viewer is
spectating. The defender is the first army not on the attacker's team, because an
ally is not who an attack is being weighed against.

## Presentation

Two, chosen by whether the board frame can hold the panel: a window on the
battlefield, or a bottom sheet. The rule reads viewport **height** as well as
width, because the board is 3:2 and its width is capped by the window's height —
a short window makes a short board however wide the screen is, and a panel taller
than its frame is a panel with its last line cut in half.

**On the sheet the strip reads last.** A phone shows one screenful, the premise
is already filled in from the match, and a player who scrolls past six fields to
reach the figure they opened the panel for has been handed a form. The strip is
still always on the panel; only its order moves.

Within a row, the target and the figures sit side by side while there is room and
the figures wrap to their own line when there is not. The header is built from
the same two parts, so it wraps identically and the column names never leave
their numbers.

## Scope and boundaries

**In scope:** `awvm::calculator` and its lowering; the `battle_forecast` and
`battle_catalog` wasm exports; `properties`, `com_towers` and `weather` on the
roster snapshot; the panel, its two presentations, and its launch key on both the
live match and the replay readout strips.

**Untouched:** the order menu, the board's own forecast, pathfinding, the command
protocol, server authority, and DESIGN.md.

**Anti-goals:**

- No second combat model, anywhere, at any cost in convenience.
- No unit, terrain, cost or commander table written in TypeScript. The pickers
  are fed from the ruleset through `battle_catalog`.
- No verdict on whether a trade is good.
- No commit path. It is a readout with inputs, not an order.
- No full unit-versus-unit matrix. That is a chart to study, not a question to
  answer, and it is hostile at phone width.

## Known gaps

- **AWVM applies terrain stars to air units.** AWBW does not — an air unit gets
  no terrain defence. The gap predates this surface and the board's own forecast
  shares it, so it was not changed here; it belongs in `spec/`, with the
  conformance corpus behind it. Until then a Fighter on a mountain reads high in
  both places, consistently and wrongly.
- Targets are told apart by their figures alone when two share a kind, terrain
  and health. The board cannot help here the way it can for a real order.
