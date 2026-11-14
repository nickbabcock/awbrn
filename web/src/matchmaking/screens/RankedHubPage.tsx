/*
 * Async ranked play is a standing arrangement, not a queue: the player says
 * how many games ranked may hold, and the server keeps that many running.
 *
 * This surface describes the player, never the pool: no count of who else is
 * seeking and no estimated wait. Elapsed wait is a fact about this player.
 */

import { Heading } from "@astryxdesign/core/Heading";
import { Icon } from "@astryxdesign/core/Icon";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { colorVars, textSizeVars } from "@astryxdesign/core/theme/tokens.stylex";
import { VisuallyHidden } from "@astryxdesign/core/VisuallyHidden";
import { useMutation, useQuery, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import * as stylex from "@stylexjs/stylex";
import { Link } from "@tanstack/react-router";
import { useEffect, useId, useMemo, useRef, useState, type ReactNode } from "react";
import { useAppSession } from "#/auth/useAppSession.ts";
import { FactionLogo } from "#/components/FactionLogo.tsx";
import { getFactionById } from "#/factions.ts";
import { Countdown } from "#/matchmaking/components/Countdown.tsx";
import { startRankedSeekFn, stopRankedSeekFn } from "#/matchmaking/matchmaking.functions.ts";
import { rankedKeys } from "#/matchmaking/matchmaking.keys.ts";
import {
  RANKED_POLL_INTERVAL_MS,
  rankedOverviewQueryOptions,
  rankedStandingsQueryOptions,
} from "#/matchmaking/matchmaking.queries.ts";
import { HARD_MAX_ACTIVE_MATCHES } from "#/matchmaking/matchmaking.ts";
import {
  formatRating,
  leadingOwedPairingId,
  OPEN_RANKED_POOLS,
  rankedPoolCopy,
} from "#/matchmaking/ranked_display.ts";
import type {
  RankedInPlaySummary,
  RankedPendingSummary,
  RankedPoolSnapshot,
} from "#/matchmaking/ranked_overview.server.ts";
import { DAY_MS, formatClockDuration, HOUR_MS } from "#/matches/match_clock.ts";
import { myMatchesQueryOptions } from "#/matches/matches.queries.ts";
import { needsViewerAction } from "#/matches/my_matches.ts";
import type { MyMatchSummary, RankedPool } from "#/matches/schemas.ts";
import { headingLooks } from "#/play/headings.ts";
import { MatchThumb } from "#/play/MatchThumb.tsx";
import { Postage } from "#/play/Postage.tsx";
import { timeLeftFigure, timeLeftLabel } from "#/play/terms.ts";
import { useListNow } from "#/play/useRankedMatches.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { RouterButton } from "#/ui/astryx-links.tsx";
import { EmptySheet, ProblemBanner } from "#/ui/EmptySheet.tsx";
import { Figure, figureStyles } from "#/ui/Figure.tsx";
import { IconMinus, IconNext, IconPlus } from "#/ui/icons.tsx";
import { AirmailBand } from "#/ui/AirmailBand.tsx";
import { Page, PageHeader } from "#/ui/Page.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { Sheet, SheetFoot, SheetHead } from "#/ui/Sheet.tsx";
import { TimeLeft } from "#/ui/TimeLeft.tsx";
import { formatCompactDuration } from "#/utils/time.ts";

/** How long a stepper press rests before the new capacity is saved. */
const CAPACITY_SAVE_DELAY_MS = 700;

export function RankedHubPage({
  pool,
  onSelectPool,
}: {
  pool: RankedPool;
  onSelectPool: (pool: RankedPool) => void;
}) {
  const { data: overview } = useSuspenseQuery({
    ...rankedOverviewQueryOptions(),
    // A pairing is made by the server and announced by nothing, so the hub
    // asks again on a slow interval and whenever the player returns to the tab.
    refetchInterval: RANKED_POLL_INTERVAL_MS,
    refetchOnWindowFocus: true,
  });
  const snapshot = overview.pools.find((entry) => entry.pool === pool) ?? overview.pools[0]!;

  return (
    <Page>
      <PageHeader title="Play" />
      {/* One open pool needs no selector; one appears when a second opens. */}
      {OPEN_RANKED_POOLS.length > 1 ? (
        <SegmentedControl
          label="Ranked pool"
          onChange={(value) => onSelectPool(value as RankedPool)}
          value={snapshot.pool}
        >
          {OPEN_RANKED_POOLS.map((entry) => (
            <SegmentedControlItem key={entry} label={rankedPoolCopy(entry).name} value={entry} />
          ))}
        </SegmentedControl>
      ) : null}
      <VStack xstyle={styles.grid}>
        <VStack xstyle={styles.column}>
          <PoolSheet
            isEmailVerified={overview.isEmailVerified}
            season={overview.season}
            snapshot={snapshot}
          />
          {overview.isEmailVerified ? null : <VerifyEmail />}
        </VStack>
        <Standings pool={snapshot.pool} />
      </VStack>
    </Page>
  );
}

function VerifyEmail() {
  return (
    <ProblemBanner
      description="Ranked play needs a verified email address, so that one player cannot hold several places in the pool."
      status="info"
      title="Verify your email to play ranked"
    />
  );
}

function PoolSheet({
  isEmailVerified,
  season,
  snapshot,
}: {
  isEmailVerified: boolean;
  season: { number: number; endsAt: string } | null;
  snapshot: RankedPoolSnapshot;
}) {
  const headingId = useId();
  const queryClient = useQueryClient();
  const isSeeking = snapshot.seek !== null;
  const saved = snapshot.seek?.maxActiveMatches ?? null;
  const [capacity, setCapacity] = useState(saved ?? Math.min(3, HARD_MAX_ACTIVE_MATCHES));
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (saved !== null) setCapacity(saved);
  }, [saved]);

  const refresh = () => queryClient.invalidateQueries({ queryKey: rankedKeys.overview() });
  const seekMutation = useMutation({
    mutationFn: (input: { maxActiveMatches: number }) =>
      startRankedSeekFn({
        data: { pool: snapshot.pool, maxActiveMatches: input.maxActiveMatches },
      }),
    // The stepper goes back to the saved arrangement: a value the server
    // refused is not the value in force, and a value that stays different
    // from the server would start the autosave again without end.
    onError: (mutationError: Error) => {
      setError(mutationError.message);
      if (saved !== null) setCapacity(saved);
    },
    onSuccess: () => {
      setError(null);
      return refresh();
    },
  });
  const stopMutation = useMutation({
    mutationFn: () => stopRankedSeekFn({ data: { pool: snapshot.pool } }),
    onError: (mutationError: Error) => setError(mutationError.message),
    onSuccess: () => {
      setError(null);
      return refresh();
    },
  });

  // A stepper press changes the standing arrangement, so it saves itself. The
  // delay lets a player walk the number from 1 to 5 with one write at the end.
  const saveCapacityRef = useRef(seekMutation.mutate);
  saveCapacityRef.current = seekMutation.mutate;
  const isSavingCapacity = seekMutation.isPending;
  useEffect(() => {
    if (saved === null || saved === capacity || isSavingCapacity) return;
    const timer = setTimeout(
      () => saveCapacityRef.current({ maxActiveMatches: capacity }),
      CAPACITY_SAVE_DELAY_MS,
    );
    return () => clearTimeout(timer);
  }, [capacity, saved, isSavingCapacity]);

  const leadingOwedId = leadingOwedPairingId(snapshot.pending);
  const seekingSlots = isSeeking
    ? Math.max(0, capacity - snapshot.inPlay.length - snapshot.pending.length)
    : 0;
  const isBusy = seekMutation.isPending || stopMutation.isPending;
  const seasonLine = season
    ? `Season ${season.number} · ${formatCompactDuration(Date.parse(season.endsAt) - Date.now())} left`
    : "No season running";

  let slotNumber = 0;

  return (
    <Sheet aria-labelledby={headingId}>
      <SheetHead>
        <HStack as="span" xstyle={styles.poolHead}>
          <Heading id={headingId} level={2} xstyle={headingLooks.title}>
            {rankedPoolCopy(snapshot.pool).name}
          </Heading>
        </HStack>
        <Postage xstyle={styles.season}>{seasonLine}</Postage>
      </SheetHead>
      {snapshot.inPlay.length + snapshot.pending.length + seekingSlots > 0 ? (
        <ol {...stylex.props(styles.slots)}>
          {snapshot.inPlay.map((game) => (
            <li key={game.matchId}>
              <PlaySlot game={game} number={++slotNumber} />
            </li>
          ))}
          {snapshot.pending.map((pairing) => (
            <li key={pairing.matchId}>
              <PairingSlot
                isLeading={pairing.matchId === leadingOwedId}
                number={++slotNumber}
                pairing={pairing}
              />
            </li>
          ))}
          {Array.from({ length: seekingSlots }, (_, index) => (
            <li key={`seek-${index}`}>
              <SeekSlot createdAt={snapshot.seek!.createdAt} number={++slotNumber} />
            </li>
          ))}
        </ol>
      ) : null}
      {error ? (
        <VStack xstyle={styles.problem}>
          <ProblemBanner title={error} />
        </VStack>
      ) : null}
      {isEmailVerified ? (
        <SheetFoot>
          <HStack as="span" aria-label="Games at a time" role="group" xstyle={styles.stepper}>
            <Button
              icon={<Icon icon={IconMinus} />}
              isDisabled={isBusy || capacity <= 1}
              isIconOnly
              label="Fewer games at a time"
              onClick={() => setCapacity((value) => clamp(value - 1))}
            />
            <Figure size="lg" xstyle={styles.capacity}>
              <output aria-live="polite">{capacity}</output>
            </Figure>
            <Button
              icon={<Icon icon={IconPlus} />}
              isDisabled={isBusy || capacity >= HARD_MAX_ACTIVE_MATCHES}
              isIconOnly
              label="More games at a time"
              onClick={() => setCapacity((value) => clamp(value + 1))}
            />
            <Text color="inherit" type="inherit" xstyle={styles.stepWord}>
              at a time
            </Text>
          </HStack>
          {isSeeking ? (
            <Button
              clickAction={() => stopMutation.mutate()}
              isLoading={stopMutation.isPending}
              label="Stop seeking"
            />
          ) : (
            <Button
              clickAction={() => seekMutation.mutate({ maxActiveMatches: capacity })}
              isLoading={seekMutation.isPending}
              label="Start seeking"
              variant={leadingOwedId !== null ? "secondary" : "primary"}
            />
          )}
        </SheetFoot>
      ) : null}
    </Sheet>
  );
}

/** The turn clock of one ranked game, read from the viewer's list of matches. */
function matchesQueryFacts(
  matches: readonly MyMatchSummary[],
  matchId: string,
): { deadlineAt: number | null; isOwed: boolean } {
  const match = matches.find((entry) => entry.matchId === matchId);
  if (!match || match.turnDeadlineAt === null) return { deadlineAt: null, isOwed: false };
  return { deadlineAt: Date.parse(match.turnDeadlineAt), isOwed: needsViewerAction(match) };
}

function clamp(value: number): number {
  return Math.min(HARD_MAX_ACTIVE_MATCHES, Math.max(1, Math.round(value)));
}

/** A game in play: the opponent over the map, whose clock is running, and the way in. */
function PlaySlot({ game, number }: { game: RankedInPlaySummary; number: number }) {
  const { data: mine } = useQuery(myMatchesQueryOptions());
  const facts = useMemo(
    () => matchesQueryFacts(mine?.matches ?? [], game.matchId),
    [game.matchId, mine],
  );
  const now = useListNow([facts.deadlineAt]);
  const remaining = facts.deadlineAt === null ? null : facts.deadlineAt - now;

  return (
    <Link
      params={{ matchId: game.matchId }}
      to="/matches/$matchId"
      {...stylex.props(styles.slot, styles.slotPlay)}
    >
      <Text color="inherit" type="inherit" xstyle={[figureStyles.figure, styles.slotNumber]}>
        {number}
      </Text>
      <MatchThumb isEager mapId={game.mapId} phoneSize={48} revision={game.mapRevision} size={56} />
      <VStack as="span" xstyle={styles.slotText}>
        <b {...stylex.props(styles.slotTitle)}>{game.opponentName ?? game.mapName}</b>
        <HStack as="span" xstyle={styles.slotMap}>
          {game.mapName}
        </HStack>
      </VStack>
      <HStack as="span" xstyle={styles.slotTime}>
        {remaining !== null ? (
          <TimeLeft
            label={timeLeftLabel(remaining)}
            tone={facts.isOwed ? "default" : "theirs"}
            value={timeLeftFigure(remaining)}
          />
        ) : null}
      </HStack>
      <Icon icon={IconNext} size="md" xstyle={[styles.chevron, styles.notPhone]} />
    </Link>
  );
}

/**
 * A pairing: the map the server chose, the army, the window, and Confirm
 * pairing. Owed, it lifts with the band; once ready it waits like a game.
 * Only the leading owed pairing's key is red; the rest are quiet.
 */
function PairingSlot({
  isLeading,
  number,
  pairing,
}: {
  isLeading: boolean;
  number: number;
  pairing: RankedPendingSummary;
}) {
  const queryClient = useQueryClient();
  const faction = getFactionById(pairing.factionId);
  // A window that runs out owes nothing: the band and the key go at once, and
  // the hub asks for the pool again rather than waiting for its next poll.
  const [isClosed, setIsClosed] = useState(false);
  const isOwed = !pairing.isReady && !isClosed;

  return (
    <VStack xstyle={[styles.slot, isOwed ? styles.slotOwed : styles.slotPlay]}>
      {isOwed ? <AirmailBand isOverlay thickness={5} /> : null}
      <Text color="inherit" type="inherit" xstyle={[figureStyles.figure, styles.slotNumber]}>
        {number}
      </Text>
      <MatchThumb
        isEager
        mapId={pairing.mapId}
        phoneSize={48}
        revision={pairing.mapRevision}
        size={56}
      />
      <VStack as="span" xstyle={styles.slotText}>
        <b {...stylex.props(styles.slotTitle)}>{pairing.mapName}</b>
        <HStack as="span" xstyle={styles.slotMap}>
          {faction ? <FactionLogo factionCode={faction.code} size={14} /> : null}
          {isOwed ? null : (
            <Text color="secondary" type="supporting">
              Ready
            </Text>
          )}
        </HStack>
      </VStack>
      <HStack as="span" xstyle={styles.slotTime}>
        <Countdown
          deadlineAt={pairing.deadlineAt}
          onExpire={() => {
            setIsClosed(true);
            void queryClient.invalidateQueries({ queryKey: rankedKeys.overview() });
          }}
          tone={isOwed ? "urgent" : "theirs"}
        />
      </HStack>
      <RouterButton
        label={isOwed ? "Confirm pairing" : "Open"}
        params={{ matchId: pairing.matchId }}
        to="/matches/$matchId"
        variant={isOwed && isLeading ? "primary" : "secondary"}
        xstyle={isOwed ? styles.owedKey : styles.notPhone}
      />
    </VStack>
  );
}

/** The seek still looking for this slot's opponent, and how long it has looked. */
function SeekSlot({ createdAt, number }: { createdAt: string; number: number }) {
  // Under an hour the figure reads to the second (`3m 42s`), so it ticks each second.
  const now = useListNow([Date.parse(createdAt) + HOUR_MS, Date.parse(createdAt) + DAY_MS]);
  return (
    <VStack xstyle={[styles.slot, styles.slotSearch]}>
      <Text color="inherit" type="inherit" xstyle={[figureStyles.figure, styles.slotNumber]}>
        {number}
      </Text>
      <VStack as="span" aria-hidden="true" xstyle={styles.slotWell} />
      <VStack as="span" xstyle={styles.slotText}>
        <b {...stylex.props(styles.seeking)}>
          <VStack as="span" aria-hidden="true" xstyle={styles.dot} />
          Seeking
        </b>
      </VStack>
      <Figure tone="primary">
        <VisuallyHidden>for </VisuallyHidden>
        {formatClockDuration(now - Date.parse(createdAt))}
      </Figure>
    </VStack>
  );
}

/** A phone drops the games column. */
function Standings({ pool }: { pool: RankedPool }) {
  const headingId = useId();
  const session = useAppSession();
  const { data } = useQuery(rankedStandingsQueryOptions(pool));
  if (!data) return null;
  const viewer = data.viewer;

  if (data.entries.length === 0 && viewer === null) {
    return <EmptySheet title="No confirmed ratings yet" />;
  }

  return (
    <Sheet aria-labelledby={headingId}>
      <SheetHead>
        <Heading id={headingId} level={2} xstyle={headingLooks.title}>
          Standings
        </Heading>
      </SheetHead>
      <table {...stylex.props(styles.table)}>
        <thead>
          <tr>
            <th {...stylex.props(styles.th, styles.placeCell)} scope="col">
              <VisuallyHidden>Place</VisuallyHidden>
            </th>
            <th {...stylex.props(styles.th)} scope="col">
              <VisuallyHidden>Player</VisuallyHidden>
            </th>
            <th {...stylex.props(styles.th, styles.ratingCell)} scope="col">
              Rating
            </th>
            <th {...stylex.props(styles.th, styles.gamesCell, styles.notPhoneCell)} scope="col">
              Games
            </th>
          </tr>
        </thead>
        <tbody>
          {data.entries.map((entry) => (
            <StandingRow
              isProvisional={entry.isProvisional}
              isViewer={entry.isViewer}
              key={entry.userId}
              name={entry.name}
              place={entry.rank}
              rating={entry.rating}
              ratedMatches={entry.ratedMatches}
            />
          ))}
          {/* Below the listed places, or with none yet, the viewer still sees theirs. */}
          {viewer && session ? (
            <StandingRow
              isProvisional={viewer.isProvisional}
              isViewer
              name={session.user.name}
              place={
                viewer.rank ?? (
                  <>
                    <Text aria-hidden="true" color="inherit" type="inherit">
                      –
                    </Text>
                    <VisuallyHidden>Not placed yet</VisuallyHidden>
                  </>
                )
              }
              rating={viewer.rating}
              ratedMatches={viewer.ratedMatches}
            />
          ) : null}
        </tbody>
      </table>
    </Sheet>
  );
}

function StandingRow({
  isProvisional,
  isViewer,
  name,
  place,
  rating,
  ratedMatches,
}: {
  isProvisional: boolean;
  isViewer: boolean;
  name: string;
  place: ReactNode;
  rating: number;
  ratedMatches: number;
}) {
  return (
    <tr {...stylex.props(isViewer && styles.youRow)}>
      <td {...stylex.props(styles.td, styles.placeCell, figureStyles.figure)}>{place}</td>
      <td {...stylex.props(styles.td)}>
        <HStack as="span" xstyle={styles.player}>
          <b>{name}</b>
          {isViewer ? (
            <HStack as="span" xstyle={styles.you}>
              YOU
            </HStack>
          ) : null}
        </HStack>
      </td>
      <td {...stylex.props(styles.td, styles.ratingCell, figureStyles.figure, styles.rating)}>
        {formatRating(rating, isProvisional)}
      </td>
      <td
        {...stylex.props(
          styles.td,
          styles.gamesCell,
          figureStyles.figure,
          styles.games,
          styles.notPhoneCell,
        )}
      >
        {ratedMatches}
      </td>
    </tr>
  );
}

const styles = stylex.create({
  grid: {
    display: "grid",
    gridTemplateColumns: {
      default: "minmax(0, 1fr) 420px",
      [pageLayout.belowRailMedia]: "minmax(0, 1fr)",
    },
    alignItems: "start",
    gap: {
      default: "24px",
      [pageLayout.phoneMedia]: "16px",
    },
  },
  column: {
    display: "grid",
    gap: "16px",
    minInlineSize: 0,
  },
  poolHead: {
    alignItems: "baseline",
    gap: "12px",
  },
  season: {
    fontSize: awbrnVars.text12,
  },
  slots: {
    display: "grid",
    gap: "8px",
    margin: 0,
    padding: {
      default: "0 12px 12px",
      [pageLayout.phoneMedia]: "0 8px 8px",
    },
    listStyle: "none",
  },
  slot: {
    position: "relative",
    display: "grid",
    gridTemplateColumns: {
      default: "18px 56px minmax(0, 1fr) auto auto",
      [pageLayout.phoneMedia]: "14px 48px minmax(0, 1fr) auto",
    },
    alignItems: "center",
    gap: {
      default: "12px",
      [pageLayout.phoneMedia]: "10px",
    },
    minBlockSize: {
      default: "72px",
      [pageLayout.phoneMedia]: "64px",
    },
    paddingBlock: "8px",
    paddingInlineStart: {
      default: "8px",
      [pageLayout.phoneMedia]: "6px",
    },
    paddingInlineEnd: {
      default: "12px",
      [pageLayout.phoneMedia]: "10px",
    },
    borderRadius: "6px",
    overflow: "hidden",
    color: colorVars["--color-text-primary"],
    textDecoration: "none",
  },
  slotPlay: {
    backgroundColor: awbrnVars.paper,
    boxShadow: {
      default: `inset 0 0 0 1px ${awbrnVars.rule}`,
      ":hover": { default: null, [pageLayout.hoverMedia]: `inset 0 0 0 1px ${awbrnVars.ink900}` },
      ":active": `inset 0 0 0 1px ${awbrnVars.ink900}`,
    },
  },
  slotOwed: {
    paddingBlockStart: "14px",
    paddingBlockEnd: {
      default: "8px",
      [pageLayout.phoneMedia]: "10px",
    },
    backgroundColor: awbrnVars.sheet,
    boxShadow: `inset 0 0 0 1px ${awbrnVars.ruleStrong}, ${awbrnVars.shadowSlot}`,
  },
  slotSearch: {
    gridTemplateColumns: {
      default: "18px 56px minmax(0, 1fr) auto",
      [pageLayout.phoneMedia]: "14px 48px minmax(0, 1fr) auto",
    },
    borderColor: awbrnVars.ruleStrong,
    borderStyle: "dashed",
    borderWidth: "1.5px",
  },
  slotNumber: {
    fontSize: awbrnVars.text12,
    color: colorVars["--color-text-secondary"],
    textAlign: "end",
  },
  slotText: {
    display: "grid",
    gap: "4px",
    minInlineSize: 0,
  },
  slotTitle: {
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    fontSize: awbrnVars.text15_5,
    fontWeight: 750,
  },
  slotMap: {
    display: "inline-flex",
    alignItems: "center",
    gap: "6px",
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    color: colorVars["--color-text-secondary"],
    fontSize: textSizeVars["--font-size-sm"],
  },
  slotTime: {
    display: "inline-flex",
    whiteSpace: "nowrap",
  },
  slotWell: {
    inlineSize: {
      default: "56px",
      [pageLayout.phoneMedia]: "48px",
    },
    blockSize: {
      default: "56px",
      [pageLayout.phoneMedia]: "48px",
    },
    borderRadius: "6px",
    backgroundImage: awbrnVars.hatchWell,
    boxShadow: `inset 0 0 0 1px ${awbrnVars.rule}`,
  },
  seeking: {
    display: "inline-flex",
    alignItems: "center",
    gap: "6px",
    color: awbrnVars.ok,
    fontSize: awbrnVars.text15_5,
    fontWeight: 750,
  },
  dot: {
    inlineSize: "8px",
    blockSize: "8px",
    borderRadius: "50%",
    backgroundColor: "currentColor",
  },
  chevron: {
    color: awbrnVars.textTertiary,
  },
  owedKey: {
    gridColumn: {
      default: "auto",
      [pageLayout.phoneMedia]: "1 / -1",
    },
    minBlockSize: {
      default: null,
      [pageLayout.phoneMedia]: "52px",
    },
  },
  notPhone: {
    display: {
      default: null,
      [pageLayout.phoneMedia]: "none",
    },
  },
  problem: {
    paddingInline: "12px",
    paddingBlockEnd: "12px",
  },
  stepper: {
    display: "inline-flex",
    alignItems: "center",
    gap: "12px",
  },
  capacity: {
    minInlineSize: "1ch",
    textAlign: "center",
  },
  stepWord: {
    color: colorVars["--color-text-secondary"],
    fontSize: awbrnVars.text13_5,
  },
  table: {
    inlineSize: "100%",
    tableLayout: "fixed",
    borderCollapse: "collapse",
    fontSize: awbrnVars.text14_5,
  },
  th: {
    paddingBlock: "10px",
    paddingInline: "20px",
    borderBlockEndColor: awbrnVars.rule,
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: "1px",
    color: colorVars["--color-text-secondary"],
    fontSize: awbrnVars.text12,
    fontWeight: 650,
    letterSpacing: "0.1em",
    textAlign: "start",
    textTransform: "uppercase",
    fontStretch: "85%",
  },
  td: {
    paddingBlock: "12px",
    paddingInline: "20px",
    borderBlockEndColor: awbrnVars.rule,
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: "1px",
  },
  placeCell: {
    inlineSize: "48px",
  },
  ratingCell: {
    inlineSize: "84px",
    textAlign: "end",
  },
  gamesCell: {
    inlineSize: "72px",
    textAlign: "end",
  },
  notPhoneCell: {
    display: {
      default: "table-cell",
      [pageLayout.phoneMedia]: "none",
    },
  },
  rating: {
    fontSize: textSizeVars["--font-size-base"],
    fontWeight: 600,
  },
  games: {
    fontSize: textSizeVars["--font-size-sm"],
    color: colorVars["--color-text-secondary"],
  },
  youRow: {
    backgroundColor: awbrnVars.ink100,
  },
  player: {
    display: "inline-flex",
    alignItems: "center",
    gap: "8px",
    minInlineSize: 0,
  },
  you: {
    display: "inline-flex",
    alignItems: "center",
    blockSize: "18px",
    paddingInline: "6px",
    borderRadius: "4px",
    backgroundColor: awbrnVars.ink900,
    color: awbrnVars.textOnInkStrong,
    fontSize: awbrnVars.textMicro,
    fontWeight: 750,
    letterSpacing: "0.14em",
  },
});
