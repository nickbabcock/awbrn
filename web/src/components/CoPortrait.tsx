import { Avatar } from "@astryxdesign/core/Avatar";
import { borderVars, colorVars, radiusVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import type { CSSProperties } from "react";
import type { CoPortraitCatalog } from "./co_portraits";
import { loadCoPortraitCatalog, resolveCoPortrait } from "./co_portraits";

interface CoPortraitProps {
  catalog: CoPortraitCatalog | null;
  coKey: string | null | undefined;
  fallbackLabel: string;
  /** Rendered height in px. The portrait keeps its own aspect ratio. */
  size?: number;
}

export function CoPortrait({ catalog, coKey, fallbackLabel, size }: CoPortraitProps) {
  const portrait = resolveCoPortrait(catalog ?? loadCoPortraitCatalog(), coKey);

  if (!portrait) {
    return <Avatar name={fallbackLabel} size="lg" tooltip={false} />;
  }

  // Scaling through the background box keeps the cell on whole pixels, so the
  // art stays crisp instead of being resampled by a transform.
  const scale = size ? size / portrait.height : 1;
  const style: CSSProperties = {
    width: `${Math.round(portrait.width * scale)}px`,
    height: `${Math.round(portrait.height * scale)}px`,
    backgroundImage: `url(${portrait.sheetUrl})`,
    backgroundSize: `${Math.round(portrait.sheetWidth * scale)}px ${Math.round(portrait.sheetHeight * scale)}px`,
    backgroundPosition: `-${Math.round(portrait.x * scale)}px -${Math.round(portrait.y * scale)}px`,
  };

  return (
    <span
      aria-label={portrait.displayName}
      role="img"
      style={style}
      title={portrait.displayName}
      {...stylex.props(styles.portrait)}
    />
  );
}

const styles = stylex.create({
  portrait: {
    display: "block",
    flex: "0 0 auto",
    // The sprite cell is sized exactly; the frame is drawn outside it.
    boxSizing: "content-box",
    backgroundRepeat: "no-repeat",
    imageRendering: "pixelated",
    borderWidth: borderVars["--border-width"],
    borderStyle: "solid",
    borderColor: colorVars["--color-border-emphasized"],
    borderRadius: radiusVars["--radius-element"],
  },
});
