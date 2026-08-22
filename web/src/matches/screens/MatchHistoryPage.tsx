import { useSuspenseInfiniteQuery } from "@tanstack/react-query";
import { Banner } from "@astryxdesign/core/Banner";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Grid } from "@astryxdesign/core/Grid";
import { Heading } from "@astryxdesign/core/Heading";
import { Section } from "@astryxdesign/core/Section";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { VisuallyHidden } from "@astryxdesign/core/VisuallyHidden";
import { colorVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import { Download as DownloadIcon } from "pixelarticons/react/Download";
import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { awbwSmallMapAssetPath } from "#/awbw/paths.ts";
import { CoPortrait } from "#/components/CoPortrait.tsx";
import { getCoPortraitByAwbwId, loadCoPortraitCatalog } from "#/components/co_portraits.ts";
import { FactionLogo } from "#/components/FactionLogo.tsx";
import { getFactionById } from "#/factions.ts";
import {
  formatMatchDuration,
  formatSeatResultReason,
  formatVerdict,
  opposingSeats,
  viewerOutcome,
} from "#/matches/match_history.ts";
import { myCompletedMatchesQueryOptions } from "#/matches/matches.queries.ts";
import { matchReplayDownloadPath } from "#/matches/replay_archive.ts";
import type { MatchHistoryEntry, MatchHistorySeat, MatchOutcome } from "#/matches/schemas.ts";
import { awbrnVars, matchHistoryVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { RouterButton, RouterTextLink } from "#/ui/astryx-links.tsx";
import { Thumbnail } from "#/ui/Thumbnail.tsx";
import { MATCH_REPORT_MEDIA_SIZE, TWO_COLUMN_GRID_MIN_WIDTH } from "#/ui/layout.ts";

const dateFormat = new Intl.DateTimeFormat(undefined, {
  day: "numeric",
  month: "short",
  year: "numeric",
});

/**
 * Every battle the player has finished, each one as its own after action
 * report.
 *
 * The page is a stack of debriefs and nothing else: no filters and no tally
 * above them, because the reports are what the player came back for. Each one
 * opens on a band of the armies that fought it, so the matchup reads before the
 * name does, and closes on the two things a finished battle is still good for —
 * opening it again, and taking the replay away.
 */
export function MatchHistoryPage() {
  const historyQuery = useSuspenseInfiniteQuery(myCompletedMatchesQueryOptions());
  const [paginationError, setPaginationError] = useState<string | null>(null);
  const matches = historyQuery.data.pages.flatMap((page) => page.matches);
  const portraitCatalog = loadCoPortraitCatalog();

  async function handleLoadMore(): Promise<void> {
    if (historyQuery.isFetchingNextPage || !historyQuery.hasNextPage) return;

    setPaginationError(null);
    try {
      await historyQuery.fetchNextPage();
    } catch (nextError) {
      setPaginationError(
        nextError instanceof Error ? nextError.message : "More completed games failed to load.",
      );
    }
  }

  return (
    <Section padding={6} variant="transparent">
      <VStack gap={6}>
        <Grid
          align="end"
          columns={{ minWidth: TWO_COLUMN_GRID_MIN_WIDTH, max: 2, repeat: "fit" }}
          gap={4}
        >
          <VStack gap={2}>
            <Heading level={1} type="display-2" xstyle={styles.pageTitle}>
              Completed games
            </Heading>
            <Text color="secondary" type="large">
              Every battle you have finished, and the replay archive it left behind.
            </Text>
          </VStack>
          <HStack gap={2} justify="end" wrap="wrap">
            <RouterButton label="Ongoing games" to="/my/matches" variant="secondary" />
            <RouterButton label="Create match" to="/matches/new" variant="primary" />
          </HStack>
        </Grid>

        {paginationError ? (
          <Banner
            description={paginationError}
            endContent={
              <Button clickAction={handleLoadMore} label="Retry" size="sm" variant="secondary" />
            }
            status="error"
            title="More completed games failed to load"
          />
        ) : null}

        {matches.length === 0 ? (
          <EmptyState
            actions={
              <HStack gap={2} justify="center" wrap="wrap">
                <RouterButton label="Browse lobbies" to="/matches" variant="primary" />
                <RouterButton label="Ongoing games" to="/my/matches" variant="secondary" />
              </HStack>
            }
            description="Play a match to the end and its report lands here, with the replay archive beside it."
            headingLevel={2}
            title="You have not finished a battle yet"
          />
        ) : (
          <Section padding={0} variant="section">
            <VStack as="ul" gap={0}>
              {matches.map((entry, index) => (
                <AfterActionReport
                  entry={entry}
                  hasRuleAbove={index > 0}
                  key={entry.matchId}
                  portraitCatalog={portraitCatalog}
                />
              ))}
            </VStack>
          </Section>
        )}

        {matches.length > 0 && historyQuery.hasNextPage ? (
          <HStack justify="center">
            <Button
              clickAction={handleLoadMore}
              isLoading={historyQuery.isFetchingNextPage}
              label="Load more"
              size="sm"
              variant="secondary"
            />
          </HStack>
        ) : null}
      </VStack>
    </Section>
  );
}

/**
 * One finished battle.
 *
 * The band across the top is the armies that fought, in seat order and in their
 * own colors. It is also the rule between one report and the next, so the stack
 * divides on the matchup rather than on a neutral line. Color is never the only
 * carrier: every army named in the band is named again below it, beside its
 * insignia and its CO.
 */
function AfterActionReport({
  entry,
  hasRuleAbove,
  portraitCatalog,
}: {
  entry: MatchHistoryEntry;
  hasRuleAbove: boolean;
  portraitCatalog: ReturnType<typeof loadCoPortraitCatalog>;
}) {
  const viewerSeats = entry.seats.filter((seat) =>
    entry.viewerSlotIndexes.includes(seat.slotIndex),
  );
  const opponents = opposingSeats(entry.seats, entry.viewerSlotIndexes);
  const outcome = viewerOutcome(viewerSeats);
  const isHotseat = viewerSeats.length > 1;
  const duration = formatMatchDuration(entry.startedAt, entry.completedAt);

  const details = [
    dateFormat.format(new Date(entry.completedAt)),
    duration,
    `Map ${entry.mapId}`,
    entry.settings.fogEnabled ? "Fog on" : "Fog off",
    `${entry.settings.startingFunds.toLocaleString()} funds`,
    entry.isPrivate ? "Private" : null,
  ].filter((detail): detail is string => detail !== null);

  return (
    <VStack as="li" gap={0} xstyle={[styles.report, hasRuleAbove && styles.reportRule]}>
      <ArmyBand seats={entry.seats} />
      <HStack align="start" gap={4} wrap="wrap" xstyle={styles.reportBody}>
        <HStack align="start" gap={3} xstyle={styles.brief}>
          <Thumbnail
            alt={`Map preview for ${entry.name}`}
            label={`${entry.name} map`}
            src={entry.awbwMapId === null ? undefined : awbwSmallMapAssetPath(entry.awbwMapId)}
          />
          <VStack gap={2} xstyle={styles.briefText}>
            <Heading level={2}>
              <RouterTextLink params={{ matchId: entry.matchId }} to="/matches/$matchId">
                {entry.name}
              </RouterTextLink>
            </Heading>
            <HStack align="center" gap={3} wrap="wrap">
              {viewerSeats.map((seat) => (
                <SeatMark
                  isViewer
                  key={seat.slotIndex}
                  portraitCatalog={portraitCatalog}
                  seat={seat}
                />
              ))}
              {opponents.length > 0 ? (
                <Text color="secondary" type="label" xstyle={styles.versus}>
                  vs
                </Text>
              ) : null}
              {opponents.map((seat) => (
                <SeatMark key={seat.slotIndex} portraitCatalog={portraitCatalog} seat={seat} />
              ))}
            </HStack>
            <Text color="secondary" type="label">
              {details.join(" · ")}
            </Text>
          </VStack>
        </HStack>

        <VStack align="end" gap={2} xstyle={styles.verdictColumn}>
          <Verdict isHotseat={isHotseat} outcome={outcome} seats={viewerSeats} />
          {entry.hasReplay ? (
            <Button
              as="a"
              href={matchReplayDownloadPath(entry.matchId)}
              icon={<DownloadIcon />}
              label={`Replay archive for ${entry.name}`}
              size="sm"
              variant="secondary"
            >
              Replay
            </Button>
          ) : (
            <Text color="secondary" type="label">
              No replay stored
            </Text>
          )}
        </VStack>
      </HStack>
    </VStack>
  );
}

/**
 * The armies that fought, as one band across the top of the report.
 *
 * Decorative on purpose: it repeats what the seats below already say in words,
 * so nothing here has to be read to understand the battle.
 */
function ArmyBand({ seats }: { seats: readonly MatchHistorySeat[] }) {
  return (
    <HStack aria-hidden="true" gap={0} xstyle={styles.band}>
      {seats.map((seat) => {
        const faction = getFactionById(seat.factionId);
        return (
          <HStack
            as="span"
            gap={0}
            key={seat.slotIndex}
            xstyle={[
              styles.bandSegment,
              faction && styles.factionBandSegment(`var(--color-faction-${faction.code}-accent)`),
            ]}
          />
        );
      })}
    </HStack>
  );
}

/** One army in the matchup: its insignia, its CO, and the player who ran it. */
function SeatMark({
  isViewer = false,
  portraitCatalog,
  seat,
}: {
  isViewer?: boolean;
  portraitCatalog: ReturnType<typeof loadCoPortraitCatalog>;
  seat: MatchHistorySeat;
}) {
  const faction = getFactionById(seat.factionId);
  const co = seat.coId === null ? null : getCoPortraitByAwbwId(seat.coId);
  const armyName = faction?.displayName ?? "Unknown army";

  return (
    <HStack align="center" gap={1.5}>
      <CoPortrait
        catalog={portraitCatalog}
        coKey={co?.key ?? null}
        fallbackLabel={co?.displayName ?? "No CO"}
        size={MATCH_REPORT_MEDIA_SIZE.portrait}
      />
      <VStack gap={0.5}>
        <Text maxLines={1} type="label" weight={isViewer ? "bold" : undefined}>
          {seat.userName}
          {isViewer ? <VisuallyHidden> (you)</VisuallyHidden> : null}
        </Text>
        <HStack align="center" gap={1}>
          {faction ? (
            <FactionLogo
              factionCode={faction.code}
              isFramed={false}
              isLabelHidden
              size={MATCH_REPORT_MEDIA_SIZE.crest}
            />
          ) : null}
          <Text color="secondary" maxLines={1} type="label">
            {armyName}
            {co ? ` · ${co.displayName}` : ""}
          </Text>
        </HStack>
      </VStack>
    </HStack>
  );
}

/**
 * How the battle ended for the player.
 *
 * A hotseat match the player ran from every seat has no personal verdict, so it
 * reports the army that won instead of claiming a victory nobody contested.
 */
function Verdict({
  isHotseat,
  outcome,
  seats,
}: {
  isHotseat: boolean;
  outcome: MatchOutcome | null;
  seats: readonly MatchHistorySeat[];
}) {
  if (isHotseat) {
    const winner = seats.find((seat) => seat.outcome === "win");
    const winningArmy = winner ? getFactionById(winner.factionId)?.displayName : null;
    return (
      <VStack align="end" gap={0.5}>
        <Text type="label" xstyle={[styles.verdict, styles.verdictNeutral]}>
          Hotseat
        </Text>
        <Text color="secondary" type="label">
          {winningArmy ? `${winningArmy} won` : "No result recorded"}
        </Text>
      </VStack>
    );
  }

  const seat = seats[0];
  return (
    <VStack align="end" gap={0.5}>
      <Text type="label" xstyle={[styles.verdict, verdictStyle(outcome)]}>
        {formatVerdict(outcome)}
      </Text>
      <Text color="secondary" type="label">
        {formatSeatResultReason(outcome, seat?.reason ?? null)}
      </Text>
    </VStack>
  );
}

function verdictStyle(outcome: MatchOutcome | null) {
  switch (outcome) {
    case "win":
      return styles.verdictWin;
    case "loss":
      return styles.verdictLoss;
    case "draw":
      return styles.verdictDraw;
    case null:
      return styles.verdictNeutral;
  }
}

const styles = stylex.create({
  // The signage face sets one long word here, and "COMPLETED" is wider than a
  // phone at the display size. It scales with the viewport below the two-column
  // breakpoint rather than being clipped.
  pageTitle: {
    fontSize: {
      default: null,
      "@media (max-width: 640px)": `clamp(${matchHistoryVars.titleMinimumSize}, ${matchHistoryVars.titleFluidSize}, ${matchHistoryVars.titleMaximumSize})`,
    },
  },
  // Once the armies stack, the separator takes its own line; left at the end of
  // a wrapped row it reads as a stray label rather than as a matchup.
  versus: {
    flexBasis: {
      default: "auto",
      "@media (max-width: 640px)": "100%",
    },
  },
  report: {
    listStyle: "none",
  },
  // The band is the rule between reports, so no second line is drawn above it.
  reportRule: {
    borderTopWidth: "var(--border-width)",
    borderTopStyle: "solid",
    borderTopColor: awbrnVars.colorBorderSoft,
  },
  reportBody: {
    paddingBlock: spacingVars["--spacing-4"],
    paddingInline: spacingVars["--spacing-4"],
  },
  // Wide enough that the map, the matchup, and the metadata hold one column
  // together, and narrow enough that the verdict wraps beneath them on a phone.
  brief: {
    flexGrow: 1,
    flexShrink: 1,
    flexBasis: matchHistoryVars.briefWidth,
    minWidth: 0,
  },
  briefText: {
    minWidth: 0,
  },
  verdictColumn: {
    flexGrow: 1,
    flexShrink: 0,
    flexBasis: "auto",
  },
  band: {
    height: spacingVars["--spacing-1-5"],
    width: "100%",
  },
  bandSegment: {
    flexGrow: 1,
    flexBasis: 0,
    backgroundColor: colorVars["--color-background-muted"],
  },
  factionBandSegment: (backgroundColor: string) => ({
    backgroundColor,
  }),
  // The verdict is a stamp: outlined, filled, and named. It never depends on
  // its fill to be read.
  verdict: {
    borderWidth: "var(--border-width)",
    borderStyle: "solid",
    borderRadius: "var(--radius-inner)",
    paddingBlock: spacingVars["--spacing-1"],
    paddingInline: spacingVars["--spacing-2"],
  },
  // The status color outlines and fills the stamp, but the word is set in ink:
  // a verdict has to clear its own ground, and no status hue does that at
  // label size against its own tint.
  verdictWin: {
    backgroundColor: "var(--color-success-muted)",
    borderColor: "var(--color-success)",
    color: colorVars["--color-text-primary"],
  },
  verdictLoss: {
    backgroundColor: "var(--color-error-muted)",
    borderColor: "var(--color-error)",
    color: colorVars["--color-text-primary"],
  },
  verdictDraw: {
    backgroundColor: "var(--color-warning-muted)",
    borderColor: "var(--color-warning)",
    color: colorVars["--color-text-primary"],
  },
  verdictNeutral: {
    backgroundColor: colorVars["--color-background-muted"],
    borderColor: "var(--color-border-emphasized)",
    color: colorVars["--color-text-primary"],
  },
});
