import { Skeleton } from "@astryxdesign/core/Skeleton";
import { Stack } from "@astryxdesign/core/Stack";
import * as stylex from "@stylexjs/stylex";
import { useQuery } from "@tanstack/react-query";
import { mapCatalogEntryQueryOptions } from "#/maps/maps.queries.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";

/**
 * A map at the size of a row: the 1x minimap (4px a tile), cropped to the
 * middle, in an ink square with 6px corners (Whole Scale Or Crop).
 *
 * - `sm` 48px: docks and ranked slots.
 * - `md` 56px: a compact row or a summary.
 * - `lg` 80px, 64px on a phone: lobby, match and picker rows.
 *
 * The picture is read from the map's catalog entry, which is cached for good,
 * so a list that names one map ten times reads it once.
 */
export function MapThumb({
  mapId,
  revision,
  size = "sm",
}: {
  mapId: string;
  revision: number;
  size?: "sm" | "md" | "lg";
}) {
  const { data: map } = useQuery(mapCatalogEntryQueryOptions(mapId, revision));

  return (
    <Stack as="span" xstyle={[styles.thumb, sizes[size]]}>
      {map ? (
        // The minimap crops to the middle at its own pixels, which no Astryx image does.
        <img alt="" decoding="async" src={map.screenshot.small} {...stylex.props(styles.picture)} />
      ) : (
        <Skeleton height="100%" width="100%" />
      )}
    </Stack>
  );
}

const styles = stylex.create({
  thumb: {
    display: "block",
    flexShrink: 0,
    overflow: "hidden",
    borderRadius: "6px",
    backgroundColor: awbrnVars.ink900,
    boxShadow: `inset 0 0 0 1px ${awbrnVars.ink700}`,
  },
  // `object-fit: none` keeps the minimap at its own pixels and crops it to
  // the middle of the square.
  picture: {
    display: "block",
    inlineSize: "100%",
    blockSize: "100%",
    objectFit: "none",
    objectPosition: "50% 50%",
    imageRendering: "pixelated",
  },
});

const sizes = stylex.create({
  sm: { inlineSize: "48px", blockSize: "48px" },
  md: { inlineSize: "56px", blockSize: "56px" },
  lg: {
    inlineSize: { default: "80px", [pageLayout.phoneMedia]: "64px" },
    blockSize: { default: "80px", [pageLayout.phoneMedia]: "64px" },
  },
});
