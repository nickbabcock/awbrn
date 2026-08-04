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
import * as stylex from "@stylexjs/stylex";
import { useEffect, useMemo, useRef, useState } from "react";
import { Cancel as CancelIcon } from "pixelarticons/react/Cancel";
import { Check as CheckIcon } from "pixelarticons/react/Check";
import { Logout as LogoutIcon } from "pixelarticons/react/Logout";
import { Plus as PlusIcon } from "pixelarticons/react/Plus";
import { useAppSession } from "#/auth/useAppSession.ts";
import { awbwMapDataQueryOptions } from "#/awbw/awbw.queries.ts";
import { CoPortrait } from "#/components/CoPortrait.tsx";
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
import {
  lobbyPollInterval,
  lobbySignature,
  STARTING_POLL_INTERVAL_MS,
} from "#/matches/lobby_poll.ts";
import { mutateMatchFn } from "#/matches/matches.functions.ts";
import { matchKeys } from "#/matches/matches.keys.ts";
import { matchDetailQueryOptions } from "#/matches/matches.queries.ts";
import type { MatchMutationRequest, MatchSnapshot } from "#/matches/schemas.ts";

/** How long an armed "Confirm leave" stays armed before it disarms itself. */
const LEAVE_CONFIRM_TIMEOUT_MS = 5_000;

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
  const [actionError, setActionError] = useState<string | null>(null);
  const [pendingAction, setPendingAction] = useState<string | null>(null);
  const [isLeaveConfirming, setIsLeaveConfirming] = useState(false);
  const [lastChangeAt, setLastChangeAt] = useState(() => Date.now());
  const detailQueryOptions = matchDetailQueryOptions(matchId, joinSlug);
  // The lobby is a wait, and everything worth waiting for happens on someone
  // else's request: a seat claimed, a player readying, the match starting. The
  // match record is the only place those land, because no lobby channel exists
  // until the durable object is created at start. React Query pauses this while
  // the tab is in the background, and the poll stands down while this player's
  // own change is in flight so it cannot land stale data over the optimistic
  // update.
  const { data: match } = useSuspenseQuery({
    ...detailQueryOptions,
    // The app turns focus refetching off, which is right for a page you read
    // once. A lobby is the opposite: coming back to the tab after hours is the
    // most common way a play-by-web wait ends, and it must not show yesterday.
    refetchOnWindowFocus: true,
    refetchInterval: (query) => {
      if (pendingAction !== null) return false;

      const phase = query.state.data?.phase ?? null;
      if (phase === "starting") return STARTING_POLL_INTERVAL_MS;
      if (phase !== "lobby") return false;

      return lobbyPollInterval(Date.now() - lastChangeAt);
    },
  });
  const mapQuery = useQuery(awbwMapDataQueryOptions(match.mapId));
  const mapData = mapQuery.data ?? null;
  // The origin is unknown while rendering on the server. Resolving it after
  // mount keeps the row itself present in the first paint, so the link the host
  // came for does not appear late and push the panel down.
  const [origin, setOrigin] = useState("");

  useEffect(() => {
    setActionError(null);
    setPendingAction(null);
    setIsLeaveConfirming(false);
    setLastChangeAt(Date.now());
  }, [matchId, joinSlug]);

  useEffect(() => {
    setOrigin(window.location.origin);
  }, []);

  // Any real movement in the lobby restarts the poll at its quickest step, so a
  // page that has been quiet for hours becomes attentive again the moment
  // someone arrives.
  const signature = lobbySignature(match);
  const signatureRef = useRef(signature);
  useEffect(() => {
    if (signatureRef.current === signature) return;

    signatureRef.current = signature;
    setLastChangeAt(Date.now());
  }, [signature]);

  // A confirmation the player walks away from should not stay armed.
  useEffect(() => {
    if (!isLeaveConfirming) return;

    const timer = setTimeout(() => setIsLeaveConfirming(false), LEAVE_CONFIRM_TIMEOUT_MS);
    return () => clearTimeout(timer);
  }, [isLeaveConfirming]);

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

  const sharePath =
    match.isPrivate && match.joinSlug ? `/matches/${match.matchId}?join=${match.joinSlug}` : null;
  const shareUrl = sharePath === null ? null : `${origin}${sharePath}`;

  return (
    <Section padding={6} variant="transparent">
      <VStack gap={6}>
        <Grid align="end" columns={{ minWidth: 320, max: 2, repeat: "fit" }} gap={5}>
          <VStack gap={2}>
            <Text color="accent" type="supporting" weight="bold">
              {formatPhaseLabel(match.phase)}
            </Text>
            {/* A match name is free text, so it may arrive as one unbroken run. */}
            <Heading level={1} type="display-2" xstyle={styles.breakAnywhere}>
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
              <MetadataListItem label="Private join link">
                {/* A slug has no spaces, so the URL is one unbreakable run. */}
                <Text type="supporting" xstyle={styles.breakAnywhere}>
                  {shareUrl}
                </Text>
              </MetadataListItem>
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
                <Banner
                  endContent={
                    <Button
                      clickAction={() => void mapQuery.refetch()}
                      isLoading={mapQuery.isFetching}
                      label="Retry"
                      size="sm"
                      variant="secondary"
                    />
                  }
                  status="warning"
                  title="Map metadata could not be loaded"
                />
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

              {/* These messages arrive without the viewer acting, now that the
                  record is polled, so they are announced rather than only
                  drawn. */}
              <VStack as="output" gap={3}>
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
              </VStack>

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
                          isFactionLocked={!isInteractive || isLocked}
                          name={participant ? participant.userName : "Open seat"}
                          onFactionChange={
                            participant === null || !isInteractive
                              ? undefined
                              : (nextValue) =>
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
                                  {/* Leaving forfeits the seat and cannot be
                                      undone if someone else claims it, and this
                                      button sits 8px from Ready on a phone. It
                                      takes two presses, and the second one says
                                      what it does. */}
                                  <Button
                                    clickAction={() => {
                                      if (!isLeaveConfirming) {
                                        setIsLeaveConfirming(true);
                                        return;
                                      }
                                      setIsLeaveConfirming(false);
                                      void submitAction({ action: "leave" }, "leave");
                                    }}
                                    icon={<LogoutIcon aria-hidden />}
                                    isDisabled={isLocked}
                                    label={isLeaveConfirming ? "Confirm leave" : "Leave"}
                                    size="sm"
                                    variant={isLeaveConfirming ? "destructive" : "secondary"}
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

const styles = stylex.create({
  breakAnywhere: {
    overflowWrap: "anywhere",
  },
});

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
