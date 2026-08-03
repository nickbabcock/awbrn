---
version: 1
slug: "web-src-matches-components-unitactionmenu-tsx"
primary_target: "web/src/matches/components/UnitActionMenu.tsx"
related_targets:
  [
    "crates/awvm/src/query.rs",
    "crates/awvm/src/transition/attack.rs",
    "crates/awbrn-client/src/modes/play/mod.rs",
    "crates/awbrn-client/src/features/event_bus.rs",
    "web/src/components/TileInfoBar.tsx",
    "web/src/matches/screens/MatchActivePage.tsx",
  ]
---

# Surface Brief: The Attack Half of a Move

Mode: **Operate**. This extends
[Unit Command on the Board](./web-src-matches-screens-matchactivepage-tsx.md),
which owns the move gesture and the single-exit action menu. Everything that
brief settles still holds; this one settles only what happens when the order is
Fire.

## Job and audience

The same two players, at the one moment in a turn that decides matches.

- The **desk player** wants to compare three or four engagements before
  committing one, the way they would with AWBW's damage calculator open in a
  second tab.
- The **phone player** wants to know whether this attack trades up before they
  spend the unit, without opening anything.

Both already know that Advance Wars combat is a range, not a number, and both
already know that a direct attack invites a counter. Neither can currently see
either one in AWBRN.

## Outcome and proof

**Primary task:** choose which enemy to attack, from where, knowing what the
exchange costs.

**Success:** a player picks the better of two available attacks because the
interface showed them the difference, and is never surprised by a counter they
did not know was coming.

**Proof it works:** no player opens the AWBW damage calculator in another tab
while playing AWBRN. That is the whole point of the feature.

## What is wrong today

- Attack targets do not exist until a destination is proposed. A player cannot
  see who is in danger while choosing where to go, which is exactly when they
  need it.
- A target is named by bare coordinates — `Fire  12, 7` — so the player must
  find the tile on the board to learn what unit it is and what shape it is in.
- Nothing anywhere reports damage. The one irreversible order on the menu is the
  one order whose result the interface refuses to predict.
- Indirect units get no treatment at all. The rule that an indirect cannot move
  and fire in the same turn is invisible, and is discovered by losing a turn.
- On a phone, a target is chosen by landing a thumb on a 40px enemy tile that
  may be one of four crowded together.

## Selected direction

**Thesis: the forecast is part of the order, not a panel beside it.**

The `Fire` row stops being a word and a coordinate and becomes the engagement
itself: who the target is, what this attack deals, and what comes back. There is
no second panel, no versus screen, and no new thing to dismiss. The single-exit
rule the parent brief established is untouched — the menu is still the only
thing that commits, and now it is also the only thing that forecasts.

```
┌ [ART] ARTILLERY ───── 14, 17 ┐   an indirect: nothing answers
│ [INF]  DEAL   72 – 79%       │
│ [INF]₆ DEAL   81 – 89%       │
│ WAIT                         │
└──────────────────────────────┘

┌ [INF] INFANTRY ────── 15, 17 ┐   a direct: everything answers
│ [INF]  DEAL   44 – 51%       │
│        TAKE   26 – 34%       │
│ [MEC]₉ DEAL   38 – 44%       │
│        TAKE   51 – 60%       │
│ WAIT                         │
└──────────────────────────────┘
```

**Direction is named, and this is not negotiable.** Two passes tried to avoid
naming it and both failed the same way. Recording them so a third is not
attempted:

1. **Arrows pointing out and back.** An arrow needs two things to point between.
   AWBW's forecast has them because both units sit on the line; this menu had
   moved the attacker into the header to save width, so the arrows pointed at
   empty margin in both directions.
2. **The sprite of whoever takes the damage on that line.** A sprite beside a
   number reads as _that unit's_ number, and a unit's number means what it deals
   long before it means what it suffers. Putting the target's art next to the
   damage done to it had readers concluding the target was the attacker.
   Inverting the convention does not fix this; it moves which half is misread.

`DEAL` and `TAKE` say it in eight characters, from the seat the player occupies,
and nothing about them has to be learned. `TAKE 1ST` marks a `counter-first`
commander, because the order of two lines is not something a reader is owed.
The width was paid for by dropping the army, the unit name and the standalone
health column.

**The attacker is named once, in the header.** It is the same unit for every
order, so repeating it per row would spend the width twice on a fact that does
not change. AWBW's popup cannot do this because it forecasts one engagement at a
time and this menu enumerates them all.

**Nothing is written where nothing comes back.** A row with no counter is one
line high, not two lines with a line saying so. An indirect gets no reply from
anything it can reach, so a menu that spelled it out would say it on every row
and mean it once; the contrast between a one-line and a two-line row is itself
the information.

**Health rides on the sprite, and only when it matters.** The `Healthv2` digit
sits in the sprite's lower right, where the board puts it, and follows the
board's own rule of appearing only below full strength. A standalone HP column
cost width, and a menu of whole units showed a column of tens saying nothing.

**Damage is a percentage, uncapped.** `awvm::combat::damage` already works in
points on a 0–100 scale and derives the 1–10 visual HP from it, so the
percentage is the engine's own number. It is reported before the clamp, the way
AWBW reports it: `104 – 116%` against a whole unit is an overkill worth sending
something cheaper at, and collapsing it to `100%` makes that indistinguishable
from a hit that barely finishes. `Hit::raw_damage` carries it; the reducer still
runs on the clamped value, because a unit cannot lose more health than it has.

**No name and no army on the row.** Everything a unit can fire at is an enemy,
so the army was decoration. The name went for a sharper reason: **it does not
answer the question the row raises.** Two Infantry targets produce two rows
reading `INFANTRY`, and what separates them is which tile they stand on. A name
cannot say that, and a coordinate only helps a player already hunting the board
for it. **Slice 2 owes this surface its answer** — focusing a row paints its
target — and until it lands, two same-kind targets are told apart only by their
figures. Known and accepted, not an oversight.

**Focal moment:** selecting a unit and watching two enemies light red while a
third does not, then stepping onto a tile and watching one of them go dark. The
rule teaches itself in the two seconds before any order is chosen.

**Anti-reference:** the spreadsheet. This is not a stat table bolted to a game;
it is the CO intel readout that DESIGN.md already borrows from, doing the one
job a player currently leaves the product to do somewhere else.

## Slices

Three, in this order. Each ships alone and each de-risks the next.

1. **The forecast.** New AWVM observed-forecast query; `Fire` rows carry the
   target's identity and both ranges. No new interaction to learn.
2. **The threat field.** Red glass on attackable enemies at selection time;
   clicking an enemy paths to it and preselects its order.
3. **Indirect fire range, target cycling, and the targeted-tile readout.** The
   phone-completeness slice.

## The numbers

### Where they come from

`awvm::query::observed_forecasts(observation, unit, from, targets)`, built
the same way `observed_actions_at` is: reify the observation and put the
question to the same code that answers at execution time. It **reuses
`transition::attack::Engagement`** rather than restating a rule. That is not
tidiness — `Engagement` is where counter eligibility, effective attack range,
concealment compatibility, commander attack and defense modifiers, effective
enemy terrain stars, and the `counter-first` inversion all live. A forecast that
recomputed any of them would be a second combat model that drifts from the first,
and the product's fifth principle forbids exactly that.

### Which roll is which

Luck is drawn as `good − bad`, each from its commander's inclusive `Domain`. So:

- **low** damage uses `good_luck.minimum − bad_luck.maximum`
- **high** damage uses `good_luck.maximum − bad_luck.minimum`

For a commander with no luck rules this is `0..=9 − 0..=0`, which is the familiar
AWBW spread.

**The counter range is correlated with the attack range, not independent of it.**
Counter damage is scored from the defender's HP _after_ the attack lands, so the
two ranges cannot both be rolled freely. The pairing is fixed and stated here so
a builder does not invent a different one:

- **counter low** = the defender's weakest reply after the attacker's _best_
  roll.
- **counter high** = the defender's strongest reply after the attacker's _worst_
  roll.

That reads as the honest bracket from the attacker's seat: `DEAL 65 – 75% /
TAKE 18 – 22%` means the good outcome is the top of the first line paired with
the bottom of the second. Showing a wider, uncorrelated counter range would be a
lie in the player's favor half the time.

### What the row says when there is no range

| Case                                                               | The row shows                                   |
| ------------------------------------------------------------------ | ----------------------------------------------- |
| Nothing survives the attacker's weakest roll                       | one line; the figure against the health says it |
| No counter possible (indirect, out of reach, no weapon that bites) | one line                                        |
| Counter possible only on the attacker's weak rolls                 | two lines, the reply starting at `0%`           |
| Attacking a destructible tile                                      | one line; a pipe seam does not answer           |

A reply of `0%` and no reply at all stay different facts: the first is a second
line reading `← 0 – 12%`, the second is no second line.

Low equals high collapses to one number. A range of one value written as a range
is noise, and the case is common under commanders with no luck.

A kill is not marked on the visible row. With the figure uncapped and the health
worn on the sprite, the damage against the health is the evidence, and the
accessible name still says "destroying it".

### Honesty under fog

`observed_forecast` inherits every limit `observed_actions_at` documents: it is
computed against the recipient's own projection, so a hidden blocker or a
concealed unit can make it wrong. It is advisory, exactly as the order list
already is. It does **not** get a disclaimer in the interface. A permanent
asterisk on every forecast in a game where fog is optional teaches players to
ignore an asterisk. When the forecast and the outcome disagree, the resolution
animation is what says so.

## The threat field

### Direct units

**At `UnitSelected`, every enemy this unit could attack from any reachable tile
wears red glass on its own tile.** Not the tiles it could be attacked from —
those are already the cyan movement field, and painting a second field over it
would leave the board unreadable. The red marks _who_, the cyan marks _where_.

**At `DestinationSelected`, the red set narrows to what is attackable from that
tile.** Enemies that fall out lose their glass. This transition is the single
most instructive thing on the board and it costs nothing to author: it is the
same set, recomputed.

Red is `{colors.damage-red}` at the alpha structure the movement glass already
uses, with the same light and dark edge pair so the two fields read as the same
material in two colors. **Color is not the only signal:** an attackable enemy
also takes a bracket reticle from the UI atlas, because faction identity on this
board is already carried by color and the product's accessibility note forbids
adding a second meaning to hue alone.

### Indirect units

An indirect that moves cannot fire. That rule gets drawn rather than explained.

**At `UnitSelected`, an indirect shows two fields at once:** the cyan movement
glass over its reachable tiles, and its **fire ring** — the `min..=max` band
around its _current_ position — as a red edge outline with attackable enemies
inside it in solid red glass. Outline rather than fill, because the two fields
overlap and a filled red band would bury the movement field underneath it.

**The moment a destination other than the origin is proposed, the fire ring and
every red highlight disappear.** The player watches their own artillery lose its
shot as they walk it forward. That is the rule, shown at the instant it applies,
and it replaces a turn spent learning it the hard way.

## Choosing a target

### Clicking an enemy

Clicking or tapping a red-highlighted enemy is the fast path on every platform.
The engine picks the destination — never TypeScript, per the parent brief — and
opens the menu with that target's `Fire` row preselected. It does not commit.

- **Direct:** the cheapest reachable tile adjacent to that enemy, tie-broken in
  the same map order `shortest_path` already uses. `attack_approach` already
  does this for a drag released on an enemy; this extends it to a discrete tap,
  which is the gesture the parent brief calls Entry A.
- **Indirect:** the origin, since firing is the only thing that reaches. If the
  enemy is outside the fire ring, the tap is an ordinary tile tap and nothing is
  proposed.

### Cycling, and why it is not a separate control

The cartridge let an attacker step through its targets with the shoulder keys.
That behavior is worth having and does **not** need a new component, because the
menu is already a list of targets: after slice 1, every `Fire` row names its
enemy and forecasts the exchange. The cycler is the menu's own focus traversal.

**Focusing a `Fire` row paints its target on the board** — reticle, red pulse,
and the route drawn to the destination — **and retargets the tile readout to
that enemy.** Arrow-key traversal already exists in `BoardMenuShell`, so the
desk player and the keyboard player get the cartridge cycler for free the moment
the board feedback is wired.

On coarse pointers the sheet gains **`◀` and `▶` stepper keys in its header**
that move focus between `Fire` rows without committing, and pan the camera to
keep the focused target on screen. A thumb gets the same one-key-at-a-time sweep
the shoulder buttons gave, and the commit stays where it has always been: the
row itself.

This adds **no second commit path**, which is the whole reason to build it this
way. A `FIRE` key on a floating target stepper would be a second way out of the
state machine, and the parent brief's one-way-out rule is the thing keeping a
mis-tap from costing a unit.

### The targeted-tile readout

`TileInfoBar` gains a targeted tile that outranks the hovered tile. Precedence:
targeted, then hovered, then nothing. When it is reporting a target it wears a
red rule at its leading edge so it is visibly the target readout rather than the
pointer readout, and it names the target in its accessible description rather
than relying on that rule.

This is the answer to "we should have the unit tile info of the targeted unit":
the readout that already exists, pointed at the right tile, rather than a second
readout that duplicates it. The row's own summary — sprite, army, name — stays,
because a player comparing two rows should not have to move focus to read the
second one.

## Scope and boundaries

**In scope:** the AWVM forecast query, the forecast in the order row, the red
threat field and its two tiers, the indirect fire ring, click-an-enemy
targeting, `Fire`-row board feedback and the coarse-pointer stepper, and the
targeted-tile readout.

**Untouched:** pathfinding, move-range computation, the gesture layer, the
camera policy, the `MatchCommand` protocol shape, server authority, the rollback
snapshot behavior, and replay mode.

**Anti-goals:**

- **No separate forecast panel.** A second floating object over the board is a
  second thing to dismiss and a second place for the phone to run out of room.
- **No HP bars.** The percentage is the number; a bar beside it says the same
  thing less precisely and costs a phone sheet a row of height.
- **No commit outside the menu.** No `FIRE` key on any stepper, reticle, or
  board overlay.
- **No confidence language.** No "likely", no "risky", no color-coded verdict on
  whether a trade is good. The numbers are the product; judging them is the
  player's job and is what they came to do.
- **No forecast disclaimer chrome.** See "Honesty under fog".
- **No auto-attack.** Clicking an enemy proposes; it never fires.

## States and ranges

A unit has 0 to roughly 8 attackable targets as a direct, and up to a few dozen
as a long-range indirect on a crowded map. The menu already carries 1 to 7
orders; with per-target `Fire` rows a Rocket in a crowd can push that well past
what a board menu should hold, so **the order list scrolls inside a fixed
maximum height** and the stepper becomes the primary way through it. Damage
values run 0–100%. Counter values run 0–100% or absent.

Material states beyond the parent brief's: no target in range; exactly one
target; many targets; target destroyed by the low roll; target that cannot
reply; indirect that has already moved; indirect whose ring is entirely empty;
and a forecast the query could not produce, where the row falls back to today's
bare `Fire` rather than showing a wrong number.

## Constraints and resolved decisions

Rules truth comes from AWVM through `Engagement`; the client and the web app
never compute damage. The forecast rides on `UnitActionOption` so it arrives
with the order it describes and cannot be paired with the wrong row. Menu chrome
stays React and Astryx per `web/AGENTS.md`, in the HUD face at the 12px bitmap
floor DESIGN.md sets. Board highlights stay engine-drawn sprites on the existing
`MOVE_RANGE_OVERLAY` layer. The menu stays keyboard-operable and focus-restoring.

Settled, and not to be decided again by a builder:

1. **Percentages, not HP, and uncapped.** The engine's own unit, the audience's
   own vocabulary, and the overkill margin kept rather than clamped away.
2. **The counter range is correlated with the attack range**, paired as
   specified above.
3. **The cycler is the menu's focus traversal**, not a new control, and it never
   commits.
4. **Red marks who, cyan marks where.** Directs highlight enemies; only
   indirects highlight a range band, and only as an outline.
5. **The attacker is named once, in the header**, and the two figures are named
   `DEAL` and `TAKE`. Neither an arrow nor a sprite can carry direction on a row
   this shape; both were built and both misread.
6. **A row says only what is true and surprising.** No reply line when nothing
   replies, no health digit at full strength, no army and no unit name.
7. **Telling two same-kind targets apart is the board's job, not the row's.**
   Slice 2 must paint the focused row's target; no text column substitutes.
