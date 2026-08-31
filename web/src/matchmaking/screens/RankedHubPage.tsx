/*
 * THE RANKED HUB
 *
 * THESIS: async ranked play is a standing arrangement, not a queue. The
 *   player states how much of their attention ranked may take, and the server
 *   keeps that many games running. So the surface is one dial, one switch, and
 *   one list, and the dial is the page's centre of gravity.
 * OWN-WORLD: the panel is a CO intel readout. The rating, the slot meter and
 *   the status line are HUD; the sentences under them are the briefing voice.
 * RULE: this surface describes the player, never the pool. It has no count of
 *   who else is seeking, no queue depth, and no estimated wait, because the
 *   ladder opens with very few players and a population figure would read as
 *   an empty room. Elapsed wait is a fact about this player and is allowed.
 * STORY: a player reads their rating, sets the number of games they want,
 *   presses one key, and comes back later to find pairings waiting at the top
 *   of the list.
 */

import { useMutation, useQuery, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { Badge } from "@astryxdesign/core/Badge";
import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "#/ui/Button.tsx";
import { Card } from "@astryxdesign/core/Card";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Grid } from "@astryxdesign/core/Grid";
import { Heading } from "@astryxdesign/core/Heading";
import { List } from "@astryxdesign/core/List";
import { MetadataList, MetadataListItem } from "@astryxdesign/core/MetadataList";
import { NumberInput } from "@astryxdesign/core/NumberInput";
import { Section } from "@astryxdesign/core/Section";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Table, proportional, pixel } from "@astryxdesign/core/Table";
import { Text } from "@astryxdesign/core/Text";
import { useEffect, useRef, useState } from "react";
import { FactionLogo } from "#/components/FactionLogo.tsx";
import { getFactionById } from "#/factions.ts";
import { Countdown } from "#/matchmaking/components/Countdown.tsx";
import { SlotMeter } from "#/matchmaking/components/SlotMeter.tsx";
import { startRankedSeekFn, stopRankedSeekFn } from "#/matchmaking/matchmaking.functions.ts";
import { rankedKeys } from "#/matchmaking/matchmaking.keys.ts";
import {
  RANKED_POLL_INTERVAL_MS,
  rankedOverviewQueryOptions,
  rankedStandingsQueryOptions,
} from "#/matchmaking/matchmaking.queries.ts";
import { HARD_MAX_ACTIVE_MATCHES } from "#/matchmaking/matchmaking.ts";
import {
  RANKED_POOL_ORDER,
  capacityHelperLine,
  formatRating,
  isRankedPoolOpen,
  rankedPoolCopy,
  seekStatusLine,
  seekWaitPhase,
  slotMeter,
} from "#/matchmaking/ranked_display.ts";
import type {
  RankedInPlaySummary,
  RankedPendingSummary,
  RankedPoolSnapshot,
  StandingsEntry,
} from "#/matchmaking/ranked_overview.server.ts";

/** The table reads rows by key, so its row type carries an index signature. */
interface StandingsRow extends StandingsEntry {
  [key: string]: unknown;
}
import type { RankedPool } from "#/matches/schemas.ts";
import { RouterButton, RouterListItem } from "#/ui/astryx-links.tsx";
import { ROSTER_MEDIA_SIZE, TWO_COLUMN_GRID_MIN_WIDTH } from "#/ui/layout.ts";
import { formatCompactDuration, formatRelativeTime } from "#/utils/time.ts";

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
  const copy = rankedPoolCopy(snapshot.pool);

  return (
    <Section padding={6} variant="transparent">
      <VStack gap={6}>
        <Grid
          align="end"
          columns={{ minWidth: TWO_COLUMN_GRID_MIN_WIDTH, max: 2, repeat: "fit" }}
          gap={4}
        >
          <VStack gap={2}>
            <Heading level={1} type="display-2">
              Ranked play
            </Heading>
            <Text color="secondary" type="large">
              Say how many games you want at a time. We find the opponents.
            </Text>
          </VStack>
          <SeasonReadout season={overview.season} />
        </Grid>

        <VStack gap={2}>
          <SegmentedControl
            label="Ranked pool"
            layout="fill"
            onChange={(value) => onSelectPool(value as RankedPool)}
            value={snapshot.pool}
          >
            {RANKED_POOL_ORDER.map((entry) => (
              <SegmentedControlItem
                isDisabled={!isRankedPoolOpen(entry)}
                key={entry}
                label={rankedPoolCopy(entry).name}
                value={entry}
              />
            ))}
          </SegmentedControl>
          <Text color="secondary" type="supporting">
            {copy.summary} Fog and live pools open later.
          </Text>
        </VStack>

        <SeekPanel isEmailVerified={overview.isEmailVerified} snapshot={snapshot} />

        {snapshot.pending.length > 0 ? <PendingPanel pending={snapshot.pending} /> : null}

        <InPlayPanel snapshot={snapshot} />

        <StandingsPanel pool={snapshot.pool} />
      </VStack>
    </Section>
  );
}

function SeasonReadout({
  season,
}: {
  season: { number: number; startsAt: string; endsAt: string } | null;
}) {
  if (season === null) {
    return (
      <Text color="secondary" type="supporting">
        No season is running. Ratings do not change until one opens.
      </Text>
    );
  }

  return (
    <MetadataList>
      <MetadataListItem label="Season">{`Season ${season.number}`}</MetadataListItem>
      <MetadataListItem label="Ends in">
        {formatCompactDuration(Date.parse(season.endsAt) - Date.now())}
      </MetadataListItem>
    </MetadataList>
  );
}

function SeekPanel({
  isEmailVerified,
  snapshot,
}: {
  isEmailVerified: boolean;
  snapshot: RankedPoolSnapshot;
}) {
  const queryClient = useQueryClient();
  const isSeeking = snapshot.seek !== null;
  const [capacity, setCapacity] = useState(
    snapshot.seek?.maxActiveMatches ?? Math.min(3, HARD_MAX_ACTIVE_MATCHES),
  );
  const [error, setError] = useState<string | null>(null);

  const saved = snapshot.seek?.maxActiveMatches ?? null;
  useEffect(() => {
    if (saved !== null) setCapacity(saved);
  }, [saved]);

  const seekMutation = useMutation({
    mutationFn: (input: { maxActiveMatches: number }) =>
      startRankedSeekFn({
        data: { pool: snapshot.pool, maxActiveMatches: input.maxActiveMatches },
      }),
    // The stepper goes back to the saved arrangement, because a value which
    // the server refused is not the value which is in force, and a value which
    // stays different from the server starts the autosave again without end.
    onError: (mutationError: Error) => {
      setError(mutationError.message);
      if (saved !== null) setCapacity(saved);
    },
    onSuccess: () => {
      setError(null);
      return queryClient.invalidateQueries({ queryKey: rankedKeys.overview() });
    },
  });
  const stopMutation = useMutation({
    mutationFn: () => stopRankedSeekFn({ data: { pool: snapshot.pool } }),
    onError: (mutationError: Error) => setError(mutationError.message),
    onSuccess: () => {
      setError(null);
      return queryClient.invalidateQueries({ queryKey: rankedKeys.overview() });
    },
  });

  // A stepper press changes the standing arrangement, so it saves itself. The
  // delay lets a player walk the number from 1 to 5 with one write at the end.
  // The mutation is read through a ref, because a query-client mutation is a
  // new object on every render and would restart the delay on every one.
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

  const now = Date.now();
  const waitLabel = snapshot.seek
    ? formatCompactDuration(now - Date.parse(snapshot.seek.createdAt))
    : "0s";
  const statusLine = seekStatusLine({
    isSeeking,
    activeMatches: snapshot.activeMatches,
    maxActiveMatches: capacity,
    waitPhase: snapshot.seek ? seekWaitPhase(snapshot.seek.createdAt, now) : "searching",
    waitLabel,
  });
  const helperLine = capacityHelperLine({
    isSeeking,
    activeMatches: snapshot.activeMatches,
    maxActiveMatches: capacity,
  });
  const isBusy = seekMutation.isPending || stopMutation.isPending;

  return (
    <Card elevation="med" padding={6}>
      <VStack gap={5}>
        <HStack align="start" gap={4} justify="between" wrap="wrap">
          <VStack gap={1}>
            <Heading level={2}>{rankedPoolCopy(snapshot.pool).name}</Heading>
            <Text color="secondary" type="supporting">
              {isSeeking
                ? "We pair you with an opponent near your rating."
                : "Enter the pool and we pair you with an opponent near your rating."}
            </Text>
          </VStack>
          <RatingReadout rating={snapshot.rating} />
        </HStack>

        <VStack gap={3}>
          <Text type="supporting" weight="bold">
            {statusLine}
          </Text>
          <SlotMeter
            slots={slotMeter({
              activeMatches: snapshot.activeMatches,
              maxActiveMatches: capacity,
              isSeeking,
            })}
          />
          {helperLine ? (
            <Text color="secondary" type="supporting">
              {helperLine}
            </Text>
          ) : null}
        </VStack>

        {isEmailVerified ? (
          <VStack gap={2}>
            <HStack align="end" gap={4} wrap="wrap">
              <NumberInput
                hasNumberSteppers
                isDisabled={isBusy}
                isIntegerOnly
                isWheelEnabled={false}
                label="Games at a time"
                max={HARD_MAX_ACTIVE_MATCHES}
                min={1}
                onChange={(value) => setCapacity(clampCapacity(value))}
                value={capacity}
                width={200}
              />
              {isSeeking ? (
                <Button
                  clickAction={() => stopMutation.mutate()}
                  isLoading={stopMutation.isPending}
                  label="Stop seeking"
                  variant="secondary"
                />
              ) : (
                <Button
                  clickAction={() => seekMutation.mutate({ maxActiveMatches: capacity })}
                  isLoading={seekMutation.isPending}
                  label="Start seeking"
                  variant="primary"
                />
              )}
            </HStack>
            <Text color="secondary" type="supporting">
              {isSeeking
                ? "Stopping ends new pairings only. Your games in play continue."
                : "A pairing gives you 24 hours to read the map, pick a commander, and start."}
            </Text>
          </VStack>
        ) : (
          <Banner
            collapsible={false}
            description="Ranked play needs a verified email address, so that one player cannot hold several places in the pool."
            status="info"
            title="Verify your email address to play ranked"
          />
        )}

        {error ? (
          <Text color="accent" role="alert" type="supporting">
            {error}
          </Text>
        ) : null}
      </VStack>
    </Card>
  );
}

function clampCapacity(value: number): number {
  if (!Number.isFinite(value)) return 1;
  return Math.min(HARD_MAX_ACTIVE_MATCHES, Math.max(1, Math.round(value)));
}

function RatingReadout({ rating }: { rating: RankedPoolSnapshot["rating"] }) {
  const value = rating ? formatRating(rating.rating, rating.deviation) : "1500?";
  const isProvisionalRating = rating === null || rating.isProvisional;
  const played = rating?.ratedMatches ?? 0;

  return (
    <VStack align="end" gap={1}>
      <Text color="secondary" type="supporting" weight="bold">
        Your rating
      </Text>
      <Heading level={3}>{value}</Heading>
      <Text color="secondary" type="supporting">
        {isProvisionalRating
          ? played === 0
            ? "No rated games yet"
            : `Provisional after ${played === 1 ? "1 rated game" : `${played} rated games`}`
          : `${played} rated games`}
      </Text>
    </VStack>
  );
}

function PendingPanel({ pending }: { pending: RankedPendingSummary[] }) {
  return (
    <List
      density="spacious"
      hasDividers
      header={
        <HStack align="center" gap={2}>
          <Text type="supporting" weight="bold">
            {pending.length === 1 ? "1 pairing needs you" : `${pending.length} pairings need you`}
          </Text>
          <Badge label="Confirm" variant="warning" />
        </HStack>
      }
    >
      {pending.map((match) => {
        const faction = getFactionById(match.factionId);
        return (
          <RouterListItem
            description={
              <Text color="secondary" type="supporting">
                {[
                  faction?.displayName ?? "Unknown army",
                  match.slotIndex === 0 ? "Moves first" : "Moves second",
                  match.hasCommander ? "Commander chosen" : "No commander yet",
                ].join(" · ")}
              </Text>
            }
            endContent={<Countdown deadlineAt={match.deadlineAt} />}
            key={match.matchId}
            label={match.mapName}
            params={{ matchId: match.matchId }}
            startContent={
              faction ? (
                <FactionLogo factionCode={faction.code} size={ROSTER_MEDIA_SIZE.crest} />
              ) : null
            }
            to="/matches/$matchId"
          />
        );
      })}
    </List>
  );
}

function InPlayPanel({ snapshot }: { snapshot: RankedPoolSnapshot }) {
  if (snapshot.inPlay.length === 0) {
    return (
      <EmptyState
        actions={<RouterButton label="Completed games" to="/my/history" variant="secondary" />}
        description={
          snapshot.seek
            ? "Your first pairing appears here. You can close the page: the seek keeps running."
            : "Start seeking and your games appear here."
        }
        headingLevel={2}
        isCompact
        title="No ranked games in play"
      />
    );
  }

  return (
    <List
      density="spacious"
      hasDividers
      header={
        <Text type="supporting" weight="bold">
          {snapshot.inPlay.length === 1
            ? "1 game in play"
            : `${snapshot.inPlay.length} games in play`}
        </Text>
      }
    >
      {snapshot.inPlay.map((match) => (
        <InPlayRow key={match.matchId} match={match} />
      ))}
    </List>
  );
}

function InPlayRow({ match }: { match: RankedInPlaySummary }) {
  const faction = getFactionById(match.factionId);
  return (
    <RouterListItem
      description={
        <Text color="secondary" type="supporting">
          {[match.mapName, faction?.displayName ?? "Unknown army"].join(" · ")}
        </Text>
      }
      endContent={
        <Text color="secondary" type="supporting">
          {formatRelativeTime(match.updatedAt, Date.now())}
        </Text>
      }
      label={match.opponentName ? `vs ${match.opponentName}` : "Ranked game"}
      params={{ matchId: match.matchId }}
      startContent={
        faction ? <FactionLogo factionCode={faction.code} size={ROSTER_MEDIA_SIZE.crest} /> : null
      }
      to="/matches/$matchId"
    />
  );
}

function StandingsPanel({ pool }: { pool: RankedPool }) {
  const { data, isPending } = useQuery(rankedStandingsQueryOptions(pool));

  if (isPending || !data) {
    return (
      <Text color="secondary" type="supporting">
        Loading standings…
      </Text>
    );
  }

  return (
    <VStack gap={3}>
      <Heading level={2}>Standings</Heading>
      {data.entries.length === 0 ? (
        <EmptyState
          description="A player joins the standings once a rated game confirms their rating."
          headingLevel={3}
          isCompact
          title="No confirmed ratings yet"
        />
      ) : (
        <Table
          columns={[
            { key: "rank", header: "#", width: pixel(56) },
            {
              key: "name",
              header: "Player",
              width: proportional(2),
              renderCell: (row: StandingsRow) =>
                row.isViewer ? (
                  <HStack align="center" gap={2}>
                    <Text weight="bold">{row.name}</Text>
                    <Badge label="You" variant="neutral" />
                  </HStack>
                ) : (
                  <Text>{row.name}</Text>
                ),
            },
            {
              key: "rating",
              header: "Rating",
              width: pixel(96),
              renderCell: (row: StandingsRow) => <Text>{Math.round(row.rating)}</Text>,
            },
            {
              key: "ratedMatches",
              header: "Rated games",
              width: pixel(120),
            },
          ]}
          data={data.entries as StandingsRow[]}
          density="compact"
          idKey="userId"
        />
      )}
      {data.viewer?.isProvisional ? (
        <Card padding={4} variant="muted">
          <VStack gap={1}>
            <Text type="supporting" weight="bold">
              Provisional · {formatRating(data.viewer.rating, 350)}
            </Text>
            <Text color="secondary" type="supporting">
              {data.viewer.ratedMatches === 0
                ? "Play a rated game and your rating settles into the standings."
                : "A few more rated games settle your rating and enter you in the standings."}
            </Text>
          </VStack>
        </Card>
      ) : null}
    </VStack>
  );
}
