import { Avatar } from "@astryxdesign/core/Avatar";
import { PortraitSprite, StampFrame } from "#/components/CoStamp.tsx";
import type { CoPortraitCatalog } from "./co_portraits";
import { loadCoPortraitCatalog, resolveCoPortrait } from "./co_portraits";

interface CoPortraitProps {
  catalog: CoPortraitCatalog | null;
  coKey: string | null | undefined;
  fallbackLabel: string;
  /**
   * Whether the portrait wears the stamp's perforated white margin. Off where
   * the portrait already sits inside something framed (a picker cell).
   */
  hasFrame?: boolean;
  /** Rendered height in px. The portrait keeps its own aspect ratio. */
  size?: number;
}

/** A commander's portrait at any size. Prefer `CoStamp`, which keeps whole pixels. */
export function CoPortrait({
  catalog,
  coKey,
  fallbackLabel,
  hasFrame = true,
  size,
}: CoPortraitProps) {
  const portrait = resolveCoPortrait(catalog ?? loadCoPortraitCatalog(), coKey);

  if (!portrait) {
    return <Avatar name={fallbackLabel} size="lg" tooltip={false} />;
  }

  const sprite = (
    <PortraitSprite
      aria-label={portrait.displayName}
      portrait={portrait}
      role="img"
      scale={size ? size / portrait.height : 1}
      title={portrait.displayName}
    />
  );

  if (!hasFrame) {
    return sprite;
  }

  return <StampFrame isSmall={(size ?? 32) < 48}>{sprite}</StampFrame>;
}
