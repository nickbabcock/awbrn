import { Heading } from "@astryxdesign/core/Heading";
import { Icon } from "@astryxdesign/core/Icon";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { colorVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import { useSuspenseInfiniteQuery } from "@tanstack/react-query";
import * as stylex from "@stylexjs/stylex";
import { Link } from "@tanstack/react-router";
import { useId, useState } from "react";
import { rankedPoolCopy } from "#/matchmaking/ranked_display.ts";
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
import type { MatchHistoryEntry } from "#/matches/schemas.ts";
import { headingLooks } from "#/play/headings.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { ProblemBanner } from "#/ui/EmptySheet.tsx";
import { IconNext } from "#/ui/icons.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { rowLinkStyles } from "#/ui/rowLink.ts";
import { Sheet, SheetFoot, SheetHead, SheetRow, SheetRows } from "#/ui/Sheet.tsx";
import { playerHistoryQueryOptions } from "../players.queries.ts";

/** Rows the profile shows before Show more: fewer on a phone, so COs stay in reach. */
const INLINE_ROWS = 8;
const INLINE_ROWS_PHONE = 4;

/** The player's finished matches, newest first. */
export function ProfileHistory({ isViewer, userId }: { isViewer: boolean; userId: string }) {
  const headingId = useId();
  const history = useSuspenseInfiniteQuery(playerHistoryQueryOptions(userId));
  const [isExpanded, setIsExpanded] = useState(false);
  const entries = history.data.pages.flatMap((page) => page.matches);
  const shown = isExpanded ? entries : entries.slice(0, INLINE_ROWS);
  const hasMore = history.hasNextPage || (!isExpanded && entries.length > INLINE_ROWS);
  const hasMoreOnPhone = !isExpanded && entries.length > INLINE_ROWS_PHONE;

  // The first press shows the rest of the page already loaded; later ones load the next.
  async function showMore() {
    if (!isExpanded) {
      setIsExpanded(true);
      return;
    }
    if (history.hasNextPage && !history.isFetchingNextPage) await history.fetchNextPage();
  }

  return (
    <Sheet aria-labelledby={headingId}>
      <SheetHead>
        <Heading id={headingId} level={2} xstyle={headingLooks.title}>
          History
        </Heading>
      </SheetHead>
      <SheetRows>
        {shown.map((entry, index) => (
          <SheetRow
            key={entry.matchId}
            xstyle={!isExpanded && index >= INLINE_ROWS_PHONE && styles.phoneHidden}
          >
            <HistoryRow entry={entry} isViewer={isViewer} />
          </SheetRow>
        ))}
      </SheetRows>
      {history.isFetchNextPageError ? (
        <ProblemBanner
          action={<Button clickAction={showMore} label="Retry" size="sm" />}
          title="More finished battles failed to load"
        />
      ) : null}
      {hasMore || hasMoreOnPhone ? (
        <HStack xstyle={[styles.more, !hasMore && styles.phoneOnly]}>
          <SheetFoot>
            <Button
              clickAction={showMore}
              isLoading={history.isFetchingNextPage}
              label="Show more"
              xstyle={styles.moreKey}
            />
          </SheetFoot>
        </HStack>
      ) : null}
    </Sheet>
  );
}

/**
 * One finished match. The title is the row's one link, stretched over it;
 * each other player's name is a link of its own above it.
 */
function HistoryRow({ entry, isViewer }: { entry: MatchHistoryEntry; isViewer: boolean }) {
  const ownSeats = entry.seats.filter((seat) => entry.playerSlotIndexes.includes(seat.slotIndex));
  const isHotseat = ownSeats.length > 1;
  const isRanked = entry.pool !== null;
  const shared = sharedSeatReason(entry.seats);
  // The pool and the rating say ranked once; an unranked match names its map
  // unless it is named for it.
  const where = isRanked
    ? rankedPoolCopy(entry.pool!).name
    : entry.mapName === entry.name
      ? null
      : entry.mapName;
  const meta = [where, shared === null ? null : formatSeatResultReason(null, shared)]
    .filter((part) => part !== null)
    .join(" · ");

  return (
    <VStack xstyle={[styles.row, rowLinkStyles.row]}>
      <FinishedPostmark completedAt={entry.completedAt} xstyle={styles.postmark} />
      <HStack xstyle={styles.titleLine}>
        <Link
          params={{ matchId: entry.matchId }}
          to="/matches/$matchId"
          {...stylex.props(rowLinkStyles.stretch, styles.title)}
        >
          {isRanked ? entry.mapName : entry.name}
        </Link>
        <HStack as="span" xstyle={styles.sub}>
          <ResultTag
            isHotseat={isHotseat}
            isViewer={isViewer}
            outcome={viewerOutcome(ownSeats)}
            placement={ownSeats[0]?.placement ?? null}
            seatCount={entry.seats.length}
          />
          {meta ? (
            <Text color="inherit" type="inherit" xstyle={styles.meta}>
              {meta}
            </Text>
          ) : null}
        </HStack>
      </HStack>
      <ul aria-label="Seats" {...stylex.props(styles.seats)}>
        {[...ownSeats, ...opposingSeats(entry.seats, entry.playerSlotIndexes)].map((seat) => (
          <ReportSeat
            isLinked={!entry.playerSlotIndexes.includes(seat.slotIndex)}
            isStackedOnPhone
            key={seat.slotIndex}
            reason={seatReason(seat, { isHotseat, shared })}
            seat={seat}
          />
        ))}
      </ul>
      {isRanked && !isHotseat ? (
        <RatingMove isBeforeHiddenOnPhone seat={ownSeats[0]} size="md" xstyle={styles.rating} />
      ) : null}
      <Icon icon={IconNext} size="md" xstyle={styles.chevron} />
    </VStack>
  );
}

const styles = stylex.create({
  row: {
    display: "grid",
    gridTemplateColumns: "auto minmax(0, 1fr) auto auto",
    gridTemplateAreas: {
      default: '"stamp title rate chev" "stamp seats rate chev"',
      [pageLayout.phoneMedia]:
        '"stamp title rate chev" "stamp sub rate chev" "seats seats seats seats"',
    },
    alignItems: "center",
    columnGap: {
      default: spacingVars["--spacing-4"],
      [pageLayout.phoneMedia]: awbrnVars.spacing2_5,
    },
    rowGap: {
      default: spacingVars["--spacing-2"],
      [pageLayout.phoneMedia]: spacingVars["--spacing-1-5"],
    },
    paddingBlock: awbrnVars.spacing3_5,
    paddingInlineStart: {
      default: spacingVars["--spacing-5"],
      [pageLayout.phoneMedia]: spacingVars["--spacing-4"],
    },
    paddingInlineEnd: {
      default: spacingVars["--spacing-4"],
      [pageLayout.phoneMedia]: spacingVars["--spacing-3"],
    },
    color: colorVars["--color-text-primary"],
  },
  phoneHidden: {
    display: {
      default: "list-item",
      [pageLayout.phoneMedia]: "none",
    },
  },
  postmark: {
    gridArea: "stamp",
    alignSelf: {
      default: "center",
      [pageLayout.phoneMedia]: "start",
    },
    inlineSize: {
      default: "56px",
      [pageLayout.phoneMedia]: "48px",
    },
    blockSize: {
      default: "56px",
      [pageLayout.phoneMedia]: "48px",
    },
  },
  // On a phone the title and the result take a line each.
  titleLine: {
    gridArea: "title",
    display: {
      default: "flex",
      [pageLayout.phoneMedia]: "contents",
    },
    alignItems: "center",
    gap: awbrnVars.spacing2_5,
    minInlineSize: 0,
  },
  title: {
    gridArea: "title",
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    color: "inherit",
    fontSize: awbrnVars.text16,
    fontWeight: 700,
    lineHeight: 1.25,
    textDecoration: "none",
  },
  sub: {
    gridArea: "sub",
    display: "flex",
    alignItems: "center",
    gap: awbrnVars.spacing2_5,
    minInlineSize: 0,
    flexShrink: 0,
  },
  meta: {
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    color: colorVars["--color-text-secondary"],
    fontSize: awbrnVars.text13_5,
  },
  seats: {
    gridArea: "seats",
    display: {
      default: "flex",
      [pageLayout.phoneMedia]: "grid",
    },
    flexWrap: "wrap",
    gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
    alignItems: "center",
    rowGap: {
      default: spacingVars["--spacing-1-5"],
      [pageLayout.phoneMedia]: spacingVars["--spacing-2"],
    },
    columnGap: {
      default: spacingVars["--spacing-5"],
      [pageLayout.phoneMedia]: spacingVars["--spacing-3"],
    },
    margin: 0,
    marginBlockStart: {
      default: 0,
      [pageLayout.phoneMedia]: spacingVars["--spacing-1"],
    },
    padding: 0,
    listStyle: "none",
  },
  rating: {
    gridArea: "rate",
    alignSelf: {
      default: "center",
      [pageLayout.phoneMedia]: "start",
    },
  },
  chevron: {
    gridArea: "chev",
    color: awbrnVars.textTertiary,
  },
  more: {
    display: "block",
  },
  phoneOnly: {
    display: {
      default: "none",
      [pageLayout.phoneMedia]: "block",
    },
  },
  moreKey: {
    marginInline: "auto",
    inlineSize: {
      default: "auto",
      [pageLayout.phoneMedia]: "100%",
    },
  },
});
