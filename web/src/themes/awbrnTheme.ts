import { defineTheme } from "@astryxdesign/core/theme";
import { neutralTheme } from "@astryxdesign/theme-neutral";

const factionPalette = {
  os: {
    accent: "#ff4f4e",
    soft: "rgba(255, 79, 78, 0.22)",
    text: "#923243",
    wash: "rgba(255, 79, 78, 0.18)",
  },
  bm: {
    accent: "#94a2fd",
    soft: "rgba(112, 140, 254, 0.24)",
    text: "#466efe",
    wash: "rgba(112, 140, 254, 0.2)",
  },
  ge: {
    accent: "#87e287",
    soft: "rgba(97, 208, 90, 0.22)",
    text: "#3dc22d",
    wash: "rgba(97, 208, 90, 0.18)",
  },
  yc: {
    accent: "#f0d204",
    soft: "rgba(240, 210, 4, 0.24)",
    text: "#9f8f00",
    wash: "rgba(240, 210, 4, 0.2)",
  },
  bh: {
    accent: "#bbb4a5",
    soft: "rgba(116, 89, 138, 0.24)",
    text: "#74598a",
    wash: "rgba(116, 89, 138, 0.18)",
  },
  rf: {
    accent: "#c27184",
    soft: "rgba(194, 113, 132, 0.24)",
    text: "#b52744",
    wash: "rgba(194, 113, 132, 0.2)",
  },
  gs: {
    accent: "#979797",
    soft: "rgba(151, 151, 151, 0.22)",
    text: "#727272",
    wash: "rgba(151, 151, 151, 0.18)",
  },
  bd: {
    accent: "#ad7e5f",
    soft: "rgba(173, 126, 95, 0.24)",
    text: "#985333",
    wash: "rgba(173, 126, 95, 0.2)",
  },
  ab: {
    accent: "#fec078",
    soft: "rgba(254, 192, 120, 0.26)",
    text: "#fca339",
    wash: "rgba(254, 192, 120, 0.22)",
  },
  js: {
    accent: "#c4d7b4",
    soft: "rgba(196, 215, 180, 0.24)",
    text: "#6f7b67",
    wash: "rgba(196, 215, 180, 0.2)",
  },
  ci: {
    accent: "#2342ba",
    soft: "rgba(35, 66, 186, 0.24)",
    text: "#0b2070",
    wash: "rgba(35, 66, 186, 0.2)",
  },
  pc: {
    accent: "#ff99cc",
    soft: "rgba(255, 153, 204, 0.22)",
    text: "#ff66cc",
    wash: "rgba(255, 153, 204, 0.18)",
  },
  tg: {
    accent: "#6cd9d0",
    soft: "rgba(108, 217, 208, 0.24)",
    text: "#3ccdc1",
    wash: "rgba(108, 217, 208, 0.2)",
  },
  pl: {
    accent: "#a447d3",
    soft: "rgba(164, 71, 211, 0.24)",
    text: "#6f1a9b",
    wash: "rgba(164, 71, 211, 0.2)",
  },
  ar: {
    accent: "#7a9d11",
    soft: "rgba(122, 157, 17, 0.26)",
    text: "#617c0e",
    wash: "rgba(122, 157, 17, 0.22)",
  },
  wn: {
    accent: "#d4bf9f",
    soft: "rgba(212, 191, 159, 0.24)",
    text: "#cd9b9a",
    wash: "rgba(212, 191, 159, 0.2)",
  },
  aa: {
    accent: "#84dfe8",
    soft: "rgba(132, 223, 232, 0.24)",
    text: "#3a9ee6",
    wash: "rgba(132, 223, 232, 0.2)",
  },
  ne: {
    accent: "#6e6060",
    soft: "rgba(110, 96, 96, 0.24)",
    text: "#2e2626",
    wash: "rgba(110, 96, 96, 0.2)",
  },
  sc: {
    accent: "#8cacbc",
    soft: "rgba(140, 172, 188, 0.26)",
    text: "#3d6479",
    wash: "rgba(140, 172, 188, 0.22)",
  },
  uw: {
    accent: "#d47700",
    soft: "rgba(212, 119, 0, 0.24)",
    text: "#854000",
    wash: "rgba(212, 119, 0, 0.2)",
  },
} as const;

const factionCardVariants = Object.fromEntries(
  Object.entries(factionPalette).map(([code, colors]) => [
    `variant:faction-${code}`,
    { backgroundColor: colors.wash, borderColor: colors.accent },
  ]),
);

const factionTokenVariants = Object.fromEntries(
  Object.entries(factionPalette).map(([code, colors]) => [
    `color:faction-${code}`,
    {
      backgroundColor: colors.soft,
      borderColor: colors.accent,
      color: `light-dark(${colors.text}, var(--color-text-primary))`,
    },
  ]),
);

export const awbrnTheme = defineTheme({
  name: "awbrn",
  extends: neutralTheme,
  typography: {
    scale: { base: 14, ratio: 1.2 },
    body: {
      family: "Nunito",
      fallbacks: "system-ui, sans-serif",
      weight: "normal",
    },
    heading: {
      family: "Press Start 2P",
      fallbacks: "ui-monospace, monospace",
      weight: "normal",
    },
    code: {
      family: "Press Start 2P",
      fallbacks: "ui-monospace, monospace",
    },
  },
  motion: { fast: 120, medium: 180, slow: 700, ratio: 0.75 },
  tokens: {
    "--color-background-body": ["#4a3418", "#120d08"],
    "--color-background-surface": ["#fff5dc", "#2a2118"],
    "--color-background-card": ["#f8edc9", "#33271b"],
    "--color-background-popover": ["#fff6dd", "#3b2c1c"],
    "--color-background-muted": ["#ddc892", "#4a3824"],
    "--color-background-inverted": ["#120d08", "#fff5dc"],
    "--color-accent": ["#e76426", "#ff8a4c"],
    "--color-accent-muted": ["#ffd09d", "#6b321b"],
    "--color-on-accent": ["#fff5de", "#120d08"],
    "--color-neutral": ["#5a3f1e1a", "#fff1c526"],
    "--color-overlay": ["#120d0866", "#000000b3"],
    "--color-overlay-hover": ["#5a3f1e14", "#fff1c514"],
    "--color-overlay-pressed": ["#5a3f1e29", "#fff1c529"],
    "--color-text-primary": ["#1d2532", "#fff5de"],
    "--color-text-secondary": ["#49505d", "#d2b789"],
    "--color-text-disabled": ["#666b74", "#8e744c"],
    "--color-text-accent": ["#c4480f", "#ff9f6d"],
    "--color-icon-primary": ["#1d2532", "#fff5de"],
    "--color-icon-secondary": ["#49505d", "#d2b789"],
    "--color-icon-disabled": ["#666b74", "#8e744c"],
    "--color-icon-accent": ["#e76426", "#ff8a4c"],
    "--color-on-dark": "#fff5de",
    "--color-on-light": "#120d08",
    "--color-border": ["#5a3f1e", "#8e744c"],
    "--color-border-emphasized": ["#120d08", "#d2b789"],
    "--color-shadow": "#000000",
    "--color-success": ["#1a9e3f", "#70d489"],
    "--color-success-muted": ["#d7efc8", "#173d22"],
    "--color-error": ["#d42b1e", "#ff7c70"],
    "--color-error-muted": ["#ffd0c9", "#4d211d"],
    "--color-warning": ["#9f7900", "#f3d51b"],
    "--color-warning-muted": ["#fff1a8", "#493f13"],
    "--color-on-success": ["#fff5de", "#120d08"],
    "--color-on-error": "#fff5de",
    "--color-on-warning": "#120d08",
    "--color-background-red": ["#ff4f4e33", "#ff77703d"],
    "--color-border-red": ["#ff4f4e", "#ff7770"],
    "--color-icon-red": ["#c51f28", "#ff9d98"],
    "--color-text-red": ["#7a1018", "#ffc8c5"],
    "--color-background-blue": ["#466efe33", "#6f8aff3d"],
    "--color-border-blue": ["#466efe", "#7893ff"],
    "--color-icon-blue": ["#2449d2", "#9eb0ff"],
    "--color-text-blue": ["#18308c", "#c8d1ff"],
    "--color-background-green": ["#3dc22d33", "#65d4563d"],
    "--color-border-green": ["#3dc22d", "#72db64"],
    "--color-icon-green": ["#25861a", "#9ce590"],
    "--color-text-green": ["#185710", "#c9f3c1"],
    "--color-background-yellow": ["#9f8f0033", "#d6c63d3d"],
    "--color-border-yellow": ["#9f8f00", "#d6c63d"],
    "--color-icon-yellow": ["#746800", "#e7d968"],
    "--color-text-yellow": ["#504800", "#f6edaa"],
    "--color-background-purple": ["#74598a33", "#9d7db83d"],
    "--color-border-purple": ["#74598a", "#ac8bc7"],
    "--color-icon-purple": ["#5a3d70", "#c6a9dd"],
    "--color-text-purple": ["#3d294c", "#e3d0f0"],
    "--radius-inner": "3px",
    "--radius-element": "8px",
    "--radius-container": "12px",
    "--radius-page": "12px",
    "--shadow-low": "2px 2px 0 #000000",
    "--shadow-med": "4px 4px 0 #000000",
    "--shadow-high": "6px 6px 0 #000000",
  },
  components: {
    "app-shell": {
      base: { backgroundColor: "var(--color-background-body)" },
    },
    "top-nav": {
      base: {
        backgroundColor: "var(--color-background-surface)",
        borderBottom: "3px solid var(--color-border-emphasized)",
        boxShadow: "var(--shadow-high)",
      },
    },
    "top-nav-heading": {
      base: {
        fontFamily: "var(--font-family-heading)",
        letterSpacing: "0.04em",
      },
    },
    "top-nav-item": {
      base: {
        fontFamily: "var(--font-family-heading)",
        fontSize: "var(--font-size-xs)",
        letterSpacing: "0.04em",
        textTransform: "uppercase",
      },
      selected: {
        backgroundColor: "var(--color-background-muted)",
        color: "var(--color-text-primary)",
      },
    },
    button: {
      base: {
        border: "2px solid var(--color-border-emphasized)",
        boxShadow: "var(--shadow-low)",
        fontFamily: "var(--font-family-heading)",
        fontSize: "var(--font-size-xs)",
        letterSpacing: "0.04em",
        textTransform: "uppercase",
      },
      "variant:primary": {
        backgroundColor: "var(--color-accent)",
        color: "var(--color-on-accent)",
      },
      "variant:secondary": {
        backgroundColor: "var(--color-background-surface)",
        color: "var(--color-text-primary)",
      },
    },
    card: {
      ...factionCardVariants,
    },
    token: {
      ...factionTokenVariants,
    },
  },
});
