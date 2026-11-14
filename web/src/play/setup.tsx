import { Icon } from "@astryxdesign/core/Icon";
import { Skeleton } from "@astryxdesign/core/Skeleton";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { colorVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import { useQuery } from "@tanstack/react-query";
import * as stylex from "@stylexjs/stylex";
import { useState, type ReactNode } from "react";
import { MapPicture } from "#/maps/components/MapPicture.tsx";
import { mapScreenshotSize } from "#/maps/map_screenshot.ts";
import { mapCatalogEntryQueryOptions } from "#/maps/maps.queries.ts";
import { MatchThumb } from "#/play/MatchThumb.tsx";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { AirmailBand } from "#/ui/AirmailBand.tsx";
import { IconChevronDown, IconChevronUp } from "#/ui/icons.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";

/** The parts match setup (a lobby and a ranked pairing) is built from. */
export const setupStyles = stylex.create({
  head: {
    display: "flex",
    alignItems: "flex-start",
    justifyContent: "space-between",
    flexWrap: "wrap",
    gap: spacingVars["--spacing-6"],
  },
  title: {
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  grid: {
    display: "grid",
    gridTemplateColumns: {
      default: "380px minmax(0, 1fr)",
      [pageLayout.belowRailMedia]: "minmax(0, 1fr)",
    },
    alignItems: "start",
    gap: {
      default: spacingVars["--spacing-6"],
      [pageLayout.phoneMedia]: spacingVars["--spacing-4"],
    },
  },
  side: {
    display: {
      default: "flex",
      [pageLayout.phoneMedia]: "contents",
    },
    flexDirection: "column",
    gap: spacingVars["--spacing-6"],
    minInlineSize: 0,
  },
  // On a phone the side column dissolves so the picker sits between the
  // briefing and the rest.
  first: {
    order: {
      default: null,
      [pageLayout.phoneMedia]: 1,
    },
  },
  picker: {
    order: {
      default: null,
      [pageLayout.phoneMedia]: 2,
    },
    minInlineSize: 0,
  },
  after: {
    order: {
      default: null,
      [pageLayout.phoneMedia]: 3,
    },
  },
  phoneHidden: {
    display: {
      default: null,
      [pageLayout.phoneMedia]: "none",
    },
  },
  pickerBody: {
    paddingBlockStart: spacingVars["--spacing-5"],
  },
  dockKey: {
    flexGrow: 1,
    flexBasis: 0,
    minBlockSize: "52px",
  },
  dockKeyMain: {
    flexGrow: 1.4,
  },
  // Ready before a commander is chosen: a secondary key on the paper well,
  // drawn at full strength rather than as a faded red key.
  waitingKey: {
    backgroundColor: awbrnVars.paperWell,
    borderColor: awbrnVars.ruleStrong,
    color: colorVars["--color-text-secondary"],
    boxShadow: "none",
    opacity: {
      default: null,
      ":disabled": 1,
    },
  },
});

/**
 * The clock that runs against the viewer, on ink under the airmail band
 * (desktop; the task dock carries it on a phone).
 */
export function SetupClock({ children }: { children: ReactNode }) {
  return (
    <HStack xstyle={styles.clock}>
      <AirmailBand isOverlay thickness={5} />
      {children}
    </HStack>
  );
}

/**
 * The battlefield in brief, at the head of the briefing sheet: the facts that
 * bear on the pick as one line. On a phone the line is a disclosure with the
 * thumbnail that opens the whole map in place; on desktop the map has its
 * own sheet.
 */
export function BattlefieldPeek({
  facts,
  mapId,
  mapName,
  revision,
}: {
  facts: ReactNode;
  mapId: string;
  mapName: string;
  revision: number;
}) {
  const [isOpen, setIsOpen] = useState(false);
  return (
    <details
      onToggle={(event) => setIsOpen(event.currentTarget.open)}
      open={isOpen}
      {...stylex.props(styles.peek)}
    >
      <summary aria-label={`Battlefield: ${mapName}`} {...stylex.props(styles.peekSummary)}>
        <HStack as="span" xstyle={styles.peekThumb}>
          <MatchThumb mapId={mapId} revision={revision} size={56} />
        </HStack>
        <HStack as="span" xstyle={styles.facts}>
          {facts}
        </HStack>
        <Icon
          icon={isOpen ? IconChevronUp : IconChevronDown}
          size="sm"
          xstyle={styles.peekChevron}
        />
      </summary>
      <VStack xstyle={styles.peekOpen}>
        <BattlefieldPicture mapId={mapId} revision={revision} />
      </VStack>
    </details>
  );
}

/** One fact in the peek line (`[crest] Orange Star`, `Moves first`, `Async`). */
export function Fact({ children, isStrong = false }: { children: ReactNode; isStrong?: boolean }) {
  return (
    <HStack as="span" xstyle={[styles.fact, isStrong && styles.factStrong]}>
      {children}
    </HStack>
  );
}

/** The whole battlefield at 1x on its ink well. */
export function BattlefieldPicture({ mapId, revision }: { mapId: string; revision: number }) {
  const { data: map } = useQuery(mapCatalogEntryQueryOptions(mapId, revision));
  if (!map) return <Skeleton height={280} radius="none" width="100%" />;
  const picture = mapScreenshotSize("full", map.width, map.height);
  return (
    <VStack xstyle={styles.well}>
      <MapPicture
        alt={`The battlefield of ${map.name}, ${map.width} by ${map.height}`}
        maxScale={1}
        sourceHeight={picture.height}
        sourceWidth={picture.width}
        src={map.screenshot.full}
      />
    </VStack>
  );
}

const styles = stylex.create({
  clock: {
    position: "relative",
    alignItems: "center",
    gap: awbrnVars.spacing3_5,
    paddingBlockStart: awbrnVars.spacing4_5,
    paddingBlockEnd: awbrnVars.spacing3_5,
    paddingInline: spacingVars["--spacing-5"],
    overflow: "hidden",
    borderRadius: "10px",
    backgroundColor: awbrnVars.ink900,
    color: awbrnVars.textOnInkStrong,
  },
  peek: {
    marginBlockStart: spacingVars["--spacing-4"],
    marginBlockEnd: 0,
    marginInline: {
      default: spacingVars["--spacing-5"],
      [pageLayout.phoneMedia]: spacingVars["--spacing-4"],
    },
    borderRadius: "6px",
    backgroundColor: awbrnVars.paper,
  },
  peekSummary: {
    display: "flex",
    alignItems: "center",
    gap: spacingVars["--spacing-3"],
    minBlockSize: "48px",
    paddingBlock: awbrnVars.spacing2_5,
    paddingInline: spacingVars["--spacing-3"],
    listStyle: "none",
    cursor: {
      default: "default",
      [pageLayout.phoneMedia]: "pointer",
    },
    pointerEvents: {
      default: "none",
      [pageLayout.phoneMedia]: "auto",
    },
    "::-webkit-details-marker": {
      display: "none",
    },
  },
  peekThumb: {
    display: {
      default: "none",
      [pageLayout.phoneMedia]: "flex",
    },
  },
  peekChevron: {
    display: {
      default: "none",
      [pageLayout.phoneMedia]: "inline-block",
    },
    flexShrink: 0,
    marginInlineStart: "auto",
    color: colorVars["--color-text-secondary"],
  },
  peekOpen: {
    display: {
      default: "none",
      [pageLayout.phoneMedia]: "block",
    },
    marginInline: `calc(${awbrnVars.spacing2_5} * -1)`,
  },
  facts: {
    flexWrap: "wrap",
    alignItems: "center",
    columnGap: spacingVars["--spacing-4"],
    rowGap: spacingVars["--spacing-1-5"],
    minInlineSize: 0,
    color: colorVars["--color-text-secondary"],
    fontSize: awbrnVars.text13_5,
  },
  fact: {
    display: "inline-flex",
    alignItems: "center",
    gap: spacingVars["--spacing-1-5"],
    whiteSpace: "nowrap",
  },
  // On a phone a strong fact (the army, the map) holds its own line, so the
  // terms after it share the next one rather than wrapping one by one.
  factStrong: {
    flexBasis: {
      default: null,
      [pageLayout.phoneMedia]: "100%",
    },
    // A long map name wraps rather than running out of the row.
    whiteSpace: "normal",
    color: colorVars["--color-text-primary"],
    fontWeight: 650,
  },
  well: {
    display: "grid",
    placeItems: "center",
    padding: spacingVars["--spacing-4"],
    overflow: "hidden",
    borderRadius: "6px",
    backgroundColor: awbrnVars.ink900,
  },
});
