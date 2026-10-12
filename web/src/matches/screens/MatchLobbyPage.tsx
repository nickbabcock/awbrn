/*
 * THE BRIEFING ROOM
 *
 * THESIS: a lobby is a wait, and the one thing worth doing during it is
 *   choosing who commands. So the seat leads, as one column read top to
 *   bottom: the commander, then the army, then Ready. The battlefield stands
 *   beside it as reference, with the facts that bear on the pick.
 * OWN-WORLD: the map arrives as the picture that was drawn at import rather
 *   than as a live engine surface, so the board reads at native pixels from
 *   the first paint and the lobby boots no renderer at all.
 * STORY: a player opens the link, claims a seat, reads the COs in the tiers
 *   this match uses with the banned faces struck, presses one, and readies at
 *   the foot of the same panel. The other seats show that they have chosen,
 *   but not whom, until the match starts.
 */

import { useMutation, useQuery, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { Banner } from "@astryxdesign/core/Banner";
import { Badge } from "@astryxdesign/core/Badge";
import { Button } from "#/ui/Button.tsx";
import { Card } from "@astryxdesign/core/Card";
import { Heading } from "@astryxdesign/core/Heading";
import { Skeleton } from "@astryxdesign/core/Skeleton";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { VisuallyHidden } from "@astryxdesign/core/VisuallyHidden";
import { colorVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useMemo, useRef, useState } from "react";
import { useAppSession } from "#/auth/useAppSession.ts";
import { DEFAULT_CO_PORTRAIT_KEY } from "#/co_roster.ts";
import { coTierListFor, type CoTierList } from "#/co_tiers.ts";
import { mapCatalogEntryQueryOptions, mapRevisionQueryOptions } from "#/maps/maps.queries.ts";
import { mapScreenshotSize } from "#/maps/map_screenshot.ts";
import { MapPicture } from "#/maps/components/MapPicture.tsx";
import { CoPortrait } from "#/components/CoPortrait.tsx";
import { BannedCoList, CoBoard } from "#/components/CoBoard.tsx";
import {
  getCoPortraitByAwbwId,
  loadCoPortraitCatalog,
  type CoPortraitCatalog,
} from "#/components/co_portraits.ts";
import { defaultFactionIdForSlot, getFactionByCode, getFactionById } from "#/factions.ts";
import { FactionCrest } from "#/components/FactionCrest.tsx";
import {
  lobbyPollInterval,
  lobbySignature,
  STARTING_POLL_INTERVAL_MS,
} from "#/matches/lobby_poll.ts";
import { mutateMatchFn } from "#/matches/matches.functions.ts";
import { matchKeys } from "#/matches/matches.keys.ts";
import { matchDetailQueryOptions } from "#/matches/matches.queries.ts";
import { projectSeatChanges } from "#/matches/seat_changes.ts";
import { useSeatChanges } from "#/matches/useSeatChanges.ts";
import type {
  MatchMutationRequest,
  MatchParticipantSnapshot,
  MatchSnapshot,
} from "#/matches/schemas.ts";
import { RouterTextLink } from "#/ui/astryx-links.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { formatClockSummary } from "#/matches/match_clock.ts";
import { Page } from "#/ui/Page.tsx";

/** How long an armed "Confirm leave" stays armed before it disarms itself. */
const LEAVE_CONFIRM_TIMEOUT_MS = 5_000;

/** How many screen pixels a seat's CO portrait takes in the roster. */
const SEAT_PORTRAIT_SIZE = 32;

export function MatchLobbyPage({
  matchId,
  joinSlug,
}: {
  matchId: string;
  joinSlug: string | null;
}) {
  const queryClient = useQueryClient();
  const session = useAppSession();
  const portraitCatalog = useMemo(() => loadCoPortraitCatalog(), []);
  const [actionError, setActionError] = useState<string | null>(null);
  const [pendingAction, setPendingAction] = useState<string | null>(null);
  const [leaveConfirmingSlot, setLeaveConfirmingSlot] = useState<number | null>(null);
  const [lastChangeAt, setLastChangeAt] = useState(() => Date.now());
  const detailQueryOptions = matchDetailQueryOptions(matchId, joinSlug);
  // Poll for changes to other seats and the match phase. Local choices stay
  // visible until the server accepts them.
  const { data: confirmedMatch } = useSuspenseQuery({
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
  const currentUserId = session?.user.id ?? null;
  const seatChanges = useSeatChanges(
    JSON.stringify([matchId, joinSlug, currentUserId]),
    async (slotIndex, patch) => {
      const response = await mutateMatchFn({
        data: { matchId, action: { action: "updateParticipant", slotIndex, joinSlug, ...patch } },
      });
      await queryClient.cancelQueries({ queryKey: detailQueryOptions.queryKey });
      queryClient.setQueryData(detailQueryOptions.queryKey, response.match);
    },
    () =>
      Promise.all([
        queryClient.invalidateQueries({ queryKey: detailQueryOptions.queryKey }),
        queryClient.invalidateQueries({ queryKey: matchKeys.browse() }),
        queryClient.invalidateQueries({ queryKey: matchKeys.mine() }),
        queryClient.invalidateQueries({ queryKey: matchKeys.awaiting() }),
      ]),
  );
  useEffect(() => {
    seatChanges.changes.reconcile(confirmedMatch, currentUserId);
  }, [confirmedMatch, currentUserId, seatChanges.changes, seatChanges.intents]);
  const match = projectSeatChanges(confirmedMatch, currentUserId, seatChanges.intents);
  const seatError = Array.from(seatChanges.intents.values()).find((intent) => intent.error)?.error;
  const mapQuery = useQuery(mapRevisionQueryOptions(match.mapId, match.mapRevision));
  const mapData = mapQuery.data ?? null;
  // The picture of the board was drawn once, at import, and is keyed by the
  // content it draws. Reading it here is what lets the lobby show the terrain
  // without starting an engine to redraw what a PNG already holds.
  const mapEntryQuery = useQuery(mapCatalogEntryQueryOptions(match.mapId, match.mapRevision));
  const mapEntry = mapEntryQuery.data ?? null;
  // The map decides which faction each seat holds, so a seat cannot be claimed
  // before the map arrives. Until then the rows show catalog defaults and the
  // claim buttons stay disabled, which keeps the crest and the join in step.
  const slotFactionIds = mapData?.slotFactionIds ?? null;
  const bannedCoIds = useMemo(() => new Set(match.settings.bannedCoIds), [match.settings]);
  const tierList = coTierListFor({
    fogEnabled: match.settings.fogEnabled,
    playerCount: match.maxPlayers,
  });
  // The origin is unknown while rendering on the server. Resolving it after
  // mount keeps the row itself present in the first paint, so the link the host
  // came for does not appear late and push the panel down.
  const [origin, setOrigin] = useState("");

  useEffect(() => {
    setActionError(null);
    setPendingAction(null);
    setLeaveConfirmingSlot(null);
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
    if (leaveConfirmingSlot === null) return;

    const timer = setTimeout(() => setLeaveConfirmingSlot(null), LEAVE_CONFIRM_TIMEOUT_MS);
    return () => clearTimeout(timer);
  }, [leaveConfirmingSlot]);

  const participantsBySlot = useMemo(
    () => new Map(match.participants.map((participant) => [participant.slotIndex, participant])),
    [match],
  );
  const hasOwnedSeat =
    currentUserId !== null &&
    match.participants.some((participant) => participant.userId === currentUserId);

  const matchMutation = useMutation({
    mutationFn: (action: MatchMutationRequest) => mutateMatchFn({ data: { matchId, action } }),
    onMutate: () => queryClient.cancelQueries({ queryKey: detailQueryOptions.queryKey }),
    onError: (error) => {
      setActionError(error instanceof Error ? error.message : "Lobby update failed.");
    },
    onSuccess: async (response) => {
      await queryClient.cancelQueries({ queryKey: detailQueryOptions.queryKey });
      queryClient.setQueryData(detailQueryOptions.queryKey, response.match);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: matchKeys.browse() }),
        queryClient.invalidateQueries({ queryKey: matchKeys.mine() }),
        queryClient.invalidateQueries({ queryKey: matchKeys.awaiting() }),
      ]);
    },
  });

  async function submitAction(action: MatchMutationRequest, pendingLabel: string): Promise<void> {
    if (action.action === "updateParticipant") {
      if (pendingAction !== null || confirmedMatch.phase !== "lobby") return;
      const { coId, factionId, ready, slotIndex } = action;
      if (ready !== undefined && seatChanges.changes.getSnapshot().intents.size > 0) return;
      seatChanges.changes.select(slotIndex, {
        ...(coId !== undefined ? { coId, ready: false } : {}),
        ...(factionId !== undefined ? { factionId, ready: false } : {}),
        ...(ready !== undefined ? { ready } : {}),
      });
      return;
    }
    if (seatChanges.changes.getSnapshot().isSaving || pendingAction !== null) return;
    setPendingAction(pendingLabel);
    setActionError(null);
    try {
      await matchMutation.mutateAsync(action);
    } catch {
      // The mutation error handler shows the message.
    } finally {
      setPendingAction(null);
    }
  }

  function leaveSeat(slotIndex: number) {
    // Leaving forfeits the seat and cannot be undone if someone else claims it.
    // It takes two presses, and the second one says what it does.
    if (leaveConfirmingSlot !== slotIndex) {
      setLeaveConfirmingSlot(slotIndex);
      return;
    }
    setLeaveConfirmingSlot(null);
    void submitAction({ action: "leave", slotIndex }, "leave");
  }

  function setSeatReady(slotIndex: number, ready: boolean) {
    void submitAction({ action: "updateParticipant", slotIndex, ready, joinSlug }, "ready");
  }

  const sharePath =
    match.isPrivate && match.joinSlug ? `/matches/${match.matchId}?join=${match.joinSlug}` : null;
  const shareUrl = sharePath === null ? null : `${origin}${sharePath}`;
  const isLocked =
    pendingAction !== null || seatChanges.intents.size > 0 || match.phase !== "lobby";
  const mapName = mapData?.metadata.name ?? mapEntry?.name ?? `Map ${match.mapId}`;

  const ownedSlots = Array.from({ length: match.maxPlayers }, (_, slotIndex) => slotIndex).filter(
    (slotIndex) =>
      currentUserId !== null && participantsBySlot.get(slotIndex)?.userId === currentUserId,
  );
  const openSlots = Array.from({ length: match.maxPlayers }, (_, slotIndex) => slotIndex).filter(
    (slotIndex) => !participantsBySlot.has(slotIndex),
  );
  const canClaim = session !== null && (match.settings.hotseatEnabled || !hasOwnedSeat);

  function factionCodeFor(slotIndex: number): string {
    const participant = participantsBySlot.get(slotIndex) ?? null;
    const factionId =
      participant?.factionId ?? slotFactionIds?.[slotIndex] ?? defaultFactionIdForSlot(slotIndex);
    return getFactionById(factionId)?.code ?? "os";
  }

  function claim(slotIndex: number): void {
    const slotFactionId = slotFactionIds?.[slotIndex] ?? null;
    if (slotFactionId === null) return;
    void submitAction(
      { action: "join", slotIndex, factionId: slotFactionId, joinSlug },
      `join-${slotIndex}`,
    );
  }

  // A commander is chosen for the ground, so the battlefield takes the widest
  // column. The map is pixel art that scales only in whole steps, and the wide
  // column lets a common map reach its second step.
  const battlefield = (
    <Card padding={0} xstyle={[styles.panel, styles.fillColumn]}>
      {/* The picture names itself; the heading is for a reader that cannot see it. */}
      <VisuallyHidden>
        <Heading level={2}>{mapName === match.name ? "Battlefield" : mapName}</Heading>
      </VisuallyHidden>
      <VStack gap={0} xstyle={styles.battlefieldSplit}>
        <VStack align="center" gap={0} xstyle={styles.mapWell}>
          {mapEntry ? (
            <VStack gap={0} xstyle={styles.mapCap}>
              <MapPicture
                alt={`The battlefield of ${mapEntry.name}`}
                sourceHeight={mapScreenshotSize("full", mapEntry.width, mapEntry.height).height}
                sourceWidth={mapScreenshotSize("full", mapEntry.width, mapEntry.height).width}
                src={mapEntry.screenshot.full}
              />
            </VStack>
          ) : (
            <Skeleton height={280} radius="none" width="100%" />
          )}
        </VStack>
        <VStack gap={4} xstyle={[styles.panelBody, styles.facts]}>
          <Text type="label">
            {[
              match.settings.fogEnabled ? "Fog" : "No fog",
              `${match.settings.startingFunds.toLocaleString()} funds`,
              formatClockSummary(match.settings.clock),
              `Host ${match.creatorName}`,
              mapName === match.name ? null : mapName,
            ]
              .filter((part): part is string => part !== null)
              .join(" · ")}
          </Text>
          {match.settings.bannedCoIds.length > 0 ? (
            <VStack gap={2}>
              <Text type="label">Banned COs</Text>
              <BannedCoList bannedCoIds={match.settings.bannedCoIds} />
            </VStack>
          ) : null}
          {shareUrl ? (
            <VStack gap={2}>
              <Text type="label">Private join link</Text>
              {/* A slug has no spaces, so the URL is one unbreakable run. */}
              <Text type="supporting" xstyle={styles.breakAnywhere}>
                {shareUrl}
              </Text>
            </VStack>
          ) : null}
          {mapQuery.isError || mapEntryQuery.isError ? (
            <Banner
              endContent={
                <Button
                  clickAction={() => {
                    void mapQuery.refetch();
                    void mapEntryQuery.refetch();
                  }}
                  isLoading={mapQuery.isFetching || mapEntryQuery.isFetching}
                  label="Retry"
                  size="sm"
                  variant="secondary"
                />
              }
              status="warning"
              title="The map could not be read"
            />
          ) : null}
        </VStack>
      </VStack>
    </Card>
  );

  return (
    <Page width="wide">
      <VStack gap={4}>
        {/* These messages arrive without the viewer acting, now that the
            record is polled, so they are announced rather than only drawn. */}
        <VStack as="output" gap={3}>
          {actionError ? (
            <Banner description={actionError} status="error" title="Lobby update failed" />
          ) : null}
          {seatError ? (
            <HStack gap={3}>
              <Text role="alert">Could not save your choice: {seatError}</Text>
              <Button
                label="Retry"
                isDisabled={seatChanges.isSaving || match.phase !== "lobby"}
                clickAction={() => seatChanges.changes.retry()}
              />
            </HStack>
          ) : null}
          {match.phase === "starting" ? (
            <Banner status="info" title="All players are ready. Starting the match…" />
          ) : null}
          {match.phase === "active" ? (
            <Banner status="info" title="The match is active. Lobby controls are locked." />
          ) : null}
        </VStack>

        {/* The name and the seats share one row, so the map and the board
            below it fit one screen. */}
        <Card padding={0} xstyle={styles.panel}>
          <HStack align="stretch" gap={0} wrap="wrap">
            <HStack align="center" gap={2} wrap="wrap" xstyle={styles.titleBlock}>
              {/* A match name is free text, so it may arrive as one unbroken run. */}
              <Heading level={1} xstyle={styles.breakAnywhere}>
                {match.name}
              </Heading>
              <Badge label={formatPhaseLabel(match.phase)} variant="warning" />
              {match.settings.hotseatEnabled ? <Badge label="Hotseat" variant="blue" /> : null}
            </HStack>
            <HStack
              as="ul"
              gap={0}
              role="list"
              wrap="wrap"
              xstyle={[styles.roster, styles.seatStrip]}
            >
              {Array.from({ length: match.maxPlayers }, (_, slotIndex) => (
                <RosterRow
                  catalog={portraitCatalog}
                  factionCode={factionCodeFor(slotIndex)}
                  isMine={
                    currentUserId !== null &&
                    participantsBySlot.get(slotIndex)?.userId === currentUserId
                  }
                  key={slotIndex}
                  // The viewer's own crest is where the army is chosen; every
                  // other crest only shows the army.
                  onFactionChange={
                    ownedSlots.includes(slotIndex) &&
                    pendingAction === null &&
                    match.phase === "lobby" &&
                    participantsBySlot.get(slotIndex)?.ready === false
                      ? (nextValue) =>
                          submitAction(
                            {
                              action: "updateParticipant",
                              slotIndex,
                              factionId: nextValue,
                              joinSlug,
                            },
                            "faction",
                          )
                      : undefined
                  }
                  participant={participantsBySlot.get(slotIndex) ?? null}
                  phase={match.phase}
                  slotIndex={slotIndex}
                />
              ))}
            </HStack>
          </HStack>
        </Card>

        <VStack gap={4} xstyle={styles.main}>
          <VStack gap={6} xstyle={styles.battlefieldColumn}>
            {battlefield}
          </VStack>
          <VStack gap={6} xstyle={styles.seatColumn}>
            {ownedSlots.length > 0 ? (
              ownedSlots.map((slotIndex) => {
                const participant = participantsBySlot.get(slotIndex)!;
                return (
                  <OwnSeatPanel
                    bannedCoIds={bannedCoIds}
                    isCoLocked={pendingAction !== null || match.phase !== "lobby"}
                    isCoSaving={
                      seatChanges.isSaving &&
                      seatChanges.intents.get(slotIndex)?.patch.coId !== undefined &&
                      seatChanges.intents.get(slotIndex)?.error === null
                    }
                    isLeaveLocked={
                      pendingAction !== null || seatChanges.isSaving || match.phase !== "lobby"
                    }
                    isLeaveConfirming={leaveConfirmingSlot === slotIndex}
                    isLocked={isLocked}
                    isReadySaving={
                      seatChanges.isSaving &&
                      seatChanges.intents.get(slotIndex)?.patch.ready !== undefined &&
                      seatChanges.intents.get(slotIndex)?.patch.coId === undefined &&
                      seatChanges.intents.get(slotIndex)?.patch.factionId === undefined
                    }
                    key={slotIndex}
                    onLeave={() => leaveSeat(slotIndex)}
                    onPickCo={(coId) =>
                      void submitAction(
                        { action: "updateParticipant", slotIndex, coId, joinSlug },
                        "co",
                      )
                    }
                    onReadyChange={(ready) => setSeatReady(slotIndex, ready)}
                    participant={participant}
                    tierList={tierList}
                  />
                );
              })
            ) : (
              <ClaimPanel
                canClaim={canClaim && slotFactionIds !== null}
                factionCodeFor={factionCodeFor}
                isLocked={isLocked}
                isSignedIn={session !== null}
                onClaim={claim}
                openSlots={openSlots}
              />
            )}
            {ownedSlots.length > 0 &&
            canClaim &&
            openSlots.length > 0 &&
            match.settings.hotseatEnabled ? (
              <ClaimPanel
                canClaim={slotFactionIds !== null}
                factionCodeFor={factionCodeFor}
                isLocked={isLocked}
                isSignedIn
                onClaim={claim}
                openSlots={openSlots}
              />
            ) : null}
          </VStack>
        </VStack>
      </VStack>
    </Page>
  );
}

/**
 * The viewer's own seat, which is the task the lobby exists for, read top to
 * bottom: the commander, then Ready at the foot, where the pick it confirms
 * ends. The army is chosen at the seat's crest in the roster. The board of faces is the panel's body, because it is the
 * decision.
 */
function OwnSeatPanel({
  bannedCoIds,
  isCoLocked,
  isCoSaving,
  isLeaveConfirming,
  isLeaveLocked,
  isLocked,
  isReadySaving,
  onLeave,
  onPickCo,
  onReadyChange,
  participant,
  tierList,
}: {
  bannedCoIds: ReadonlySet<number>;
  isCoLocked: boolean;
  isCoSaving: boolean;
  isLeaveConfirming: boolean;
  isLeaveLocked: boolean;
  isLocked: boolean;
  isReadySaving: boolean;
  onLeave: () => void;
  onPickCo: (coId: number) => void;
  onReadyChange: (ready: boolean) => void;
  participant: MatchParticipantSnapshot;
  tierList: CoTierList;
}) {
  const hasCo = participant.coId !== null;

  return (
    <Card padding={0} xstyle={[styles.panel, styles.fillColumn]}>
      <VisuallyHidden>
        <Heading level={2}>Your seat</Heading>
      </VisuallyHidden>
      <VStack gap={3} xstyle={styles.seatBody}>
        <CoBoard
          bannedCoIds={bannedCoIds}
          isDisabled={participant.ready || isCoLocked}
          isSaving={isCoSaving}
          mode="pick"
          onPick={onPickCo}
          selectedCoId={participant.coId}
          tierList={tierList}
        />
      </VStack>
      <HStack align="center" gap={3} justify="between" wrap="wrap" xstyle={styles.panelFoot}>
        {/* Leaving forfeits the seat, so it takes two presses. */}
        <Button
          clickAction={onLeave}
          isDisabled={isLeaveLocked}
          label={isLeaveConfirming ? "Confirm leave" : "Leave"}
          size="sm"
          variant={isLeaveConfirming ? "destructive" : "secondary"}
        />
        <HStack align="center" gap={3} wrap="wrap" xstyle={styles.readyGroup}>
          {hasCo ? null : (
            <Text color="secondary" type="supporting">
              Choose a commander first
            </Text>
          )}
          <Button
            clickAction={() => onReadyChange(!participant.ready)}
            isDisabled={isLocked || (!hasCo && !participant.ready)}
            isLoading={isReadySaving}
            label={participant.ready ? "Unready" : "Ready up"}
            variant={participant.ready ? "secondary" : "primary"}
          />
        </HStack>
      </HStack>
    </Card>
  );
}

/** A viewer without a seat: the open seats, each with the key that claims it. */
function ClaimPanel({
  canClaim,
  factionCodeFor,
  isLocked,
  isSignedIn,
  onClaim,
  openSlots,
}: {
  canClaim: boolean;
  factionCodeFor: (slotIndex: number) => string;
  isLocked: boolean;
  isSignedIn: boolean;
  onClaim: (slotIndex: number) => void;
  openSlots: number[];
}) {
  return (
    <Card padding={0} xstyle={styles.panel}>
      <VStack gap={1} xstyle={styles.panelHead}>
        <Heading level={2}>Take a seat</Heading>
        <Text color="secondary">
          {!isSignedIn ? (
            <>
              <RouterTextLink search={{ mode: undefined }} to="/auth">
                Sign in
              </RouterTextLink>{" "}
              to claim a seat. You can watch the lobby fill from here.
            </>
          ) : openSlots.length === 0 ? (
            "Every seat is taken. You can watch the lobby from here."
          ) : (
            "Claim a seat, then choose your commander and ready up."
          )}
        </Text>
      </VStack>
      {openSlots.length > 0 ? (
        <VStack as="ul" gap={0} role="list" xstyle={styles.roster}>
          {openSlots.map((slotIndex, index) => (
            <HStack
              align="center"
              as="li"
              gap={3}
              justify="between"
              key={slotIndex}
              xstyle={styles.rosterRow}
            >
              <HStack align="center" gap={3}>
                <FactionCrest factionCode={factionCodeFor(slotIndex)} size={24} />
                <Text weight="bold">Seat {slotIndex + 1}</Text>
                <Text color="secondary" type="supporting">
                  {getFactionByCode(factionCodeFor(slotIndex))?.displayName ?? ""}
                </Text>
              </HStack>
              <Button
                clickAction={() => onClaim(slotIndex)}
                isDisabled={isLocked || !canClaim}
                label="Claim seat"
                size="sm"
                variant={index === 0 ? "primary" : "secondary"}
              />
            </HStack>
          ))}
        </VStack>
      ) : null}
    </Card>
  );
}

/**
 * One seat in the roster: army, face, name, and where the seat stands.
 *
 * Another seat's commander is sealed until the match starts. The roster shows
 * that the seat has chosen, with a blank face, but not whom.
 */
function RosterRow({
  catalog,
  factionCode,
  isMine,
  onFactionChange,
  participant,
  phase,
  slotIndex,
}: {
  catalog: CoPortraitCatalog;
  factionCode: string;
  isMine: boolean;
  /** Present on the viewer's own seat while its army can still change. */
  onFactionChange?: (factionId: number) => void | Promise<void>;
  participant: MatchParticipantSnapshot | null;
  phase: MatchSnapshot["phase"];
  slotIndex: number;
}) {
  const status = seatStatus(participant, phase);
  const portrait =
    participant?.coId === null || participant === null
      ? null
      : getCoPortraitByAwbwId(participant.coId);

  return (
    <HStack align="center" as="li" gap={3} xstyle={[styles.rosterRow, styles.seatCell]}>
      <FactionCrest factionCode={factionCode} onChange={onFactionChange} size={24} />
      {portrait ? (
        <CoPortrait
          catalog={catalog}
          coKey={portrait.key}
          fallbackLabel={portrait.displayName}
          size={SEAT_PORTRAIT_SIZE}
        />
      ) : participant?.isCoSealed ? (
        <CoPortrait
          catalog={catalog}
          coKey={DEFAULT_CO_PORTRAIT_KEY}
          fallbackLabel="Commander sealed"
          size={SEAT_PORTRAIT_SIZE}
        />
      ) : (
        <VStack gap={0} xstyle={styles.emptyPortrait} />
      )}
      {/* The name and the standing stack, so a seat stays readable in the
          narrow strip beside the match name. */}
      <VStack gap={0.5} xstyle={styles.seatIdentity}>
        <HStack align="center" gap={2}>
          <Text maxLines={1} weight="bold">
            {participant ? participant.userName : `Seat ${slotIndex + 1}`}
          </Text>
          {isMine ? <Badge label="You" variant="neutral" /> : null}
        </HStack>
        <Text color={status.tone} type="label">
          {status.label}
        </Text>
      </VStack>
    </HStack>
  );
}

/** What a seat is doing, in one word the roster can align on. */
function seatStatus(
  participant: MatchParticipantSnapshot | null,
  phase: MatchSnapshot["phase"],
): { label: string; tone: "accent" | "secondary" } {
  if (participant === null) return { label: "Open seat", tone: "secondary" };
  if (participant.ready) return { label: "Ready", tone: "accent" };
  if (phase === "active") return { label: "In match", tone: "secondary" };
  if (participant.coId !== null || participant.isCoSealed) {
    return { label: "CO chosen", tone: "secondary" };
  }
  return { label: "Choosing", tone: "secondary" };
}

const styles = stylex.create({
  breakAnywhere: {
    overflowWrap: "anywhere",
  },
  // The picture and its placeholder sit in the same recessed well, so the
  // panel does not change height when the picture lands.
  pictureWell: {
    borderRadius: "var(--radius-element)",
    overflow: "hidden",
  },
  seatPortrait: {
    backgroundColor: colorVars["--color-background-muted"],
    borderRadius: "var(--radius-element)",
    flex: "0 0 auto",
    lineHeight: 0,
    overflow: "hidden",
  },
  // The battlefield keeps a fixed column wide enough for its map at the second
  // pixel step. The seat takes what the map leaves, and the two stack on a phone.
  main: {
    flexDirection: {
      default: "column",
      [pageLayout.desktopMedia]: "row",
    },
    alignItems: "stretch",
  },
  // The match name takes what it needs; the seats take the rest of the row.
  titleBlock: {
    flexGrow: 1,
    flexShrink: 1,
    paddingInline: spacingVars["--spacing-5"],
    paddingBlock: spacingVars["--spacing-3"],
  },
  seatStrip: {
    flexGrow: 0,
    flexBasis: {
      default: "100%",
      [pageLayout.desktopMedia]: "auto",
    },
    minInlineSize: {
      default: 0,
      [pageLayout.desktopMedia]: "30rem",
    },
    borderBlockStartColor: colorVars["--color-border-emphasized"],
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: {
      default: "var(--border-width)",
      [pageLayout.desktopMedia]: 0,
    },
    borderInlineStartColor: colorVars["--color-border-emphasized"],
    borderInlineStartStyle: "solid",
    borderInlineStartWidth: {
      default: 0,
      [pageLayout.desktopMedia]: "var(--border-width)",
    },
  },
  // The map and the seat run to one height, and Ready stays at the seat's foot.
  fillColumn: {
    display: "flex",
    flexDirection: "column",
    blockSize: "100%",
  },
  seatBody: {
    flexGrow: 1,
    padding: spacingVars["--spacing-4"],
  },
  battlefieldColumn: {
    flexGrow: 1,
    minInlineSize: 0,
  },
  // The seat keeps a fixed column: wide enough for eight faces to a row, so
  // every tier is one row, and narrow enough to leave a 20 by 20 map its
  // second pixel step.
  seatColumn: {
    flexShrink: 0,
    width: {
      default: "auto",
      [pageLayout.desktopMedia]: "640px",
    },
  },
  panel: {
    overflow: "hidden",
  },
  panelHead: {
    padding: spacingVars["--spacing-5"],
    borderBlockEndColor: colorVars["--color-border-emphasized"],
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: "var(--border-width)",
  },
  panelBody: {
    padding: spacingVars["--spacing-5"],
  },
  // Ready closes the seat, under the board it confirms.
  panelFoot: {
    paddingInline: spacingVars["--spacing-5"],
    paddingBlock: spacingVars["--spacing-4"],
    backgroundColor: colorVars["--color-background-muted"],
    borderBlockStartColor: colorVars["--color-border-emphasized"],
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: "var(--border-width)",
  },
  readyGroup: {
    marginInlineStart: "auto",
    justifyContent: "end",
  },
  roster: {
    margin: 0,
    padding: 0,
    listStyle: "none",
  },
  // A seat in the strip sits beside the others, divided by a rule rather than
  // stacked under one.
  seatCell: {
    flexBasis: {
      default: 0,
      [pageLayout.phoneMedia]: "100%",
    },
    flexGrow: 1,
    minInlineSize: 0,
    // On a phone the seats stack, so the rule moves under each one.
    borderBlockEndWidth: {
      default: 0,
      [pageLayout.phoneMedia]: "var(--border-width)",
      ":last-child": 0,
    },
    borderInlineEndColor: "var(--color-border-soft)",
    borderInlineEndStyle: "solid",
    borderInlineEndWidth: {
      default: "var(--border-width)",
      ":last-child": 0,
      [pageLayout.phoneMedia]: 0,
    },
  },
  rosterRow: {
    paddingInline: spacingVars["--spacing-5"],
    paddingBlock: spacingVars["--spacing-3"],
    borderBlockEndColor: "var(--color-border-soft)",
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: {
      default: "var(--border-width)",
      ":last-child": 0,
    },
  },
  // The map fills its column. A small map cannot show the ground, so it takes
  // the room the roster does not need.
  mapCap: {
    inlineSize: "100%",
  },
  // The map and its facts sit side by side on a desktop and stack on a phone.
  battlefieldSplit: {
    flexDirection: "column",
    flexGrow: 1,
  },
  mapWell: {
    padding: spacingVars["--spacing-3"],
    backgroundColor: colorVars["--color-background-muted"],
    flexGrow: 1,
    minInlineSize: 0,
  },
  facts: {
    borderBlockStartColor: colorVars["--color-border-emphasized"],
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: "var(--border-width)",
  },
  // An open seat has no face yet: a dashed cell the size of one.
  emptyPortrait: {
    flexShrink: 0,
    inlineSize: `${SEAT_PORTRAIT_SIZE}px`,
    blockSize: `${SEAT_PORTRAIT_SIZE}px`,
    borderColor: "var(--color-border-soft)",
    borderStyle: "dashed",
    borderWidth: "var(--border-width)",
    borderRadius: "var(--radius-element)",
  },
  seatIdentity: {
    minWidth: 0,
    flexGrow: 1,
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
