import { Heading } from "@astryxdesign/core/Heading";
import { Icon } from "@astryxdesign/core/Icon";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { colorVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { Link } from "@tanstack/react-router";
import { useId } from "react";
import { rankedPoolCopy } from "#/matchmaking/ranked_display.ts";
import { headingLooks } from "#/play/headings.ts";
import { MatchThumb } from "#/play/MatchThumb.tsx";
import { Opponents } from "#/play/Opponents.tsx";
import { clockTerms, isClockLow, timeLeftFigure, timeLeftLabel } from "#/play/terms.ts";
import { useListNow } from "#/play/useRankedMatches.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { AirmailBand } from "#/ui/AirmailBand.tsx";
import { RouterButton } from "#/ui/astryx-links.tsx";
import { IconNext } from "#/ui/icons.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { rowLinkStyles } from "#/ui/rowLink.ts";
import { Sheet, SheetHead, SheetRow, SheetRows } from "#/ui/Sheet.tsx";
import { TimeLeft } from "#/ui/TimeLeft.tsx";
import type { ProfileOngoingMatch } from "../schemas.ts";

/** The player's games in progress. */
export function ProfileOngoing({ matches }: { matches: readonly ProfileOngoingMatch[] }) {
  const headingId = useId();
  // The clocks run against the moment, not the moment the page loaded.
  const now = useListNow(matches.map((match) => match.deadlineAt));

  return (
    <Sheet aria-labelledby={headingId}>
      <SheetHead>
        <Heading id={headingId} level={2} xstyle={headingLooks.title}>
          Ongoing
        </Heading>
      </SheetHead>
      <SheetRows>
        {matches.map((match, index) => (
          <SheetRow key={match.matchId}>
            <OngoingRow isLeading={index === 0} match={match} now={now} />
          </SheetRow>
        ))}
      </SheetRows>
    </Sheet>
  );
}

/**
 * One game: thumbnail, title, who is across the table, the pool or terms and
 * the time left. A pairing still owed carries the band and a key instead of
 * the row link; its empty chevron column keeps the clocks in line. Only the
 * leading pairing's key is red, so the page keeps one.
 */
function OngoingRow({
  isLeading,
  match,
  now,
}: {
  isLeading: boolean;
  match: ProfileOngoingMatch;
  now: number;
}) {
  const title = match.pool === null ? match.name : `Ranked · ${match.mapName}`;
  const remaining = match.deadlineAt === null ? null : match.deadlineAt - now;
  // A pairing's window is always short, so it is urgent as the Ranked hub has it.
  const tone = !match.isOwed
    ? "theirs"
    : match.isOwedPairing || (remaining !== null && isClockLow(remaining, match.clock))
      ? "urgent"
      : "default";

  return (
    <VStack xstyle={[styles.row, match.isOwedPairing ? styles.owed : rowLinkStyles.row]}>
      {match.isOwedPairing ? <AirmailBand isOverlay thickness={5} /> : null}
      <MatchThumb mapId={match.mapId} revision={match.mapRevision} size={56} />
      <VStack xstyle={styles.main}>
        {match.isOwedPairing ? (
          <Text color="inherit" type="inherit" xstyle={styles.title}>
            {title}
          </Text>
        ) : (
          <Link
            params={{ matchId: match.matchId }}
            to="/matches/$matchId"
            {...stylex.props(rowLinkStyles.stretch, styles.title)}
          >
            {title}
          </Link>
        )}
        <HStack as="span" xstyle={styles.faces}>
          <Opponents seats={match.opponents} />
        </HStack>
        <HStack as="span" xstyle={styles.line}>
          <Text color="inherit" type="inherit" xstyle={styles.terms}>
            {match.pool === null ? clockTerms(match.clock) : rankedPoolCopy(match.pool).name}
          </Text>
          {remaining !== null ? (
            <TimeLeft
              label={timeLeftLabel(remaining)}
              tone={tone}
              value={timeLeftFigure(remaining)}
            />
          ) : null}
        </HStack>
      </VStack>
      {match.isOwedPairing ? (
        <RouterButton
          label="Confirm pairing"
          params={{ matchId: match.matchId }}
          to="/matches/$matchId"
          variant={isLeading ? "primary" : "secondary"}
          xstyle={styles.key}
        />
      ) : (
        <Icon icon={IconNext} size="md" xstyle={styles.chevron} />
      )}
    </VStack>
  );
}

const styles = stylex.create({
  row: {
    display: "grid",
    gridTemplateColumns: {
      default: "auto minmax(0, 1fr) 18px",
      [pageLayout.phoneMedia]: "auto minmax(0, 1fr) 16px",
    },
    alignItems: "center",
    gap: spacingVars["--spacing-3"],
    paddingBlock: spacingVars["--spacing-3"],
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
  owed: {
    position: "relative",
    paddingBlockStart: awbrnVars.spacing4_5,
  },
  main: {
    display: "grid",
    gap: spacingVars["--spacing-1"],
    minInlineSize: 0,
  },
  title: {
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    color: "inherit",
    fontSize: awbrnVars.text15_5,
    fontWeight: 700,
    lineHeight: 1.2,
    textDecoration: "none",
  },
  faces: {
    display: "flex",
    alignItems: "center",
    minInlineSize: 0,
  },
  line: {
    display: "flex",
    alignItems: "baseline",
    justifyContent: "space-between",
    gap: spacingVars["--spacing-3"],
    minInlineSize: 0,
  },
  terms: {
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    color: colorVars["--color-text-secondary"],
    fontSize: awbrnVars.text13_5,
  },
  chevron: {
    color: awbrnVars.textTertiary,
  },
  // The one red key: under the pairing, across the row.
  key: {
    gridColumn: "1 / -1",
    minBlockSize: {
      default: null,
      [pageLayout.phoneMedia]: "52px",
    },
  },
});
