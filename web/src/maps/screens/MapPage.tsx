import { Heading } from "@astryxdesign/core/Heading";
import { Icon } from "@astryxdesign/core/Icon";
import { Skeleton } from "@astryxdesign/core/Skeleton";
import { Stack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { Token } from "@astryxdesign/core/Token";
import { colorVars, typeScaleVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useQuery } from "@tanstack/react-query";
import { useActor } from "#/auth/useActor.ts";
import { MapCurationPanel, MapTagsKey } from "#/maps/components/MapCurationPanel.tsx";
import { MapJudgmentRecord } from "#/maps/components/MapJudgmentRecord.tsx";
import { MapPicture, wholeScaleWidth } from "#/maps/components/MapPicture.tsx";
import { mapPostage } from "#/maps/components/MapPlate.tsx";
import { MapRankMedal } from "#/maps/components/MapRankMedal.tsx";
import { mapScreenshotSize } from "#/maps/map_screenshot.ts";
import { mapQueryOptions } from "#/maps/maps.queries.ts";
import { MAP_TAG_LABELS, type MapCatalogEntry } from "#/maps/schemas.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { RouterButton } from "#/ui/astryx-links.tsx";
import { BackLink } from "#/ui/BackKey.tsx";
import { Button } from "#/ui/Button.tsx";
import { EmptySheet, ProblemBanner } from "#/ui/EmptySheet.tsx";
import { figureStyles } from "#/ui/Figure.tsx";
import { IconRefresh } from "#/ui/icons.tsx";
import { Page } from "#/ui/Page.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";

const BOARD_INSET = 22;
const COLUMN_GAP = 32;
/** The rail's own width, and the narrowest it gives way to for a 2x board. */
const RAIL_WIDTH = 460;
const RAIL_MIN_WIDTH = 392;
const TABLET_MEDIA = "@media (min-width: 721px) and (max-width: 1180px)";

export function MapPage({ mapId }: { mapId: string }) {
  const actor = useActor();
  const mapQuery = useQuery(mapQueryOptions(mapId));
  const map = mapQuery.data;

  return (
    <Page>
      {/* The back link sits on the content at its own 8px, not the page's 24px. */}
      <Stack xstyle={styles.flow}>
        <BackLink label="Maps" to="/maps" />

        {mapQuery.isPending ? <DetailSkeleton /> : null}

        {mapQuery.isError ? (
          <ProblemBanner
            action={
              <Button
                clickAction={async () => {
                  await mapQuery.refetch();
                }}
                icon={<Icon icon={IconRefresh} />}
                label="Retry"
                size="sm"
              />
            }
            title="Map failed to load"
          />
        ) : null}

        {map === null ? <EmptySheet headingLevel={2} title="No such map" /> : null}

        {map ? (
          <Stack xstyle={[styles.grid, styles.columns(columnsFor(map))]}>
            <Stack direction="horizontal" xstyle={styles.head}>
              <Stack gap={2} xstyle={styles.titleBlock}>
                <Heading level={1} maxLines={1} xstyle={styles.title}>
                  {map.name}
                </Heading>
                <Text
                  as="p"
                  color="inherit"
                  display="block"
                  type="inherit"
                  xstyle={[figureStyles.figure, styles.postage]}
                >
                  {postage(map)}
                </Text>
                <Stack direction="horizontal" wrap="wrap" xstyle={styles.tags}>
                  {map.tags.map((tag) => (
                    <Token key={tag} label={MAP_TAG_LABELS[tag]} size="sm" />
                  ))}
                  <MapTagsKey actor={actor} map={map} />
                </Stack>
              </Stack>
              <MapRankMedal rank={map.rank} size="lg" xstyle={styles.medal} />
            </Stack>

            <Stack xstyle={styles.cta}>
              <RouterButton
                label="New match"
                search={{ map: map.mapId }}
                size="lg"
                to="/matches/new"
                variant="primary"
                width="100%"
              />
            </Stack>

            <MapBoard map={map} />

            <Stack xstyle={styles.curation}>
              <MapCurationPanel actor={actor} map={map} />
            </Stack>
            <Stack xstyle={styles.record}>
              <MapJudgmentRecord actor={actor} map={map} />
            </Stack>
          </Stack>
        ) : null}
      </Stack>
    </Page>
  );
}

/** Once a map has been edited, the line names the revision its rank names. */
function postage(map: MapCatalogEntry): string {
  const parts = [mapPostage(map)];
  if (map.origin) parts.push(`AWBW ${map.origin.sourceMapId}`);
  if (map.revision > 1) parts.push(`Rev ${map.revision}`);
  return parts.join(" · ");
}

/**
 * The board's width, as CSS, in a space `space` wide: the map at the largest
 * whole scale up to 2x that fits, plus the ink around it. The table hugs its
 * map; it never pads a small map with spare ink.
 */
function boardWidth(map: MapCatalogEntry, space: string): string {
  const picture = mapScreenshotSize("full", map.width, map.height);
  const room = `${space} - ${2 * BOARD_INSET}px`;
  return `calc(${wholeScaleWidth(room, picture.width, 2)} + ${2 * BOARD_INSET}px)`;
}

/**
 * The board asks for its map at 2x with the rail at its narrowest, so a map a
 * little wider than usual still draws at 2x. The rail then takes what is left
 * up to its full width, and any width beyond that opens between the two, so
 * the medal and the key stand in the same place on every map.
 */
function columnsFor(map: MapCatalogEntry): string {
  const board = boardWidth(map, `100% - ${COLUMN_GAP + RAIL_MIN_WIDTH}px`);
  return `${board} clamp(${RAIL_MIN_WIDTH}px, 100% - ${COLUMN_GAP}px - ${board}, ${RAIL_WIDTH}px)`;
}

function MapBoard({ map }: { map: MapCatalogEntry }) {
  const picture = mapScreenshotSize("full", map.width, map.height);
  return (
    <Stack
      aria-label={`The battlefield of ${map.name}`}
      as="section"
      xstyle={[styles.board, styles.boardWidth(boardWidth(map, "100%"))]}
    >
      <Stack xstyle={styles.boardPicture}>
        <MapPicture
          alt={`The battlefield of ${map.name}`}
          maxScale={2}
          sourceHeight={picture.height}
          sourceWidth={picture.width}
          src={map.screenshot.full}
        />
      </Stack>
    </Stack>
  );
}

function DetailSkeleton() {
  return (
    <Stack xstyle={[styles.grid, styles.columns(`minmax(0, 1fr) ${RAIL_WIDTH}px`)]}>
      <Stack xstyle={styles.head}>
        <Stack gap={2} xstyle={styles.titleBlock}>
          <Skeleton height={48} width="70%" />
          <Skeleton height={14} width="55%" />
        </Stack>
      </Stack>
      <Stack xstyle={[styles.board, styles.boardLoading]} />
    </Stack>
  );
}

const styles = stylex.create({
  flow: {
    display: "block",
  },
  // Desktop: the board spans the right column's rows, and a last 1fr row
  // takes whatever height the board has beyond them, so the right column's
  // sheets keep their 24px gaps.
  grid: {
    display: {
      default: "grid",
      [pageLayout.belowRailMedia]: "flex",
    },
    flexDirection: "column",
    // Below the rail breakpoint an empty sheet slot leaves no gap.
    gridTemplateAreas: '"board head" "board cta" "board curation" "board record" "board ."',
    gridTemplateRows: "auto auto auto auto 1fr",
    columnGap: `${COLUMN_GAP}px`,
    // Width the two columns leave goes between them (see `columnsFor`).
    justifyContent: {
      default: "space-between",
      [pageLayout.belowRailMedia]: "normal",
    },
    rowGap: {
      default: "24px",
      [pageLayout.phoneMedia]: "16px",
    },
    alignItems: {
      default: "start",
      [pageLayout.belowRailMedia]: "stretch",
    },
  },
  columns: (columns: string) => ({
    gridTemplateColumns: columns,
  }),
  // The title block takes the row and the pennant is pinned to its end, so
  // a short title leaves the space before the pennant and a long one ends in
  // an ellipsis instead of pushing the pennant along. An unranked map's title
  // keeps the whole row.
  head: {
    gridArea: "head",
    display: "flex",
    alignItems: "flex-start",
    justifyContent: "space-between",
    gap: "16px",
    minInlineSize: 0,
  },
  titleBlock: {
    display: "grid",
    flexGrow: 1,
    flexShrink: 1,
    flexBasis: 0,
    minInlineSize: 0,
  },
  // The theme balances headings (`text-wrap: balance`), which wraps them
  // again; the title holds one line and ends in an ellipsis.
  title: {
    whiteSpace: "nowrap",
  },
  medal: {
    marginInlineStart: "auto",
  },
  postage: {
    margin: 0,
    color: colorVars["--color-text-secondary"],
    fontSize: typeScaleVars["--text-supporting-size"],
    whiteSpace: "normal",
    overflowWrap: "anywhere",
  },
  tags: {
    // A map with no tags and no Edit tags key leaves no gap behind.
    display: {
      default: "flex",
      ":empty": "none",
    },
    alignItems: "center",
    gap: "6px",
  },
  cta: {
    gridArea: "cta",
  },
  curation: {
    gridArea: "curation",
    minInlineSize: 0,
    ":empty": {
      display: "none",
    },
  },
  record: {
    gridArea: "record",
    minInlineSize: 0,
    ":empty": {
      display: "none",
    },
  },
  board: {
    gridArea: "board",
    display: "grid",
    placeItems: "center",
    // Between a phone and the rail breakpoint the table is centered on its
    // own width; on a phone it runs edge to edge.
    alignSelf: {
      default: "start",
      [TABLET_MEDIA]: "center",
      [pageLayout.phoneMedia]: "stretch",
    },
    minInlineSize: 0,
    backgroundColor: awbrnVars.ink950,
    borderRadius: {
      default: "10px",
      [pageLayout.phoneMedia]: 0,
    },
    // The full picture carries the overhang row above the first tile row, so
    // the table needs no padding of its own above it.
    paddingBlock: {
      default: "0 20px",
      [pageLayout.phoneMedia]: "0 16px",
    },
    paddingInline: {
      default: `${BOARD_INSET}px`,
      [pageLayout.phoneMedia]: 0,
    },
    marginInline: {
      default: 0,
      [pageLayout.phoneMedia]: `calc(${pageLayout.gutterPhone} * -1)`,
    },
    overflow: "hidden",
  },
  boardWidth: (width: string) => ({
    inlineSize: {
      default: width,
      [pageLayout.phoneMedia]: "auto",
    },
  }),
  boardLoading: {
    alignSelf: "stretch",
    minBlockSize: "480px",
  },
  boardPicture: {
    inlineSize: "100%",
    filter: "drop-shadow(0 20px 36px rgba(0, 0, 0, 0.6))",
  },
});
