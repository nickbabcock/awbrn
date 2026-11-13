/**
 * A map card, and the grid a set of them is dealt onto.
 *
 * The card is an ink well holding the whole map, letterboxed, over a sheet
 * body: the name, the postage line (`2P · 20×20 · Bamboozle`) and the tags.
 * A ranked revision wears its pennant on the top edge.
 *
 * One Pitch Per Set: every minimap in a grid draws at 2x (8px a tile), so a
 * 37×37 map reads larger than a 19×19 one. A card always shows the whole map
 * (Whole Scale Or Crop): a map wider than its well at 2x (a 50×50) gives up
 * its inline letterbox first, then takes two columns of the grid, and only
 * where that is still too narrow drops, alone, to the largest whole scale that
 * fits. The scale and the span are worked out by CSS against the grid, so the
 * server render already draws them. A wide card keeps its place in the deal
 * (no dense packing), so the grid reads, and tabs, in the catalog's order.
 *
 * Row-mates share their tracks through subgrid: the wells of a row are one
 * height, each map centered in its ink, and the bodies below them line up.
 *
 * The card has two jobs and one face. On the catalog it opens the map's page
 * (`MapLinkPlate`); on New match it is one of a set being chosen from
 * (`MapSelectPlate`), and the chosen card takes an ink ring and a check.
 */

import { Icon } from "@astryxdesign/core/Icon";
import { Skeleton } from "@astryxdesign/core/Skeleton";
import { Token } from "@astryxdesign/core/Token";
import {
  borderVars,
  colorVars,
  focusVars,
  radiusVars,
  spacingVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { Link } from "@tanstack/react-router";
import { Stack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import type { ReactNode } from "react";
import { wholeScaleWidth } from "#/maps/components/MapPicture.tsx";
import { MapRankMedal } from "#/maps/components/MapRankMedal.tsx";
import { mapScreenshotSize } from "#/maps/map_screenshot.ts";
import { MAP_TAG_LABELS, type MapCatalogEntry } from "#/maps/schemas.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { figureStyles } from "#/ui/Figure.tsx";
import { IconCheck } from "#/ui/icons.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";

/** The whole scale every minimap in a set draws at. */
const SET_PITCH = 2;
/** Between a phone and the rail breakpoint: two cards across. */
const TABLET_MEDIA = "@media (min-width: 721px) and (max-width: 1180px)";
/** Ink on each side of the minimap inside its well. */
const WELL_PADDING = spacingVars["--spacing-4"];
/** The card's rule, on each side of its well. */
const CARD_BORDER = 1;

/** What a card says about a map, in one line the screen reader also reads. */
export function mapPlateSummary(map: MapCatalogEntry): string {
  const rank = map.rank ? `rank ${map.rank}` : "unranked";
  return `${map.name} by ${map.author}, ${map.playerCount} players, ${map.width} by ${map.height}, ${rank}`;
}

/** The postage line: armies, size and author, franked in mono. */
export function mapPostage(map: MapCatalogEntry): string {
  return `${map.playerCount}P · ${map.width}×${map.height} · ${map.author}`;
}

/**
 * The grid a set of map cards is dealt onto: three across, two below 1180px,
 * one on a phone. `density="compact"` is the picker inside New match, which
 * shares its row with the rules sheet and sets its cards closer.
 */
export function MapCardGrid({
  children,
  density = "catalog",
}: {
  children: ReactNode;
  density?: "catalog" | "compact";
}) {
  return (
    <Stack xstyle={[styles.grid, density === "compact" && styles.gridCompact]}>{children}</Stack>
  );
}

/** The card on the catalog: a link that opens the map's page. */
export function MapLinkPlate({ map }: { map: MapCatalogEntry }) {
  return (
    <Link
      params={{ mapId: map.mapId }}
      to="/maps/$mapId"
      {...stylex.props(styles.card, styles.span(mapCardWidth(map)))}
    >
      <MapCardFace map={map} />
    </Link>
  );
}

/** The card in New match: one of a set, and the chosen one wears a check. */
export function MapSelectPlate({
  isSelected,
  map,
  onSelect,
}: {
  isSelected: boolean;
  map: MapCatalogEntry;
  onSelect: (map: MapCatalogEntry) => void;
}) {
  return (
    <button
      aria-pressed={isSelected}
      onClick={() => onSelect(map)}
      type="button"
      {...stylex.props(
        styles.card,
        styles.span(mapCardWidth(map)),
        styles.button,
        isSelected && styles.selected,
      )}
    >
      <MapCardFace map={map} />
      {isSelected ? (
        <Stack aria-hidden="true" as="span" xstyle={styles.picked}>
          <Icon icon={IconCheck} size="sm" />
        </Stack>
      ) : null}
    </button>
  );
}

/** How wide a card must be to hold its minimap at the set's pitch. */
function mapCardWidth(map: MapCatalogEntry): number {
  return SET_PITCH * mapScreenshotSize("small", map.width, map.height).width + 2 * CARD_BORDER;
}

function MapCardFace({ map }: { map: MapCatalogEntry }) {
  const source = mapScreenshotSize("small", map.width, map.height);
  return (
    <>
      <Stack as="span" direction="horizontal" xstyle={styles.well}>
        {/* The minimap draws at a whole scale, which no Astryx image takes. */}
        <img
          alt=""
          decoding="async"
          height={source.height}
          loading="lazy"
          src={map.screenshot.small}
          width={source.width}
          {...stylex.props(
            styles.minimap,
            styles.minimapWidth(wholeScaleWidth("100%", source.width, SET_PITCH)),
          )}
        />
      </Stack>
      {map.rank ? (
        <Stack as="span" direction="horizontal" xstyle={styles.pennant}>
          <MapRankMedal rank={map.rank} size="sm" />
        </Stack>
      ) : null}
      <Stack as="span" xstyle={styles.body}>
        {/* One line and an ellipsis; the whole name stays in the card's accessible name. */}
        <Text
          color="inherit"
          hasTruncateTooltip={false}
          maxLines={1}
          type="inherit"
          xstyle={styles.name}
        >
          {map.name}
        </Text>
        <Text color="inherit" type="inherit" xstyle={[figureStyles.figure, styles.postage]}>
          {mapPostage(map)}
        </Text>
        {map.tags.length > 0 ? (
          <Stack as="span" direction="horizontal" xstyle={styles.tags}>
            {map.tags.map((tag) => (
              <Token key={tag} label={MAP_TAG_LABELS[tag]} size="sm" />
            ))}
          </Stack>
        ) : null}
      </Stack>
    </>
  );
}

/** A card still on its way, holding its place in the grid. */
export function MapLoadingPlate({ index }: { index: number }) {
  return (
    <Stack aria-hidden="true" as="span" xstyle={[styles.card, styles.loading]}>
      <Stack as="span" xstyle={[styles.well, styles.wellHeight(160)]} />
      <Stack as="span" xstyle={styles.body}>
        <Skeleton height={18} index={index} width="60%" />
        <Skeleton height={12} index={index} width="45%" />
      </Stack>
    </Stack>
  );
}

// `--map-column` is one column's width, which a card measures its map against.
const styles = stylex.create({
  grid: {
    containerType: "inline-size",
    display: "grid",
    gridTemplateColumns: {
      default: "repeat(3, minmax(0, 1fr))",
      [TABLET_MEDIA]: "repeat(2, minmax(0, 1fr))",
      [pageLayout.phoneMedia]: "minmax(0, 1fr)",
    },
    gap: {
      default: spacingVars["--spacing-5"],
      [pageLayout.phoneMedia]: awbrnVars.spacing3_5,
    },
    "--map-column": {
      default: `calc((100cqi - 2 * ${spacingVars["--spacing-5"]}) / 3)`,
      [TABLET_MEDIA]: `calc((100cqi - ${spacingVars["--spacing-5"]}) / 2)`,
      [pageLayout.phoneMedia]: "100cqi",
    },
  },
  gridCompact: {
    gap: {
      default: awbrnVars.spacing3_5,
      [pageLayout.phoneMedia]: awbrnVars.spacing3_5,
    },
    "--map-column": {
      default: `calc((100cqi - 2 * ${awbrnVars.spacing3_5}) / 3)`,
      [TABLET_MEDIA]: `calc((100cqi - ${awbrnVars.spacing3_5}) / 2)`,
      [pageLayout.phoneMedia]: "100cqi",
    },
  },
  // The card takes two of the grid's rows, well and body, so row-mates'
  // wells share one height and their bodies line up below them.
  card: {
    position: "relative",
    display: "grid",
    gridRow: "span 2",
    gridTemplateColumns: "minmax(0, 1fr)",
    gridTemplateRows: "subgrid",
    rowGap: 0,
    minInlineSize: 0,
    color: awbrnVars.ink900,
    textDecoration: "none",
    backgroundColor: awbrnVars.sheet,
    // Hover only where a pointer hovers, so a tap leaves nothing behind; a
    // press takes the same tone, and the chosen card adds its ring and check.
    borderColor: {
      default: awbrnVars.rule,
      ":hover": { default: null, [pageLayout.hoverMedia]: awbrnVars.ink900 },
      ":active": awbrnVars.ink900,
    },
    borderStyle: "solid",
    borderWidth: borderVars["--border-width"],
    borderRadius: radiusVars["--radius-container"],
    boxShadow: awbrnVars.shadowSheet,
    transitionProperty: "border-color",
    transitionDuration: {
      default: "90ms",
      [pageLayout.reducedMotionMedia]: "0s",
    },
    transitionTimingFunction: awbrnVars.easeStep,
    outlineColor: focusVars["--focus-outline-color"],
    outlineOffset: focusVars["--focus-outline-offset"],
    outlineStyle: {
      default: "none",
      ":focus-visible": focusVars["--focus-outline-style"],
    },
    outlineWidth: focusVars["--focus-outline-width"],
  },
  // Two columns when one is narrower than the map at the set's pitch. A phone
  // has one column, and its card takes it.
  span: (width: number) => ({
    gridColumn: {
      default: `span calc(1 + max(0, sign(${width}px - var(--map-column))))`,
      [pageLayout.phoneMedia]: "auto",
    },
  }),
  button: {
    padding: 0,
    fontFamily: "inherit",
    fontSize: "inherit",
    textAlign: "start",
    cursor: "pointer",
  },
  selected: {
    borderColor: awbrnVars.ink900,
    boxShadow: `0 0 0 2px ${awbrnVars.ink900}, ${awbrnVars.shadowSheet}`,
  },
  loading: {
    borderColor: awbrnVars.rule,
  },
  well: {
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    overflow: "hidden",
    // The inline letterbox is the well's slack: it gives way before the scale.
    paddingBlock: WELL_PADDING,
    backgroundColor: awbrnVars.ink900,
    borderStartStartRadius: `calc(${radiusVars["--radius-container"]} - ${borderVars["--border-width"]})`,
    borderStartEndRadius: `calc(${radiusVars["--radius-container"]} - ${borderVars["--border-width"]})`,
  },
  // The minimap's height, inside the well's ink.
  wellHeight: (height: number) => ({
    minBlockSize: `calc(${height}px + 2 * ${WELL_PADDING})`,
  }),
  minimap: {
    display: "block",
    flexShrink: 0,
    blockSize: "auto",
    imageRendering: "pixelated",
    boxShadow: `0 0 0 1px ${awbrnVars.wellHairline}`,
  },
  minimapWidth: (width: string) => ({
    inlineSize: width,
  }),
  pennant: {
    position: "absolute",
    insetBlockStart: "-1px",
    insetInlineEnd: {
      default: awbrnVars.spacing3_5,
      [pageLayout.phoneMedia]: awbrnVars.spacing2_5,
    },
    zIndex: 2,
    display: "flex",
    filter: awbrnVars.shadowPennant,
  },
  // Its lines kept at the top of the body row.
  body: {
    display: "grid",
    gridTemplateColumns: "minmax(0, 1fr)",
    alignContent: "start",
    gap: spacingVars["--spacing-2"],
    minInlineSize: 0,
    paddingBlockStart: spacingVars["--spacing-3"],
    paddingBlockEnd: awbrnVars.spacing3_5,
    paddingInline: awbrnVars.spacing3_5,
  },
  name: {
    fontSize: awbrnVars.text16,
    fontWeight: 750,
    lineHeight: 1.25,
  },
  postage: {
    color: colorVars["--color-text-secondary"],
    fontSize: awbrnVars.text11_5,
    fontWeight: 500,
    lineHeight: 1.35,
    whiteSpace: "normal",
    overflowWrap: "anywhere",
  },
  tags: {
    display: "flex",
    flexWrap: "wrap",
    gap: spacingVars["--spacing-1-5"],
  },
  picked: {
    position: "absolute",
    insetBlockStart: awbrnVars.spacing2_5,
    insetInlineStart: awbrnVars.spacing2_5,
    zIndex: 2,
    display: "inline-grid",
    placeItems: "center",
    inlineSize: spacingVars["--spacing-7"],
    blockSize: spacingVars["--spacing-7"],
    borderRadius: "50%",
    backgroundColor: awbrnVars.sheet,
    color: awbrnVars.ink900,
    boxShadow: awbrnVars.shadowSheet,
  },
});
