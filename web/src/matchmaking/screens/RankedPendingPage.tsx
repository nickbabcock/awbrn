/*
 * THE PAIRING BRIEFING
 *
 * THESIS: this is the one moment in ranked play a player can walk away from at
 *   no cost, so it is a briefing rather than a form. The map is the subject of
 *   the page, the clock is the only urgent thing on it, and there is one key
 *   to press.
 * RULE: the pairing shows the match and not the person. The server withholds
 *   the other player's name, rating and commander until both players are ready,
 *   so a player cannot decline the opponents they do not want and take the
 *   rest. The page says so in a sentence rather than leaving an empty seat
 *   unexplained.
 * OWN-WORLD: the same briefing screen the lobby uses, with the host controls
 *   taken out and a clock put in their place. Nothing about the pairing is the
 *   player's to arrange: the server chose the map, the army and the turn order.
 */

import { useMutation, useQuery, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "#/ui/Button.tsx";
import { Card } from "@astryxdesign/core/Card";
import { Grid } from "@astryxdesign/core/Grid";
import { Heading } from "@astryxdesign/core/Heading";
import { MetadataList, MetadataListItem } from "@astryxdesign/core/MetadataList";
import { Section } from "@astryxdesign/core/Section";
import { Skeleton } from "@astryxdesign/core/Skeleton";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import * as stylex from "@stylexjs/stylex";
import { useNavigate } from "@tanstack/react-router";
import { useEffect, useMemo, useState } from "react";
import { useAppSession } from "#/auth/useAppSession.ts";
import { CoBoard } from "#/components/CoBoard.tsx";
import { FactionLogo } from "#/components/FactionLogo.tsx";
import { coDisplayName } from "#/co_roster.ts";
import { getFactionById } from "#/factions.ts";
import { MapPicture } from "#/maps/components/MapPicture.tsx";
import { mapScreenshotSize } from "#/maps/map_screenshot.ts";
import { mapCatalogEntryQueryOptions, mapRevisionQueryOptions } from "#/maps/maps.queries.ts";
import { matchKeys } from "#/matches/matches.keys.ts";
import { matchDetailQueryOptions } from "#/matches/matches.queries.ts";
import type { RankedConfirmationRequest } from "#/matches/schemas.ts";
import { Countdown } from "#/matchmaking/components/Countdown.tsx";
import { updateRankedConfirmationFn } from "#/matchmaking/matchmaking.functions.ts";
import { rankedKeys } from "#/matchmaking/matchmaking.keys.ts";
import { rankedOverviewQueryOptions } from "#/matchmaking/matchmaking.queries.ts";
import { TWO_COLUMN_GRID_MIN_WIDTH } from "#/ui/layout.ts";

/** How often the page asks whether the other player has readied. */
const CONFIRMATION_POLL_INTERVAL_MS = 15_000;

/** How long an armed "Confirm decline" stays armed before it disarms itself. */
const DECLINE_CONFIRM_TIMEOUT_MS = 5_000;

const SEAT_CREST_SIZE = 32;

export function RankedPendingPage({
  joinSlug,
  matchId,
}: {
  joinSlug: string | null;
  matchId: string;
}) {
  const session = useAppSession();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const detailQueryOptions = matchDetailQueryOptions(matchId, joinSlug);
  const { data: match } = useSuspenseQuery({
    ...detailQueryOptions,
    // The other player readies without telling this page, and the window
    // closes on a server alarm. Both reach the screen only by asking again.
    refetchInterval: (query) =>
      query.state.data?.phase === "pending" ? CONFIRMATION_POLL_INTERVAL_MS : false,
    refetchOnWindowFocus: true,
  });

  const mapQuery = useQuery(mapRevisionQueryOptions(match.mapId, match.mapRevision));
  const mapEntryQuery = useQuery(mapCatalogEntryQueryOptions(match.mapId, match.mapRevision));
  const mapData = mapQuery.data ?? null;
  const mapEntry = mapEntryQuery.data ?? null;
  const mapName = mapData?.metadata.name ?? mapEntry?.name ?? `Map ${match.mapId}`;

  const currentUserId = session?.user.id ?? null;
  const seat = match.participants.find((participant) => participant.userId === currentUserId);
  const faction = seat ? getFactionById(seat.factionId) : null;
  const bannedCoIds = useMemo(() => new Set(match.settings.bannedCoIds), [match.settings]);

  const [error, setError] = useState<string | null>(null);
  const [isDeclineArmed, setIsDeclineArmed] = useState(false);

  useEffect(() => {
    if (!isDeclineArmed) return;
    const timer = setTimeout(() => setIsDeclineArmed(false), DECLINE_CONFIRM_TIMEOUT_MS);
    return () => clearTimeout(timer);
  }, [isDeclineArmed]);

  const confirmation = useMutation({
    mutationFn: (action: RankedConfirmationRequest) =>
      updateRankedConfirmationFn({ data: { matchId, action } }),
    onError: (mutationError: Error) => setError(mutationError.message),
    onSuccess: async (_result, action) => {
      setError(null);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: detailQueryOptions.queryKey }),
        queryClient.invalidateQueries({ queryKey: rankedKeys.overview() }),
        queryClient.invalidateQueries({ queryKey: matchKeys.mine() }),
        queryClient.invalidateQueries({ queryKey: matchKeys.awaiting() }),
      ]);
      if (action.action === "refuse") {
        await queryClient.invalidateQueries({ queryKey: rankedOverviewQueryOptions().queryKey });
        await navigate({ to: "/ranked" });
      }
    },
  });

  if (!seat) {
    return (
      <Section padding={6} variant="transparent">
        <Banner
          collapsible={false}
          description="A ranked pairing is readable only by the two players in it."
          status="error"
          title="This pairing is not yours"
        />
      </Section>
    );
  }

  const deadlineAt = match.confirmationDeadlineAt;
  const isReady = seat.ready;
  const isBusy = confirmation.isPending;

  return (
    <Section padding={6} variant="transparent">
      <VStack gap={6}>
        <Grid
          align="end"
          columns={{ minWidth: TWO_COLUMN_GRID_MIN_WIDTH, max: 2, repeat: "fit" }}
          gap={5}
        >
          <VStack gap={2}>
            <Heading level={1} type="display-2" xstyle={styles.breakAnywhere}>
              {mapName}
            </Heading>
            <Text color="secondary" type="large">
              A ranked pairing. The server chose the map, your army, and the turn order.
            </Text>
          </VStack>
          {deadlineAt ? (
            <Card padding={4} variant="muted">
              <VStack gap={1}>
                <Text type="supporting" weight="bold">
                  Confirm within
                </Text>
                <Countdown deadlineAt={deadlineAt} type="large" />
                <Text color="secondary" type="supporting">
                  The pairing is voided if either player does not start in time.
                </Text>
              </VStack>
            </Card>
          ) : null}
        </Grid>

        <Grid
          align="start"
          columns={{ minWidth: TWO_COLUMN_GRID_MIN_WIDTH, max: 2, repeat: "fit" }}
          gap={6}
        >
          <Card padding={5}>
            <VStack gap={4}>
              <VStack gap={1}>
                <Heading level={2}>The battlefield</Heading>
                <Text color="secondary" type="supporting">
                  {mapData
                    ? `${mapData.metadata.author} · ${mapData.width} × ${mapData.height}`
                    : "Reading the battlefield…"}
                </Text>
              </VStack>

              {mapEntry ? (
                <MapPicture
                  alt={`The battlefield of ${mapEntry.name}`}
                  sourceHeight={mapScreenshotSize("full", mapEntry.width, mapEntry.height).height}
                  sourceWidth={mapScreenshotSize("full", mapEntry.width, mapEntry.height).width}
                  src={mapEntry.screenshot.full}
                />
              ) : (
                <Section height={280} padding={0} variant="muted">
                  <Skeleton height="100%" radius="none" />
                </Section>
              )}

              <MetadataList columns={3} label={{ position: "top" }}>
                <MetadataListItem label="Your army">
                  {faction?.displayName ?? "Unknown army"}
                </MetadataListItem>
                <MetadataListItem label="Turn order">
                  {seat.slotIndex === 0 ? "You move first" : "You move second"}
                </MetadataListItem>
                <MetadataListItem label="Visibility">
                  {match.settings.fogEnabled ? "Fog enabled" : "Clear vision"}
                </MetadataListItem>
              </MetadataList>
            </VStack>
          </Card>

          <Card padding={5}>
            <VStack gap={5}>
              <HStack align="center" gap={3}>
                {faction ? <FactionLogo factionCode={faction.code} size={SEAT_CREST_SIZE} /> : null}
                <VStack gap={0}>
                  <Heading level={2}>Your commander</Heading>
                  <Text color="secondary" type="supporting">
                    {seat.coId === null
                      ? "Choose one to start the match."
                      : coDisplayName(seat.coId)}
                  </Text>
                </VStack>
              </HStack>

              <Text color="secondary" type="supporting">
                You are matched on rating alone. The other player's name and commander appear when
                the match starts, so neither of you can pick an opponent.
              </Text>

              <CoBoard
                bannedCoIds={bannedCoIds}
                isDisabled={isBusy || isReady}
                mode="pick"
                onPick={(coId) => confirmation.mutate({ action: "selectCommander", coId })}
                selectedCoId={seat.coId}
                size="sm"
              />

              {isReady ? (
                <Banner
                  collapsible={false}
                  description="The match opens as soon as the other player is ready."
                  status="success"
                  title="You are ready"
                />
              ) : (
                <HStack gap={3} wrap="wrap">
                  <Button
                    clickAction={() => confirmation.mutate({ action: "ready" })}
                    isDisabled={seat.coId === null}
                    isLoading={isBusy}
                    label="Ready"
                    variant="primary"
                  />
                  <Button
                    clickAction={() => {
                      if (!isDeclineArmed) {
                        setIsDeclineArmed(true);
                        return;
                      }
                      confirmation.mutate({ action: "refuse" });
                    }}
                    isDisabled={isBusy}
                    label={isDeclineArmed ? "Confirm decline" : "Decline this pairing"}
                    variant={isDeclineArmed ? "destructive" : "ghost"}
                  />
                </HStack>
              )}

              {isDeclineArmed ? (
                <Text color="secondary" type="supporting">
                  Declining voids this match and returns you to the pool. It does not change your
                  rating.
                </Text>
              ) : null}

              {error ? (
                <Text color="accent" role="alert" type="supporting">
                  {error}
                </Text>
              ) : null}
            </VStack>
          </Card>
        </Grid>
      </VStack>
    </Section>
  );
}

const styles = stylex.create({
  breakAnywhere: {
    overflowWrap: "anywhere",
  },
});
