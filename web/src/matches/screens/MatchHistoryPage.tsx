import { Icon } from "@astryxdesign/core/Icon";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { colorVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import { useSuspenseInfiniteQuery } from "@tanstack/react-query";
import * as stylex from "@stylexjs/stylex";
import {
  FinishedPostmark,
  RatingMove,
  ReportSeat,
  ResultTag,
} from "#/matches/components/ReportParts.tsx";
import {
  formatSeatResultReason,
  opposingSeats,
  seatReason,
  sharedSeatReason,
  viewerOutcome,
} from "#/matches/match_history.ts";
import { myCompletedMatchesQueryOptions } from "#/matches/matches.queries.ts";
import { matchReplayDownloadPath } from "#/matches/replay_archive.ts";
import type { MatchHistoryEntry } from "#/matches/schemas.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { RouterButton } from "#/ui/astryx-links.tsx";
import { EmptySheet, ProblemBanner } from "#/ui/EmptySheet.tsx";
import { IconDownload } from "#/ui/icons.tsx";
import { Page, PageHeader } from "#/ui/Page.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { Sheet, SheetRow, SheetRows } from "#/ui/Sheet.tsx";

export function MatchHistoryPage() {
  const historyQuery = useSuspenseInfiniteQuery(myCompletedMatchesQueryOptions());
  const matches = historyQuery.data.pages.flatMap((page) => page.matches);

  async function handleLoadMore(): Promise<void> {
    if (historyQuery.isFetchingNextPage || !historyQuery.hasNextPage) return;
    await historyQuery.fetchNextPage();
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

      {historyQuery.isFetchNextPageError ? (
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
  const viewerSeats = entry.seats.filter((seat) =>
    entry.playerSlotIndexes.includes(seat.slotIndex),
  );
  const opponents = opposingSeats(entry.seats, entry.playerSlotIndexes);
  const isHotseat = viewerSeats.length > 1;
  const isRanked = entry.pool !== null;
  const title = isRanked ? `Ranked · ${entry.mapName}` : entry.name;

  const shared = sharedSeatReason(entry.seats);

  // A ranked match is titled by its map, and a match named for its map says
  // it once. The result is the tag and each seat says why it ended, so the
  // line under the title is the map and a reason every seat shares.
  const meta =
    [
      isRanked || entry.mapName === entry.name ? null : entry.mapName,
      shared === null ? null : formatSeatResultReason(null, shared),
    ]
      .filter((part) => part !== null)
      .join(" · ") || null;

  return (
    <VStack xstyle={styles.report}>
      <FinishedPostmark completedAt={entry.completedAt} xstyle={styles.postmark} />
      <VStack xstyle={styles.main}>
        <HStack xstyle={styles.titleLine}>
          <Text color="inherit" type="inherit" xstyle={styles.title}>
            {title}
          </Text>
          <ResultTag
            isHotseat={isHotseat}
            isViewer
            outcome={viewerOutcome(viewerSeats)}
            placement={viewerSeats[0]?.placement ?? null}
            seatCount={entry.seats.length}
          />
        </HStack>
        {meta !== null ? (
          <Text as="div" color="inherit" display="block" type="inherit" xstyle={styles.meta}>
            {meta}
          </Text>
        ) : null}
        <ul aria-label="Seats" {...stylex.props(styles.seats)}>
          {[...viewerSeats, ...opponents].map((seat) => (
            <ReportSeat
              isLinked
              isStackedOnPhone
              key={seat.slotIndex}
              reason={seatReason(seat, { isHotseat, shared })}
              seat={seat}
            />
          ))}
        </ul>
      </VStack>
      <HStack xstyle={styles.end}>
        {isRanked && !isHotseat ? (
          <RatingMove seat={viewerSeats[0]} xstyle={styles.rating} />
        ) : null}
        <HStack as="span" xstyle={styles.keys}>
          {entry.hasReplay ? (
            <>
              <Button
                as="a"
                href={matchReplayDownloadPath(entry.matchId)}
                icon={<Icon icon={IconDownload} />}
                isIconOnly
                label={`Download the replay archive for ${title}`}
                tooltip="Replay archive"
              />
              <RouterButton
                aria-label={`Watch ${title}`}
                label="Watch"
                params={{ matchId: entry.matchId }}
                to="/matches/$matchId"
                xstyle={styles.watch}
              />
            </>
          ) : null}
        </HStack>
      </HStack>
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
    // On a phone the seats run under the postmark too, as on a player's profile.
    gridTemplateAreas: {
      default: null,
      [pageLayout.phoneMedia]: '"stamp title" "stamp meta" "seats seats" "end end"',
    },
    alignItems: "center",
    columnGap: {
      default: "20px",
      [pageLayout.phoneMedia]: "12px",
    },
    rowGap: {
      default: "20px",
      [pageLayout.phoneMedia]: "6px",
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
    gridArea: {
      default: null,
      [pageLayout.phoneMedia]: "stamp",
    },
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
    display: {
      default: "grid",
      [pageLayout.phoneMedia]: "contents",
    },
    gap: "6px",
    minInlineSize: 0,
  },
  titleLine: {
    gridArea: {
      default: null,
      [pageLayout.phoneMedia]: "title",
    },
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
  meta: {
    gridArea: {
      default: null,
      [pageLayout.phoneMedia]: "meta",
    },
    color: colorVars["--color-text-secondary"],
    fontSize: awbrnVars.text13_5,
  },
  seats: {
    gridArea: {
      default: null,
      [pageLayout.phoneMedia]: "seats",
    },
    display: {
      default: "flex",
      [pageLayout.phoneMedia]: "grid",
    },
    flexWrap: "wrap",
    gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
    alignItems: "center",
    rowGap: "8px",
    columnGap: {
      default: "20px",
      [pageLayout.phoneMedia]: spacingVars["--spacing-3"],
    },
    margin: 0,
    marginBlockStart: "4px",
    padding: 0,
    listStyle: "none",
  },
  end: {
    display: "flex",
    alignItems: "center",
    justifyContent: "flex-end",
    gap: "16px",
    gridArea: {
      default: null,
      [pageLayout.phoneMedia]: "end",
    },
    marginBlockStart: {
      default: 0,
      [pageLayout.phoneMedia]: "6px",
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
    justifyItems: {
      default: "end",
      [pageLayout.phoneMedia]: "start",
    },
    marginInlineEnd: {
      default: 0,
      [pageLayout.phoneMedia]: "auto",
    },
  },
});
