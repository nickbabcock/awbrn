import { Icon } from "@astryxdesign/core/Icon";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { colorVars, textSizeVars } from "@astryxdesign/core/theme/tokens.stylex";
import { useQuery, useSuspenseInfiniteQuery } from "@tanstack/react-query";
import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { FactionLogo } from "#/components/FactionLogo.tsx";
import { getFactionById } from "#/factions.ts";
import { mapCatalogEntryQueryOptions } from "#/maps/maps.queries.ts";
import { formatSeatResultReason, opposingSeats, viewerOutcome } from "#/matches/match_history.ts";
import { myCompletedMatchesQueryOptions } from "#/matches/matches.queries.ts";
import { matchReplayDownloadPath } from "#/matches/replay_archive.ts";
import type { MatchHistoryEntry, MatchHistorySeat, MatchOutcome } from "#/matches/schemas.ts";
import { SeatStamp } from "#/play/Opponents.tsx";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { RouterButton } from "#/ui/astryx-links.tsx";
import { EmptySheet, ProblemBanner } from "#/ui/EmptySheet.tsx";
import { Figure } from "#/ui/Figure.tsx";
import { IconDownload } from "#/ui/icons.tsx";
import { Page, PageHeader } from "#/ui/Page.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { Postmark } from "#/ui/Postmark.tsx";
import { Sheet, SheetRow, SheetRows } from "#/ui/Sheet.tsx";

const monthFormat = new Intl.DateTimeFormat("en-US", { month: "short" });
const longDateFormat = new Intl.DateTimeFormat("en-US", { dateStyle: "long" });

export function MatchHistoryPage() {
  const historyQuery = useSuspenseInfiniteQuery(myCompletedMatchesQueryOptions());
  const [paginationError, setPaginationError] = useState<string | null>(null);
  const matches = historyQuery.data.pages.flatMap((page) => page.matches);

  async function handleLoadMore(): Promise<void> {
    if (historyQuery.isFetchingNextPage || !historyQuery.hasNextPage) return;
    setPaginationError(null);
    try {
      await historyQuery.fetchNextPage();
    } catch (nextError) {
      setPaginationError(
        nextError instanceof Error ? nextError.message : "More finished battles failed to load",
      );
    }
  }

  return (
    <Page width="narrow">
      <PageHeader title="My games" />

      {matches.length === 0 ? (
        <EmptySheet title="No finished battles yet" />
      ) : (
        <Sheet aria-label="History">
          <SheetRows hasTopRule={false}>
            {matches.map((entry) => (
              <SheetRow key={entry.matchId}>
                <Report entry={entry} />
              </SheetRow>
            ))}
          </SheetRows>
        </Sheet>
      )}

      {paginationError ? (
        <ProblemBanner
          action={<Button clickAction={handleLoadMore} label="Retry" size="sm" />}
          title="More finished battles failed to load"
        />
      ) : null}

      {matches.length > 0 && historyQuery.hasNextPage ? (
        <HStack justify="center">
          <Button
            clickAction={handleLoadMore}
            isLoading={historyQuery.isFetchingNextPage}
            label="Load more"
          />
        </HStack>
      ) : null}
    </Page>
  );
}

/** One finished battle. */
function Report({ entry }: { entry: MatchHistoryEntry }) {
  const { data: map } = useQuery(mapCatalogEntryQueryOptions(entry.mapId, entry.mapRevision));
  const viewerSeats = entry.seats.filter((seat) =>
    entry.viewerSlotIndexes.includes(seat.slotIndex),
  );
  const opponents = opposingSeats(entry.seats, entry.viewerSlotIndexes);
  const isHotseat = viewerSeats.length > 1;
  const outcome = viewerOutcome(viewerSeats);
  const mapName = map?.name ?? null;
  const isRanked = entry.pool !== null;
  const title = isRanked ? `Ranked · ${mapName ?? entry.name}` : entry.name;
  const completed = new Date(entry.completedAt);

  // A ranked match is titled by its map, and a match named for its map says
  // it once. The result is the tag and each seat says why it ended, so the
  // line under the title is the map alone.
  const meta = isRanked || mapName === entry.name ? null : mapName;

  return (
    <VStack xstyle={styles.report}>
      <Postmark
        figure={completed.getDate()}
        label={`Finished ${longDateFormat.format(completed)}`}
        ringText={String(completed.getFullYear())}
        size={72}
        word={monthFormat.format(completed).toUpperCase()}
        xstyle={styles.postmark}
      />
      <VStack xstyle={styles.main}>
        <HStack xstyle={styles.titleLine}>
          <Text color="inherit" type="inherit" xstyle={styles.title}>
            {title}
          </Text>
          <ResultTag isHotseat={isHotseat} outcome={outcome} />
        </HStack>
        {meta !== null ? (
          <Text as="div" color="inherit" display="block" type="inherit" xstyle={styles.meta}>
            {meta}
          </Text>
        ) : null}
        <ul aria-label="Seats" {...stylex.props(styles.seats)}>
          {[...viewerSeats, ...opponents].map((seat) => (
            <ReportSeat isHotseat={isHotseat} key={seat.slotIndex} seat={seat} />
          ))}
        </ul>
      </VStack>
      <HStack xstyle={styles.end}>
        {isRanked && !isHotseat ? <RatingMove seat={viewerSeats[0]} /> : null}
        <HStack as="span" xstyle={styles.keys}>
          {entry.hasReplay ? (
            <Button
              as="a"
              href={matchReplayDownloadPath(entry.matchId)}
              icon={<Icon icon={IconDownload} />}
              isIconOnly
              label={`Download the replay archive for ${title}`}
              tooltip="Replay archive"
            />
          ) : null}
          <RouterButton
            label="Watch"
            params={{ matchId: entry.matchId }}
            to="/matches/$matchId"
            xstyle={styles.watch}
          />
        </HStack>
      </HStack>
    </VStack>
  );
}

/** The result in words, on a tag: "You won" in green, "You lost" plain. */
function ResultTag({ isHotseat, outcome }: { isHotseat: boolean; outcome: MatchOutcome | null }) {
  const label = isHotseat
    ? "Hotseat"
    : outcome === "win"
      ? "You won"
      : outcome === "loss"
        ? "You lost"
        : outcome === "draw"
          ? "A draw"
          : "No result";
  return (
    <HStack as="span" xstyle={[styles.tag, outcome === "win" && !isHotseat && styles.tagWin]}>
      {label}
    </HStack>
  );
}

/**
 * A seat with no recorded reason says nothing, except a hotseat's winner: its
 * tag says "Hotseat", so the seat says who won.
 */
function ReportSeat({ isHotseat, seat }: { isHotseat: boolean; seat: MatchHistorySeat }) {
  const faction = getFactionById(seat.factionId);
  const reason =
    seat.reason !== null
      ? formatSeatResultReason(seat.outcome, seat.reason)
      : isHotseat && seat.outcome === "win"
        ? "Won"
        : null;
  return (
    <li {...stylex.props(styles.seat)}>
      {faction ? <FactionLogo factionCode={faction.code} isLabelHidden size={14} /> : null}
      <SeatStamp coId={seat.coId} factionCode={faction?.code ?? null} />
      <b>{seat.userName}</b>
      {reason !== null ? (
        <Text color="inherit" type="inherit" xstyle={styles.seatReason}>
          {reason}
        </Text>
      ) : null}
    </li>
  );
}

/** What a ranked match did to the viewer's rating: `1500 → 1516` over `+16`. */
function RatingMove({ seat }: { seat: MatchHistorySeat | undefined }) {
  if (!seat || seat.ratingBefore === null || seat.ratingAfter === null) {
    return <Figure tone="secondary">Rating pending</Figure>;
  }
  const before = Math.round(seat.ratingBefore);
  const after = Math.round(seat.ratingAfter);
  const change = after - before;
  return (
    <VStack as="span" xstyle={styles.rating}>
      <Figure size="lg">
        {before} → {after}
      </Figure>
      <Figure tone={change >= 0 ? "ok" : "danger"} xstyle={styles.ratingChange}>
        {change >= 0 ? `+${change}` : `−${Math.abs(change)}`}
      </Figure>
    </VStack>
  );
}

const styles = stylex.create({
  report: {
    display: "grid",
    gridTemplateColumns: {
      default: "auto minmax(0, 1fr) auto",
      [pageLayout.phoneMedia]: "auto minmax(0, 1fr)",
    },
    alignItems: "center",
    gap: {
      default: "20px",
      [pageLayout.phoneMedia]: "12px",
    },
    paddingBlock: {
      default: "16px",
      [pageLayout.phoneMedia]: "14px",
    },
    paddingInline: {
      default: "20px",
      [pageLayout.phoneMedia]: "16px",
    },
  },
  postmark: {
    inlineSize: {
      default: "72px",
      [pageLayout.phoneMedia]: "56px",
    },
    blockSize: {
      default: "72px",
      [pageLayout.phoneMedia]: "56px",
    },
  },
  main: {
    display: "grid",
    gap: "6px",
    minInlineSize: 0,
  },
  titleLine: {
    display: "flex",
    alignItems: "center",
    flexWrap: "wrap",
    gap: "8px",
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
  tag: {
    display: "inline-flex",
    alignItems: "center",
    blockSize: "22px",
    paddingInline: "8px",
    borderRadius: "4px",
    backgroundColor: awbrnVars.paperWell,
    color: colorVars["--color-text-secondary"],
    fontSize: awbrnVars.text12,
    fontWeight: 650,
    letterSpacing: "0.02em",
    whiteSpace: "nowrap",
  },
  tagWin: {
    backgroundColor: "var(--color-success-muted)",
    color: awbrnVars.ok,
  },
  meta: {
    color: colorVars["--color-text-secondary"],
    fontSize: awbrnVars.text13_5,
  },
  seats: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    rowGap: "8px",
    columnGap: "20px",
    margin: 0,
    marginBlockStart: "4px",
    padding: 0,
    listStyle: "none",
  },
  seat: {
    display: "inline-flex",
    alignItems: "center",
    gap: "8px",
    fontSize: awbrnVars.text14_5,
  },
  seatReason: {
    color: colorVars["--color-text-secondary"],
    fontSize: textSizeVars["--font-size-sm"],
  },
  end: {
    display: "flex",
    alignItems: "center",
    justifyContent: "flex-end",
    gap: "16px",
    gridColumn: {
      default: "auto",
      [pageLayout.phoneMedia]: "1 / -1",
    },
  },
  keys: {
    display: "flex",
    gap: "8px",
    flexGrow: {
      default: 0,
      [pageLayout.phoneMedia]: 1,
    },
  },
  watch: {
    flexGrow: {
      default: 0,
      [pageLayout.phoneMedia]: 1,
    },
  },
  rating: {
    display: "grid",
    justifyItems: {
      default: "end",
      [pageLayout.phoneMedia]: "start",
    },
    gap: "4px",
    marginInlineEnd: {
      default: 0,
      [pageLayout.phoneMedia]: "auto",
    },
  },
  ratingChange: {
    fontWeight: 650,
  },
});
