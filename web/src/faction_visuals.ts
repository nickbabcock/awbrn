import factionLogoSheetUrl from "../../assets/textures/logos.png?url";

const LOGO_CODES = [
  "os",
  "bm",
  "ge",
  "yc",
  "bh",
  "rf",
  "gs",
  "bd",
  "ab",
  "js",
  "ci",
  "pc",
  "tg",
  "pl",
  "ar",
  "wn",
  "aa",
  "ne",
  "sc",
  "uw",
] as const;

const LOGO_COLUMNS = 10;
const LOGO_TILE_SIZE = 14;

const FACTION_COLORS = {
  os: { accent: "#ff4f4e", text: "#923243", wash: "rgba(255, 79, 78, 0.18)" },
  bm: { accent: "#94a2fd", text: "#466efe", wash: "rgba(112, 140, 254, 0.2)" },
  ge: { accent: "#87e287", text: "#3dc22d", wash: "rgba(97, 208, 90, 0.18)" },
  yc: { accent: "#f0d204", text: "#9f8f00", wash: "rgba(240, 210, 4, 0.2)" },
  bh: { accent: "#bbb4a5", text: "#74598a", wash: "rgba(116, 89, 138, 0.18)" },
  rf: { accent: "#c27184", text: "#b52744", wash: "rgba(194, 113, 132, 0.2)" },
  gs: { accent: "#979797", text: "#727272", wash: "rgba(151, 151, 151, 0.18)" },
  bd: { accent: "#ad7e5f", text: "#985333", wash: "rgba(173, 126, 95, 0.2)" },
  ab: { accent: "#fec078", text: "#fca339", wash: "rgba(254, 192, 120, 0.22)" },
  js: { accent: "#c4d7b4", text: "#6f7b67", wash: "rgba(196, 215, 180, 0.2)" },
  ci: { accent: "#2342ba", text: "#0b2070", wash: "rgba(35, 66, 186, 0.2)" },
  pc: { accent: "#ff99cc", text: "#ff66cc", wash: "rgba(255, 153, 204, 0.18)" },
  tg: { accent: "#6cd9d0", text: "#3ccdc1", wash: "rgba(108, 217, 208, 0.2)" },
  pl: { accent: "#a447d3", text: "#6f1a9b", wash: "rgba(164, 71, 211, 0.2)" },
  ar: { accent: "#7a9d11", text: "#617c0e", wash: "rgba(122, 157, 17, 0.22)" },
  wn: { accent: "#d4bf9f", text: "#cd9b9a", wash: "rgba(212, 191, 159, 0.2)" },
  aa: { accent: "#84dfe8", text: "#3a9ee6", wash: "rgba(132, 223, 232, 0.2)" },
  ne: { accent: "#6e6060", text: "#2e2626", wash: "rgba(110, 96, 96, 0.2)" },
  sc: { accent: "#8cacbc", text: "#3d6479", wash: "rgba(140, 172, 188, 0.22)" },
  uw: { accent: "#d47700", text: "#854000", wash: "rgba(212, 119, 0, 0.2)" },
} as const;

const LOGO_INDEX = new Map<string, number>(LOGO_CODES.map((code, index) => [code, index]));

const FALLBACK_VISUAL = {
  accent: "#8a7860",
  text: "#5a4a38",
  wash: "rgba(138, 120, 96, 0.16)",
};

export interface FactionVisual {
  accent: string;
  text: string;
  wash: string;
  logoUrl: string;
  logoPosition: string;
}

export function getFactionVisual(code: string): FactionVisual {
  const colors = FACTION_COLORS[code as keyof typeof FACTION_COLORS] ?? FALLBACK_VISUAL;
  const index = LOGO_INDEX.get(code) ?? 0;
  const x = (index % LOGO_COLUMNS) * LOGO_TILE_SIZE;
  const y = Math.floor(index / LOGO_COLUMNS) * LOGO_TILE_SIZE;

  return {
    accent: colors.accent,
    text: colors.text,
    wash: colors.wash,
    logoUrl: factionLogoSheetUrl,
    logoPosition: `-${x}px -${y}px`,
  };
}
