import { Icon } from "@astryxdesign/core/Icon";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { colorVars, textSizeVars } from "@astryxdesign/core/theme/tokens.stylex";
import { VisuallyHidden } from "@astryxdesign/core/VisuallyHidden";
import { useQuery } from "@tanstack/react-query";
import * as stylex from "@stylexjs/stylex";
import type { ReactNode } from "react";
import { mapCatalogEntryQueryOptions } from "#/maps/maps.queries.ts";
import { hiddenOpponentUserId } from "#/matches/match_visibility.ts";
import { matchDetailQueryOptions } from "#/matches/matches.queries.ts";
import type { MyMatchSummary } from "#/matches/schemas.ts";
import { MatchThumb } from "#/play/MatchThumb.tsx";
import { Opponents, SeatStamp, STACKED_STAMPS, type OpponentSeat } from "#/play/Opponents.tsx";
import { matchKeyLabel, type MatchRowData } from "#/play/match_rows.ts";
import { isClockLow, seatsFigure, timeLeftFigure, timeLeftLabel } from "#/play/terms.ts";
import { RouterButton } from "#/ui/astryx-links.tsx";
import { figureStyles } from "#/ui/Figure.tsx";
import { IconOpenSeat } from "#/ui/icons.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { TimeLeft } from "#/ui/TimeLeft.tsx";

/**
 * One of the viewer's matches as triage. The row lays itself out from its own
 * width (a container query), so the same row reads in Home's main column and
 * My games' narrower list.
 */
export function MatchRow({
  isAboveFold = false,
  isKeyPrimary = false,
  now,
  row,
  viewerId,
}: {
  /** The row draws with the page, so its minimap loads with it too. */
  isAboveFold?: boolean;
  /** The one red key on the screen. */
  isKeyPrimary?: boolean;
  now: number;
  row: MatchRowData;
  viewerId: string;
}) {
  const { match } = row;
  return (
    <VStack xstyle={styles.container}>
      <VStack xstyle={styles.row}>
        <HStack as="span" xstyle={styles.thumb}>
          <MatchThumb
            isEager={isAboveFold}
            mapId={match.mapId}
            phoneSize={64}
            revision={match.mapRevision}
          />
        </HStack>
        <VStack xstyle={styles.main}>
          <Text color="inherit" type="inherit" xstyle={styles.title}>
            <MatchTitle isRanked={row.isRanked} mapName={row.mapName} match={match} />
          </Text>
          <HStack as="span" xstyle={styles.meta}>
            <MatchFaces match={match} viewerId={viewerId} />
          </HStack>
        </VStack>
        <HStack xstyle={styles.end}>
          <HStack as="span" xstyle={styles.time}>
            <RowTimeLeft now={now} row={row} />
          </HStack>
          <RouterButton
            label={matchKeyLabel(row)}
            params={{ matchId: match.matchId }}
            to="/matches/$matchId"
            variant={isKeyPrimary ? "primary" : "secondary"}
            xstyle={styles.key}
          />
        </HStack>
      </VStack>
    </VStack>
  );
}

/**
 * A ranked match is titled by its map; any other by the name its host gave it.
 * The ranked pools name the map; the catalog names it when they do not.
 */
export function MatchTitle({
  isRanked,
  mapName = null,
  match,
}: {
  isRanked: boolean;
  mapName?: string | null;
  match: Pick<MyMatchSummary, "mapId" | "mapRevision" | "name">;
}) {
  const { data: map } = useQuery({
    ...mapCatalogEntryQueryOptions(match.mapId, match.mapRevision),
    enabled: isRanked && mapName === null,
  });
  if (!isRanked) return <>{match.name}</>;
  return <>Ranked · {mapName ?? map?.name ?? match.name}</>;
}

/**
 * Who the viewer faces. The seats come from the match record, which the list
 * of matches does not carry, so the line fills in a moment after the row and
 * holds its height while it does. A lobby still filling shows its seats.
 */
export function MatchFaces({
  match,
  viewerId,
}: {
  match: Pick<
    MyMatchSummary,
    "matchId" | "phase" | "maxPlayers" | "participantCount" | "viewerParticipants"
  >;
  viewerId: string;
}): ReactNode {
  const isFilling = match.phase === "lobby" || match.phase === "draft";
  const { data: detail, isError } = useQuery({
    ...matchDetailQueryOptions(match.matchId, null),
    enabled: !isFilling,
  });

  if (isFilling) {
    return (
      <HStack
        as="span"
        aria-label={`${match.participantCount} of ${match.maxPlayers} seats taken`}
        role="img"
        xstyle={styles.seats}
      >
        <Icon icon={IconOpenSeat} size="sm" />
        <Text color="inherit" type="inherit" xstyle={figureStyles.figure}>
          {seatsFigure(match.participantCount, match.maxPlayers)}
        </Text>
      </HStack>
    );
  }

  // Until the record arrives, a paper stamp holds each place an opponent will
  // take; a record that cannot load leaves blank stamps.
  if (!detail) {
    const places = Math.min(Math.max(match.maxPlayers - 1, 1), STACKED_STAMPS);
    return (
      <HStack as="span" xstyle={styles.facesPending}>
        {Array.from({ length: places }, (_, index) =>
          isError ? (
            <SeatStamp
              coId={null}
              factionCode={null}
              isDecorative
              key={index}
              xstyle={index > 0 && styles.placeStacked}
            />
          ) : (
            <VStack
              as="span"
              aria-hidden="true"
              key={index}
              xstyle={[styles.place, index > 0 && styles.placeStacked]}
            />
          ),
        )}
        {isError ? <VisuallyHidden>Opponents unavailable</VisuallyHidden> : null}
      </HStack>
    );
  }

  const opponents: OpponentSeat[] = detail.participants
    .filter((seat) => seat.userId !== viewerId)
    .sort((left, right) => left.slotIndex - right.slotIndex)
    .map((seat) => ({
      ...seat,
      isHidden: seat.userId === hiddenOpponentUserId(seat.slotIndex),
    }));
  return <Opponents seats={opponents} />;
}

/** The viewer's own clock, someone else's, or a pairing's window. */
function RowTimeLeft({ now, row }: { now: number; row: MatchRowData }) {
  if (row.deadlineAt === null) return null;
  const remaining = row.deadlineAt - now;
  const isTurn = row.match.phase === "active";
  const tone = !row.isOwed
    ? "theirs"
    : isTurn && isClockLow(remaining, row.match.settings.clock)
      ? "urgent"
      : "default";
  return (
    <TimeLeft label={timeLeftLabel(remaining)} tone={tone} value={timeLeftFigure(remaining)} />
  );
}

const styles = stylex.create({
  container: {
    containerType: "inline-size",
  },
  row: {
    display: "grid",
    gridTemplateColumns: {
      default: "auto minmax(0, 1fr) auto",
      "@container (min-width: 900px)": "80px minmax(0, 1fr) 300px",
      [pageLayout.phoneMedia]: "auto minmax(0, 1fr)",
    },
    gridTemplateAreas: {
      default: '"thumb main end"',
      [pageLayout.phoneMedia]: '"thumb main" "end end"',
    },
    alignItems: "center",
    columnGap: "16px",
    rowGap: "12px",
    paddingBlock: "16px",
    paddingInline: {
      default: "20px",
      [pageLayout.phoneMedia]: "16px",
    },
  },
  thumb: {
    gridArea: "thumb",
  },
  main: {
    gridArea: "main",
    display: "grid",
    gap: "4px",
    minInlineSize: 0,
  },
  title: {
    fontSize: awbrnVars.text16,
    fontWeight: 700,
    lineHeight: 1.25,
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  meta: {
    alignItems: "center",
    minBlockSize: "40px",
    minInlineSize: 0,
  },
  facesPending: {
    display: "inline-flex",
    alignItems: "center",
    blockSize: "40px",
  },
  // A stamp's footprint, in paper well.
  place: {
    display: "block",
    inlineSize: "40px",
    blockSize: "40px",
    borderRadius: "2px",
    backgroundColor: awbrnVars.paperWell,
  },
  placeStacked: {
    marginInlineStart: "-6px",
  },
  seats: {
    display: "inline-flex",
    alignItems: "center",
    gap: "6px",
    color: colorVars["--color-text-secondary"],
    fontSize: textSizeVars["--font-size-sm"],
    fontWeight: 600,
  },
  end: {
    gridArea: "end",
    alignItems: "center",
    justifyContent: "flex-end",
    gap: "16px",
  },
  // One track for every row, as wide as the longest figure (`23h 59m`), so the
  // key starts in the same place whether a row has a clock, a short one, or
  // none (a lobby filling).
  time: {
    display: "inline-flex",
    justifyContent: {
      default: "flex-end",
      [pageLayout.phoneMedia]: "flex-start",
    },
    inlineSize: {
      default: "auto",
      [pageLayout.phoneMedia]: "100px",
    },
    minInlineSize: "100px",
    flexShrink: 0,
  },
  key: {
    inlineSize: {
      default: "auto",
      "@container (min-width: 900px)": "148px",
      [pageLayout.phoneMedia]: "auto",
    },
    flexGrow: {
      default: 0,
      [pageLayout.phoneMedia]: 1,
    },
  },
});
