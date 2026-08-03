---
version: 1
slug: "web-src-matches-screens-matchactivepage-tsx"
primary_target: "web/src/matches/screens/MatchActivePage.tsx"
related_targets:
  [
    "crates/awbrn-client/src/modes/play/mod.rs",
    "crates/awbrn-client/src/features/input.rs",
    "crates/awbrn-client/src/features/camera.rs",
    "web/src/matches/components/BuildMenu.tsx",
  ]
---

# Surface Brief: Unit Command on the Board

Mode: **Operate**. The visitor completes a task, and a wrong outcome is
irreversible and expensive.

## Job and audience

Two players, one surface.

- A **desk player** with a mouse, keyboard, and 25+ tiles in view, who wants to
  issue turns fast and route units precisely.
- A **phone player** on a 360–430px viewport, one thumb, often mid-commute, who
  wants one confident, correct move without a mistake that costs a unit.

Both arrive knowing Advance Wars. The move-then-menu loop is muscle memory they
already have. Every gesture invented here is a gesture they must learn instead.

## Outcome and proof

**Primary task:** select a unit, choose where it goes, choose what it does
there, commit.

**Success:** a phone player completes a move-and-attack in one uninterrupted
sequence, at a zoom where they can see the enemy they attack, without ever
committing something they did not intend. Failure is not slowness. Failure is
any commit the player did not mean.

**Proof it works:** every irreversible command passes through a labeled,
dismissable control. No irreversible action is reachable by a bare tap on the
map.

## Selected direction

**Interaction thesis: one state machine, two ways in, one way out.**

The `Idle → UnitSelected → DestinationSelected` machine is correct and stays.
What changes is that proposing a destination has two equal entry gestures, and
committing has exactly one exit. Today the machine has two entrances that behave
differently and three exits of unequal safety.

- **Entry A, discrete taps** (both platforms): tap unit, then tap destination.
- **Entry B, continuous drag** (both platforms): press on a friendly active
  unit, drag, release. The path previews live under the pointer for the whole
  drag. This gives touch the route preview that desktop gets from hover, and it
  makes the phone the fastest device for a simple move rather than the most
  dangerous one.
- **The single exit, the action menu**, anchored at the destination: Wait,
  Attack, Capture, Load, Join, Supply, Unload, Cancel. It commits. Nothing else
  does.

**Focal moment:** the instant the menu opens at the destination tile with the
ghosted unit standing there and the route drawn behind it. The player sees the
whole intent rendered before it becomes real, and it is the same moment on both
platforms.

**Implementation consequence:** the action menu generalizes the shipped
`BuildMenu` rather than duplicating it. `BuildMenu` already carries the dual
`sheet`/`board` presentation, board anchoring from a press point, dismiss on
outside-press and Cancel and Escape, focus restore, and arrow-key traversal.
`TileClicked` is replaced by a gesture layer that both `detect_map_clicks` and
`detect_touch_taps` collapse into.

## Scope and boundaries

**In scope:** the gesture recognition layer, the coarse-pointer camera policy,
the action menu and its states, the touch cancel and undo affordances, and
pointer-conditional HUD copy.

**Untouched:** pathfinding, move-range computation, fuel and occupancy
revalidation on commit, the `MatchCommand` protocol shape, server authority, the
visual system, and replay-mode input.

**Anti-goals:**

- No long-press. It fights the drag gesture and adds a hidden timing rule.
- No confirmation dialog on top of the action menu. The menu is the
  confirmation.
- No mobile fork of the play surface. Same machine, same menu, different input
  recognition and camera defaults.
- No slop correction toward attack targets. Slop may only resolve toward
  reachable destination tiles. A deliberate release on an enemy is a different
  thing, defined below, and is not slop.

## States and ranges

Maps run about 15x10 to 30x30. A phone at fit-zoom on the largest map is the
worst case and the one to design against. Move ranges span 1 tile, for a damaged
low-fuel unit, to about 40, for an unblocked Bike or Copter. The action menu
carries 1 to 7 orders. The one-order case still shows the menu, because a commit
target that changes size teaches the player nothing they can rely on.

Material states: no unit selected; unit selected with range shown; drag in
flight; destination proposed; menu open; command sent and awaiting the server;
server rejection; connection lost mid-gesture; and not-your-turn, where the
board stays inspectable but never commits.

## Interaction and layout

### Gesture layer

One recognizer emits `Tap`, `DragStart`/`DragMove`/`DragEnd`, and `Pinch` from
mouse and touch alike, replacing the two current detectors. The mouse is held to
the discipline touch already has: **fire on release, with a movement
threshold**. This alone removes the left-drag-pans-and-also-clicks collision.

Disambiguation happens at press time. No timers:

| Press lands on       | Single-pointer drag          | Release without moving |
| -------------------- | ---------------------------- | ---------------------- |
| Friendly active unit | Move drag, live path preview | Select unit            |
| Anywhere else        | Camera pan                   | Tap that tile          |

Two pointers always pinch-zoom, and cancel any move drag in flight without
committing.

### Camera policy for coarse pointers

**The touch floor is 40 CSS px per tile.** This is deliberately below the Apple
44pt and Material 48dp minimums, and the interaction model earns the difference:
those minimums assume a blind tap on a control the finger cannot adjust. A drag
lands anywhere and the player corrects before release while watching the route
redraw, so landing precision does not matter. A tap gets slop correction. The
floor therefore only has to protect the bare tap that selects a unit, which is
the cheapest and most recoverable action in the flow. A strict 44 would cost
about three tiles of view on a 390px phone for no gain the model does not
already provide.

The coarse-pointer default zoom comes from viewport width and never starts below
the floor. Selecting a unit eases the camera to keep the unit and its reachable
range in view. The player must never have to choose between seeing the board and
being able to touch it.

Zooming out below the floor stays allowed, because orientation on a large map
needs it. Below the floor, taps commit nothing: they select only, and the camera
returns to the working zoom.

### Slop correction

When a tap lands on a non-reachable tile within a small radius of a highlighted
one, resolve to the nearest reachable tile. The reachable set is already
computed in `MoveRange`, so this is nearly free. Destinations only.

### Feedback

Move range and route keep their current treatment. Add a ghosted unit at the
proposed destination. `DestinationSelected` is currently almost
indistinguishable from `UnitSelected`, which is what makes an accidental second
tap so costly today. The route redraws continuously during a drag.

### Drag release

**The route clamps during the drag, not on release.** Once the pointer leaves
the reachable range the route stops following it and holds at the last valid
tile, so the preview never shows an illegal state and there is no moment where
the player believes something wrong. Overshoot is the most common drag error,
and cancelling the whole gesture on overshoot punishes it hardest. The decoupled
route needs a visible signal, or the hold reads as a freeze.

**Release on an enemy unit is explicit attack intent**, not slop, and is
therefore exempt from the anti-goal above. It clamps to the cheapest reachable
tile adjacent to that enemy, breaking ties deterministically in the same map
order `shortest_path` already uses, and opens the menu with Attack
pre-highlighted. It does not auto-commit. The menu still commits. Leaving this
case to fall out of the clamping rule is how a player comes to believe they
attacked when they did not.

### Cancel and undo, on every platform

Escape and Backspace keep their desktop meanings and each gain a touch
equivalent. The menu's Cancel and an outside-press return to `UnitSelected`, not
to `Idle`. A mis-tap must never discard the whole selection, which is today's
behavior. A drag released on the origin tile cancels harmlessly.

Custom route tracing stays a desktop power feature on Shift. Touch gets
equivalent expressiveness from the drag path itself, not from a Shift
substitute.

### Menu placement

`board` presentation anchors near the destination without covering the
destination or the route. `sheet` presentation on coarse pointers or under
767px, matching the rule `BuildMenu` already follows. Orders use the game's own
vocabulary and order. Cancel is always last and always present.

## Constraints and resolved decisions

Rust and Bevy engine over a `CanvasCourier` DOM bridge. Menu chrome is React and
Astryx per `web/AGENTS.md`. Both sides must agree on which tile is under the
pointer, so the destination the menu names comes from the engine and is never
recomputed in TypeScript. Available orders come from AWVM, not from a
hand-written list in the client, following the discipline
`emit_production_options` already uses. The menu stays keyboard-operable and
focus-restoring, as `BuildMenu` is.

Three decisions were open and are now settled. A builder implements these as
written rather than choosing again:

1. **Touch floor: 40 CSS px per tile**, for the reasons in the camera policy
   above.
2. **A rejected command restores the selection at the origin with the route
   intact**, so the player adjusts and retries in one tap. Today
   `handle_play_tile_clicks` calls `clear_selection_state` the instant it emits,
   so a rejection loses the unit, the range, and the route and returns a generic
   banner. This requires the client to hold a rollback snapshot until the server
   acknowledges, which touches `ReplayAdvanceLock` and the animation path
   because the unit may already be moving on screen. That cost is accepted.
3. **The route clamps during the drag**, with release-on-enemy treated as
   explicit attack intent, as specified above.

## Defects to fix regardless

These are live today and are not contingent on this brief proceeding:

- The left-drag/click collision. `detect_map_clicks` fires on `just_pressed`
  with no movement threshold while `handle_mouse_pan` claims the same button, so
  every pan drag emits a click at the drag origin.
- The HUD copy at `MatchActivePage.tsx:522` instructs phone players to hover,
  Shift-trace, and press Backspace. It must be pointer-conditional.
- The send-failure message at `MatchActivePage.tsx:170` says "The attack could
  not be sent" for every command, including a plain move.
