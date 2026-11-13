import { Stack } from "@astryxdesign/core/Stack";
import { Icon } from "@astryxdesign/core/Icon";
import {
  colorVars,
  fontWeightVars,
  spacingVars,
  textSizeVars,
  typeScaleVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { UiSprite } from "#/components/UiSprite.tsx";
import { figureStyles } from "#/ui/Figure.tsx";
import { IconFog } from "#/ui/icons.tsx";

/**
 * A match's terms as figures, said once per screen (lobbies, setup, the Match
 * menu): the clock terms in mixed-case mono (`Live 5m +2m`), Fog only when it
 * is on, and the funds on the coin (`1,000`).
 *
 *   <Terms clock="Async 7d +2d" funds={1000} hasFog />
 */
export function Terms({
  clock,
  funds,
  hasFog = false,
}: {
  clock: string;
  funds?: number | null;
  hasFog?: boolean;
}) {
  return (
    <Stack as="span" direction="horizontal" xstyle={[figureStyles.figure, styles.terms]}>
      <Stack as="span" direction="horizontal" xstyle={styles.item}>
        {clock}
      </Stack>
      {hasFog ? (
        <Stack as="span" direction="horizontal" xstyle={[styles.item, styles.fog]}>
          <Icon icon={IconFog} size="xsm" />
          Fog
        </Stack>
      ) : null}
      {funds != null ? (
        <Stack as="span" direction="horizontal" xstyle={styles.item}>
          <UiSprite name="Coin.png" />
          {funds.toLocaleString("en-US")}
        </Stack>
      ) : null}
    </Stack>
  );
}

const styles = stylex.create({
  terms: {
    display: "inline-flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: `${spacingVars["--spacing-1"]} ${spacingVars["--spacing-3"]}`,
    fontSize: textSizeVars["--font-size-sm"],
    fontWeight: typeScaleVars["--text-code-weight"],
    lineHeight: typeScaleVars["--text-code-leading"],
    letterSpacing: "0.01em",
    whiteSpace: "normal",
    color: colorVars["--color-text-secondary"],
  },
  item: {
    display: "inline-flex",
    alignItems: "center",
    gap: spacingVars["--spacing-1"],
    whiteSpace: "nowrap",
  },
  fog: {
    color: colorVars["--color-text-primary"],
    fontWeight: fontWeightVars["--font-weight-semibold"],
  },
});
