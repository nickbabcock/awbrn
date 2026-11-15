import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import {
  colorVars,
  radiusVars,
  spacingVars,
  textSizeVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { FactionLogo } from "#/components/FactionLogo.tsx";
import { getFactionById } from "#/factions.ts";
import { formatReportResult } from "#/matches/match_history.ts";
import type { MatchHistorySeat, MatchOutcome } from "#/matches/schemas.ts";
import { SeatStamp } from "#/play/Opponents.tsx";
import { PlayerLink } from "#/players/components/PlayerLink.tsx";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Figure } from "#/ui/Figure.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { Postmark, usePostmarkDate } from "#/ui/Postmark.tsx";

/*
 * The parts of a finished match's report, shared by My games' History and a
 * player's profile so the two lists say the same things the same way.
 */

/** The day the match finished, as a postmark. */
export function FinishedPostmark({
  completedAt,
  xstyle,
}: {
  completedAt: string;
  xstyle?: stylex.StyleXStyles;
}) {
  const completed = usePostmarkDate(completedAt);
  return (
    <Postmark
      figure={completed.day}
      label={`Finished ${completed.long}`}
      ringText={String(completed.year)}
      size={72}
      word={completed.month}
      xstyle={xstyle}
    />
  );
}

/** The result in words, on a tag: a win in green, a place outlined, the rest plain. */
export function ResultTag(props: {
  isHotseat: boolean;
  isViewer: boolean;
  outcome: MatchOutcome | null;
  placement: number | null;
  seatCount: number;
}) {
  const { label, tone } = formatReportResult(props);
  return (
    <HStack
      as="span"
      xstyle={[styles.tag, tone === "win" && styles.tagWin, tone === "place" && styles.tagPlace]}
    >
      {label}
    </HStack>
  );
}

/**
 * One seat: crest, stamp, name and why it ended there. A linked name opens
 * the player's profile; a seat the server played is no one to visit.
 * `isStackedOnPhone` sets the reason under the name on a phone.
 */
export function ReportSeat({
  isLinked,
  isStackedOnPhone = false,
  reason,
  seat,
}: {
  isLinked: boolean;
  isStackedOnPhone?: boolean;
  reason: string | null;
  seat: MatchHistorySeat;
}) {
  const faction = getFactionById(seat.factionId);
  const name = <b {...stylex.props(styles.name)}>{seat.userName}</b>;
  return (
    <li {...stylex.props(styles.seat)}>
      {faction ? <FactionLogo factionCode={faction.code} isLabelHidden size={14} /> : null}
      <SeatStamp coId={seat.coId} factionCode={faction?.code ?? null} />
      <HStack as="span" xstyle={[styles.who, isStackedOnPhone && styles.whoStacked]}>
        {isLinked && seat.userId !== null ? (
          <PlayerLink userId={seat.userId}>{name}</PlayerLink>
        ) : (
          name
        )}
        {reason !== null ? (
          <Text color="inherit" type="inherit" xstyle={styles.reason}>
            {reason}
          </Text>
        ) : null}
      </HStack>
    </li>
  );
}

/**
 * What a ranked match did to the rating: `1500 → 1516` over `+16`. A phone
 * may keep only where it ended (`isBeforeHiddenOnPhone`).
 */
export function RatingMove({
  isBeforeHiddenOnPhone = false,
  seat,
  size = "lg",
  xstyle,
}: {
  isBeforeHiddenOnPhone?: boolean;
  seat: MatchHistorySeat | undefined;
  size?: "md" | "lg";
  xstyle?: stylex.StyleXStyles;
}) {
  if (!seat || seat.ratingBefore === null || seat.ratingAfter === null) {
    return <Figure tone="secondary">Rating pending</Figure>;
  }
  const before = Math.round(seat.ratingBefore);
  const after = Math.round(seat.ratingAfter);
  const change = after - before;
  return (
    <VStack as="span" xstyle={[styles.rating, xstyle]}>
      <Figure size={size} xstyle={styles.ratingLine}>
        <Text
          color="inherit"
          type="inherit"
          xstyle={isBeforeHiddenOnPhone && styles.notPhone}
        >{`${before} → `}</Text>
        {after}
      </Figure>
      <Figure tone={change >= 0 ? "ok" : "danger"} xstyle={styles.ratingChange}>
        {change >= 0 ? `+${change}` : `−${Math.abs(change)}`}
      </Figure>
    </VStack>
  );
}

const styles = stylex.create({
  tag: {
    display: "inline-flex",
    alignItems: "center",
    flexShrink: 0,
    blockSize: "22px",
    paddingInline: spacingVars["--spacing-2"],
    borderRadius: radiusVars["--radius-inner"],
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
  tagPlace: {
    backgroundColor: "transparent",
    boxShadow: `inset 0 0 0 1px ${awbrnVars.ruleStrong}`,
    color: colorVars["--color-text-primary"],
  },
  seat: {
    display: "inline-flex",
    alignItems: "center",
    gap: spacingVars["--spacing-2"],
    minInlineSize: 0,
    fontSize: {
      default: awbrnVars.text14_5,
      [pageLayout.phoneMedia]: awbrnVars.text14,
    },
  },
  who: {
    display: "inline-flex",
    alignItems: "baseline",
    gap: spacingVars["--spacing-2"],
    minInlineSize: 0,
  },
  whoStacked: {
    display: {
      default: "inline-flex",
      [pageLayout.phoneMedia]: "grid",
    },
    gap: {
      default: spacingVars["--spacing-2"],
      [pageLayout.phoneMedia]: spacingVars["--spacing-1"],
    },
  },
  name: {
    display: "block",
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  reason: {
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    color: colorVars["--color-text-secondary"],
    fontSize: {
      default: textSizeVars["--font-size-sm"],
      [pageLayout.phoneMedia]: awbrnVars.text12,
    },
    lineHeight: {
      default: null,
      [pageLayout.phoneMedia]: 1.2,
    },
  },
  rating: {
    display: "grid",
    justifyItems: "end",
    gap: spacingVars["--spacing-1"],
  },
  ratingLine: {
    whiteSpace: "nowrap",
  },
  ratingChange: {
    fontWeight: 650,
  },
  notPhone: {
    display: {
      default: "inline",
      [pageLayout.phoneMedia]: "none",
    },
  },
});
