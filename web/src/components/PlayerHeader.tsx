import type { CSSProperties, ReactNode } from "react";
import * as stylex from "@stylexjs/stylex";
import { getFactionVisual } from "#/faction_visuals.ts";
import { tokens } from "#/ui/theme.stylex.ts";
import { sx, type XStyle } from "#/ui/stylex.ts";

/**
 * Inline CSSProperties for the faction gradient header surface.
 * Exported so callers can apply it to arbitrary elements.
 */
export const playerHeaderSurface = (factionCode: string): CSSProperties => {
  const { accent, text } = getFactionVisual(factionCode);
  return {
    backgroundImage: [
      "linear-gradient(135deg, rgba(58, 35, 21, 0.28), rgba(58, 35, 21, 0.1))",
      `linear-gradient(135deg, ${accent} 0%, ${text} 100%)`,
    ].join(", "),
  };
};

/**
 * Inline CSSProperties for the faction text-shadow on the gradient surface.
 */
export const playerNameShadow = (factionCode: string): CSSProperties => {
  const { text } = getFactionVisual(factionCode);
  return { textShadow: `0 1px 0 ${text}` };
};

/**
 * Faction-coloured gradient header bar.
 *
 * - `name` renders in light text with a faction-tinted shadow.
 * - `trailing` is right-aligned (badges, action buttons, etc.).
 * - `xstyle` lets callers apply layout-specific overrides (e.g. gridArea,
 *   marginInline bleed) without breaking the shared visual.
 */
export function PlayerHeader({
  factionCode,
  name,
  trailing,
  xstyle,
}: {
  factionCode: string;
  name: string;
  trailing?: ReactNode;
  xstyle?: XStyle;
}) {
  return (
    <div style={playerHeaderSurface(factionCode)} {...sx(styles.header, xstyle)}>
      <span style={playerNameShadow(factionCode)} {...stylex.props(styles.name)}>
        {name}
      </span>
      {trailing ? <div {...stylex.props(styles.trailing)}>{trailing}</div> : null}
    </div>
  );
}

/**
 * 24×24 faction logo badge sized for use on a dark/gradient header surface.
 * The white-alpha fill is intentional — it reads against the gradient.
 */
export function FactionBadge({ factionCode, title }: { factionCode: string; title?: string }) {
  const visual = getFactionVisual(factionCode);
  return (
    <span
      aria-label={title ? `Faction: ${title}` : undefined}
      title={title}
      {...stylex.props(styles.badge)}
    >
      <span
        aria-hidden="true"
        style={{
          backgroundImage: `url(${visual.logoUrl})`,
          backgroundPosition: visual.logoPosition,
        }}
        {...stylex.props(styles.badgeLogo)}
      />
    </span>
  );
}

const styles = stylex.create({
  header: {
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
    gap: tokens.space2,
    paddingBlock: 5,
    paddingInline: tokens.space3,
    minHeight: 32,
  },
  name: {
    color: "#fff5de",
    fontFamily: tokens.fontBody,
    fontSize: 15,
    fontWeight: 800,
    lineHeight: 1.3,
    minWidth: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  trailing: {
    display: "flex",
    alignItems: "center",
    gap: tokens.space2,
    flexShrink: 0,
  },
  badge: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    width: 24,
    height: 24,
    borderRadius: tokens.radius1,
    backgroundColor: "rgba(255, 255, 255, 0.16)",
    borderWidth: 2,
    borderStyle: "solid",
    borderColor: "rgba(255, 255, 255, 0.24)",
  },
  badgeLogo: {
    display: "block",
    width: 14,
    height: 14,
    // This matches the logos atlas geometry in `faction_visuals.ts`:
    // `LOGO_COLUMNS = 10` and `LOGO_TILE_SIZE = 14`, so the sheet is `140px 28px`.
    backgroundSize: "140px 28px",
    backgroundRepeat: "no-repeat",
    imageRendering: "pixelated",
  },
});
