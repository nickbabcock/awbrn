import { defineTheme } from "@astryxdesign/core/theme";
import { neutralTheme } from "@astryxdesign/theme-neutral";

/*
 * AWBRN theme: the Advance Wars daylight system.
 *
 * The system takes one job from each surface of the source game:
 *   - in-game menu chrome  -> panels, buttons, black rules, the cast shadow
 *   - the map tile grid    -> the ground plane every page floats on
 *   - CO intel screens     -> portraits, meters, and stat readouts
 *   - box art and manual   -> the display voice and the HUD labels
 *
 * The theme is daylight only. Do not add a dark tuple to these tokens; the app
 * pins <Theme mode="light">, and a half-built night palette reads as a bug.
 */

/** Ink: the outline on every piece of chrome. The system has exactly one black. */
const INK = "#16181D";
/** The cast shadow under a menu panel. Translucent, so panels sit on terrain. */
const CAST = "rgba(22, 24, 29, 0.34)";
const CAST_SOFT = "rgba(22, 24, 29, 0.16)";

/**
 * Army colors, from AWBW's own faction palette.
 *
 * `accent` is the army's identity color, `wash` tints a whole panel, `soft`
 * fills a chip, and `text` is the darkened value that stays legible on `soft`.
 * Color never carries faction identity alone: every faction surface also shows
 * the army's letter or name.
 */
const factionPalette = {
  os: { accent: "#ff4f4e", soft: "#ff4f4e33", text: "#8a1420", wash: "#ff4f4e26" },
  bm: { accent: "#708cfe", soft: "#708cfe38", text: "#1c3aa8", wash: "#708cfe2b" },
  ge: { accent: "#61d05a", soft: "#61d05a3d", text: "#1d6b16", wash: "#61d05a2b" },
  yc: { accent: "#f0d204", soft: "#f0d20447", text: "#6b5f00", wash: "#f0d2042e" },
  bh: { accent: "#74598a", soft: "#74598a33", text: "#4a3160", wash: "#74598a26" },
  rf: { accent: "#c27184", soft: "#c2718438", text: "#8d1c33", wash: "#c271842b" },
  gs: { accent: "#979797", soft: "#97979738", text: "#4d4d4d", wash: "#9797972b" },
  bd: { accent: "#ad7e5f", soft: "#ad7e5f38", text: "#6d3a18", wash: "#ad7e5f2b" },
  ab: { accent: "#fec078", soft: "#fca33947", text: "#7a4405", wash: "#fca3392e" },
  js: { accent: "#c4d7b4", soft: "#a9c48f47", text: "#4a5c38", wash: "#a9c48f2e" },
  ci: { accent: "#2342ba", soft: "#2342ba33", text: "#152a7d", wash: "#2342ba26" },
  pc: { accent: "#ff99cc", soft: "#ff99cc47", text: "#98134f", wash: "#ff99cc2e" },
  tg: { accent: "#6cd9d0", soft: "#6cd9d047", text: "#106b64", wash: "#6cd9d02e" },
  pl: { accent: "#a447d3", soft: "#a447d333", text: "#5f1487", wash: "#a447d326" },
  ar: { accent: "#7a9d11", soft: "#7a9d1140", text: "#42550a", wash: "#7a9d1129" },
  wn: { accent: "#d4bf9f", soft: "#d4bf9f4d", text: "#6b5334", wash: "#d4bf9f33" },
  aa: { accent: "#84dfe8", soft: "#84dfe84d", text: "#0d5f74", wash: "#84dfe833" },
  ne: { accent: "#6e6060", soft: "#6e606038", text: "#3a3030", wash: "#6e60602b" },
  sc: { accent: "#8cacbc", soft: "#8cacbc4d", text: "#2f5468", wash: "#8cacbc33" },
  uw: { accent: "#d47700", soft: "#d4770040", text: "#7a4400", wash: "#d4770029" },
} as const;

/** The HUD voice: bitmap type, uppercase, tracked out so it stays legible small. */
const hudLabel = {
  fontFamily: "var(--font-family-code)",
  fontSize: "var(--font-size-sm)",
  letterSpacing: "0.06em",
  textTransform: "uppercase",
} as const;

/** A menu panel: black outline, square corners, cast onto the terrain below. */
const panel = {
  borderColor: INK,
  borderStyle: "solid",
  borderWidth: "var(--border-width)",
  borderRadius: "var(--radius-container)",
} as const;

/**
 * Faction panels wear the army's color as a bar across the top, the way the
 * unit-info window does in game, then the standard black outline and shadow.
 */
const factionCardVariants = Object.fromEntries(
  Object.entries(factionPalette).map(([code, colors]) => [
    `variant:faction-${code}`,
    {
      ...panel,
      backgroundColor: colors.wash,
      boxShadow: `inset 0 6px 0 0 ${colors.accent}, var(--shadow-med)`,
    },
  ]),
);

/**
 * The same army colors as custom properties, so a component the theme does not
 * own (a roster row, a sprite frame) can wear a faction without a hardcoded
 * hex. The code is part of the token name because the army is only known at
 * runtime.
 */
const factionTokens = Object.fromEntries(
  Object.entries(factionPalette).flatMap(([code, colors]) =>
    Object.entries(colors).map(([role, value]) => [`--color-faction-${code}-${role}`, value]),
  ),
);

/** Values the base token set has no name for. */
const extraTokens: Record<string, string> = {
  "--border-width": "2px",
  "--offset-control-pressed": "2px",
  // A disabled menu key keeps a visible outline, softened from the active rule.
  "--color-border-disabled": "rgba(22, 24, 29, 0.4)",
  // The rule between rows of one readout. Lighter than the panel outline,
  // because it separates lines of the same block rather than two panels.
  "--color-border-soft": "rgba(22, 24, 29, 0.18)",
};

const factionTokenVariants = Object.fromEntries(
  Object.entries(factionPalette).map(([code, colors]) => [
    `color:faction-${code}`,
    {
      backgroundColor: colors.soft,
      borderColor: colors.accent,
      borderStyle: "solid",
      borderWidth: "2px",
      color: colors.text,
    },
  ]),
);

export const awbrnTheme = defineTheme({
  name: "awbrn",
  extends: neutralTheme,

  typography: {
    // 16px base so body copy reads at a comfortable size; a wide 1.333 ratio
    // gives the display voice room to behave like box art.
    scale: { base: 16, ratio: 1.333 },
    body: {
      family: "Nunito",
      fallbacks: "system-ui, sans-serif",
      weight: "normal",
    },
    heading: {
      // Bungee is signage lettering: chunky, layered, built to be shouted.
      family: "Bungee",
      fallbacks: "system-ui, sans-serif",
      weight: "normal",
    },
    code: {
      // Silkscreen is the HUD bitmap: funds, day counts, unit stats, labels.
      family: "Silkscreen",
      fallbacks: "ui-monospace, monospace",
    },
  },

  // Menu snap, not web easing. Short durations, hard exponential settle.
  motion: { fast: 90, medium: 150, slow: 520, ratio: 0.7, easing: "cubic-bezier(0.16, 1, 0.3, 1)" },

  tokens: {
    // -- Ground and chrome -------------------------------------------------
    // The body is the map: open sky. Content rides above it in menu panels.
    "--color-background-body": "#6bb4db",
    "--color-background-surface": "#fff8e4",
    "--color-background-card": "#fffcf2",
    "--color-background-popover": "#fffcf2",
    "--color-background-muted": "#efdcae",
    "--color-background-inverted": INK,

    // -- Command orange ----------------------------------------------------
    // Black on orange, the relationship the logo itself uses.
    "--color-accent": "#f07c1e",
    "--color-accent-muted": "#fbd9af",
    "--color-on-accent": INK,

    "--color-neutral": "rgba(22, 24, 29, 0.1)",
    "--color-overlay": "rgba(22, 24, 29, 0.55)",
    "--color-overlay-hover": "rgba(22, 24, 29, 0.08)",
    "--color-overlay-pressed": "rgba(22, 24, 29, 0.18)",
    "--color-tint-hover": "black",

    // -- Text: warm browns pulled from the cream, never neutral gray -------
    // Secondary is darker than a cream-only palette would need, because it
    // also has to clear 4.5:1 against the open sky it sits on.
    "--color-text-primary": INK,
    "--color-text-secondary": "#443c2e",
    "--color-text-disabled": "#6d6350",
    "--color-text-accent": "#a34803",
    "--color-icon-primary": INK,
    "--color-icon-secondary": "#443c2e",
    "--color-icon-disabled": "#6d6350",
    "--color-icon-accent": "#c25a08",
    "--color-on-dark": "#fff8e4",
    "--color-on-light": INK,

    // -- Rules -------------------------------------------------------------
    "--color-border": "rgba(22, 24, 29, 0.85)",
    "--color-border-emphasized": INK,
    "--color-shadow": CAST,
    "--color-skeleton": "#e2d2a8",
    "--color-track": "#d8c79c",

    // -- Status, in the game's own register: repair, damage, low supply ----
    "--color-success": "#1e8a3c",
    "--color-success-muted": "#c8eccc",
    "--color-on-success": "#fff8e4",
    "--color-error": "#c9221a",
    "--color-error-muted": "#fbd2cb",
    "--color-on-error": "#fff8e4",
    "--color-background-error-inverted": "#8d1410",
    "--color-warning": "#b98100",
    "--color-warning-muted": "#fbe9a8",
    "--color-on-warning": INK,

    // -- Named color families, retuned to the armies -----------------------
    "--color-background-red": "#ff4f4e2b",
    "--color-border-red": "#e3392f",
    "--color-icon-red": "#c51f28",
    "--color-text-red": "#7a1018",
    "--color-background-blue": "#466efe26",
    "--color-border-blue": "#466efe",
    "--color-icon-blue": "#2449d2",
    "--color-text-blue": "#18308c",
    "--color-background-green": "#3dc22d2b",
    "--color-border-green": "#2fa522",
    "--color-icon-green": "#25861a",
    "--color-text-green": "#185710",
    "--color-background-yellow": "#f0d20440",
    "--color-border-yellow": "#b99f00",
    "--color-icon-yellow": "#8f7c00",
    "--color-text-yellow": "#504800",
    "--color-background-orange": "#fca3392b",
    "--color-border-orange": "#d4770b",
    "--color-icon-orange": "#a85b00",
    "--color-text-orange": "#6b3403",
    "--color-background-purple": "#a447d32b",
    "--color-border-purple": "#8b34b8",
    "--color-icon-purple": "#6f1a9b",
    "--color-text-purple": "#3d294c",
    "--color-background-teal": "#6cd9d038",
    "--color-border-teal": "#16a79c",
    "--color-icon-teal": "#0e7f77",
    "--color-text-teal": "#08403c",
    "--color-background-cyan": "#84dfe838",
    "--color-border-cyan": "#1f9bc4",
    "--color-icon-cyan": "#0e7595",
    "--color-text-cyan": "#06414f",
    "--color-background-pink": "#ff99cc38",
    "--color-border-pink": "#e2559a",
    "--color-icon-pink": "#b92e73",
    "--color-text-pink": "#6b0f3e",
    "--color-background-gray": "#97979738",
    "--color-border-gray": "#6e6e6e",
    "--color-icon-gray": "#5a5a5a",
    "--color-text-gray": "#2e2626",

    // Small headings drop the signage face, so they need weight to stay a
    // step above body copy. Component declarations take precedence over these
    // tokens, so the weight lives here alone instead of being overridden later.
    "--text-heading-3-weight": "800",
    "--text-heading-4-weight": "800",

    // -- Chrome is square. Pixel art has no radius. ------------------------
    "--radius-inner": "0px",
    "--radius-element": "2px",
    "--radius-container": "4px",
    "--radius-page": "4px",
    "--radius-chat": "4px",

    // -- Depth: a hard pixel step, plus a soft blur that lands it on ground.
    "--shadow-low": `2px 2px 0 0 ${CAST}, 2px 3px 6px 0 ${CAST_SOFT}`,
    "--shadow-med": `3px 3px 0 0 ${CAST}, 4px 6px 10px 0 ${CAST_SOFT}`,
    "--shadow-high": `5px 5px 0 0 ${CAST}, 7px 10px 18px 0 ${CAST_SOFT}`,
    "--shadow-inset-hover": `inset 0 0 0 2px ${INK}`,
    "--shadow-inset-selected": "inset 0 0 0 2px #f07c1e",
    "--shadow-inset-success": "inset 0 0 0 2px rgba(30, 138, 60, 0.5)",
    "--shadow-inset-warning": "inset 0 0 0 2px rgba(185, 129, 0, 0.5)",
    "--shadow-inset-error": "inset 0 0 0 2px rgba(201, 34, 26, 0.5)",

    ...extraTokens,

    // -- The armies, addressable by code -----------------------------------
    ...factionTokens,
  },

  components: {
    // -- The ground plane --------------------------------------------------
    // Sky with a fixed tile grid and a sun wash at the horizon. The grid stays
    // put while content scrolls, so panels read as moving over a map.
    "app-shell": {
      base: {
        backgroundColor: "var(--color-background-body)",
        backgroundImage: [
          "radial-gradient(120% 65% at 50% -12%, rgba(255, 255, 255, 0.38), rgba(255, 255, 255, 0) 62%)",
          "linear-gradient(to right, rgba(255, 255, 255, 0.11) 2px, rgba(255, 255, 255, 0) 2px)",
          "linear-gradient(to bottom, rgba(255, 255, 255, 0.11) 2px, rgba(255, 255, 255, 0) 2px)",
        ].join(", "),
        backgroundSize: "100% 100%, 32px 32px, 32px 32px",
        backgroundRepeat: "no-repeat, repeat, repeat",
        backgroundAttachment: "scroll, fixed, fixed",
      },
    },

    // The shell's own regions paint a flat fill over the ground. Clear them so
    // the terrain and its grid stay continuous from the nav to the last panel.
    "app-shell-header": {
      base: { backgroundColor: "transparent" },
    },
    "layout-content": {
      base: { backgroundColor: "transparent" },
    },

    // -- Top nav: the status bar across the top of the screen --------------
    "top-nav": {
      base: {
        backgroundColor: "var(--color-background-surface)",
        borderBottom: `3px solid ${INK}`,
        boxShadow: "var(--shadow-med)",
      },
    },
    "top-nav-heading": {
      base: {
        fontFamily: "var(--font-family-heading)",
        fontSize: "var(--font-size-lg)",
        letterSpacing: "0.01em",
      },
    },
    "top-nav-item": {
      base: {
        ...hudLabel,
        border: "2px solid transparent",
        borderRadius: "var(--radius-element)",
        color: "var(--color-text-secondary)",
      },
      // The selected tab is the cursor: orange fill, black outline, no shadow,
      // because the cursor sits flush on the chrome rather than above it.
      selected: {
        backgroundColor: "var(--color-accent)",
        border: `2px solid ${INK}`,
        color: "var(--color-on-accent)",
      },
    },

    // -- Buttons: menu commands -------------------------------------------
    button: {
      base: {
        ...hudLabel,
        border: `2px solid ${INK}`,
        borderRadius: "var(--radius-element)",
        boxShadow: "var(--shadow-low)",
        "--button-focus-offset": "3px",
      },
      "variant:primary": {
        backgroundColor: "var(--color-accent)",
        color: "var(--color-on-accent)",
      },
      "variant:secondary": {
        backgroundColor: "var(--color-background-surface)",
        color: "var(--color-text-primary)",
      },
      "variant:destructive": {
        backgroundColor: "var(--color-error)",
        color: "var(--color-on-error)",
      },
      // Ghost is the one command that is not a raised menu key.
      "variant:ghost": {
        backgroundColor: "transparent",
        border: "2px solid transparent",
        boxShadow: "none",
        color: "var(--color-text-primary)",
      },
    },

    // -- Panels ------------------------------------------------------------
    card: {
      base: {
        ...panel,
        boxShadow: "var(--shadow-med)",
      },
      "variant:muted": {
        backgroundColor: "var(--color-background-muted)",
      },
      "variant:transparent": {
        border: "none",
        boxShadow: "none",
      },
      ...factionCardVariants,
    },

    // A map on the board is a key on the menu, and the chosen one wears the
    // cursor: the accent outline with the accent ring inside it, sitting flush
    // on the board instead of above it.
    "selectable-card": {
      base: {
        ...panel,
        backgroundColor: "var(--color-background-surface)",
        boxShadow: "var(--shadow-med)",
      },
      "selected:true": {
        borderColor: "var(--color-accent)",
        boxShadow: "var(--shadow-inset-selected)",
      },
    },
    section: {
      "variant:section": {
        ...panel,
        boxShadow: "var(--shadow-med)",
      },
      "variant:muted": {
        ...panel,
        backgroundColor: "var(--color-background-muted)",
        boxShadow: "var(--shadow-med)",
      },
    },
    banner: {
      base: {
        ...panel,
        boxShadow: "var(--shadow-low)",
      },
    },
    // A dialog is a menu the game opened over the map, so it takes the panel
    // outline and the highest cast shadow rather than a floating white card.
    dialog: {
      base: {
        ...panel,
        backgroundColor: "var(--color-background-surface)",
        boxShadow: "var(--shadow-high)",
      },
    },
    // The system is daylight only, so a tooltip is the game's own help window:
    // a cream menu panel cast over what it explains. The stock dark chip is the
    // one surface in the app that would have had no daylight in it, and text
    // colored for cream is unreadable on it.
    tooltip: {
      base: {
        ...panel,
        backgroundColor: "var(--color-background-popover)",
        color: "var(--color-text-primary)",
        boxShadow: "var(--shadow-high)",
      },
    },
    // An empty state always sits inside a panel already. It gets a recessed
    // well rather than a second outline; stacked chrome is the tell of a
    // system that stopped thinking about where its components live.
    "empty-state": {
      base: {
        backgroundColor: "var(--color-background-muted)",
        border: `2px dashed rgba(22, 24, 29, 0.3)`,
        borderRadius: "var(--radius-element)",
        boxShadow: "none",
      },
    },

    // -- Type: box-art headings, HUD labels, briefing body -----------------
    heading: {
      "level:1": {
        letterSpacing: "0.005em",
        lineHeight: "1.15",
      },
      "level:2": {
        letterSpacing: "0.01em",
        lineHeight: "1.2",
      },
      // Below level 2 the signage face turns into noise; the body face carries
      // small headings at full weight instead.
      "level:3": { fontFamily: "var(--font-family-body)" },
      "level:4": { fontFamily: "var(--font-family-body)" },
      "level:5": { ...hudLabel, color: "var(--color-text-secondary)" },
      "level:6": { ...hudLabel, color: "var(--color-text-secondary)" },
    },
    text: {
      // Supporting text stays in the body voice and preserves authored casing.
      "type:supporting": {
        fontFamily: "var(--font-family-body)",
        letterSpacing: "0.05em",
        textTransform: "none",
      },
      "type:label": {
        ...hudLabel,
        color: "var(--color-text-secondary)",
      },
    },
    code: {
      base: {
        backgroundColor: "var(--color-background-muted)",
        border: `2px solid ${INK}`,
        borderRadius: "var(--radius-inner)",
        fontFamily: "var(--font-family-code)",
        fontSize: "var(--font-size-sm)",
      },
    },

    // -- HUD readouts: the stat block on a CO intel screen -----------------
    "metadata-list": {
      base: {
        borderTop: `2px solid ${INK}`,
      },
    },
    "metadata-list-item": {
      base: {
        borderBottom: `2px solid var(--color-border-soft)`,
      },
    },

    // -- Rows: the unit roster --------------------------------------------
    list: {
      base: {
        backgroundColor: "var(--color-background-surface)",
        border: `2px solid ${INK}`,
        borderRadius: "var(--radius-container)",
        boxShadow: "var(--shadow-med)",
        overflow: "hidden",
      },
    },
    "list-item": {
      base: {
        borderBottom: `2px solid var(--color-border-soft)`,
      },
    },

    // A chosen crest wears the cursor, the same way a selected tab does: orange
    // fill, flush on the surface rather than raised above it.
    "toggle-button": {
      "isPressed:true": {
        backgroundColor: "var(--color-accent)",
        borderColor: INK,
        boxShadow: "none",
      },
    },

    // -- Chips and markers -------------------------------------------------
    badge: {
      base: {
        ...hudLabel,
        border: `2px solid ${INK}`,
        borderRadius: "var(--radius-inner)",
        fontSize: "var(--font-size-sm)",
      },
    },
    token: {
      base: {
        border: `2px solid ${INK}`,
        borderRadius: "var(--radius-inner)",
        fontFamily: "var(--font-family-code)",
        // Bitmap glyphs lose their shape below this; a faction letter that
        // cannot be read is a faction encoded by color alone.
        fontSize: "var(--font-size-base)",
        fontWeight: "700",
        letterSpacing: "0.02em",
      },
      ...factionTokenVariants,
    },
    statusdot: {
      base: {
        border: `2px solid ${INK}`,
      },
    },

    // -- Portraits and art: never round off a sprite -----------------------
    avatar: {
      base: {
        border: `2px solid ${INK}`,
        borderRadius: "var(--radius-element)",
        imageRendering: "pixelated",
      },
    },
    thumbnail: {
      base: {
        border: `2px solid ${INK}`,
        borderRadius: "var(--radius-element)",
        imageRendering: "pixelated",
      },
    },

    // -- Fields: recessed, the way an input box sinks into a menu ----------
    "text-input": {
      base: {
        backgroundColor: "#fffdf7",
        border: `2px solid ${INK}`,
        borderRadius: "var(--radius-element)",
        boxShadow: `inset 2px 2px 0 0 rgba(22, 24, 29, 0.12)`,
      },
    },
    "number-input": {
      base: {
        backgroundColor: "#fffdf7",
        border: `2px solid ${INK}`,
        borderRadius: "var(--radius-element)",
        boxShadow: `inset 2px 2px 0 0 rgba(22, 24, 29, 0.12)`,
      },
    },
    selector: {
      base: {
        backgroundColor: "#fffdf7",
        border: `2px solid ${INK}`,
        borderRadius: "var(--radius-element)",
        boxShadow: `inset 2px 2px 0 0 rgba(22, 24, 29, 0.12)`,
      },
    },
    "file-input": {
      base: {
        backgroundColor: "#fffdf7",
        border: `2px dashed ${INK}`,
        borderRadius: "var(--radius-element)",
      },
    },
    checkbox: {
      base: {
        border: `2px solid ${INK}`,
        borderRadius: "var(--radius-inner)",
      },
    },
  },
});
