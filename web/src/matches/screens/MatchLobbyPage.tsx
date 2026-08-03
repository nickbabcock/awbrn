import { useMutation, useQuery, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { Grid } from "@astryxdesign/core/Grid";
import { Heading } from "@astryxdesign/core/Heading";
import { MetadataList, MetadataListItem } from "@astryxdesign/core/MetadataList";
import { Section } from "@astryxdesign/core/Section";
import { Selector } from "@astryxdesign/core/Selector";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { useEffect, useMemo, useState } from "react";
import { Cancel as CancelIcon } from "pixelarticons/react/Cancel";
import { Check as CheckIcon } from "pixelarticons/react/Check";
import { Logout as LogoutIcon } from "pixelarticons/react/Logout";
import { Plus as PlusIcon } from "pixelarticons/react/Plus";
import { useAppSession } from "#/auth/useAppSession.ts";
import { awbwMapDataQueryOptions } from "#/awbw/awbw.queries.ts";
import { CoPortrait } from "#/components/CoPortrait.tsx";
import { FactionSelectionControl } from "#/components/FactionSelectionControl.tsx";
import {
  DEFAULT_CO_PORTRAIT_KEY,
  getCoPortraitByAwbwId,
  listCoPortraits,
  loadCoPortraitCatalog,
  type CoPortraitCatalog,
} from "#/components/co_portraits.ts";
import { PlayerHeader } from "#/components/PlayerHeader.tsx";
import { usePreviewRunner } from "#/engine/runtime_context.tsx";
import { defaultFactionIdForSlot, getFactionById } from "#/factions.ts";
import { MatchMapPreview } from "#/matches/components/MatchMapPreview.tsx";
import { mutateMatchFn } from "#/matches/matches.functions.ts";
import { matchKeys } from "#/matches/matches.keys.ts";
import { matchDetailQueryOptions } from "#/matches/matches.queries.ts";
import type { MatchMutationRequest, MatchSnapshot } from "#/matches/schemas.ts";

const selectableCoOptions = listCoPortraits().filter(
  (option) => option.key !== DEFAULT_CO_PORTRAIT_KEY,
);
const coSelectorOptions = [
  { label: "No CO", value: "none" },
  ...selectableCoOptions.map((option) => ({
    label: option.displayName,
    value: String(option.awbwId),
  })),
];

export function MatchLobbyPage({
  matchId,
  joinSlug,
}: {
  matchId: string;
  joinSlug: string | null;
}) {
  const queryClient = useQueryClient();
  const session = useAppSession();
  const previewRunner = usePreviewRunner("match-lobby");
  const portraitCatalog = useMemo(() => loadCoPortraitCatalog(), []);
  const detailQueryOptions = matchDetailQueryOptions(matchId, joinSlug);
  const { data: match } = useSuspenseQuery(detailQueryOptions);
  const mapQuery = useQuery(awbwMapDataQueryOptions(match.mapId));
  const mapData = mapQuery.data ?? null;
  const [actionError, setActionError] = useState<string | null>(null);
  const [pendingAction, setPendingAction] = useState<string | null>(null);

  useEffect(() => {
    setActionError(null);
    setPendingAction(null);
  }, [matchId, joinSlug]);

  const currentUserId = session?.user.id ?? null;
  const participantsBySlot = useMemo(
    () => new Map(match.participants.map((participant) => [participant.slotIndex, participant])),
    [match],
  );
  const myParticipant =
    currentUserId === null
      ? null
      : (match.participants.find((participant) => participant.userId === currentUserId) ?? null);

  const matchMutation = useMutation({
    mutationFn: (action: MatchMutationRequest) => mutateMatchFn({ data: { matchId, action } }),
    onMutate: async (action) => {
      await queryClient.cancelQueries({ queryKey: detailQueryOptions.queryKey });
      const previousMatch = queryClient.getQueryData<MatchSnapshot>(detailQueryOptions.queryKey);

      if (action.action === "updateParticipant" && previousMatch && currentUserId !== null) {
        queryClient.setQueryData<MatchSnapshot>(detailQueryOptions.queryKey, {
          ...previousMatch,
          participants: previousMatch.participants.map((participant) =>
            participant.userId !== currentUserId
              ? participant
              : {
                  ...participant,
                  ...(action.coId !== undefined ? { coId: action.coId } : {}),
                  ...(action.factionId !== undefined ? { factionId: action.factionId } : {}),
                  ...(action.ready !== undefined ? { ready: action.ready } : {}),
                },
          ),
        });
      }

      return { previousMatch };
    },
    onError: (error, _action, context) => {
      if (context?.previousMatch) {
        queryClient.setQueryData(detailQueryOptions.queryKey, context.previousMatch);
      }
      setActionError(error instanceof Error ? error.message : "Lobby update failed.");
    },
    onSuccess: async (response) => {
      queryClient.setQueryData(detailQueryOptions.queryKey, response.match);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: matchKeys.browse() }),
        queryClient.invalidateQueries({ queryKey: matchKeys.mine() }),
      ]);
    },
  });

  async function submitAction(action: MatchMutationRequest, pendingLabel: string): Promise<void> {
    setPendingAction(pendingLabel);
    setActionError(null);
    try {
      await matchMutation.mutateAsync(action);
    } catch {
      // onError owns the user-facing message and optimistic rollback.
    } finally {
      setPendingAction(null);
    }
  }

  const shareUrl =
    match.isPrivate && match.joinSlug && typeof window !== "undefined"
      ? `${window.location.origin}/matches/${match.matchId}?join=${match.joinSlug}`
      : null;

  return (
    <Section padding={6} variant="transparent">
      <VStack gap={6}>
        <Grid align="end" columns={{ minWidth: 320, max: 2, repeat: "fit" }} gap={5}>
          <VStack gap={2}>
            <Text color="accent" type="supporting" weight="bold">
              {formatPhaseLabel(match.phase)}
            </Text>
            <Heading level={1} type="display-2">
              {match.name}
            </Heading>
            <Text color="secondary" type="large">
              Map {match.mapId} · {match.maxPlayers} players ·{" "}
              {match.isPrivate ? "Private invite" : "Open lobby"}
            </Text>
          </VStack>
          <MetadataList columns="single" label={{ position: "start", width: 120 }}>
            <MetadataListItem label="Creator">{match.creatorName}</MetadataListItem>
            <MetadataListItem label="Match rules">
              {match.settings.fogEnabled ? "Fog on" : "Fog off"} ·{" "}
              {match.settings.startingFunds.toLocaleString()} funds
            </MetadataListItem>
            {shareUrl ? (
              <MetadataListItem label="Private join link">{shareUrl}</MetadataListItem>
            ) : null}
          </MetadataList>
        </Grid>

        <Grid align="start" columns={{ minWidth: 300, max: 2, repeat: "fit" }} gap={6}>
          <Section padding={5} variant="muted">
            <VStack gap={4}>
              <VStack gap={1}>
                <Text color="accent" type="supporting" weight="bold">
                  Map
                </Text>
                <Heading level={2}>{mapData?.Name ?? `Map ${match.mapId}`}</Heading>
                <Text color="secondary" type="supporting">
                  {mapData
                    ? `${mapData.Author} · ${mapData["Size X"]} × ${mapData["Size Y"]}`
                    : "Preview the terrain before everyone locks in."}
                </Text>
              </VStack>
              <MatchMapPreview mapId={match.mapId} runner={previewRunner} />
              <MetadataList columns={3} label={{ position: "top" }}>
                <MetadataListItem label="Layout">
                  {mapData
                    ? `${mapData["Size X"]} × ${mapData["Size Y"]}`
                    : `${match.maxPlayers} player map`}
                </MetadataListItem>
                <MetadataListItem label="Visibility">
                  {match.settings.fogEnabled ? "Fog enabled" : "Clear vision"}
                </MetadataListItem>
                <MetadataListItem label="Economy">
                  {match.settings.startingFunds.toLocaleString()} starting funds
                </MetadataListItem>
              </MetadataList>
              {mapQuery.isError ? (
                <Banner status="warning" title="Map metadata could not be loaded" />
              ) : null}
            </VStack>
          </Section>

          <Section padding={5} variant="section">
            <VStack gap={4}>
              <VStack gap={1}>
                <Text color="accent" type="supporting" weight="bold">
                  Roster
                </Text>
                <Heading level={2}>Choose CO and army look</Heading>
              </VStack>

              {actionError ? (
                <Banner description={actionError} status="error" title="Lobby update failed" />
              ) : null}
              {!session ? (
                <Banner status="info" title="Sign in to claim a seat in the lobby" />
              ) : null}
              {match.phase === "starting" ? (
                <Banner status="info" title="All players are ready. Starting the match…" />
              ) : null}
              {match.phase === "active" ? (
                <Banner status="info" title="The match is active. Lobby controls are locked." />
              ) : null}

              <VStack gap={3}>
                {Array.from({ length: match.maxPlayers }, (_, slotIndex) => {
                  const participant = participantsBySlot.get(slotIndex) ?? null;
                  const isMine = participant?.userId === currentUserId;
                  const fallbackFactionId =
                    participant?.factionId ?? defaultFactionIdForSlot(slotIndex);
                  const faction = getFactionById(fallbackFactionId);
                  const isInteractive = isMine && match.phase === "lobby";
                  const isLocked = pendingAction !== null || match.phase !== "lobby";

                  return (
                    <Section key={slotIndex} padding={0} variant="muted">
                      <VStack gap={2}>
                        <PlayerHeader
                          factionCode={faction?.code ?? "os"}
                          name={participant ? participant.userName : "Open seat"}
                          trailing={
                            participant !== null ? (
                              <FactionSelectionControl
                                disabled={isLocked}
                                factionCode={faction?.code ?? "os"}
                                onChange={(nextValue) =>
                                  submitAction(
                                    {
                                      action: "updateParticipant",
                                      factionId: nextValue,
                                      joinSlug,
                                    },
                                    "faction",
                                  )
                                }
                              />
                            ) : null
                          }
                        />

                        {participant === null ? (
                          <Section padding={3} variant="transparent">
                            <Button
                              clickAction={() =>
                                submitAction(
                                  {
                                    action: "join",
                                    slotIndex,
                                    factionId: defaultFactionIdForSlot(slotIndex),
                                    joinSlug,
                                  },
                                  `join-${slotIndex}`,
                                )
                              }
                              icon={<PlusIcon aria-hidden height={14} width={14} />}
                              isDisabled={isLocked || !session || myParticipant !== null}
                              label="Claim seat"
                              size="sm"
                              variant="primary"
                              width="100%"
                            />
                          </Section>
                        ) : (
                          <Section padding={3} variant="transparent">
                            <VStack gap={3}>
                              <CoSelectionControl
                                catalog={portraitCatalog}
                                coId={participant.coId}
                                disabled={isLocked}
                                interactive={isInteractive}
                                onChange={(nextValue) =>
                                  void submitAction(
                                    { action: "updateParticipant", coId: nextValue, joinSlug },
                                    "co",
                                  )
                                }
                              />
                              <Text
                                color={participant.ready ? "accent" : "secondary"}
                                type="supporting"
                                weight="bold"
                              >
                                {participant.ready
                                  ? "Ready"
                                  : match.phase === "active"
                                    ? "In match"
                                    : "Waiting"}
                              </Text>
                              {isMine ? (
                                <HStack gap={2} wrap="wrap">
                                  <Button
                                    clickAction={() =>
                                      submitAction(
                                        {
                                          action: "updateParticipant",
                                          ready: !participant.ready,
                                          joinSlug,
                                        },
                                        "ready",
                                      )
                                    }
                                    icon={
                                      participant.ready ? (
                                        <CancelIcon aria-hidden height={14} width={14} />
                                      ) : (
                                        <CheckIcon aria-hidden height={14} width={14} />
                                      )
                                    }
                                    isDisabled={isLocked}
                                    label={participant.ready ? "Unready" : "Ready up"}
                                    size="sm"
                                    variant="primary"
                                  />
                                  <Button
                                    clickAction={() => submitAction({ action: "leave" }, "leave")}
                                    icon={<LogoutIcon aria-hidden height={14} width={14} />}
                                    isDisabled={isLocked}
                                    label="Leave"
                                    size="sm"
                                    variant="secondary"
                                  />
                                </HStack>
                              ) : null}
                            </VStack>
                          </Section>
                        )}
                      </VStack>
                    </Section>
                  );
                })}
              </VStack>
            </VStack>
          </Section>
        </Grid>
      </VStack>
    </Section>
  );
}

function CoSelectionControl({
  catalog,
  coId,
  disabled,
  interactive,
  onChange,
}: {
  catalog: CoPortraitCatalog;
  coId: number | null;
  disabled: boolean;
  interactive: boolean;
  onChange: (nextValue: number | null) => void;
}) {
  const selectedPortrait = getCoPortraitByAwbwId(coId);
  const title = selectedPortrait?.displayName ?? "No CO";

  return (
    <HStack align="center" gap={3} wrap="wrap">
      <CoPortrait
        catalog={catalog}
        coKey={selectedPortrait?.key ?? DEFAULT_CO_PORTRAIT_KEY}
        fallbackLabel={title}
      />
      {interactive ? (
        <Selector
          isDisabled={disabled}
          label="Commanding officer"
          onChange={(value) => onChange(value === "none" ? null : Number(value))}
          options={coSelectorOptions}
          size="sm"
          value={coId === null ? "none" : String(coId)}
          width={200}
        />
      ) : (
        <Text weight="bold">{title}</Text>
      )}
    </HStack>
  );
}

function formatPhaseLabel(phase: MatchSnapshot["phase"] | null): string {
  switch (phase) {
    case "active":
      return "Match active";
    case "starting":
      return "Match starting";
    case "completed":
      return "Match complete";
    case "cancelled":
      return "Match cancelled";
    case "draft":
      return "Draft";
    case "lobby":
      return "Lobby setup";
    default:
      return "Lobby";
  }
}
