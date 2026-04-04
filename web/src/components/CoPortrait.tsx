import type { CSSProperties } from "react";
import * as stylex from "@stylexjs/stylex";
import type { CoPortraitCatalog } from "./co_portraits";
import { loadCoPortraitCatalog, resolveCoPortrait } from "./co_portraits";

const styles = stylex.create({
  portrait: {
    display: "block",
    flex: "0 0 auto",
    imageRendering: "pixelated",
    backgroundRepeat: "no-repeat",
  },
  fallback: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    width: 56,
    height: 64,
    borderRadius: 10,
    backgroundColor: "rgba(255, 249, 235, 0.18)",
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "rgba(255, 255, 255, 0.12)",
    color: "#fff7eb",
    fontFamily: '"Press Start 2P", monospace',
    fontSize: 10,
  },
});

interface CoPortraitProps {
  catalog: CoPortraitCatalog | null;
  coKey: string | null | undefined;
  fallbackLabel: string;
}

export function CoPortrait({ catalog, coKey, fallbackLabel }: CoPortraitProps) {
  const portrait = resolveCoPortrait(
    catalog === undefined ? loadCoPortraitCatalog() : catalog,
    coKey,
  );

  if (!portrait) {
    return (
      <div aria-label={fallbackLabel} role="img" {...stylex.props(styles.fallback)}>
        {fallbackLabel.slice(0, 1)}
      </div>
    );
  }

  const style: CSSProperties = {
    width: portrait.width,
    height: portrait.height,
    backgroundImage: `url(${portrait.sheetUrl})`,
    backgroundPosition: `-${portrait.x}px -${portrait.y}px`,
  };

  return (
    <div
      aria-label={portrait.displayName}
      role="img"
      style={style}
      title={portrait.displayName}
      {...stylex.props(styles.portrait)}
    />
  );
}
