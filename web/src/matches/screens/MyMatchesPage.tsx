import { useSuspenseQuery } from "@tanstack/react-query";
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
import { formatMyMatchPhaseLabel, myMatchActionLabel } from "#/matches/my_matches.ts";
import { myMatchesQueryOptions } from "#/matches/matches.queries.ts";
import { TWO_COLUMN_GRID_MIN_WIDTH } from "#/ui/layout.ts";
import { formatRelativeTime } from "#/utils/time.ts";

export function MyMatchesPage() {
  const { data } = useSuspenseQuery(myMatchesQueryOptions());
  const { loadedAt, matches } = data;

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
          </HStack>
        </Grid>

        {matches.length === 0 ? (
          <EmptyState
            actions={
              <HStack gap={2} justify="center" wrap="wrap">
                <RouterButton label="Create match" to="/matches/new" variant="primary" />
                <RouterButton label="Browse lobbies" to="/matches" variant="secondary" />
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
              <MyMatchRow key={match.matchId} loadedAt={loadedAt} match={match} />
            ))}
          </List>
        )}
      </VStack>
    </Section>
  );
}

function MyMatchRow({ loadedAt, match }: { loadedAt: string; match: MyMatchSummary }) {
  const details = [
    `Host ${match.creatorName}`,
    `Map ${match.mapId}`,
    match.isPrivate ? "Private" : "Public",
    match.settings.fogEnabled ? "Fog on" : "Fog off",
    `${match.settings.startingFunds.toLocaleString()} funds`,
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
      return "warning";
    case "completed":
      return "neutral";
  }
}
