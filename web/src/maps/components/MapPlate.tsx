/**
 * One map on a board, and the states a board draws around it.
 *
 * A map is recognized by its shape before its name, so the plate is mostly
 * picture: the board at native pixels in a recessed tan well, its grade
 * stamped in the corner the way the game stamps a battle report, and two
 * lines of HUD readout underneath.
 *
 * The plate has two jobs on two boards and one appearance. On the catalog it
 * opens the map's own page, so it is a key that goes somewhere. On the create
 * screen it is one of a set being chosen from, so it wears the cursor. Both
 * are the same panel, because they are the same map.
 */

import { AspectRatio } from "@astryxdesign/core/AspectRatio";
import { Card } from "@astryxdesign/core/Card";
import { Skeleton } from "@astryxdesign/core/Skeleton";
import { SelectableCard } from "@astryxdesign/core/SelectableCard";
import { VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { colorVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { MapPicture } from "#/maps/components/MapPicture.tsx";
import { MapRankMedal } from "#/maps/components/MapRankMedal.tsx";
import { mapScreenshotSize } from "#/maps/map_screenshot.ts";
import { MAP_TAG_LABELS, type MapCatalogEntry } from "#/maps/schemas.ts";
import { RouterClickableCard } from "#/ui/astryx-links.tsx";

/** The picture size of every plate on a board, which fixes their shared multiple. */
export interface BoardPictureSize {
  width: number;
  height: number;
}

/**
 * The largest picture the board holds.
 *
 * Every well on a board is one size, so one multiple has to serve all of
 * them, and it is the multiple the largest map fits in. That is what keeps a
 * big map reading as bigger than a small one.
 */
export function boardPictureSize(maps: readonly MapCatalogEntry[]): BoardPictureSize {
  return maps.reduce(
    (largest, map) => {
      const picture = mapScreenshotSize("small", map.width, map.height);
      return {
        width: Math.max(largest.width, picture.width),
        height: Math.max(largest.height, picture.height),
      };
    },
    { width: 1, height: 1 },
  );
}

/** What a plate says about a map, in one line the screen reader also reads. */
export function mapPlateSummary(map: MapCatalogEntry): string {
  const rank = map.rank ? `rank ${map.rank}` : "unranked";
  return `${map.name} by ${map.author}, ${map.playerCount} players, ${map.width} by ${map.height}, ${rank}`;
}

/** The plate on the catalog board: a key that opens the map's own page. */
export function MapLinkPlate({
  boardPicture,
  map,
}: {
  boardPicture: BoardPictureSize;
  map: MapCatalogEntry;
}) {
  return (
    <RouterClickableCard
      label={mapPlateSummary(map)}
      padding={2}
      params={{ mapId: map.mapId }}
      to="/maps/$mapId"
    >
      <MapPlateFace boardPicture={boardPicture} map={map} />
    </RouterClickableCard>
  );
}

/** The plate on the create screen: one of a set, and the chosen one wears the cursor. */
export function MapSelectPlate({
  boardPicture,
  isSelected,
  map,
  onSelect,
}: {
  boardPicture: BoardPictureSize;
  isSelected: boolean;
  map: MapCatalogEntry;
  onSelect: (map: MapCatalogEntry) => void;
}) {
  return (
    <SelectableCard
      isSelected={isSelected}
      label={mapPlateSummary(map)}
      onChange={() => onSelect(map)}
      padding={2}
    >
      <MapPlateFace boardPicture={boardPicture} map={map} />
    </SelectableCard>
  );
}

function MapPlateFace({
  boardPicture,
  map,
}: {
  boardPicture: BoardPictureSize;
  map: MapCatalogEntry;
}) {
  const picture = mapScreenshotSize("small", map.width, map.height);

  return (
    <VStack gap={2}>
      <VStack xstyle={styles.stamped}>
        <MapPicture
          alt=""
          ratio={1}
          scaleFrom={boardPicture}
          sourceHeight={picture.height}
          sourceWidth={picture.width}
          src={map.screenshot.small}
        />
        {/* An unranked map shows nothing here: an empty corner is what
            unranked looks like on a board, and the dashed slot belongs on the
            map's own page where the grade is the subject. */}
        {map.rank ? (
          <VStack xstyle={styles.stamp}>
            <MapRankMedal rank={map.rank} size="sm" />
          </VStack>
        ) : null}
      </VStack>
      <VStack gap={0.5}>
        <Text maxLines={1} weight="bold">
          {map.name}
        </Text>
        {/* Two readouts and never one line of both: a plate is narrow, and a
            size that truncates is worse than a plate one line taller. */}
        <Text maxLines={1} type="label">
          {map.playerCount}P · {map.width}×{map.height}
        </Text>
        {map.tags.length > 0 ? (
          <Text color="secondary" maxLines={1} type="label">
            {map.tags.length === 1
              ? MAP_TAG_LABELS[map.tags[0]]
              : `${MAP_TAG_LABELS[map.tags[0]]} +${map.tags.length - 1}`}
          </Text>
        ) : null}
      </VStack>
    </VStack>
  );
}

/** A plate still on its way, holding its place on the board. */
export function MapLoadingPlate({ index }: { index: number }) {
  return (
    <Card padding={2}>
      <VStack gap={2}>
        <AspectRatio ratio={1} xstyle={styles.well}>
          <Skeleton index={index} radius="none" />
        </AspectRatio>
        <Skeleton height={16} index={index} radius={0} />
        <Skeleton height={12} index={index} radius={0} width="60%" />
      </VStack>
    </Card>
  );
}

const styles = stylex.create({
  well: {
    backgroundColor: colorVars["--color-background-muted"],
    borderRadius: "var(--radius-element)",
  },
  stamped: {
    position: "relative",
  },
  // The grade is stamped on the report rather than filed beside it, so it sits
  // inside the well, in the corner, over the map it grades.
  stamp: {
    insetBlockStart: "var(--spacing-1)",
    insetInlineEnd: "var(--spacing-1)",
    position: "absolute",
  },
});
