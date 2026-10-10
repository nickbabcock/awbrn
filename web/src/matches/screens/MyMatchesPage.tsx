import { useSuspenseQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { Badge } from "@astryxdesign/core/Badge";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { List } from "@astryxdesign/core/List";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { getCoPortraitByAwbwId } from "#/components/co_portraits.ts";
import { getFactionById } from "#/factions.ts";
import { MapThumb } from "#/maps/components/MapThumb.tsx";
import { PushToggle } from "#/players/components/PushToggle.tsx";
import { RouterButton, RouterListItem } from "#/ui/astryx-links.tsx";
import { Page, PageHeader } from "#/ui/Page.tsx";
import type { MatchPhase, MyMatchSummary } from "#/matches/schemas.ts";
import {
  formatMyMatchPhaseLabel,
  needsViewerAction,
  viewerStatusLabel,
} from "#/matches/my_matches.ts";
import { myMatchesQueryOptions } from "#/matches/matches.queries.ts";
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

  const owed = matches.filter((match) => needsViewerAction(match));
  const waiting = matches.filter((match) => !needsViewerAction(match));

  return (
    <Page>
      <VStack gap={6}>
        <PageHeader
          actions={<PushToggle isSignedIn />}
          description="Every match and lobby you are part of. The ones waiting on you come first."
          title="My games"
        />

        {matches.length === 0 ? (
          <EmptyState
            actions={
              <HStack gap={2} justify="center" wrap="wrap">
                <RouterButton label="Browse lobbies" to="/matches" variant="primary" />
                <RouterButton label="New match" to="/matches/new" variant="secondary" />
              </HStack>
            }
            description="Join an open lobby or start a match, and it is kept here until it ends."
            headingLevel={2}
            title="You are not in a match yet"
          />
        ) : (
          <VStack gap={8}>
            {owed.length > 0 ? (
              <MatchGroup
                loadedAt={loadedAt}
                matches={owed}
                now={now}
                title={owed.length === 1 ? "1 waits on you" : `${owed.length} wait on you`}
              />
            ) : null}
            {waiting.length > 0 ? (
              <MatchGroup
                loadedAt={loadedAt}
                matches={waiting}
                now={now}
                title={
                  waiting.length === 1
                    ? "1 waits on someone else"
                    : `${waiting.length} wait on someone else`
                }
              />
            ) : null}
          </VStack>
        )}
      </VStack>
    </Page>
  );
}

function MatchGroup({
  loadedAt,
  matches,
  now,
  title,
}: {
  loadedAt: string;
  matches: MyMatchSummary[];
  now: number;
  title: string;
}) {
  return (
    <List density="spacious" hasDividers header={<Text type="label">{title}</Text>}>
      {matches.map((match) => (
        <MyMatchRow key={match.matchId} loadedAt={loadedAt} match={match} now={now} />
      ))}
    </List>
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
    match.isPrivate ? "Private" : "Public",
    match.settings.fogEnabled ? "Fog" : "No fog",
    `${match.settings.startingFunds.toLocaleString()} funds`,
    formatClockSummary(match.settings.clock),
  ].join(" · ");
  const seats = match.viewerParticipants
    .map((participant) => {
      const faction = getFactionById(participant.factionId);
      const coName = getCoPortraitByAwbwId(participant.coId)?.displayName ?? "No CO";
      return `${faction?.displayName ?? "Unknown army"} · ${coName}${participant.ready ? "" : " · not ready"}`;
    })
    .join("; ");

  return (
    <RouterListItem
      description={
        <VStack gap={1}>
          <Text color="secondary" type="label">
            {details}
          </Text>
          <Text color="secondary" type="supporting">
            You: {seats}
          </Text>
        </VStack>
      }
      endContent={
        <VStack align="end" gap={1}>
          {remaining ? (
            <Text type="label" weight="bold">
              {remaining}
            </Text>
          ) : (
            <Text type="label">
              {match.participantCount}/{match.maxPlayers} seats
            </Text>
          )}
          <Text color="secondary" type="supporting">
            {formatRelativeTime(match.updatedAt, Date.parse(loadedAt))}
          </Text>
        </VStack>
      }
      label={
        <HStack align="center" gap={2} wrap="wrap">
          <Text weight="bold">{match.name}</Text>
          {isWaiting ? (
            <Badge label={viewerStatusLabel(match)} variant="warning" />
          ) : (
            <Badge
              label={formatMyMatchPhaseLabel(match.phase)}
              variant={phaseBadgeVariant(match.phase)}
            />
          )}
          {match.settings.hotseatEnabled ? <Badge label="Hotseat" variant="blue" /> : null}
        </HStack>
      }
      params={{ matchId: match.matchId }}
      startContent={<MapThumb mapId={match.mapId} revision={match.mapRevision} size="md" />}
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
