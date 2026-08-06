import * as stylex from "@stylexjs/stylex";
import { borderVars, colorVars, radiusVars } from "@astryxdesign/core/theme/tokens.stylex";
import type { CSSProperties } from "react";
import logosTextureUrl from "../../../assets/textures/logos.png?url";
import { factions, getFactionByCode } from "#/factions.ts";

/** The sheet is a plain grid of 14x14 cells in faction catalog order. */
const LOGO_COLUMNS = 10;
const LOGO_ROWS = Math.ceil(factions.length / LOGO_COLUMNS);

const LOGO_INDEX_BY_CODE = new Map(factions.map((faction, index) => [faction.code, index]));

/**
 * Background style for one army logo, scaled to `size`.
 *
 * The sprite is scaled by the background size instead of a transform, so the
 * cell stays on whole pixels and the art keeps its hard edges.
 */
export function factionLogoStyle(factionCode: string, size: number): CSSProperties | null {
  const index = LOGO_INDEX_BY_CODE.get(factionCode);
  if (index === undefined) {
    return null;
  }

  const column = index % LOGO_COLUMNS;
  const row = Math.floor(index / LOGO_COLUMNS);

  return {
    width: `${size}px`,
    height: `${size}px`,
    backgroundImage: `url(${logosTextureUrl})`,
    backgroundSize: `${LOGO_COLUMNS * size}px ${LOGO_ROWS * size}px`,
    backgroundPosition: `-${column * size}px -${row * size}px`,
    backgroundRepeat: "no-repeat",
    imageRendering: "pixelated",
  };
}

/**
 * The army's own insignia, framed like every other sprite in the interface.
 *
 * Each logo already carries its army color, so the mark identifies the faction
 * by shape and by hue at the same time. Callers that show the army name next to
 * it pass `isLabelHidden`, because two announcements of the same army read as
 * two armies.
 */
export function FactionLogo({
  factionCode,
  isFramed = true,
  isLabelHidden = false,
  size = 28,
}: {
  factionCode: string;
  /** Off when the logo already sits inside a control that draws the outline. */
  isFramed?: boolean;
  isLabelHidden?: boolean;
  size?: number;
}) {
  const style = factionLogoStyle(factionCode, size);
  if (!style) {
    return null;
  }

  const name = getFactionByCode(factionCode)?.displayName ?? factionCode.toUpperCase();

  return (
    <span
      aria-hidden={isLabelHidden ? "true" : undefined}
      aria-label={isLabelHidden ? undefined : name}
      role={isLabelHidden ? undefined : "img"}
      style={style}
      {...stylex.props(styles.logo, isFramed && styles.logoFrame)}
    />
  );
}

const styles = stylex.create({
  logo: {
    display: "block",
    flex: "0 0 auto",
    // The sprite cell is sized exactly; the frame is drawn outside it.
    boxSizing: "content-box",
  },
  logoFrame: {
    borderWidth: borderVars["--border-width"],
    borderStyle: "solid",
    borderColor: colorVars["--color-border-emphasized"],
    borderRadius: radiusVars["--radius-element"],
  },
});
