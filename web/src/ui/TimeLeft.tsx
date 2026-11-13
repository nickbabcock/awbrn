import { HStack } from "@astryxdesign/core/Stack";
import { textSizeVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { UiSprite } from "#/components/UiSprite.tsx";
import { Figure, type FigureTone } from "#/ui/Figure.tsx";

/**
 * Time left: the clock sprite and the figure (`3m 42s`, `2d 6h`). No words.
 *
 * - `tone="urgent"`: a live clock running low. Warn ink (warn fill on ink).
 *   A low clock is a warning, never red.
 * - `tone="theirs"`: someone else's clock. Text 2.
 * - `size`: `row` 17px (match rows), `lead` 29px (Home's lead card), `strip`
 *   20px (the turn strip on ink).
 * - `isOnInk` on the ink table and the navy rail.
 *
 * Pass `label` ("3 minutes 42 seconds left") so the figure reads as a time.
 *
 *   <TimeLeft label="3 minutes 42 seconds left" tone="urgent" value="3m 42s" />
 */
export function TimeLeft({
  isOnInk = false,
  label,
  size = "row",
  tone = "default",
  value,
}: {
  isOnInk?: boolean;
  label?: string;
  size?: "row" | "lead" | "strip";
  tone?: "default" | "urgent" | "theirs";
  value: string;
}) {
  return (
    <HStack
      align="center"
      aria-label={label}
      as="span"
      gap={1.5}
      role={label ? "timer" : undefined}
      xstyle={styles.root}
    >
      <UiSprite name="Clock.png" />
      <Figure size="lg" tone={(isOnInk ? inkTones : paperTones)[tone]} xstyle={sizes[size]}>
        {value}
      </Figure>
    </HStack>
  );
}

const paperTones: Record<"default" | "urgent" | "theirs", FigureTone> = {
  default: "primary",
  urgent: "warn",
  theirs: "secondary",
};

const inkTones: Record<"default" | "urgent" | "theirs", FigureTone> = {
  default: "on-ink-strong",
  urgent: "warn-on-ink",
  theirs: "on-ink-secondary",
};

const styles = stylex.create({
  root: {
    display: "inline-flex",
    whiteSpace: "nowrap",
  },
});

/** The figure-lg face at the size each place sets: 17px, 29px, 20px. */
const sizes = stylex.create({
  row: { fontSize: textSizeVars["--font-size-lg"] },
  lead: { fontSize: textSizeVars["--font-size-3xl"] },
  strip: { fontSize: textSizeVars["--font-size-xl"] },
});
