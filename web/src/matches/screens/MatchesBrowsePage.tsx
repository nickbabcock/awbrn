import { useSuspenseInfiniteQuery } from "@tanstack/react-query";
import { Banner } from "@astryxdesign/core/Banner";
import { Badge } from "@astryxdesign/core/Badge";
import { Button } from "@astryxdesign/core/Button";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Heading } from "@astryxdesign/core/Heading";
import { List } from "@astryxdesign/core/List";
import { Section } from "@astryxdesign/core/Section";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { Thumbnail } from "@astryxdesign/core/Thumbnail";
import { useState } from "react";
import { awbwSmallMapAssetPath } from "#/awbw/paths.ts";
import { RouterButton, RouterListItem } from "#/ui/astryx-links.tsx";
import { matchesBrowseQueryOptions } from "#/matches/matches.queries.ts";
import type { MatchBrowseSummary } from "#/matches/schemas.ts";
import { formatRelativeTime } from "#/utils/time.ts";

export function MatchesBrowsePage() {
  const browseQuery = useSuspenseInfiniteQuery(matchesBrowseQueryOptions());
  const [paginationError, setPaginationError] = useState<string | null>(null);
  const matches = browseQuery.data.pages.flatMap((page) => page.matches);
  const relativeTimeBaseMs = parseLoadedAt(
    browseQuery.data.pages[browseQuery.data.pages.length - 1]?.loadedAt,
  );

  async function handleLoadMore(): Promise<void> {
    if (browseQuery.isFetchingNextPage || !browseQuery.hasNextPage) return;

    setPaginationError(null);
    try {
      await browseQuery.fetchNextPage();
    } catch (nextError) {
      setPaginationError(
        nextError instanceof Error ? nextError.message : "More lobbies failed to load.",
      );
    }
  }

  return (
    <Section padding={6} variant="transparent">
      <VStack gap={6}>
        <VStack gap={2}>
          <Heading level={1} type="display-2">
            Open lobbies
          </Heading>
          <Text color="secondary" type="large">
            Join a public room or create a new match.
          </Text>
        </VStack>

        {paginationError ? (
          <Banner
            description={paginationError}
            endContent={
              <Button clickAction={handleLoadMore} label="Retry" size="sm" variant="secondary" />
            }
            status="error"
            title="More lobbies failed to load"
          />
        ) : null}

        {matches.length === 0 ? (
          <EmptyState
            actions={<RouterButton label="Create match" to="/matches/new" variant="primary" />}
            description="Create a new match to start the next lobby."
            headingLevel={2}
            title="No public lobbies are open right now"
          />
        ) : (
          <List
            density="spacious"
            hasDividers
            header={
              <Text color="secondary" type="supporting" weight="bold">
                Public open lobbies
              </Text>
            }
          >
            {matches.map((lobby) => (
              <LobbyRow key={lobby.matchId} lobby={lobby} relativeTimeBaseMs={relativeTimeBaseMs} />
            ))}
          </List>
        )}

        {matches.length > 0 && browseQuery.hasNextPage ? (
          <HStack justify="center">
            <Button
              clickAction={handleLoadMore}
              isLoading={browseQuery.isFetchingNextPage}
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

function LobbyRow({
  lobby,
  relativeTimeBaseMs,
}: {
  lobby: MatchBrowseSummary;
  relativeTimeBaseMs: number;
}) {
  const details = [
    `Host ${lobby.creatorName}`,
    `Map ${lobby.mapId}`,
    lobby.settings.fogEnabled ? "Fog on" : "Fog off",
    `${lobby.settings.startingFunds.toLocaleString()} funds`,
  ].join(" · ");
  const joined =
    lobby.joinedPlayerNames.length > 0
      ? `Joined: ${lobby.joinedPlayerNames.join(", ")}`
      : "Joined: No players yet";

  return (
    <RouterListItem
      description={
        <VStack gap={1}>
          <Text color="secondary" type="supporting">
            {details}
          </Text>
          <Text color="secondary" type="supporting">
            {joined}
          </Text>
        </VStack>
      }
      endContent={
        <VStack align="end" gap={1}>
          <Text type="supporting" weight="bold">
            {lobby.participantCount} / {lobby.maxPlayers} seats
          </Text>
          <Text color="secondary" type="supporting">
            {lobby.openSlotCount} open · {formatRelativeTime(lobby.createdAt, relativeTimeBaseMs)}
          </Text>
        </VStack>
      }
      label={
        <HStack align="center" gap={2} wrap="wrap">
          <Heading level={2}>{lobby.name}</Heading>
          {lobby.settings.hotseatEnabled ? <Badge label="Hotseat" variant="blue" /> : null}
        </HStack>
      }
      params={{ matchId: lobby.matchId }}
      startContent={
        <Thumbnail
          alt={`Map preview for ${lobby.name}`}
          label={`${lobby.name} map`}
          src={awbwSmallMapAssetPath(lobby.mapId)}
        />
      }
      to="/matches/$matchId"
    />
  );
}

function parseLoadedAt(iso: string | undefined): number {
  const parsed = iso ? Date.parse(iso) : Number.NaN;
  return Number.isNaN(parsed) ? Date.now() : parsed;
}
