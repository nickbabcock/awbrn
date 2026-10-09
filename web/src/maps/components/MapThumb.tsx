import { useQuery } from "@tanstack/react-query";
import { AspectRatio } from "@astryxdesign/core/AspectRatio";
import { Skeleton } from "@astryxdesign/core/Skeleton";
import { borderVars, colorVars, radiusVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { MapPicture } from "#/maps/components/MapPicture.tsx";
import { mapScreenshotSize } from "#/maps/map_screenshot.ts";
import { mapCatalogEntryQueryOptions } from "#/maps/maps.queries.ts";

/** The edge of a thumbnail in a row, in px, which the row's text aligns against. */
const THUMB_SIZES = { sm: 48, md: 72 } as const;

/**
 * A map at the size of a row: the small picture in a square tan well.
 *
 * A row in a list of matches is recognized by its map before its name, the
 * same way a plate on the catalog is. The picture is read from the map's
 * catalog entry, which is cached for good, so a list that names one map ten
 * times reads it once.
 */
export function MapThumb({
  mapId,
  revision,
  size = "sm",
}: {
  mapId: string;
  revision: number;
  size?: keyof typeof THUMB_SIZES;
}) {
  const { data: map } = useQuery(mapCatalogEntryQueryOptions(mapId, revision));
  const edge = THUMB_SIZES[size];

  if (!map) {
    return (
      <AspectRatio ratio={1} xstyle={[styles.well, size === "sm" ? styles.sm : styles.md]}>
        <Skeleton height="100%" width="100%" />
      </AspectRatio>
    );
  }

  const picture = mapScreenshotSize("small", map.width, map.height);
  return (
    <AspectRatio ratio={1} xstyle={[styles.well, size === "sm" ? styles.sm : styles.md]}>
      <MapPicture
        alt=""
        ratio={1}
        scaleFrom={{ width: Math.max(picture.width, edge), height: Math.max(picture.height, edge) }}
        sourceHeight={picture.height}
        sourceWidth={picture.width}
        src={map.screenshot.small}
      />
    </AspectRatio>
  );
}

const styles = stylex.create({
  well: {
    flexShrink: 0,
    overflow: "hidden",
    backgroundColor: colorVars["--color-background-muted"],
    borderColor: colorVars["--color-border-emphasized"],
    borderStyle: "solid",
    borderWidth: borderVars["--border-width"],
    borderRadius: radiusVars["--radius-element"],
  },
  sm: { inlineSize: `${THUMB_SIZES.sm}px` },
  md: { inlineSize: `${THUMB_SIZES.md}px` },
});
