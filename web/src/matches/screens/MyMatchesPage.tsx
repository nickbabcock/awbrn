import { useSuspenseQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { Badge } from "@astryxdesign/core/Badge";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Grid } from "@astryxdesign/core/Grid";
import { Heading } from "@astryxdesign/core/Heading";
import { List } from "@astryxdesign/core/List";
import { Section } from "@astryxdesign/core/Section";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { getCoPortraitByAwbwId } from "#/components/co_portraits.ts";
import { getFactionById } from "#/factions.ts";
import { RouterButton, RouterListItem } from "#/ui/astryx-links.tsx";
import type { MatchPhase, MyMatchSummary } from "#/matches/schemas.ts";
import {
  formatMyMatchPhaseLabel,
  myMatchActionLabel,
  needsViewerAction,
} from "#/matches/my_matches.ts";
import { myMatchesQueryOptions } from "#/matches/matches.queries.ts";
import { TWO_COLUMN_GRID_MIN_WIDTH } from "#/ui/layout.ts";
import { formatRelativeTime } from "#/utils/time.ts";
import { clockTickMs, formatClockSummary, formatTurnRemaining } from "#/matches/match_clock.ts";

export function MyMatchesPage() {
  const { data } = useSuspenseQuery(myMatchesQueryOptions());
  const { loadedAt, matches } = data;
  // The turn deadlines are read against a clock that is running, not against
  // the moment the page loaded. A page left open used to hold whatever the
  // countdown said when it arrived.
  const deadlines = useMemo(() => turnDeadlines(matches), [matches]);
  const now = useCountdownNow(deadlines);

  return (
    <Section padding={6} variant="transparent">
      <VStack gap={6}>
        <Grid
          align="end"
          columns={{ minWidth: TWO_COLUMN_GRID_MIN_WIDTH, max: 2, repeat: "fit" }}
          gap={4}
        >
          <VStack gap={2}>
            <Text color="accent" type="supporting" weight="bold">
              My matches
            </Text>
            <Heading level={1} type="display-2">
              Ongoing games
            </Heading>
            <Text color="secondary" type="large">
              Jump back into lobbies and active matches you have joined.
            </Text>
          </VStack>
          <HStack gap={2} justify="end" wrap="wrap">
            <RouterButton label="Create match" to="/matches/new" variant="primary" />
            <RouterButton label="Browse lobbies" to="/matches" variant="secondary" />
            <RouterButton label="Completed games" to="/my/history" variant="secondary" />
          </HStack>
        </Grid>

        {matches.length === 0 ? (
          <EmptyState
            actions={
              <HStack gap={2} justify="center" wrap="wrap">
                <RouterButton label="Create match" to="/matches/new" variant="primary" />
                <RouterButton label="Browse lobbies" to="/matches" variant="secondary" />
                <RouterButton label="Completed games" to="/my/history" variant="secondary" />
              </HStack>
            }
            description="Create a match or join an open lobby to see it here."
            headingLevel={2}
            title="You are not in any active matches or lobbies"
          />
        ) : (
          <List
            density="spacious"
            hasDividers
            header={
              <Text color="secondary" type="supporting" weight="bold">
                {matches.length === 1 ? "1 ongoing game" : `${matches.length} ongoing games`}
              </Text>
            }
          >
            {matches.map((match) => (
              <MyMatchRow key={match.matchId} loadedAt={loadedAt} match={match} now={now} />
            ))}
          </List>
        )}
      </VStack>
    </Section>
  );
}

/** Every turn deadline the page is counting down, soonest first. */
function turnDeadlines(matches: MyMatchSummary[]): number[] {
  return matches
    .filter((match) => match.phase === "active" && needsViewerAction(match))
    .map((match) => (match.turnDeadlineAt === null ? null : Date.parse(match.turnDeadlineAt)))
    .filter((deadline): deadline is number => deadline !== null)
    .sort((left, right) => left - right);
}

/**
 * A clock for the page, redrawn only as often as the nearest deadline needs.
 *
 * A list of correspondence matches with days on them costs one redraw a minute
 * rather than one a second, and a turn in its last hour still ticks.
 */
function useCountdownNow(deadlines: number[]): number {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    // Only a deadline still ahead has anything left to count: one that has
    // passed reads "Overdue" and stays there, so redrawing for it would be a
    // timer that never stopped and never changed anything. The soonest one
    // still ahead is taken rather than the soonest of all, so a turn that is
    // running does not stop ticking behind one that has already run out.
    const next = deadlines.find((deadline) => deadline > now);
    if (next === undefined) return;
    // Each redraw schedules the next one rather than running on a fixed
    // interval, so the rate tightens on its own as the deadline comes closer.
    const timer = setTimeout(() => setNow(Date.now()), clockTickMs(next - now));
    return () => clearTimeout(timer);
  }, [deadlines, now]);

  return now;
}

function MyMatchRow({
  loadedAt,
  match,
  now,
}: {
  loadedAt: string;
  match: MyMatchSummary;
  now: number;
}) {
  const isWaiting = needsViewerAction(match);
  // How long the viewer has left, which is what stops a match being lost to a
  // clock nobody was watching. Only an open turn has a deadline to report, and
  // one that has already run out says so rather than reading as no time left.
  const remaining =
    isWaiting && match.phase === "active" && match.turnDeadlineAt !== null
      ? formatTurnRemaining(Date.parse(match.turnDeadlineAt) - now)
      : null;
  const details = [
    `Host ${match.creatorName}`,
    `Map ${match.mapId}`,
    match.isPrivate ? "Private" : "Public",
    match.settings.fogEnabled ? "Fog on" : "Fog off",
    `${match.settings.startingFunds.toLocaleString()} funds`,
    formatClockSummary(match.settings.clock),
  ].join(" · ");

  return (
    <RouterListItem
      description={
        <VStack gap={1}>
          <Text color="secondary" type="supporting">
            {details}
          </Text>
          {match.viewerParticipants.map((participant) => {
            const faction = getFactionById(participant.factionId);
            const coName = getCoPortraitByAwbwId(participant.coId)?.displayName ?? "No CO";
            return (
              <Text color="secondary" key={participant.slotIndex} type="supporting">
                Slot {participant.slotIndex + 1}: {faction?.displayName ?? "Unknown army"} ·{" "}
                {coName}
                {" · "}
                {participant.ready ? "Ready" : "Not ready"}
              </Text>
            );
          })}
        </VStack>
      }
      endContent={
        <VStack align="end" gap={1}>
          <Text type="supporting" weight="bold">
            {match.participantCount} / {match.maxPlayers} seats
          </Text>
          {remaining ? (
            <Text type="supporting" weight="bold">
              {remaining}
            </Text>
          ) : null}
          <Text color="secondary" type="supporting">
            {formatRelativeTime(match.updatedAt, Date.parse(loadedAt))} ·{" "}
            {myMatchActionLabel(match.phase)}
          </Text>
        </VStack>
      }
      label={
        <HStack align="center" gap={2} wrap="wrap">
          <Heading level={2}>{match.name}</Heading>
          <Badge
            label={formatMyMatchPhaseLabel(match.phase)}
            variant={phaseBadgeVariant(match.phase)}
          />
          {isWaiting ? (
            <Badge label={match.phase === "active" ? "Your turn" : "Needs you"} variant="warning" />
          ) : null}
          {match.settings.hotseatEnabled ? <Badge label="Hotseat" variant="blue" /> : null}
        </HStack>
      }
      params={{ matchId: match.matchId }}
      to="/matches/$matchId"
    />
  );
}

function phaseBadgeVariant(
  phase: MatchPhase,
): "neutral" | "info" | "success" | "warning" | "error" {
  switch (phase) {
    case "active":
      return "success";
    case "starting":
      return "info";
    case "cancelled":
      return "error";
    case "draft":
    case "lobby":
    case "pending":
      return "warning";
    case "completed":
      return "neutral";
  }
}
