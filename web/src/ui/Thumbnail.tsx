import { Thumbnail as AstryxThumbnail, type ThumbnailProps } from "@astryxdesign/core/Thumbnail";
import * as stylex from "@stylexjs/stylex";

export type { ThumbnailProps } from "@astryxdesign/core/Thumbnail";

/** Thumbnail that keeps game artwork pixelated. */
export function Thumbnail({ xstyle, ...props }: ThumbnailProps) {
  return <AstryxThumbnail {...props} xstyle={[styles.root, xstyle]} />;
}

const styles = stylex.create({
  root: {
    imageRendering: "pixelated",
  },
});
