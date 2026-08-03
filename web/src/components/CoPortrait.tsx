import { Avatar } from "@astryxdesign/core/Avatar";
import type { CSSProperties } from "react";
import type { CoPortraitCatalog } from "./co_portraits";
import { loadCoPortraitCatalog, resolveCoPortrait } from "./co_portraits";

interface CoPortraitProps {
  catalog: CoPortraitCatalog | null;
  coKey: string | null | undefined;
  fallbackLabel: string;
}

export function CoPortrait({ catalog, coKey, fallbackLabel }: CoPortraitProps) {
  const portrait = resolveCoPortrait(catalog ?? loadCoPortraitCatalog(), coKey);

  if (!portrait) {
    return <Avatar name={fallbackLabel} size="lg" tooltip={false} />;
  }

  const style: CSSProperties = {
    display: "inline-block",
    flex: "0 0 auto",
    width: portrait.width,
    height: portrait.height,
    backgroundImage: `url(${portrait.sheetUrl})`,
    backgroundPosition: `-${portrait.x}px -${portrait.y}px`,
    backgroundRepeat: "no-repeat",
    imageRendering: "pixelated",
  };

  return (
    <span aria-label={portrait.displayName} role="img" style={style} title={portrait.displayName} />
  );
}
