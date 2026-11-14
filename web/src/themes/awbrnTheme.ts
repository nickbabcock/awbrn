import { defineTheme } from "@astryxdesign/core/theme";
import { neutralTheme } from "@astryxdesign/theme-neutral";

/*
 * The theme is light only; the app pins <Theme mode="light">. The ink table of
 * the match screens is a surface inside the light theme, not a dark mode.
 *
 * Red fills at most one control per viewport, so the accent token is ink
 * (switches, checks, the chosen tab) and only the primary button wears red.
 */

const ink = {
  950: "#0a1128",
  900: "#101b3a",
  850: "#15234a",
  800: "#1b2c5a",
  700: "#283d78",
  600: "#3b5299",
  300: "#9eacd6",
  100: "#e4e9f8",
} as const;

const paper = {
  ground: "#e6edf7",
  well: "#d9e3f1",
  sheet: "#fbfcff",
  rule: "#c7d2e5",
  ruleStrong: "#8f9dbf",
} as const;

const text = {
  primary: "#101b3a",
  secondary: "#43507a",
  tertiary: "#5d6a92",
  onInk: "#f3f5fc",
  onInkSecondary: "#a9b6de",
  onInkStrong: "#ffffff",
} as const;

const airmail = {
  red: "#d42a3c",
  redWash: "#fbe3e6",
  blue: "#2747b0",
  blueOnInk: "#9db2ff",
  violet: "#5a43b5",
  violetOnInk: "#b9abff",
} as const;

const status = {
  ok: "#17784b",
  okOnInk: "#63d59f",
  okWash: "#d8f1e4",
  warn: "#9a6400",
  warnFill: "#f4b73a",
  warnWash: "#fdecc8",
  danger: "#b3192e",
  dangerWash: "#fdecee",
  gold: "#f6c544",
  goldDim: "rgba(246, 197, 68, 0.22)",
  goldOnPaper: "#e2a400",
  goldTrackOnPaper: "#d6deec",
} as const;

const FONT_UI = "Archivo Variable";
const FONT_UI_FALLBACKS = "Archivo, ui-sans-serif, system-ui, sans-serif";
const FONT_FIGURE = "Martian Mono Variable";
const FONT_FIGURE_FALLBACKS = "'Martian Mono', ui-monospace, monospace";

/**
 * Hover and selection step in two visible frames: halfway at once, then there.
 * `jump-none` would hold the old state for the first frame and only delay a
 * cut.
 */
const STEP = "steps(2, jump-start)";

const SCRIM = {
  dialog: "rgba(6, 10, 26, 0.35)",
  sheet: "rgba(6, 10, 26, 0.45)",
} as const;

const SHADOW_SHEET = "0 1px 2px rgba(16, 27, 58, 0.06), 0 6px 16px -10px rgba(16, 27, 58, 0.28)";
const SHADOW_OWED = "0 1px 2px rgba(16, 27, 58, 0.08), 0 14px 32px -14px rgba(16, 27, 58, 0.42)";
const SHADOW_FLOAT = "0 2px 6px rgba(6, 10, 26, 0.22), 0 22px 48px -16px rgba(6, 10, 26, 0.55)";
const SHADOW_FLOAT_UP = "0 -12px 40px -12px rgba(6, 10, 26, 0.6)";
const SHADOW_BOARD = `0 0 0 2px ${ink[700]}, 0 24px 60px -20px rgba(0, 0, 0, 0.8)`;
/**
 * A filter, not a box shadow: a map picture's mountains and towers rise above
 * the first tile row, so the shadow follows the sprites rather than the box.
 */
const SHADOW_BOARD_PICTURE = "drop-shadow(0 24px 40px rgba(0, 0, 0, 0.65))";
const HATCH_WELL =
  "repeating-linear-gradient(-45deg, transparent 0 6px, rgba(16, 27, 58, 0.07) 6px 12px)";

const STAMP_WHITE = "#ffffff";
const INK_HAIRLINE = "rgba(16, 27, 58, 0.14)";
const WELL_HAIRLINE = "rgba(255, 255, 255, 0.06)";
const SHADOW_PENNANT = "drop-shadow(0 2px 2px rgba(6, 10, 26, 0.45))";
const SHADOW_SLOT = "0 10px 24px -16px rgba(16, 27, 58, 0.45)";
const SHADOW_DOCK = "0 -12px 24px -16px rgba(16, 27, 58, 0.5)";

const AIRMAIL_BAND = `repeating-linear-gradient(-45deg, ${airmail.red} 0 10px, ${STAMP_WHITE} 10px 15px, ${airmail.blue} 15px 25px, ${STAMP_WHITE} 25px 30px)`;

const capsLabel = {
  fontFamily: "var(--font-family-body)",
  fontSize: "var(--text-12-size)",
  fontWeight: "650",
  fontStretch: "85%",
  letterSpacing: "0.06em",
  wordSpacing: "0.1em",
  lineHeight: "1.2",
  textTransform: "uppercase",
  color: "var(--color-text-secondary)",
} as const;

const field = {
  backgroundColor: "#ffffff",
  borderColor: paper.ruleStrong,
  borderStyle: "solid",
  borderWidth: "1px",
  borderRadius: "var(--radius-element)",
  boxShadow: "none",
} as const;

const sheet = {
  backgroundColor: paper.sheet,
  borderColor: paper.rule,
  borderStyle: "solid",
  borderWidth: "1px",
  borderRadius: "var(--radius-container)",
  boxShadow: SHADOW_SHEET,
} as const;

const stepped = {
  transitionTimingFunction: STEP,
  transitionDuration: "90ms",
} as const;

/**
 * Army colors, from AWBW's own faction palette. `wash` tints a whole card,
 * plate or ledger row, `soft` fills a chip, `field` is the stamp's ground
 * behind the commander, and `text` stays legible on `soft`.
 */
const factionPalette = {
  os: {
    accent: "#ff4f4e",
    soft: "#ff4f4e33",
    text: "#8a1420",
    wash: "#ff4f4e26",
    field: "#ffb3ad",
  },
  bm: {
    accent: "#708cfe",
    soft: "#708cfe38",
    text: "#1c3aa8",
    wash: "#708cfe2b",
    field: "#b9c7ff",
  },
  ge: {
    accent: "#61d05a",
    soft: "#61d05a3d",
    text: "#1d6b16",
    wash: "#61d05a2b",
    field: "#b4ecaf",
  },
  yc: {
    accent: "#f0d204",
    soft: "#f0d20447",
    text: "#6b5f00",
    wash: "#f0d2042e",
    field: "#f8eb8e",
  },
  bh: {
    accent: "#74598a",
    soft: "#74598a33",
    text: "#4a3160",
    wash: "#74598a26",
    field: "#c6b4d4",
  },
  rf: {
    accent: "#c27184",
    soft: "#c2718438",
    text: "#8d1c33",
    wash: "#c271842b",
    field: "#e6bcc6",
  },
  gs: {
    accent: "#979797",
    soft: "#97979738",
    text: "#4d4d4d",
    wash: "#9797972b",
    field: "#d3d3d3",
  },
  bd: {
    accent: "#ad7e5f",
    soft: "#ad7e5f38",
    text: "#6d3a18",
    wash: "#ad7e5f2b",
    field: "#dcc4b3",
  },
  ab: {
    accent: "#fec078",
    soft: "#fca33947",
    text: "#7a4405",
    wash: "#fca3392e",
    field: "#ffdcb1",
  },
  js: {
    accent: "#c4d7b4",
    soft: "#a9c48f47",
    text: "#4a5c38",
    wash: "#a9c48f2e",
    field: "#dce9d1",
  },
  ci: {
    accent: "#2342ba",
    soft: "#2342ba33",
    text: "#152a7d",
    wash: "#2342ba26",
    field: "#a9b6e6",
  },
  pc: {
    accent: "#ff99cc",
    soft: "#ff99cc47",
    text: "#98134f",
    wash: "#ff99cc2e",
    field: "#ffc2df",
  },
  tg: {
    accent: "#6cd9d0",
    soft: "#6cd9d047",
    text: "#106b64",
    wash: "#6cd9d02e",
    field: "#a8ece6",
  },
  pl: {
    accent: "#a447d3",
    soft: "#a447d333",
    text: "#5f1487",
    wash: "#a447d326",
    field: "#d8b2ec",
  },
  ar: {
    accent: "#7a9d11",
    soft: "#7a9d1140",
    text: "#42550a",
    wash: "#7a9d1129",
    field: "#c6d79a",
  },
  wn: {
    accent: "#d4bf9f",
    soft: "#d4bf9f4d",
    text: "#6b5334",
    wash: "#d4bf9f33",
    field: "#ebdfcd",
  },
  aa: {
    accent: "#84dfe8",
    soft: "#84dfe84d",
    text: "#0d5f74",
    wash: "#84dfe833",
    field: "#c0eff4",
  },
  ne: {
    accent: "#6e6060",
    soft: "#6e606038",
    text: "#3a3030",
    wash: "#6e60602b",
    field: "#c3bbbb",
  },
  sc: {
    accent: "#8cacbc",
    soft: "#8cacbc4d",
    text: "#2f5468",
    wash: "#8cacbc33",
    field: "#c9d8e0",
  },
  uw: {
    accent: "#d47700",
    soft: "#d4770040",
    text: "#7a4400",
    wash: "#d4770029",
    field: "#efc48c",
  },
} as const;

const factionCardVariants = Object.fromEntries(
  Object.entries(factionPalette).map(([code, colors]) => [
    `variant:faction-${code}`,
    {
      backgroundColor: `color-mix(in srgb, ${colors.wash}, ${paper.sheet})`,
      borderColor: paper.rule,
    },
  ]),
);

/** The army colors as custom properties, addressable by code at runtime. */
const factionTokens = Object.fromEntries(
  Object.entries(factionPalette).flatMap(([code, colors]) =>
    Object.entries(colors).map(([role, value]) => [`--color-faction-${code}-${role}`, value]),
  ),
);

const factionTokenVariants = Object.fromEntries(
  Object.entries(factionPalette).map(([code, colors]) => [
    `color:faction-${code}`,
    {
      backgroundColor: colors.soft,
      borderColor: "transparent",
      color: colors.text,
    },
  ]),
);

/** Custom properties Astryx has no name for; StyleX reads them through `awbrnVars`. */
const airmailTokens: Record<string, string> = {
  "--color-ink-950": ink[950],
  "--color-ink-900": ink[900],
  "--color-ink-850": ink[850],
  "--color-ink-800": ink[800],
  "--color-ink-700": ink[700],
  "--color-ink-600": ink[600],
  "--color-ink-300": ink[300],
  "--color-ink-100": ink[100],
  "--color-paper": paper.ground,
  "--color-paper-well": paper.well,
  "--color-sheet": paper.sheet,
  "--color-rule": paper.rule,
  "--color-rule-strong": paper.ruleStrong,
  "--color-text-tertiary": text.tertiary,
  "--color-text-on-ink": text.onInk,
  "--color-text-on-ink-secondary": text.onInkSecondary,
  "--color-text-on-ink-strong": text.onInkStrong,
  "--color-airmail-red": airmail.red,
  "--color-airmail-red-wash": airmail.redWash,
  "--color-airmail-blue": airmail.blue,
  "--color-airmail-blue-on-ink": airmail.blueOnInk,
  "--color-stamp-violet": airmail.violet,
  "--color-stamp-violet-on-ink": airmail.violetOnInk,
  "--color-ok": status.ok,
  "--color-ok-on-ink": status.okOnInk,
  "--color-warn": status.warn,
  "--color-warn-fill": status.warnFill,
  "--color-danger": status.danger,
  "--color-power-gold": status.gold,
  "--color-power-gold-dim": status.goldDim,
  "--color-power-gold-on-paper": status.goldOnPaper,
  "--color-power-gold-track-on-paper": status.goldTrackOnPaper,
  "--airmail-band": AIRMAIL_BAND,
  "--shadow-sheet": SHADOW_SHEET,
  "--shadow-board": SHADOW_BOARD,
  "--shadow-board-picture": SHADOW_BOARD_PICTURE,
  "--hatch-well": HATCH_WELL,
  "--color-stamp-white": STAMP_WHITE,
  "--color-ink-hairline": INK_HAIRLINE,
  "--color-well-hairline": WELL_HAIRLINE,
  "--shadow-pennant": SHADOW_PENNANT,
  "--shadow-slot": SHADOW_SLOT,
  "--shadow-dock": SHADOW_DOCK,
  "--shadow-stamp":
    "drop-shadow(0 0 0.75px rgba(10, 17, 40, 0.55)) drop-shadow(0 1px 1.5px rgba(10, 17, 40, 0.3))",
  "--ease-step": STEP,
  // Astryx eases what leaves on its own curve; here it steps like the rest.
  "--ease-exit": STEP,
  // Menus, popovers and tooltips (Astryx layers) enter over the fast-max duration.
  "--duration-fast-max": "120ms",
  "--spacing-2-5": "0.625rem",
  "--spacing-3-5": "0.875rem",
  "--spacing-4-5": "1.125rem",
  "--spacing-5-5": "1.375rem",
  "--text-nav-size": "11.5px",
  "--text-micro-size": "11px",
  // Pixels, not Astryx's rem steps, so a screen that names one keeps its size exactly.
  "--text-11-5-size": "11.5px",
  "--text-12-size": "12px",
  "--text-12-5-size": "12.5px",
  "--text-13-size": "13px",
  "--text-13-5-size": "13.5px",
  "--text-14-size": "14px",
  "--text-14-5-size": "14.5px",
  "--text-15-size": "15px",
  "--text-15-5-size": "15.5px",
  "--text-16-size": "16px",
  "--text-17-size": "17px",
  "--text-26-size": "26px",
  "--size-current-mark": "3px",
};

/** Width comes from `font-stretch` on the heading component. */
const TYPE_TOKENS = {
  "--text-heading-1-size": "52px",
  "--text-heading-1-weight": "820",
  "--text-heading-1-leading": "0.96",
  "--text-heading-2-size": "26px",
  "--text-heading-2-weight": "800",
  "--text-heading-2-leading": "1.05",
  "--text-heading-3-size": "17px",
  "--text-heading-3-weight": "700",
  "--text-heading-3-leading": "1.25",
  "--text-heading-4-size": "15px",
  "--text-heading-4-weight": "700",
  "--text-heading-4-leading": "1.3",
  "--text-heading-5-size": "12px",
  "--text-heading-5-weight": "650",
  "--text-heading-5-leading": "1.2",
  "--text-heading-6-size": "12px",
  "--text-heading-6-weight": "650",
  "--text-heading-6-leading": "1.2",
  "--text-display-1-size": "52px",
  "--text-display-1-weight": "820",
  "--text-display-1-leading": "0.96",
  "--text-display-2-size": "38px",
  "--text-display-2-weight": "820",
  "--text-display-2-leading": "1",
  "--text-display-3-size": "26px",
  "--text-display-3-weight": "800",
  "--text-display-3-leading": "1.05",
  "--text-body-size": "15px",
  "--text-body-weight": "400",
  "--text-body-leading": "1.5",
  "--text-large-size": "17px",
  "--text-large-weight": "500",
  "--text-large-leading": "1.45",
  "--text-label-size": "14px",
  "--text-label-weight": "650",
  "--text-label-leading": "1.3",
  "--text-supporting-size": "13px",
  "--text-supporting-weight": "400",
  "--text-supporting-leading": "1.4",
  "--text-code-size": "12.5px",
  "--text-code-weight": "500",
  "--text-code-leading": "1.3",
} as const;

const PHONE_TYPE_TOKENS = {
  "--text-heading-1-size": "38px",
  "--text-heading-2-size": "22px",
  "--text-display-1-size": "38px",
  "--text-display-2-size": "30px",
  "--text-display-3-size": "22px",
} as const;

const ELEMENT_SIZES = {
  "--size-element-sm": "32px",
  "--size-element-md": "40px",
  "--size-element-lg": "48px",
} as const;
const COARSE_ELEMENT_SIZES = {
  "--size-element-sm": "44px",
  "--size-element-md": "44px",
  "--size-element-lg": "52px",
} as const;

const oneLineField = {
  ...field,
  "--size-element-md": COARSE_ELEMENT_SIZES["--size-element-md"],
} as const;

export const awbrnTheme = defineTheme({
  name: "awbrn",
  extends: neutralTheme,

  typography: {
    scale: { base: 15, ratio: 1.2 },
    body: {
      family: FONT_UI,
      fallbacks: FONT_UI_FALLBACKS,
      weight: "normal",
    },
    heading: {
      family: FONT_UI,
      fallbacks: FONT_UI_FALLBACKS,
      weight: "bold",
    },
    code: {
      family: FONT_FIGURE,
      fallbacks: FONT_FIGURE_FALLBACKS,
    },
  },

  // 720px and below is a phone; the rail drops below 1180px.
  adaptations: {
    widthBreakpoints: { sm: 640, md: 721, lg: 1024, xl: 1181, "2xl": 1536 },
    rules: [
      // The inherited touch rule rewrites the whole type scale for a touch
      // screen. Airmail keeps one scale, so this restores it on every coarse
      // pointer, and gives every target its 44px.
      {
        when: { pointer: "coarse" },
        value: { tokens: { ...TYPE_TOKENS, ...COARSE_ELEMENT_SIZES } },
      },
      {
        when: { width: { below: "md" } },
        value: { tokens: { ...PHONE_TYPE_TOKENS, "--color-overlay": SCRIM.sheet } },
      },
    ],
  },

  motion: {
    tempo: 0.75,
    fast: 90,
    medium: 120,
    slow: 240,
    ratio: 0.7,
    easing: STEP,
  },

  tokens: {
    // -- Ground and surfaces ----------------------------------------------
    "--color-background-body": paper.ground,
    "--color-background-surface": paper.sheet,
    "--color-background-card": paper.sheet,
    "--color-background-popover": paper.sheet,
    "--color-background-muted": paper.well,
    "--color-background-inverted": ink[900],

    // -- Accent -------------------------------------------------------------
    "--color-accent": ink[900],
    "--color-accent-muted": ink[100],
    "--color-on-accent": "#ffffff",
    "--focus-outline-color": airmail.blue,
    "--focus-outline-offset": "2px",

    "--color-neutral": "rgba(16, 27, 58, 0.08)",
    "--color-overlay": SCRIM.dialog,
    "--color-overlay-hover": "rgba(16, 27, 58, 0.06)",
    "--color-overlay-pressed": "rgba(16, 27, 58, 0.12)",
    "--color-tint-hover": ink[900],

    // -- Text ---------------------------------------------------------------
    "--color-text-primary": text.primary,
    "--color-text-secondary": text.secondary,
    "--color-text-disabled": text.tertiary,
    "--color-text-accent": airmail.blue,
    "--color-icon-primary": text.primary,
    "--color-icon-secondary": text.secondary,
    "--color-icon-disabled": paper.ruleStrong,
    "--color-icon-accent": airmail.blue,
    "--color-on-dark": text.onInk,
    "--color-on-light": text.primary,

    // -- Rules ----------------------------------------------------------------
    "--color-border": paper.rule,
    "--color-border-emphasized": paper.ruleStrong,
    "--color-shadow": "rgba(16, 27, 58, 0.28)",
    "--color-skeleton": paper.well,
    "--color-track": paper.rule,

    // -- Status ---------------------------------------------------------------
    "--color-success": status.ok,
    "--color-success-muted": status.okWash,
    "--color-on-success": "#ffffff",
    "--color-error": status.danger,
    "--color-error-muted": status.dangerWash,
    "--color-on-error": "#ffffff",
    "--color-background-error-inverted": "#6d0f1c",
    "--color-warning": status.warn,
    "--color-warning-muted": status.warnWash,
    "--color-on-warning": "#ffffff",
    "--color-info": airmail.blue,
    "--color-info-muted": ink[100],

    // -- Named color families, retuned to the armies ---------------------------
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
    "--color-background-gray": paper.well,
    "--color-border-gray": paper.ruleStrong,
    "--color-icon-gray": text.secondary,
    "--color-text-gray": text.secondary,

    ...TYPE_TOKENS,
    ...ELEMENT_SIZES,

    // -- Shapes -------------------------------------------------------------------
    "--border-width": "1px",
    "--radius-inner": "4px",
    "--radius-element": "6px",
    "--radius-container": "10px",
    "--radius-page": "10px",
    "--radius-chat": "10px",

    // -- Depth --------------------------------------------------------------------
    "--shadow-low": SHADOW_SHEET,
    "--shadow-med": SHADOW_OWED,
    "--shadow-high": SHADOW_FLOAT,
    "--shadow-inset-hover": `inset 0 0 0 1px ${ink[900]}`,
    "--shadow-inset-selected": `inset 0 0 0 1px ${ink[900]}`,
    "--shadow-inset-success": "inset 0 0 0 1px rgba(23, 120, 75, 0.5)",
    "--shadow-inset-warning": "inset 0 0 0 1px rgba(154, 100, 0, 0.5)",
    "--shadow-inset-error": "inset 0 0 0 1px rgba(179, 25, 46, 0.5)",

    ...airmailTokens,
    ...factionTokens,
  },

  components: {
    // -- The ground ---------------------------------------------------------
    "app-shell": {
      base: { backgroundColor: paper.ground },
    },
    // Astryx pins the header at z-index 1, under page content that stacks
    // itself (a map card's rank pennant sits at 2), so scrolled content painted
    // over the bar. It sits below the phone task dock and tab bar.
    "app-shell-header": {
      base: { backgroundColor: ink[900], zIndex: "10" },
    },
    "layout-content": {
      base: { backgroundColor: "transparent" },
    },

    // -- Buttons: keys --------------------------------------------------------
    button: {
      base: {
        ...stepped,
        borderStyle: "solid",
        borderWidth: "1px",
        borderRadius: "var(--radius-element)",
        boxShadow: "none",
        fontFamily: "var(--font-family-body)",
        fontWeight: "650",
        letterSpacing: "normal",
        textTransform: "none",
        "--button-focus-offset": "2px",
      },
      "size:sm": { fontSize: "var(--text-13-5-size)" },
      "size:md": { fontSize: "var(--text-14-5-size)" },
      "size:lg": { fontSize: "var(--text-16-size)" },
      "variant:primary": {
        backgroundColor: airmail.red,
        borderColor: airmail.red,
        color: "#ffffff",
        boxShadow: "0 1px 0 rgba(0, 0, 0, 0.15), 0 6px 14px -8px rgba(180, 20, 40, 0.7)",
      },
      "variant:secondary": {
        backgroundColor: paper.sheet,
        borderColor: paper.ruleStrong,
        color: text.primary,
      },
      "variant:ghost": {
        backgroundColor: "transparent",
        borderColor: "transparent",
        color: text.secondary,
      },
      "variant:destructive": {
        backgroundColor: paper.sheet,
        borderColor: `color-mix(in oklab, ${status.danger} 45%, white)`,
        color: status.danger,
      },
      "variant:ink": {
        backgroundColor: ink[900],
        borderColor: ink[900],
        color: "#ffffff",
      },
      "variant:on-ink": {
        backgroundColor: ink[850],
        borderColor: ink[700],
        color: text.onInk,
      },
      "variant:ghost-on-ink": {
        backgroundColor: "transparent",
        borderColor: "transparent",
        color: text.onInkSecondary,
      },
    },

    // -- Placeholders: nothing pulses ------------------------------------------
    skeleton: {
      base: {
        backgroundColor: paper.well,
        opacity: "1",
        animationName: "none",
      },
    },

    // -- Sheets ----------------------------------------------------------------
    card: {
      base: sheet,
      "variant:muted": {
        backgroundColor: paper.well,
        borderColor: "transparent",
        boxShadow: "none",
      },
      "variant:transparent": {
        backgroundColor: "transparent",
        borderColor: "transparent",
        boxShadow: "none",
      },
      "variant:well": {
        backgroundColor: paper.well,
        borderColor: "transparent",
        boxShadow: "none",
      },
      "variant:owed": {
        boxShadow: SHADOW_OWED,
      },
      "variant:ink": {
        backgroundColor: ink[900],
        borderColor: ink[800],
        boxShadow: "none",
        color: text.onInk,
      },
      ...factionCardVariants,
    },
    "selectable-card": {
      base: {
        ...stepped,
        backgroundColor: "#ffffff",
        borderColor: paper.ruleStrong,
        borderStyle: "solid",
        borderWidth: "1px",
        borderRadius: "var(--radius-element)",
        boxShadow: "none",
      },
      "selected:true": {
        backgroundColor: ink[100],
        borderColor: ink[900],
        boxShadow: `inset 0 0 0 1px ${ink[900]}`,
      },
    },
    "clickable-card": {
      base: { ...stepped, ...sheet },
    },
    section: {
      "variant:section": sheet,
      "variant:muted": {
        backgroundColor: paper.well,
        borderRadius: "var(--radius-container)",
        boxShadow: "none",
      },
    },
    banner: {
      base: {
        borderStyle: "solid",
        borderWidth: "1px",
        borderRadius: "var(--radius-container)",
        boxShadow: "none",
      },
      "status:error": {
        backgroundColor: status.dangerWash,
        borderColor: "#f2b6bf",
        color: "#6d0f1c",
      },
      "status:success": {
        backgroundColor: "#e3f4ea",
        borderColor: "#a9dcc0",
        color: "#0e4f31",
      },
    },
    dialog: {
      base: {
        backgroundColor: paper.sheet,
        borderStyle: "none",
        borderRadius: "var(--radius-container)",
        boxShadow: SHADOW_FLOAT,
      },
    },
    "bottom-sheet": {
      base: {
        backgroundColor: paper.sheet,
        borderTopLeftRadius: "16px",
        borderTopRightRadius: "16px",
        boxShadow: SHADOW_FLOAT_UP,
      },
    },
    popover: {
      base: {
        backgroundColor: paper.sheet,
        borderColor: "rgba(10, 17, 40, 0.25)",
        borderStyle: "solid",
        borderWidth: "1px",
        borderRadius: "var(--radius-container)",
        boxShadow: SHADOW_FLOAT,
      },
    },
    tooltip: {
      base: {
        backgroundColor: ink[900],
        color: text.onInk,
        borderRadius: "var(--radius-inner)",
        boxShadow: SHADOW_FLOAT,
      },
    },
    "empty-state": {
      base: {
        backgroundColor: paper.sheet,
        borderColor: paper.ruleStrong,
        borderStyle: "dashed",
        borderWidth: "1px",
        borderRadius: "var(--radius-container)",
        boxShadow: "none",
      },
    },
    "empty-state-title": {
      base: { fontSize: "18px", fontWeight: "700" },
    },

    // -- Type --------------------------------------------------------------------
    heading: {
      // The condensed cut, its word space opened back up so "Your move" never
      // reads "Yourmove".
      "level:1": {
        fontStretch: "62%",
        letterSpacing: "-0.01em",
        wordSpacing: "0.12em",
        textWrap: "balance",
      },
      "level:2": {
        fontStretch: "68%",
        letterSpacing: "-0.005em",
        wordSpacing: "0.1em",
        textWrap: "balance",
      },
      "level:3": { fontStretch: "100%" },
      "level:4": { fontStretch: "100%" },
      "level:5": capsLabel,
      "level:6": capsLabel,
    },
    text: {
      "color:danger": { color: status.danger },
      // 4.6:1 on white.
      "color:tertiary": { color: text.tertiary },
      "color:on-ink": { color: text.onInk },
      "color:on-ink-secondary": { color: text.onInkSecondary },
      "type:supporting": {
        color: "var(--color-text-secondary)",
        letterSpacing: "normal",
        textTransform: "none",
      },
      "type:label": {
        letterSpacing: "normal",
        textTransform: "none",
      },
      "type:code": {
        fontStretch: "87.5%",
        fontVariantNumeric: "tabular-nums",
      },
    },
    code: {
      base: {
        backgroundColor: paper.well,
        borderStyle: "none",
        borderRadius: "var(--radius-inner)",
        fontFamily: "var(--font-family-code)",
        fontStretch: "87.5%",
        fontSize: "var(--text-12-5-size)",
      },
    },
    link: {
      base: {
        textUnderlineOffset: "3px",
        textDecorationThickness: "1px",
      },
    },

    // -- Rows -------------------------------------------------------------------
    "metadata-list": {
      base: { borderTop: `1px solid ${paper.rule}` },
    },
    "metadata-list-item": {
      base: { borderBottom: `1px solid ${paper.rule}` },
    },
    list: {
      base: { ...sheet, overflow: "hidden" },
    },
    "list-item": {
      base: { borderBottom: `1px solid ${paper.rule}` },
    },
    "table-header-cell": {
      base: capsLabel,
    },

    "toggle-button": {
      base: { ...stepped },
      "isPressed:true": {
        backgroundColor: ink[900],
        borderColor: ink[900],
        color: "#ffffff",
        boxShadow: "none",
      },
    },

    // -- Chips and marks ------------------------------------------------------------
    badge: {
      base: {
        borderStyle: "none",
        borderRadius: "999px",
        fontFamily: "var(--font-family-body)",
        fontSize: "var(--text-nav-size)",
        fontWeight: "700",
        fontVariantNumeric: "tabular-nums",
        letterSpacing: "normal",
        textTransform: "none",
      },
    },
    // The fill sits on the neutral color so Astryx's named colors keep theirs.
    token: {
      base: {
        borderStyle: "none",
        borderRadius: "var(--radius-inner)",
        fontFamily: "var(--font-family-body)",
        fontSize: "var(--text-12-size)",
        fontWeight: "650",
        letterSpacing: "0.02em",
        wordSpacing: "0.14em",
      },
      // One size on every pointer: the coarse rule's 44px controls would
      // otherwise make a 36px tag.
      "size:sm": { height: "22px", paddingInline: "8px" },
      "color:default": {
        backgroundColor: paper.well,
        color: text.secondary,
      },
      ...factionTokenVariants,
    },
    "status-dot": {
      base: { borderStyle: "none" },
    },

    // -- Pixel art: never round or smooth a sprite -----------------------------------
    avatar: {
      base: {
        borderStyle: "none",
        imageRendering: "pixelated",
      },
    },
    thumbnail: {
      base: {
        backgroundColor: ink[900],
        borderStyle: "none",
        borderRadius: "var(--radius-element)",
        boxShadow: `inset 0 0 0 1px ${ink[700]}`,
        imageRendering: "pixelated",
      },
    },

    // -- Fields -----------------------------------------------------------------------
    "field-label": {
      base: {
        color: text.primary,
        fontWeight: "var(--text-label-weight)",
      },
    },
    "segmented-control": {
      base: {
        backgroundColor: paper.well,
        borderColor: paper.rule,
        borderStyle: "solid",
        borderWidth: "1px",
        borderRadius: "var(--radius-element)",
        boxShadow: "none",
        gap: "3px",
        padding: "3px",
      },
    },
    "segmented-control-item": {
      base: {
        ...stepped,
        borderStyle: "none",
        borderRadius: "var(--radius-inner)",
        boxShadow: "none",
        color: text.secondary,
        fontWeight: "600",
        letterSpacing: "normal",
        textTransform: "none",
      },
      selected: {
        backgroundColor: "#ffffff",
        color: text.primary,
        boxShadow: "0 1px 2px rgba(16, 27, 58, 0.18)",
      },
    },
    // A one-line field stands 44px on every pointer; buttons keep the 40px
    // control height.
    "text-input": { base: oneLineField },
    "number-input": { base: oneLineField },
    "text-area": { base: field },
    selector: { base: oneLineField },
    "file-input": {
      base: {
        ...field,
        borderStyle: "dashed",
      },
    },
    "checkbox-indicator": {
      base: {
        borderColor: paper.ruleStrong,
        borderRadius: "var(--radius-inner)",
      },
    },
    switch: {
      base: { ...stepped },
    },

    // -- Navigation ---------------------------------------------------------------------
    tab: {
      base: {
        ...stepped,
        color: text.secondary,
        fontWeight: "600",
        letterSpacing: "normal",
        textTransform: "none",
      },
      selected: { color: text.primary },
    },
    "tab-indicator": {
      base: {
        backgroundColor: ink[900],
        borderTopLeftRadius: "3px",
        borderTopRightRadius: "3px",
      },
    },
    "dropdown-menu-item": {
      base: { ...stepped, fontWeight: "600" },
    },
  },
});
