import { AspectRatio } from "@astryxdesign/core/AspectRatio";
import { colorVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useRef, useState } from "react";

/**
 * A picture of a map, drawn at a whole multiple of its own pixels.
 *
 * A browser scaling sprite art by 3.1 gives one source pixel three screen
 * pixels and the next one four, which is the smear this component exists to
 * stop. The picture takes a whole multiple of its own size, and only falls to
 * a part of one when the whole picture is wider than the space it has.
 */
export function MapPicture({
  alt,
  ratio,
  scaleFrom,
  sourceHeight,
  sourceWidth,
  src,
}: {
  alt: string;
  /**
   * Hold the well to this shape. A board of plates wants one shape for every
   * map; a briefing wants the well to sit against the picture it holds.
   */
  ratio?: number;
  /**
   * Choose the multiple as if the picture were this size.
   *
   * A board passes the size of its largest map here. Every well on a board is
   * the same size, so every plate then lands on the same multiple without
   * having to be told what it is, and a large map keeps reading as larger than
   * a small one instead of the other way round.
   */
  scaleFrom?: { width: number; height: number };
  /** The picture's own size, which the map's size already states. */
  sourceHeight: number;
  sourceWidth: number;
  src: string;
}) {
  const pictureRef = useRef<HTMLImageElement>(null);
  const [room, setRoom] = useState<{ width: number; height: number } | null>(null);

  useEffect(() => {
    const well = pictureRef.current?.parentElement;
    if (!well) return;

    const measure = () => {
      const { height, width } = well.getBoundingClientRect();
      if (width > 0) setRoom({ width, height });
    };

    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(well);
    return () => observer.disconnect();
  }, []);

  const scale = chooseScale({
    room,
    ratio,
    scaleHeight: scaleFrom?.height ?? sourceHeight,
    scaleWidth: scaleFrom?.width ?? sourceWidth,
  });
  const picture = (
    <img
      alt={alt}
      height={sourceHeight * scale}
      ref={pictureRef}
      src={src}
      width={sourceWidth * scale}
      {...stylex.props(styles.picture)}
    />
  );

  if (ratio === undefined) {
    return <div {...stylex.props(styles.well, styles.hugging)}>{picture}</div>;
  }

  return (
    <AspectRatio fit="center" ratio={ratio} xstyle={styles.well}>
      {picture}
    </AspectRatio>
  );
}

/**
 * How many screen pixels one source pixel takes.
 *
 * Always a whole number, except where the picture is larger than the space it
 * has: nothing below one whole pixel can be drawn without resampling, so the
 * picture is fitted instead. Before the well has been measured the picture
 * draws at its own size, which is the value a server render also produces.
 */
function chooseScale({
  ratio,
  room,
  scaleHeight,
  scaleWidth,
}: {
  ratio: number | undefined;
  room: { width: number; height: number } | null;
  scaleHeight: number;
  scaleWidth: number;
}): number {
  if (!room) return 1;

  // A well with no shape of its own only limits the width; its height follows
  // whatever the picture turns out to be.
  const fits = Math.min(
    room.width / scaleWidth,
    ratio === undefined ? Number.POSITIVE_INFINITY : room.height / scaleHeight,
  );

  return fits >= 1 ? Math.floor(fits) : fits;
}

const styles = stylex.create({
  // A well is recessed into the panel it sits in, so it takes the road-tan
  // fill and none of the outline the panel already carries.
  well: {
    backgroundColor: colorVars["--color-background-muted"],
    borderRadius: "var(--radius-element)",
  },
  hugging: {
    alignItems: "center",
    display: "flex",
    justifyContent: "center",
    padding: "var(--spacing-2)",
    width: "100%",
  },
  picture: {
    imageRendering: "pixelated",
  },
});
