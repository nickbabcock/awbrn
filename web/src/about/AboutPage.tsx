import * as stylex from "@stylexjs/stylex";
import {
  CodeText,
  Frame,
  Heading,
  Kicker,
  Page,
  Section,
  Stack,
  Text,
  Wordmark,
} from "#/ui/primitives.tsx";
import { tokens } from "#/ui/theme.stylex.ts";

const letters = [
  { letter: "A", faction: "os", word: "Advance", cls: "wm-os" },
  { letter: "W", faction: "bm", word: "Wars", cls: "wm-bm" },
  { letter: "B", faction: "ge", word: "By", cls: "wm-ge" },
  { letter: "R", faction: "yc", word: "Rust", cls: "wm-yc" },
  { letter: "N", faction: "bh", word: "(New)", cls: "wm-bh" },
] as const;

export function AboutPage() {
  return (
    <Page width="wide">
      <Section>
        <div {...stylex.props(styles.layout)}>
          <Frame xstyle={styles.introFrame}>
            <Stack gap="lg">
              <Kicker xstyle={styles.introKicker}>About</Kicker>
              <Heading size="display">What&apos;s in a Name</Heading>
              <Text size="lg" tone="strong" xstyle={styles.copy}>
                AWBRN, pronounced auburn, is a replay viewer and game toolkit for Advance Wars By
                Web. It is built to make battle review readable at a glance, with recognizable CO
                portraits, stable terrain rendering, and a browser-native flow backed by Rust and
                WebAssembly.
              </Text>
            </Stack>
          </Frame>
          <Frame xstyle={styles.panel}>
            <Stack gap="lg">
              <Wordmark shadow />
              <div {...stylex.props(styles.acronymGrid)}>
                {letters.map(({ letter, word, cls }) => (
                  <div key={letter} {...stylex.props(styles.acronymRow)}>
                    <span
                      {...stylex.props(styles.acronymLetter, acronymStyleMap[cls] || styles.bh)}
                    >
                      {letter}
                    </span>
                    <span {...stylex.props(styles.dash)}>—</span>
                    <span {...stylex.props(styles.word)}>{word}</span>
                  </div>
                ))}
              </div>
              <Text size="sm" tone="muted">
                Load a <CodeText>.zip</CodeText> replay, step through every turn, and inspect the
                battlefield without losing the character of the source game.
              </Text>
            </Stack>
          </Frame>
        </div>
      </Section>
    </Page>
  );
}

const styles = stylex.create({
  layout: {
    display: "grid",
    gap: tokens.space8,
    gridTemplateColumns: {
      default: "minmax(0, 1.1fr) minmax(320px, 0.9fr)",
      "@media (max-width: 860px)": "1fr",
    },
    alignItems: "start",
  },
  copy: {
    maxWidth: 620,
  },
  introFrame: {
    backgroundColor: tokens.panelRaised,
    backgroundImage:
      "linear-gradient(180deg, rgba(255,255,255,0.26), transparent 38%), linear-gradient(135deg, rgba(231, 100, 38, 0.12), transparent 55%)",
  },
  introKicker: {
    color: tokens.brandHover,
  },
  panel: {
    backgroundImage:
      "linear-gradient(180deg, rgba(255,255,255,0.18), transparent 35%), linear-gradient(135deg, rgba(29, 37, 50, 0.08), transparent 40%)",
  },
  acronymGrid: {
    display: "grid",
    gap: tokens.space3,
  },
  acronymRow: {
    display: "grid",
    gridTemplateColumns: "24px 18px minmax(0, 1fr)",
    gap: tokens.space3,
    alignItems: "baseline",
  },
  acronymLetter: {
    fontFamily: tokens.fontPixel,
    fontSize: 18,
    lineHeight: 1,
  },
  dash: {
    color: tokens.inkMuted,
    fontFamily: tokens.fontBody,
    fontSize: 18,
  },
  word: {
    color: tokens.inkStrong,
    fontFamily: tokens.fontBody,
    fontSize: 18,
    fontWeight: 700,
  },
  os: { color: tokens.factionOs },
  bm: { color: tokens.factionBm },
  ge: { color: tokens.factionGe },
  yc: { color: tokens.factionYc },
  bh: { color: tokens.factionBh },
});

const acronymStyleMap: Record<(typeof letters)[number]["cls"], typeof styles.os> = {
  "wm-os": styles.os,
  "wm-bm": styles.bm,
  "wm-ge": styles.ge,
  "wm-yc": styles.yc,
  "wm-bh": styles.bh,
} as const;
