---
version: 1
slug: "web-src-components-tileinfobar-tsx"
primary_target: "web/src/components/TileInfoBar.tsx"
related_targets:
  [
    "crates/awbrn-client/src/modes/play/mod.rs",
    "crates/awvm/src/semantic/visibility.rs",
    "web/src/matches/screens/MatchActivePage.tsx",
  ]
---

# Surface Brief: Reading a Unit

Mode: **Operate**. Extends
[Unit Command on the Board](./web-src-matches-screens-matchactivepage-tsx.md) and
[The Attack Half of a Move](./web-src-matches-components-unitactionmenu-tsx.md).
Everything those settle still holds. This one settles what happens when the
player wants to **know** rather than **do**.

## Job and audience

The same two players, in the half of the match where they are not moving
anything.

- The **desk player** wants to answer "can that Rocket reach my Tank if I stop
  here" without counting tiles, and to audit their own fog coverage before
  ending a turn.
- The **phone player** is watching an opponent's turn or spectating. They cannot
  command anything. Today the board is inert for them, which is most of the
  wall-clock time they spend in the product.

Both know Advance Wars. Neither should have to learn a gesture to ask a question
the board already knows the answer to.

## Outcome and proof

**Primary task:** point at any unit on the board and learn what it can reach,
what it can hit, and what it can see, without changing anything.

**Success:** a player stops counting tiles with a finger on the screen, and a
player on a fog map stops losing a unit to an indirect whose range they
misjudged.

**Proof it works:** a spectator with no seat in the match still finds the board
worth touching.

## What is wrong today

- Tapping a unit you cannot command does nothing. `selectable_unit_at` returns
  `None` for any enemy, so `handle_tap` falls through to
  `close_production_options` and returns. The board actively teaches that
  tapping enemies is useless.
- On the opponent's turn and in spectator mode, **every** unit is uncommandable,
  so the entire board is inert.
- Vision is not visualizable at all. `ViewerVisibility` reports the _result_ of
  fog, which tiles are dark, and never which unit produces the sight.
- The indirect fire ring and the enemy threat glass are specified in the attack
  brief's slices 2 and 3 and are **not built**. There is no fire ring in the
  client.
- The one question every player leaves the product to answer, where it is safe
  to stand, has no answer anywhere in the interface.

## Selected direction

### Thesis: inspection is not a mode, it is what the board does when it is not being commanded

Do not add a state. Add a **subject**.

One new resource, `InspectedUnit(Option<Entity>)`, sits orthogonal to
`SelectedUnit` and to `PlayUiPhase`. Three rules:

- **`SelectedUnit` implies `InspectedUnit`.** Commanding a unit and reading a
  unit paint the _same three fields_. The player learns one visual language,
  once, and it does not change when the seat changes.
- **Tapping an un-commandable unit sets `InspectedUnit` alone.** `PlayUiPhase`
  stays `Idle`. It reaches no menu and no command.
- **Inspection has no commit path at all.** This satisfies the parent brief's
  one-way-out rule by construction rather than by discipline.

Not-your-turn, spectator, and replay all fall out for free: every unit is
uncommandable there, so every unit is inspectable. Replay, currently the more
finished surface, gets the whole feature at no additional cost.

**Why not AWBW's cycle.** AWBW rotates one unit through movement, then range,
then vision, on repeated clicks. That control is unlabeled, modal, and serial: a
player taps three times to reach vision and has nothing on screen telling them
which of the three they are looking at. It is portable to a thumb and it is
still the wrong control. It is replaced, not kept as a fallback.

### The encoding: one fill, two edges, one mark

Three simultaneous fields are only legible if they differ in **form**, not only
in hue. PRODUCT.md forbids hue carrying meaning alone, so this is a requirement.

- **Movement is the only fill.** Cyan glass, unchanged. It is the only field
  that is a region a unit can stand in.
- **Fire is a solid outline** in `{colors.damage-red}`, traced on the boundary of
  the threat envelope. Outline lies over the movement glass without burying it.
- **Vision is a dashed outline** in `{colors.low-supply-amber}`. A dashed line is
  a soft claim: sight is contestable, where fire range is a hard rule. **The
  line style carries the epistemics.**
- **Attackable enemies are the mark:** solid red glass on the enemy's own tile
  plus the bracket reticle, exactly as the attack brief settles. Red marks
  _who_, cyan marks _where_.

Directs draw their fire outline around `movement` union `reach`. Indirects draw
the `min..=max` band around their **current** position, and the parent brief's
rule stands: propose a destination other than the origin and the ring vanishes,
because the unit just gave up its shot.

### Vision, drawn true

This is where the design stops matching AWBW and starts beating it. AWBW's flat
yellow circle is wrong on most maps: it claims sight into terrain that conceals,
and it does not shrink under rain.

- **The ring shrinks under rain** and the readout shows the reduced number, so
  the player watches sight collapse when the weather turns.
  `semantic/visibility.rs` computes `(vision + bonus + rain).max(1)`.
- **The ring grows on a mountain** for units with `elevated_vision`.
- **Blind tiles inside the ring are marked.** Concealing terrain within sight
  range but outside the adjacency rule takes a small hatch inside the amber
  boundary. These are the tiles the unit is looking at and cannot see into. A
  flat yellow circle claims them; this one does not.
- Commanders that see through cover (`reveals_concealing`) simply have no blind
  tiles, and the absence is the information.

Every value comes from AWVM. The client never restates a vision rule, exactly as
it never restates a damage rule.

### The readout is the legend, the answer, and the control

`TileInfoBar` gains three lines when a unit is inspected:

```
+ [ART] ARTILLERY          +
| # MOVE    5              |
| o RANGE   2-3            |
| : SIGHT   1  v2          |
+--------------------------+
```

Each line does three jobs at once, which is the whole reason this beats a cycle:

1. **It is the legend.** The glyph and color on the line are the glyph and color
   on the board. Nothing is inferred from paint alone.
2. **It is the numeric answer.** Often the number _is_ the question, and the
   player never has to read the board.
3. **It is the control.** Tapping a line mutes that field. All three start on;
   the mute persists for the session.

A muted line takes DESIGN.md's **disabled** treatment: 40% ink outline, road-tan
fill, no shadow, lying flat. That is already the system's exact vocabulary for a
key that is off, so the control needs no new visual language. A marker beside a
value flags a stat the weather or terrain has moved off its base.

Against the cycle, point by point: a cycle is unlabeled, this is labeled; a
cycle is serial, this shows any subset including all three; a cycle gives no
number, this gives three; a cycle costs two taps to reach vision, this costs
one; a cycle has no discoverability, this is on screen the moment a unit is
read.

### Discovery on a phone: there is nothing to discover

Inspection is not a feature a player should find. It is what the tap they
already make does. A player taps an enemy out of curiosity in the first minute
of any match; today that teaches them the board is dead. One tap that pays off
teaches the entire feature with no onboarding, no tooltip, and no coach mark.

The one copy change that carries it already exists. `TileInfoBar`'s
coarse-pointer hint reads "Tap a tile". It becomes **"Tap a unit to read it"**.
That is the whole discovery surface, already built, already docked, already
conditional on a coarse pointer.

### Focal moment

**Walking a Recon forward on a fog map and watching the amber ring travel with
the ghost.** At `DestinationSelected` the vision ring moves to the proposed
destination while the fire outline narrows to what is attackable from there. The
player sees what the move uncovers before committing it.

### The danger zone

A board-level toggle, not part of the per-unit readout, because it is a
statement about the whole board.

- **It is a hatch, not a glass.** Diagonal pixel hatch in damage red at low
  alpha. Hatch reads _through_ the cyan movement glass because it is texture
  rather than tint, so both survive being on screen together, which is required:
  "if I move here, am I in danger" is the question. The rule this establishes is
  clean: **glass is now, hatch is next turn.**
- **It takes the selected unit's kind.** With nothing selected it shows where any
  enemy can reach. With an Infantry picked up it shows where something that
  hurts infantry can reach, which is a different and more actionable shape.
- **Under fog it says so once, on the control.** The toggle reads `DANGER · SEEN`
  on a fog map and `DANGER` otherwise. This departs from the attack brief's
  no-disclaimer rule deliberately, because the failure costs differ: a wrong
  forecast costs one exchange, a wrong danger zone costs the unit and reads as
  the interface having lied. One word on the control is not an asterisk on the
  board.

## Reconciliation with the attack brief

Slices 2 and 3 of _The Attack Half of a Move_ (the red threat field,
click-an-enemy targeting, the indirect fire ring) are not built and are
**subsumed by slice 1 below**. A builder implementing them separately would
produce a second visual language for the same fields. Their settled decisions
all survive intact; only the ownership moves.

## Scope and boundaries

**In scope:** the `InspectedUnit` subject and its tap path; the three
simultaneous fields and their form encoding; true vision including blind tiles,
weather, and elevation; the readout's three lines as legend, answer, and
control; ring-follows-ghost at `DestinationSelected`; the danger-zone toggle;
inspection in spectator and replay.

**Untouched:** the command state machine, every commit path, pathfinding, the
attack forecast, the camera policy, the `MatchCommand` protocol, and server
authority.

**Anti-goals:**

- **No cycling.** It is the thing being replaced, and a fallback cycle makes the
  replacement worse.
- **No inspect mode to arm.** A mode that must be turned on before tapping is
  worse than a cycle, not better.
- **No long-press.** Inherited, and still correct.
- **No painting on hover.** Hover keeps reporting text; the board would strobe as
  the mouse crossed it. Both platforms paint on the same gesture.
- **No new motion.** DESIGN.md permits exactly one authored gesture. Three
  simultaneous fields must not pulse.
- **No second commit path.** Inspection commits nothing, ever.

## States and ranges

Vision runs 1, the rain floor, to 5 or more for a Recon or a boosted unit on a
mountain. Indirect ranges run 2-3 for Artillery through 2-6 for a Battleship.
Move ranges run 1 to about 40, per the parent brief. A 30x30 map may hold 40
enemy units, which is the danger zone's worst case.

Material states: nothing inspected; a unit inspected that is also selected;
inspected but not commandable; inspected during `DestinationSelected` with the
ring on the ghost; a unit with no weapon, where the range line reads as absent
rather than being dropped, so the absence stays legible; a unit inside a
transport; fog on, with the danger zone showing a floor; rain reducing sight; a
field muted by the player; and a turn boundary, which clears the inspection.

## Constraints and open decisions

- **`awbrn-client` does not depend on `awbrn-ai`.** `ThreatMap` is where the
  union-of-enemy-reach work already lives and it is not reachable from the
  client today. The builder either adds the dependency or composes the union
  from `awvm::query::Sweep::reachable_into` per enemy. A real cost, decided
  before the danger-zone slice starts rather than during it.
- **The danger zone must be cached.** Computed once per turn boundary,
  invalidated on any state change. Three per-unit fields per tap are cheap;
  forty move fields are not.
- **Accessibility.** The three readout lines are focusable controls, so a screen
  reader gets the whole feature as text, independent of any paint on the canvas.
- **Open:** whether inspecting a loaded transport lets the player read its cargo
  from the transport's tile, which answers "what could this unload into". Leaning
  yes, out of slice 1.
- **Open:** whether a commander power that changes range or vision mid-turn
  repaints an open inspection. Leaning yes, and cheap, since the fields are
  recomputed per tap.

## Slices

1. **Inspection as a subject.** `InspectedUnit`, tap any unit anywhere, the
   three fields simultaneously, the readout's three lines read-only, on every
   seat and in replay. This alone answers the original question and subsumes the
   attack brief's unbuilt slices.
2. **The lines become controls, and vision becomes true.** Per-field muting with
   session persistence; blind tiles, weather-reduced sight, elevation bonus.
3. **The danger zone.** Board toggle, hatched, unit-kind-aware, fog-labeled.
