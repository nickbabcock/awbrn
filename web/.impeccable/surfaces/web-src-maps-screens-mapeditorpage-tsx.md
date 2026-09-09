---
version: 1
slug: "web-src-maps-screens-mapeditorpage-tsx"
primary_target: "web/src/maps/screens/MapEditorPage.tsx"
related_targets:
  [
    "web/src/maps/editor/SymmetryPanel.tsx",
    "web/src/maps/editor/PaletteRail.tsx",
    "web/src/maps/editor/useEditorShortcuts.ts",
    "web/src/maps/map_editor.ts",
  ]
---

# Surface Brief: The Drafting Table

Mode: **Operate**. The task is drawing a battlefield that plays fair, and
finishing it in one sitting rather than in twenty passes of counting tiles.

## Job and audience

An AWBW player who makes maps. They already know what a good map is: two
armies with the same ground, the same buildings and the same distance to the
middle. What they do not have is a tool that keeps that promise for them. The
incumbent editor makes them draw each half by hand and then find the tile they
got wrong by losing a match on it.

They arrive one of three ways: a blank board, a fork of somebody else's map,
or another revision of a map they already wrote.

## Outcome and proof

**Primary task:** draw a symmetric map and keep it in the catalog.

**Success:** half a map is drawn, and the other half is already correct —
including which army each building at the far end belongs to.

**Proof it works:** a stroke on the left is a stroke on the right; the
headquarters that arrives opposite an Orange Star headquarters is Blue Moon's;
and the panel says so before the map is saved rather than after a match is
lost on it.

## Selected direction

**One drafting table, not three floating panels.** The board, the brushes and
the settings are one outlined cream panel divided by the same black rule every
panel in this system wears. Three cards on the sky would read as three
unrelated tools; a map maker is doing one thing.

**The fold is the screen, not a setting on it.** Eight authored diagrams, each
showing the board, the part of it that is drawn on, and what happens to the
rest. A mode the board cannot hold keeps its key, dimmed, and says why: the
answer to "where is the quarter turn" is "make the board square", and hiding
the key hides the answer with it.

**The rail is the game's own art.** Terrain is drawn from the same atlas cell
the board draws, overhang and all, so the tile on the key is the tile that
lands on the map. Names stay under the art, because an airport and a port are
one roof apart at this size.

Every key holds two lines for its name whether it needs them or not. A rail of
keys each as tall as its own name has a ragged floor, and the eye reads the
ragged edge as the tiles being different sizes rather than the words being
different lengths. A name too long for one line breaks at a soft hyphen the
screen marks — "PIPE-RUNNER", not "PIPERUNN ER" — from a short list in
`keyName`, because automatic hyphenation needs a dictionary the browser may not
have and a rail that reads differently on two machines is worse than a list.

The key is five to a row, not four. The terrain art is thirty-two pixels wide
at the scale the board draws it, and the key around it was carrying half its
own width in air; the rail is twenty-four pixels wider and every drawer is a
row shorter for it. The name keeps its twelve-pixel bitmap — the system's floor
— so the shorter line is paid for with three more breaks in `keyName`, not with
a size the face stops being readable at.

**The rail has three drawers, not five headings.** Fifty keys under a heading
for each group is a rail longer than the window, and a map maker who scrolls
to reach the sea loses the board while they do it. The land, the buildings and
the units open one at a time, and the longest of them — twenty-six units —
fits on the screen whole. Inside a drawer the order still runs ground, water,
ways, which is the grouping the headings used to carry.

A drawer is shut to buy height, so a rail with height to spare shuts nothing.
The rail measures itself and opens every further drawer that fits whole under
the one in use: at 1380px the land and the buildings are both under the hand,
at 900px the land is open alone, and the answer follows a window that is
resized. The drawers never change places while this happens, the name of each
is always on the screen — it is the name that opens it, and the drawer in use
wears the orange rule because it is the one the bracket keys step along — and
the rail itself never scrolls. Only the drawer in use gives up height, because
it is the only one opened without first being measured against the room left
for it. It says so, and it says so in something it draws itself: the edge
that has more behind it fades out. A scrollbar is the platform's to draw or to
hide, and a system with overlay scrollbars leaves the drawer looking sliced off
at the bottom with nothing to say it continues, which reads as broken rather
than as deep. A fade costs no layout, is the same on every machine, and goes
away at the end of the scroll, which a gutter held open cannot do. The
platform's own scrollbar is still asked for — drawn in the panel's ink at
`thin` width, its gutter held open so the keys never rewrap as a drawer fills —
but it is the second answer now, not the only one.

A drawer that scrolls stops on whole rows. A row of keys cut through the middle
at the edge is the other half of reading as broken, so every key is a snap point
at proximity: a correction to where a scroll lands, never a rail that takes the
scroll over.

**The muster is the half symmetry cannot promise.** A fold keeps the ground
equal and says nothing about whether the map is a match. It prices what each
army would hold and names what is missing — an army with no headquarters, an
army that cannot build, a board that has drifted out of its own fold. It never
blocks a save: a board part way through is not an error. It reads across,
under the board, because it is a readout of the map beside it and a column
would cost the board the width the readout is written in.

**What the board is worth is one figure about the board.** `HQ 1 · Build 2 ·
Prop 3 · Unit 2` is four numbers a map maker has to do arithmetic on to reach
the one question they were asking, which is whether the map pays fairly. Money
answers it, so the strip under the board prints what every income-paying
building on the board is worth per turn, and that figure shared evenly between
the armies the fold seats — the number a map maker is drawing towards, which is
what a half of this map is worth when the halves are equal.

It is global on purpose. Splitting the money between the seats means deciding
which half of the map each neutral building ends up in, and any rule for that —
nearest on foot, nearest in a straight line — is a model of a match rather than
a fact about the board. A map maker who disagrees with the model gets a per-army
figure they cannot trust, which is worse than one they have to compare against
the board themselves. The share is arithmetic that cannot be wrong.

The rate is AWVM's, not the screen's. Which buildings pay is
`TerrainTrait::Income` — a com tower and a lab are held like any other building
and pay nothing, so the money is not the building count — and what one pays is
the same constant a real match is set up with, in `awbrn-map::rules`. That is
what keeps this a query rather than a multiplication typed onto a screen.

The seat tiles keep their counts and gain a headquarters mark. One headquarters
is the playable case; none and two are both faults the notes name in words, so a
mark that tried to carry three states would be a puzzle.

**A control appears when the thing it is for does.** The resize anchor is on
the panel only while a resize is waiting to be made. Sitting under two figures
at rest it read as a third thing about the board rather than as the one
question a resize asks, and appearing with the enabled key answers "what is
this for" without a sentence.

**The whole table is one screen.** Nothing on this surface scrolls the page:
the rail, the board and the settings are all within reach of the hand that is
drawing. That budget is what pays for the drawers, for the muster reading
across, and for keeping the map's name and the save key up in the deck rather
than at the bottom of a settings column. The screen carries one heading — the
fold — because a labelled control does not need a heading over it as well.

**The armies are the map's own.** A board that comes out of the catalog seats
the armies drawn on it, so a map of Red Fire and Brown Desert paints as Red
Fire and Brown Desert. The fold it opens under is read off the board too: a
mirrored map opens mirrored, and the first stroke keeps the symmetry rather
than breaking it.

**The rail answers to the keyboard, and the board hands brushes back.** A map
maker on their tenth board does not cross the screen fifty times. `1`, `2` and
`3` open the drawers, `[` and `]` step along the open one, `E` loads its
eraser, and shift with a number paints as a seat. Alt and a click on the board
loads the brush that drew the tile under the pointer — the eyedropper, which is
the one action a tile editor is unusable without, because a correction should
cost pointing at what is already right rather than hunting for it among fifty
keys. Adding shift reaches the ground under a unit.

The keys are printed down the settings column rather than hidden behind a key
of their own. This is a tool somebody sits at for an hour: the legend is part
of the instrument. It is the one place the screen carries a second heading,
because rows of keys with no name over them are a puzzle rather than a legend.

A row is a fragment, not a sentence: "Step the brush", not "Step to the brush
before or after this one". An expert legend is a table of keys, and a column of
seven sentences beside the board is prose a map maker has to read past to reach
what they came for. The pointer gestures are not in it at all — they are in the
strip under the board, where the hand that makes them is — so each list has one
voice, and the alt glyph the system prints for a Mac option key is off a screen
that is mostly used on Windows and Linux.

**The fold is drawn, not described.** Eight diagrams with a name under each are
the explanation; the line of prose that used to sit under them said the same
thing again in words. The sentence is kept on the key itself, where somebody
who wants it can rest on it.

**The fold is shown, not explained.** While a fold is set, the tile cursor is
drawn at every image of the tile under the pointer, dimmer than the one the
pointer is on. It is the answer to "where is this stroke going to land", given
before the stroke rather than in a sentence in the settings column, and the
images come from `MapEditor::images` — the same walk `paint` makes — so the
cursor cannot disagree with the stroke that follows it. A tile on the fold's
own axis is its own image and is drawn once. The images go away while the
eyedropper is held, because a pick lands on the one tile it points at.

**The fold promises the ground and nothing else.** A competitive map answers
the first-turn advantage with things that are meant to be uneven: a base set
where the second army takes it on turn one, a city one side reaches first, a
unit given to whoever moves second. So the readout checks the shape of the
land, the water and the ways across both, and leaves buildings and units to
whoever placed them. The one exception is a pair — two buildings of the same
kind facing each other across the fold — which must be in the armies the fold
assigns, because a map mirrored by hand almost always leaves one headquarters
in the colour it was copied from. That is the mistake the check exists for, and
it is the only one it claims.

Detection reads strictly where the readout reads leniently, and the split is
deliberate: detection is inferring which fold a map it has never seen was drawn
under, and the buildings are the best evidence there is.

**The muster counts and does not judge.** It prints what each army holds and
names what a board cannot be played without — an army with no headquarters, an
army that cannot build, ground that does not fold. It has no opinion about the
armies holding different numbers of buildings or units, because that difference
is usually the design rather than a defect.

**A fault names its tiles.** "The board does not read the same under half turn"
tells a map maker that something is wrong and nothing about where, which on a
19 by 19 board is 361 places to look. The note carries the count and the first
tile — "3 tiles do not fold under half turn: (4, 7) and 2 more" — because a
count says how much work is left and a coordinate says where to start. The
tiles come from `MapEditor::asymmetries`, which is the same walk the fold makes;
`is_symmetric` is now the convenience over it, so the note and the check can
never disagree.

## Constraints carried

- The board is the engine's own board. A map is drawn on exactly the surface
  it will be played on, which is also why the editor costs no second renderer.
- Every rule of an edit — autotiling, the fold, which army an image belongs to
  — lives in `awbrn-map::editor`. The screen decides what things are called
  and nothing else.
- Desktop first by request. The composition stacks rather than breaking below
  the three-column width, but the phone case has not been designed and the
  board is not usable with a finger yet. See the note in PRODUCT.md about
  phone-viability being a product requirement: this surface does not meet it.

## Interaction

- Drag to draw. Space and drag moves the board; the wheel zooms. A drag is
  claimed by the brush, so the board never slides under a stroke.
- Ctrl+Z and Ctrl+Shift+Z, and the two keys beside the title. One drag is one
  undo step.
- Saving is three different acts wearing one button: a new map is kept, an
  author's map takes another revision, and anybody else's map is forked. The
  button says which.
