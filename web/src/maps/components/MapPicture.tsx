import { Stack } from "@astryxdesign/core/Stack";
import { colorVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";

/**
 * A picture of a map, drawn at a whole multiple of its own pixels.
 *
 * A browser scaling sprite art by 3.1 gives one source pixel three screen
 * pixels and the next one four, which is the smear this component exists to
 * stop. The picture takes a whole multiple of its own size, and only falls to
 * a part of one when the whole picture is wider than the space it has.
 *
 * The scale is worked out by CSS against the well's width, so the server
 * render and the first paint already draw it, and a resize never waits on a
 * measurement.
 */
export function MapPicture({
  alt,
  maxScale,
  sourceHeight,
  sourceWidth,
  src,
  tone = "well",
}: {
  alt: string;
  /** The largest whole multiple the picture may take (a map's page opens at 2x). */
  maxScale?: number;
  /** The picture's own size, which the map's size already states. */
  sourceHeight: number;
  sourceWidth: number;
  src: string;
  /**
   * `well` sits the picture in a recessed paper well; `bare` draws it with no
   * ground of its own, for a well the caller paints (the ink board on a map's
   * page).
   */
  tone?: "well" | "bare";
}) {
  return (
    <Stack
      direction="horizontal"
      xstyle={[styles.hugging, tone === "well" && [styles.well, styles.wellPadding]]}
    >
      {/* A raw image: it must take a whole multiple of its own pixels, which no Astryx image component draws. */}
      <img
        alt={alt}
        height={sourceHeight}
        src={src}
        width={sourceWidth}
        {...stylex.props(
          styles.picture,
          styles.width(wholeScaleWidth("100%", sourceWidth, maxScale)),
        )}
      />
    </Stack>
  );
}

/**
 * The width, as CSS, of a picture `sourceWidth` wide drawn at the largest
 * whole multiple that fits `room` (a CSS length), at most `maxScale`.
 *
 * Below one whole multiple the picture is fitted to the room instead: nothing
 * smaller than a whole pixel can be drawn without resampling. A screen that
 * sizes its own columns around the picture (a map's page) asks with the same
 * room, so the column and the picture agree to the pixel.
 */
export function wholeScaleWidth(room: string, sourceWidth: number, maxScale?: number): string {
  const whole = `max(round(down, ${room}, ${sourceWidth}px), min(${room}, ${sourceWidth}px))`;
  return maxScale === undefined ? whole : `min(${maxScale * sourceWidth}px, ${whole})`;
}

const styles = stylex.create({
  // A well is recessed into the panel it sits in: the paper well, and none
  // of the outline the panel already carries.
  well: {
    backgroundColor: colorVars["--color-background-muted"],
    borderRadius: "var(--radius-element)",
  },
  wellPadding: {
    padding: "var(--spacing-2)",
  },
  hugging: {
    alignItems: "center",
    display: "flex",
    justifyContent: "center",
    width: "100%",
  },
  picture: {
    display: "block",
    flexShrink: 0,
    blockSize: "auto",
    imageRendering: "pixelated",
  },
  width: (width: string) => ({
    inlineSize: width,
  }),
});
