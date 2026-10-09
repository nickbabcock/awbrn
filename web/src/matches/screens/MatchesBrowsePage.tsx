import { useSuspenseInfiniteQuery } from "@tanstack/react-query";
import { Banner } from "@astryxdesign/core/Banner";
import { Badge } from "@astryxdesign/core/Badge";
import { Button } from "#/ui/Button.tsx";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { List } from "@astryxdesign/core/List";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { useState } from "react";
import { MapThumb } from "#/maps/components/MapThumb.tsx";
import { RouterButton, RouterListItem } from "#/ui/astryx-links.tsx";
import { Page, PageHeader } from "#/ui/Page.tsx";
import { matchesBrowseQueryOptions } from "#/matches/matches.queries.ts";
import type { MatchBrowseSummary } from "#/matches/schemas.ts";
import { formatRelativeTime } from "#/utils/time.ts";
import { formatClockSummary } from "#/matches/match_clock.ts";

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
    <Page>
      <VStack gap={6}>
        <PageHeader
          description="Public matches looking for players. Open one to read its briefing and take a seat."
          title="Play"
        />

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
              <Text type="label">
                {matches.length === 1 ? "1 open lobby" : `${matches.length} open lobbies`}
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
    </Page>
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
    lobby.settings.fogEnabled ? "Fog" : "No fog",
    `${lobby.settings.startingFunds.toLocaleString()} funds`,
    formatClockSummary(lobby.settings.clock),
  ].join(" · ");
  const joined =
    lobby.joinedPlayerNames.length > 0
      ? `Hosted by ${lobby.creatorName} · with ${lobby.joinedPlayerNames.join(", ")}`
      : `Hosted by ${lobby.creatorName}`;

  return (
    <RouterListItem
      description={
        <VStack gap={1}>
          <Text color="secondary" type="label">
            {details}
          </Text>
          <Text color="secondary" type="supporting">
            {joined}
          </Text>
        </VStack>
      }
      endContent={
        <VStack align="end" gap={1}>
          <Text type="label" weight="bold">
            {lobby.participantCount}/{lobby.maxPlayers} seats
          </Text>
          <Text color="secondary" type="supporting">
            {formatRelativeTime(lobby.createdAt, relativeTimeBaseMs)}
          </Text>
        </VStack>
      }
      label={
        <HStack align="center" gap={2} wrap="wrap">
          <Text weight="bold">{lobby.name}</Text>
          {lobby.settings.hotseatEnabled ? <Badge label="Hotseat" variant="blue" /> : null}
        </HStack>
      }
      params={{ matchId: lobby.matchId }}
      startContent={<MapThumb mapId={lobby.mapId} revision={lobby.mapRevision} size="md" />}
      to="/matches/$matchId"
    />
  );
}

function parseLoadedAt(iso: string | undefined): number {
  const parsed = iso ? Date.parse(iso) : Number.NaN;
  return Number.isNaN(parsed) ? Date.now() : parsed;
}
