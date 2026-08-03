---
version: 1
slug: "web-src-matches-components-battlecalculator-tsx"
primary_target: "web/src/matches/components/BattleCalculator.tsx"
related_targets:
  [
    "crates/awvm/src/calculator.rs",
    "crates/awbrn-wasm/src/calculator.rs",
    "web/src/matches/components/battle_calculator.ts",
    "web/src/engine/worker_module.ts",
  ]
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

**Thesis: two armies meet, and the exchange is read across the axis between
them.**

Superseded (2026-08-11): this surface first shipped as one persistent premise
strip over a re-scoring list of targets. The list answered "which of these six
targets is the best trade". The question players actually arrive with — the one
the second tab is open for — is "what happens when these two meet, and what
happens when it goes the other way". That is a duel, and a duel is symmetric.

Both armies are fully specified as facing columns: commander, power, treasury,
property and com tower counts, and the unit itself. A swap key sits on the
centreline between them, because turning the engagement around is a thing done
to the two columns rather than a setting on one. The exchange is read across
that axis, at a scale the board's own forecast never uses.

```
┌ BATTLE CALCULATOR ──────────────────────── X ┐
│ ATTACKER         │ DEFENDER                  │
│ [CO▾]            │ [CO▾]                     │
│ D2D ★ ★★         │ D2D ★ ★★                  │
│ FUNDS PROPS TWRS │ FUNDS PROPS TWRS          │
│ [INF▾][10♥][Plain▾]  ⇄  [INF▾][10♥][Plain▾]  │
├──────────────────────────────────────────────┤
│      DEALS       │      TAKES                │
│     49 – 57%     │  @6 HP  21 – 25%          │
│    490 – 570     │  @7 HP  26 – 29%          │
│                  │      210 – 290            │
├──────────────────────────────────────────────┤
│              NET  +200 – +360                │
└──────────────────────────────────────────────┘
```

**The figures leave the HUD face.** Silkscreen reads at 12px because that is
the size it was drawn at, and enlarging a bitmap enlarges its jaggedness
rather than its legibility. The percentages and the net are set in the body
face at full weight, which is the move the system already documents for its
signage face at title sizes; the HUD face keeps the labels, at the size it was
drawn for.

**The powers wear the source tool's own stars.** `CO/Power.png` and
`CO/SuperPower.png` — red for the power, blue for the super — are packed into
the UI atlas from `Textures/CO` by `collect_co_power_sprites`. The atlas also
holds `NormalPower`/`SuperPower`, but that art is lettering 66px wide: a
banner rather than a key on a menu strip. Day-to-day keeps a word, because
there is no mark for a thing that is not happening.

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
6. **Each column carries what the unit standing in it does.** The attacker's
   damage sits under the attacker and the reply under the defender, which is
   the reference tool's own arrangement and the one every player already
   reads. It is not re-derived here. `Deals` and `Takes` are still said from
   the attacking seat, exactly as the order menu says them.
7. **The seat is named and the army is not.** Which army is standing in a seat
   is already told by the portrait, by the colours its unit sprite is drawn
   in, and by the match the panel opened over; spelling out "Orange Star"
   beside all three buys a line of height to repeat what is on screen twice.
   The faction wash stays as an echo of that, not as the thing saying it — it
   is aesthetic here rather than informational, unlike a faction chip on a
   roster row.
8. **"No CO" is the ruleset's neutral commander, not an absent one.** An absent
   commander leaves the commander algebra entirely, and com towers and treasury
   effects live inside that algebra — a state with no commander reports a tower
   as worth nothing.
9. **A tower is a property.** The two counts bound each other; they never
   describe an army holding three towers and one building.
10. **An impossible pairing is reported, never hidden.** A player who asked what
    an Anti-Air does to a Battleship is owed "cannot reach this target".
11. **It commits nothing.** The action menu remains the only thing on the board
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

**In scope:** `awvm::calculator` and its lowering;
`awvm::transition::forecast_counter_steps` and `combat::CounterStep`; the
`battle_forecast` and `battle_catalog` wasm exports; `properties`, `com_towers`
and `weather` on the roster snapshot; the panel, its two presentations, the
swap, and its launch key on both the live match and the replay readout strips.

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
- No second combat model, including for the counter rungs. They are scored
  through the reducer's own `StrikeValues` or they are not shown.

## Known gaps

- **AWVM applies terrain stars to air units.** AWBW does not — an air unit gets
  no terrain defence. The gap predates this surface and the board's own forecast
  shares it, so it was not changed here; it belongs in `spec/`, with the
  conformance corpus behind it. Until then a Fighter on a mountain reads high in
  both places, consistently and wrongly.
- **The duel reads one target.** The multi-target column the first direction
  was built around has no place in a symmetric duel, and `Add target` has
  nothing to act on. Comparing several targets against one premise is a real
  job this surface no longer does; whether it returns as a second mode or is
  dropped is undecided, and inventing an answer here would be guessing.
- Counter rungs are grouped by the bar the board draws. Within a bar the range
  still spans AWVM's point-level scoring, so a rung is not a single number.
