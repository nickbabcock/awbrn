import type { CSSProperties } from "react";
import type { CoPortraitCatalog } from "./co_portraits";
import { resolveCoPortrait } from "./co_portraits";

interface CoPortraitProps {
  catalog: CoPortraitCatalog | null;
  coKey: string | null | undefined;
  fallbackLabel: string;
}

export function CoPortrait({ catalog, coKey, fallbackLabel }: CoPortraitProps) {
  const portrait = resolveCoPortrait(catalog, coKey);

  if (!portrait) {
    return (
      <div aria-label={fallbackLabel} className="co-portrait co-portrait--fallback" role="img">
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
      className="co-portrait"
      role="img"
      style={style}
      title={portrait.displayName}
    />
  );
}
