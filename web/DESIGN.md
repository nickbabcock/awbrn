---
name: AWBRN
description: A browser client for Advance Wars By Web that looks like the game it serves.
colors:
  ink: "#16181d"
  sky: "#6bb4db"
  menu-cream: "#fff8e4"
  panel-white: "#fffcf2"
  road-tan: "#efdcae"
  command-orange: "#f07c1e"
  orange-wash: "#fbd9af"
  briefing-brown: "#443c2e"
  briefing-brown-faded: "#6d6350"
  deep-orange: "#a34803"
  repair-green: "#1e8a3c"
  damage-red: "#c9221a"
  low-supply-amber: "#b98100"
typography:
  display:
    fontFamily: "Bungee, system-ui, sans-serif"
    fontSize: "37.9px"
    fontWeight: 400
    lineHeight: 1.15
    letterSpacing: "0.005em"
  headline:
    fontFamily: "Bungee, system-ui, sans-serif"
    fontSize: "28.4px"
    fontWeight: 400
    lineHeight: 1.2
    letterSpacing: "0.01em"
  title:
    fontFamily: "Nunito, system-ui, sans-serif"
    fontSize: "21.3px"
    fontWeight: 700
    lineHeight: 1.4
  body:
    fontFamily: "Nunito, system-ui, sans-serif"
    fontSize: "16px"
    fontWeight: 400
    lineHeight: 1.5
  label:
    fontFamily: "Silkscreen, ui-monospace, monospace"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.5
    letterSpacing: "0.06em"
rounded:
  inner: "0px"
  element: "2px"
  container: "4px"
spacing:
  tight: "4px"
  snug: "8px"
  base: "16px"
  panel: "24px"
  section: "32px"
components:
  button-primary:
    backgroundColor: "{colors.command-orange}"
    textColor: "{colors.ink}"
    rounded: "{rounded.element}"
    typography: "{typography.label}"
  button-secondary:
    backgroundColor: "{colors.menu-cream}"
    textColor: "{colors.ink}"
    rounded: "{rounded.element}"
    typography: "{typography.label}"
  button-disabled:
    backgroundColor: "{colors.road-tan}"
    textColor: "{colors.briefing-brown-faded}"
    rounded: "{rounded.element}"
  card:
    backgroundColor: "{colors.panel-white}"
    textColor: "{colors.ink}"
    rounded: "{rounded.container}"
    padding: "{spacing.panel}"
  card-muted:
    backgroundColor: "{colors.road-tan}"
    textColor: "{colors.ink}"
    rounded: "{rounded.container}"
    padding: "{spacing.panel}"
  text-input:
    backgroundColor: "#fffdf7"
    textColor: "{colors.ink}"
    rounded: "{rounded.element}"
  nav-item:
    textColor: "{colors.briefing-brown}"
    typography: "{typography.label}"
    rounded: "{rounded.element}"
  nav-item-selected:
    backgroundColor: "{colors.command-orange}"
    textColor: "{colors.ink}"
    rounded: "{rounded.element}"
  empty-state:
    backgroundColor: "{colors.road-tan}"
    textColor: "{colors.ink}"
    rounded: "{rounded.element}"
---

# Design System: AWBRN

## Overview

**Creative North Star: "The Daylight Command Menu"**

AWBRN takes its design system from four surfaces of Advance Wars at once, each
assigned one job. The **in-game menu chrome** supplies the component language:
cream panels outlined in a single black, square-cornered, cast onto what is
behind them. The **map tile grid** supplies the ground plane, an open sky under
a fixed grid that every page floats above. The **CO intel and briefing screens**
supply data display: portraits at native resolution, meters, and stat readouts.
The **box art and manual** supply the voices, a signage display face and a
bitmap HUD face.

The system is daylight only, and that is a commitment rather than a default. The
source game is played in the open under a high sun; there is no night palette,
and adding one would mean inventing a world the game does not have. Interface
quality is the product's stated position, so the system is built to be
recognized before a word is read: a player who knows the game should know this
is for them from the first viewport.

The anti-reference is the neutral SaaS dashboard, which is where every tool in
this category lands, and which the previous AWBRN theme was drifting toward: a
generic component library wearing a retro font. The refusal is specific. There
are no soft grays, no ambient blur-only shadows, no rounded-rectangle cards on a
white page.

**Key Characteristics:**

- One black outline on every piece of chrome, two pixels wide
- Panels float on visible terrain, never on a flat page
- Three type voices with strictly separated jobs
- Square corners; the largest radius in the system is 4px
- Color carries army identity, but never alone

## Colors

A bright daylight palette: an open sky, cream field-manual paper, and one
command orange for action, all held together by a single black.

### Primary

- **Command Orange** (`{colors.command-orange}`): the only call to action in the
  system. It fills the primary button, the selected navigation tab, and the
  focus ring. Text on it is always black, never white, which is the relationship
  the game's own logo uses.
- **Deep Orange** (`{colors.deep-orange}`): the accent hue darkened to clear
  4.5:1 on cream. Used only for accent-colored text and inline links, never as a
  fill.

### Secondary

- **Open Sky** (`{colors.sky}`): the ground plane. It is the page itself, under a
  fixed grid, and it is never used as a fill for a component. It is the one
  large field of saturated color in the system, and it earns that by being
  terrain rather than decoration.

### Neutral

- **Ink** (`{colors.ink}`): the single black. Every outline, every primary text
  run, every icon at full strength. The system has exactly one black and does
  not blend toward gray.
- **Menu Cream** (`{colors.menu-cream}`): the standard panel and navigation-bar
  fill, the paper of the field manual.
- **Panel White** (`{colors.panel-white}`): the lifted panel fill, a half-step
  brighter than cream, used for cards that sit above the standard surface.
- **Road Tan** (`{colors.road-tan}`): the recessed fill, for muted sections and
  empty-state wells. It reads as the road running through the map.
- **Briefing Brown** (`{colors.briefing-brown}`): secondary text. It is darker
  than a cream-only palette would need, because it must also clear 4.5:1 against
  the open sky it sits on.

### Tertiary

Status colors are named for what they mean in the game, not for a generic
severity scale: **Repair Green** (`{colors.repair-green}`), **Damage Red**
(`{colors.damage-red}`), and **Low Supply Amber** (`{colors.low-supply-amber}`).

Alongside these, twenty army palettes (Orange Star, Blue Moon, Green Earth, and
the rest) come from AWBW's own faction colors. Each has an identity accent, a
chip fill, a panel wash, and a darkened text value. They are faction data, not
theme decoration, and they do not participate in the general color scale.

### Named Rules

**The One Black Rule.** Every border, divider, outline, and primary text run uses
the same ink. There is no border gray, no text gray, and no softened outline. If
something needs to recede, it changes opacity of that one black.

**The Army Never Rides Alone Rule.** A faction's color never carries its identity
by itself. Every faction surface also shows the army's letter or its name.
Faction panels wear the color as a bar across the top and still name the army in
text.

**The Terrain Is Not A Fill Rule.** Sky is the ground plane and nothing else. A
component that wants to be blue is wrong; components are cream, white, or tan.

## Typography

**Display Font:** Bungee (with system-ui, sans-serif)
**Body Font:** Nunito (with system-ui, sans-serif)
**Label/HUD Font:** Silkscreen (with ui-monospace, monospace)

**Character:** Three voices, each borrowed from a different surface of the source
game and never mixed inside one sentence. Bungee is signage lettering: chunky,
layered, built to be shouted, and it stands in for box art. Silkscreen is the
in-game HUD bitmap, for readouts and labels. Nunito is warm and rounded, the
voice of a briefing or a line of dialogue, and it carries everything a person
actually reads.

The scale runs from a 16px base at a wide 1.333 ratio, which gives the display
voice enough room to behave like a title screen rather than a heading.

### Hierarchy

- **Display** (Bungee 400, 37.9px, 1.15): page titles only. One per screen.
- **Headline** (Bungee 400, 28.4px, 1.2): panel titles and match names.
- **Title** (Nunito 700, 21.3px, 1.4): headings below level 2. The signage face
  turns into noise at these sizes, so the body face carries them at full weight.
- **Body** (Nunito 400, 16px, 1.5): all prose. Measure stays within 65–75ch.
- **Label** (Silkscreen 400, 12px, 0.06em, uppercase): HUD readouts, metadata
  strips, navigation tabs, button text, badges, and section labels.

### Named Rules

**The One Voice Per Sentence Rule.** A single sentence never mixes the HUD face
with the body face. A metadata strip ("Host · Map 162795 · Fog off") is HUD. A
sentence with a verb is body. A line that has a link inside it is body, always.

**The Bitmap Floor Rule.** Silkscreen never renders below 12px, and never
sets more than two lines of running text. Below that size, or beyond that
length, the bitmap stops being readable and becomes texture.

## Layout

Content sits in panels on a continuous ground plane. The app shell paints the
sky, a sun wash at the horizon, and a 32px tile grid at 11% white, all fixed to
the viewport, so the terrain stays put while panels scroll over it. Every shell
region between the shell and the content is transparent; a flat fill anywhere in
that chain breaks the ground and reads as a rendering bug.

Spacing runs on a 4px scale. Panels take 24px of internal padding, panel stacks
separate at 24–32px, and grouped items inside a panel sit at 8–16px. Headings
take more space above than below.

The two-column arrangements collapse to one at their grid minimum, and the
navigation collapses to a hamburger with the sign-in and register actions kept
visible. Phone width is a hard requirement, not a fallback: no surface may rely
on desktop width to be usable, and the body never scrolls horizontally.

## Elevation & Depth

Depth is a two-part shadow: a hard pixel step with no blur, plus a soft blur
underneath that lands the panel on the ground. The hard step is the GBA menu
drop shadow and carries the pixel character; the blur is what keeps it from
reading as a sticker outline. Neither works alone, and the shadow color is a
translucent ink rather than solid black, because a panel is casting onto terrain
rather than being outlined.

### Shadow Vocabulary

- **Low** (`box-shadow: 2px 2px 0 0 rgba(22,24,29,0.34), 2px 3px 6px 0 rgba(22,24,29,0.16)`):
  buttons and banners, chrome that sits just off the surface.
- **Med** (`box-shadow: 3px 3px 0 0 rgba(22,24,29,0.34), 4px 6px 10px 0 rgba(22,24,29,0.16)`):
  the default for panels, cards, sections, lists, and the navigation bar.
- **High** (`box-shadow: 5px 5px 0 0 rgba(22,24,29,0.34), 7px 10px 18px 0 rgba(22,24,29,0.16)`):
  reserved for content that overlays other content.

### Named Rules

**The No Stacked Chrome Rule.** Anything already inside a panel does not get its
own outline and shadow. Empty states, wells, and nested regions get a recessed
tan fill with a dashed rule instead. Two borders in a row is the tell of a
system that stopped thinking about where its components live.

**The Press Goes Down Rule.** The system has exactly one authored gesture. A
pressed button or navigation tab moves 2px into its own shadow and the shadow is
removed, so the key physically goes down. Nothing else in the system scales,
fades, or lifts on interaction.

## Shapes

Square. The radius scale tops out at 4px and starts at 0, because pixel art has
no radius and the source game's menus are boxes. Chips and code use 0, controls
use 2px, panels use 4px. Nothing is pill-shaped and nothing is circular except a
status dot.

Every panel, control, field, chip, portrait, and thumbnail carries a 2px solid
ink border. The border is the system's most consistent element and the fastest
way to tell whether a new component belongs.

Inputs invert the depth model: instead of a cast shadow they take a 2px inset
shadow, so a field reads as sinking into the panel while a button reads as
rising off it.

## Components

### Buttons

- **Shape:** square (2px radius), 2px ink border, low shadow.
- **Typography:** Silkscreen, 12px, uppercase, 0.06em tracking.
- **Primary:** command orange fill with black text.
- **Secondary:** menu cream fill with black text.
- **Destructive:** damage red fill with cream text.
- **Ghost:** no fill, no border, no shadow; the one command that is not a raised
  menu key.
- **Active:** translates 2px into its shadow, shadow removed.
- **Disabled:** keeps the outline at 40% ink, takes a road-tan fill, loses the
  shadow, and lies flat. A disabled command in game is still a key on the menu;
  dropping the outline makes it read as broken rather than unavailable.

### Cards / Containers

- **Corner Style:** 4px.
- **Background:** panel white by default, road tan for the muted variant.
- **Border:** 2px ink.
- **Shadow:** med.
- **Internal Padding:** 24px.
- **Faction variants:** the army's color as a 6px inset bar across the top,
  the army's wash as the fill, and the standard black outline and shadow.

### Inputs / Fields

- **Style:** near-white fill, 2px ink border, 2px radius, 2px inset shadow so the
  field sinks rather than rises.
- **File input:** the same, with a dashed border to mark a drop target.
- **Checkbox:** square, 0px radius, 2px ink border.

### Navigation

- Cream bar with a 3px ink bottom rule and a med shadow, sitting directly on the
  sky.
- Wordmark in Bungee at 21.3px; tabs in Silkscreen uppercase at 12px.
- The selected tab is the game's cursor: orange fill, 2px ink border, no shadow,
  because a cursor sits flush on the chrome rather than above it.
- Below the grid minimum the tabs collapse to a hamburger and the sign-in and
  register actions stay visible.

### Lists and Readouts

- Lists are single outlined cream panels with med shadow and hidden overflow;
  rows divide with a 2px rule at 18% ink. Rows are edge-to-edge and are never
  individually card-wrapped.
- Metadata lists carry a 2px ink rule on top and 18% ink rules between items,
  with values in the HUD voice. This is the CO intel readout, and it is where the
  bitmap face does its best work.

### Sprites and Portraits

Every sprite surface renders `image-rendering: pixelated` and takes a 2px ink
border with a 2px radius. This covers CO portraits, unit icons, UI atlas
sprites, map thumbnails, avatars, and thumbnails. Sprite art is the product's
material; a browser smoothing it is a defect.

## Do's and Don'ts

### Do:

- **Do** give every panel, control, and sprite a 2px `{colors.ink}` border.
- **Do** put content in panels on the terrain, and keep every shell region
  between the app shell and the content transparent.
- **Do** pair the hard shadow step with its soft blur. Both halves, always.
- **Do** use the HUD face for readouts, labels, tabs, buttons, and badges, and
  the body face for anything with a verb in it.
- **Do** render sprite art with `image-rendering: pixelated`.
- **Do** name a status for what it means in the game (repair, damage, low
  supply) rather than for a severity level.
- **Do** check secondary text against the sky as well as against cream. Sky is a
  mid-tone and it is the harder ground.

### Don't:

- **Don't** add a dark mode. The system is daylight only and the app pins
  `<Theme mode="light">`. A half-built night palette reads as a bug.
- **Don't** put an eyebrow or kicker above a heading. The heading carries its own
  weight.
- **Don't** give a component its own border and shadow when it already sits
  inside a panel. Use a recessed tan well with a dashed rule.
- **Don't** set running prose in Silkscreen, or set it below 12px.
- **Don't** use sky as a component fill, or any faction color as a page ground.
- **Don't** encode a faction by color alone; always show the letter or the name.
- **Don't** introduce a second black, a gray border, or a radius above 4px.
- **Don't** add a second interaction gesture. The press-into-shadow is the whole
  motion vocabulary.
- **Don't** override `--color-*` in `:root`. Brand and accent changes go through
  `src/themes/awbrnTheme.ts` and `pnpm run theme:build`.
