/*
 * A lobby is a wait, and the one thing worth doing during it is choosing who
 * commands, so the seat leads. Other seats show that they have chosen, but not
 * whom, until the match starts.
 */

import { Heading } from "@astryxdesign/core/Heading";
import { Icon } from "@astryxdesign/core/Icon";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { colorVars, textSizeVars } from "@astryxdesign/core/theme/tokens.stylex";
import { useMutation, useQuery, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useId, useMemo, useRef, useState, type ReactNode } from "react";
import { useAppSession } from "#/auth/useAppSession.ts";
import { coTierListFor } from "#/co_tiers.ts";
import { FactionCrest } from "#/components/FactionCrest.tsx";
import { FactionLogo } from "#/components/FactionLogo.tsx";
import { defaultFactionIdForSlot, factions, getFactionById } from "#/factions.ts";
import { seatArmies } from "#/matches/seat_armies.ts";
import { mapCatalogEntryQueryOptions, mapRevisionQueryOptions } from "#/maps/maps.queries.ts";
import {
  lobbyPollInterval,
  lobbySignature,
  STARTING_POLL_INTERVAL_MS,
} from "#/matches/lobby_poll.ts";
import { ordinal } from "#/matches/match_history.ts";
import { banDraft, seatBans, seatsAlike, type BanDraft } from "#/matches/co_bans.ts";
import { BanEditor, BanLines } from "#/matches/components/SeatBans.tsx";
import { TeamKey, TeamShape, TeamTag } from "#/matches/components/Teams.tsx";
import { mutateMatchFn } from "#/matches/matches.functions.ts";
import { matchKeys } from "#/matches/matches.keys.ts";
import { matchDetailQueryOptions } from "#/matches/matches.queries.ts";
import type { MatchMutationRequest, MatchParticipantSnapshot } from "#/matches/schemas.ts";
import { projectSeatChanges } from "#/matches/seat_changes.ts";
import { useSeatChanges } from "#/matches/useSeatChanges.ts";
import { ClosedPairing } from "#/matchmaking/screens/RankedPendingPage.tsx";
import { CoBrief } from "#/play/CoBrief.tsx";
import { headingLooks } from "#/play/headings.ts";
import { useOpeningCo } from "#/play/opening_co.ts";
import { PlayerLink } from "#/players/components/PlayerLink.tsx";
import { SeatStamp } from "#/play/Opponents.tsx";
import { BattlefieldPeek, BattlefieldPicture, Fact, setupStyles } from "#/play/setup.tsx";
import { TaskDock } from "#/play/TaskDock.tsx";
import { clockTerms } from "#/play/terms.ts";
import { TierPicker } from "#/play/TierPicker.tsx";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { RouterButton } from "#/ui/astryx-links.tsx";
import { BackLink } from "#/ui/BackKey.tsx";
import { CoStamp } from "#/components/CoStamp.tsx";
import { ProblemBanner } from "#/ui/EmptySheet.tsx";
import { figureStyles } from "#/ui/Figure.tsx";
import { IconCheck, IconLink } from "#/ui/icons.tsx";
import { ModalSheet } from "#/ui/ModalSheet.tsx";
import { Page } from "#/ui/Page.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { Sheet, SheetBody, SheetFoot, SheetHead, SheetRow, SheetRows } from "#/ui/Sheet.tsx";
import { Terms } from "#/ui/Terms.tsx";

export function MatchLobbyPage({
  matchId,
  joinSlug,
}: {
  matchId: string;
  joinSlug: string | null;
}) {
  const { data: match } = useSuspenseQuery(matchDetailQueryOptions(matchId, joinSlug));
  // Only a ranked pairing is ever canceled, and it lands here once its
  // window closes: it owes nothing now, so it is a banner back to Ranked.
  if (match.phase === "cancelled") return <ClosedPairing match={match} />;
  return <LobbyRoom joinSlug={joinSlug} matchId={matchId} />;
}

function LobbyRoom({ matchId, joinSlug }: { matchId: string; joinSlug: string | null }) {
  const queryClient = useQueryClient();
  const session = useAppSession();
  const briefId = useId();
  const seatsId = useId();
  const [actionError, setActionError] = useState<string | null>(null);
  const [pendingAction, setPendingAction] = useState<string | null>(null);
  const [leavingSlot, setLeavingSlot] = useState<number | null>(null);
  const [lastChangeAt, setLastChangeAt] = useState(() => Date.now());
  const [activeSlot, setActiveSlot] = useState<number | null>(null);
  const [previewCoId, setPreviewCoId] = useState<number | null>(null);
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
  const mapEntryQuery = useQuery(mapCatalogEntryQueryOptions(match.mapId, match.mapRevision));
  const mapEntry = mapEntryQuery.data ?? null;
  // The map decides which army each seat holds, so a seat cannot be claimed
  // before the map arrives.
  const slotFactionIds = mapData?.slotFactionIds ?? null;
  const teams = match.settings.teams ?? [];
  const isTeamMatch = teams.length > 0;
  const isHost = currentUserId !== null && currentUserId === match.creatorUserId;
  const tierList = coTierListFor({
    fogEnabled: match.settings.fogEnabled,
    playerCount: match.maxPlayers,
  });
  // The origin is unknown while rendering on the server. Resolving it after
  // mount keeps the row present in the first paint.
  const [origin, setOrigin] = useState("");

  useEffect(() => {
    setActionError(null);
    setPendingAction(null);
    setLeavingSlot(null);
    // An open ban editor drafted another match's lists; closing it must not
    // save them here.
    setBanEdit(null);
    setLastChangeAt(Date.now());
  }, [matchId, joinSlug]);

  useEffect(() => {
    setOrigin(window.location.origin);
  }, []);

  // Any real movement in the lobby restarts the poll at its quickest step, so
  // a page that has been quiet for hours becomes attentive again the moment
  // someone arrives.
  const signature = lobbySignature(match);
  const signatureRef = useRef(signature);
  useEffect(() => {
    if (signatureRef.current === signature) return;
    signatureRef.current = signature;
    setLastChangeAt(Date.now());
  }, [signature]);

  const participantsBySlot = useMemo(
    () => new Map(match.participants.map((participant) => [participant.slotIndex, participant])),
    [match],
  );
  const slots = Array.from({ length: match.maxPlayers }, (_, slotIndex) => slotIndex);
  const ownedSlots = slots.filter(
    (slotIndex) =>
      currentUserId !== null && participantsBySlot.get(slotIndex)?.userId === currentUserId,
  );
  const openSlots = slots.filter((slotIndex) => !participantsBySlot.has(slotIndex));
  // An open seat whose map army another seat took has the side that seat left.
  const armies = useMemo(
    () =>
      slotFactionIds === null
        ? null
        : seatArmies(
            slotFactionIds,
            new Map(match.participants.map((seat) => [seat.slotIndex, seat.factionId])),
            factions.map((faction) => faction.id),
          ),
    [match, slotFactionIds],
  );
  const hasOwnedSeat = ownedSlots.length > 0;
  const canClaim =
    session !== null && (match.settings.hotseatEnabled || !hasOwnedSeat) && slotFactionIds !== null;
  // The seat the briefing and the picker are for: the one the viewer chose in
  // a hotseat lobby, or else their first.
  const seatSlot =
    activeSlot !== null && ownedSlots.includes(activeSlot) ? activeSlot : (ownedSlots[0] ?? null);
  const seat = seatSlot === null ? null : (participantsBySlot.get(seatSlot) ?? null);
  // The picker hides this seat's bans: All and its own.
  const bannedCoIds = useMemo(
    () =>
      seatSlot === null ? new Set(match.settings.bannedCoIds) : seatBans(match.settings, seatSlot),
    [match.settings, seatSlot],
  );
  const viewerTeam = seatSlot === null ? null : (teams[seatSlot] ?? null);
  // The host edits the bans of the seats no person holds, so no player's list
  // changes under them.
  const lockedBanSlots = new Set(
    match.participants.filter((entry) => entry.userId !== null).map((entry) => entry.slotIndex),
  );
  const areBansAlike = seatsAlike(match.settings, match.maxPlayers);
  // A 1v1 always waits on an open seat, so there the host's Edit shows only
  // once the lists differ: an ordinary 1v1 lobby stays as it was.
  const canEditBans =
    isHost && lockedBanSlots.size < match.maxPlayers && (match.maxPlayers > 2 || !areBansAlike);
  // The lists as the editor opened, so a close saves only what this editor
  // changed, never an older copy of a seat edited elsewhere meanwhile.
  const [banEdit, setBanEdit] = useState<{ opened: BanDraft; draft: BanDraft } | null>(null);

  // A seat opens on a commander (the player's last, else Andy), so the
  // briefing leads with a stamp and Ready is the red key from the first look,
  // as in a ranked pairing. It is only shown until the player acts: Ready
  // saves it first, and a tap on another stamp replaces it.
  const needsOpeningCo =
    seat !== null && seat.coId === null && !seat.ready && confirmedMatch.phase === "lobby";
  const openingCo = useOpeningCo(bannedCoIds, matchId, { isEnabled: needsOpeningCo });
  const shownCoId = seat?.coId ?? (needsOpeningCo ? (openingCo ?? null) : null);

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

  function claim(slotIndex: number): void {
    const factionId = armies?.[slotIndex] ?? null;
    if (factionId === null) return;
    void submitAction({ action: "join", slotIndex, factionId, joinSlug }, `join-${slotIndex}`);
  }

  /**
   * Save the host's ban edits as the editor closes: one write per changed
   * seat, so a CPU picks again at most once and nobody watches a list change
   * stamp by stamp.
   */
  async function saveSeatBans(opened: BanDraft, draft: BanDraft): Promise<void> {
    if (pendingAction !== null || !isLobby) return;
    const changed = draft.seats.flatMap((coIds, slotIndex) =>
      !lockedBanSlots.has(slotIndex) && coIds.join() !== opened.seats[slotIndex]!.join()
        ? [{ slotIndex, coIds }]
        : [],
    );
    if (changed.length === 0) return;
    setPendingAction("bans");
    setActionError(null);
    try {
      // A poll already in flight would land the old lists over the saved ones.
      await queryClient.cancelQueries({ queryKey: detailQueryOptions.queryKey });
      for (const { slotIndex, coIds } of changed) {
        const response = await mutateMatchFn({
          data: { matchId, action: { action: "setSeatBans", slotIndex, coIds } },
        });
        queryClient.setQueryData(detailQueryOptions.queryKey, response.match);
      }
    } catch (error) {
      setActionError(error instanceof Error ? error.message : "Lobby update failed.");
      await queryClient.invalidateQueries({ queryKey: detailQueryOptions.queryKey });
    } finally {
      setPendingAction(null);
    }
  }

  function factionCodeFor(slotIndex: number): string {
    const participant = participantsBySlot.get(slotIndex) ?? null;
    const factionId =
      participant?.factionId ?? armies?.[slotIndex] ?? defaultFactionIdForSlot(slotIndex);
    return getFactionById(factionId)?.code ?? "os";
  }

  const sharePath =
    match.isPrivate && match.joinSlug ? `/matches/${match.matchId}?join=${match.joinSlug}` : null;
  const shareUrl = sharePath === null ? null : `${origin}${sharePath}`;
  const isLobby = match.phase === "lobby";
  const isLocked = pendingAction !== null || seatChanges.intents.size > 0 || !isLobby;
  const mapName = mapData?.metadata.name ?? mapEntry?.name ?? `Map ${match.mapId}`;
  const faction = seat ? getFactionById(seat.factionId) : null;
  const savingIntent =
    seat !== null && seatChanges.isSaving ? seatChanges.intents.get(seat.slotIndex) : undefined;
  const isCoSaving = savingIntent?.patch.coId !== undefined && savingIntent.error === null;
  const isReadySaving =
    pendingAction === "ready" ||
    (savingIntent?.patch.ready !== undefined &&
      savingIntent.patch.coId === undefined &&
      savingIntent.patch.factionId === undefined);
  const isLeaveLocked = pendingAction !== null || seatChanges.isSaving || !isLobby;

  /**
   * Ready with the commander the seat only shows: save it, then ready. The
   * server unreadies a seat whose commander changes, so these are two writes.
   */
  async function readyWithShownCo(slotIndex: number, coId: number): Promise<void> {
    if (pendingAction !== null || seatChanges.changes.getSnapshot().intents.size > 0) return;
    if (confirmedMatch.phase !== "lobby") return;
    setPendingAction("ready");
    setActionError(null);
    try {
      await mutateMatchFn({
        data: { matchId, action: { action: "updateParticipant", slotIndex, coId, joinSlug } },
      });
      await matchMutation.mutateAsync({
        action: "updateParticipant",
        slotIndex,
        ready: true,
        joinSlug,
      });
    } catch (error) {
      setActionError(error instanceof Error ? error.message : "Lobby update failed.");
    } finally {
      setPendingAction(null);
    }
  }

  const readyKey = (isDock: boolean) =>
    seat ? (
      <Button
        clickAction={() => {
          if (seat.coId === null && shownCoId !== null && !seat.ready) {
            void readyWithShownCo(seat.slotIndex, shownCoId);
            return;
          }
          void submitAction(
            {
              action: "updateParticipant",
              slotIndex: seat.slotIndex,
              ready: !seat.ready,
              joinSlug,
            },
            "ready",
          );
        }}
        isDisabled={isLocked || (shownCoId === null && !seat.ready)}
        isLoading={isReadySaving}
        label={seat.ready ? "Unready" : "Ready"}
        size="lg"
        variant={seat.ready || shownCoId === null ? "secondary" : "primary"}
        xstyle={[
          isDock && [setupStyles.dockKey, setupStyles.dockKeyMain],
          shownCoId === null && !seat.ready && setupStyles.waitingKey,
        ]}
      />
    ) : null;
  const leaveKey = (isDock: boolean) =>
    seat ? (
      <Button
        isDisabled={isLeaveLocked}
        label="Leave"
        onClick={() => setLeavingSlot(seat.slotIndex)}
        size="lg"
        variant="destructive"
        xstyle={isDock ? setupStyles.dockKey : undefined}
      />
    ) : null;
  const firstOpen = openSlots[0];
  const claimKey = (isDock: boolean) =>
    session === null ? (
      <RouterButton
        label="Sign in"
        search={{ mode: undefined }}
        size="lg"
        to="/auth"
        variant="primary"
        xstyle={isDock ? setupStyles.dockKey : undefined}
      />
    ) : canClaim && firstOpen !== undefined ? (
      <Button
        isDisabled={isLocked}
        label="Take a seat"
        onClick={() => claim(firstOpen)}
        size="lg"
        variant="primary"
        xstyle={isDock ? setupStyles.dockKey : undefined}
      />
    ) : null;

  const terms = (
    <Terms
      clock={clockTerms(match.settings.clock)}
      funds={match.settings.startingFunds}
      hasFog={match.settings.fogEnabled}
    />
  );

  return (
    <Page>
      <header>
        <BackLink label="Play" to="/matches" />
        <HStack xstyle={setupStyles.head}>
          <Heading level={1} xstyle={setupStyles.title}>
            {match.name}
          </Heading>
        </HStack>
      </header>

      {/* A failed action is urgent, so it is an alert, spoken once. */}
      {actionError ? <ProblemBanner title={actionError} /> : null}
      {/* These arrive without the viewer acting, now that the record is
          polled, so they are announced rather than only drawn. */}
      <output {...stylex.props(styles.output)}>
        {seatError ? (
          <ProblemBanner
            action={
              <Button
                isDisabled={seatChanges.isSaving || !isLobby}
                label="Retry"
                onClick={() => seatChanges.changes.retry()}
                size="sm"
              />
            }
            title="Your choice was not saved"
          />
        ) : null}
        {match.phase === "starting" ? (
          <ProblemBanner status="info" title="Everyone is ready. The match is starting" />
        ) : null}
        {match.phase === "active" ? (
          <ProblemBanner
            action={
              <RouterButton
                label="Open"
                params={{ matchId }}
                search={joinSlug ? { join: joinSlug } : {}}
                size="sm"
                to="/matches/$matchId"
              />
            }
            status="info"
            title="The match has started"
          />
        ) : null}
      </output>
      {mapQuery.isError || mapEntryQuery.isError ? (
        <ProblemBanner
          action={
            <Button
              clickAction={() => {
                void mapQuery.refetch();
                void mapEntryQuery.refetch();
              }}
              isLoading={mapQuery.isFetching || mapEntryQuery.isFetching}
              label="Retry"
              size="sm"
            />
          }
          status="warning"
          title="The map could not be read"
        />
      ) : null}

      <VStack xstyle={setupStyles.grid}>
        <VStack xstyle={setupStyles.side}>
          <VStack xstyle={setupStyles.first}>
            <Sheet
              aria-labelledby={seat ? briefId : undefined}
              aria-label={seat ? undefined : "Battlefield"}
            >
              <BattlefieldPeek
                facts={
                  <>
                    {mapName !== match.name ? <Fact isStrong>{mapName}</Fact> : null}
                    {seat && faction ? (
                      <Fact isStrong>
                        <FactionLogo factionCode={faction.code} isLabelHidden size={14} />
                        {faction.displayName}
                        {viewerTeam !== null ? (
                          <TeamTag isMine size={18} team={viewerTeam} />
                        ) : null}
                      </Fact>
                    ) : null}
                    {seat ? (
                      <Fact>
                        {seat.slotIndex === 0
                          ? "Moves first"
                          : `Moves ${ordinal(seat.slotIndex + 1)}`}
                      </Fact>
                    ) : null}
                    <Fact>{terms}</Fact>
                    {match.settings.hotseatEnabled ? <Fact>Hotseat</Fact> : null}
                  </>
                }
                mapId={match.mapId}
                mapName={mapName}
                revision={match.mapRevision}
              />
              {seat ? (
                <>
                  <SheetBody xstyle={setupStyles.pickerBody}>
                    <CoBrief
                      coId={previewCoId ?? shownCoId}
                      factionCode={faction?.code ?? null}
                      headingId={briefId}
                    />
                  </SheetBody>
                  {isLobby ? (
                    <VStack xstyle={setupStyles.phoneHidden}>
                      <SheetFoot>
                        {leaveKey(false)}
                        {readyKey(false)}
                      </SheetFoot>
                    </VStack>
                  ) : null}
                </>
              ) : (
                <SheetBody xstyle={setupStyles.pickerBody}>
                  {claimKey(false) ? (
                    <VStack hAlign="start" xstyle={setupStyles.phoneHidden}>
                      {claimKey(false)}
                    </VStack>
                  ) : openSlots.length === 0 ? (
                    <Text color="secondary">Every seat is taken</Text>
                  ) : null}
                </SheetBody>
              )}
            </Sheet>
          </VStack>

          {/* In a team match a phone shows allies and every seat's bans
              before the pick, and the host's team keys within reach. */}
          <VStack xstyle={[setupStyles.after, isTeamMatch && styles.seatsFirst]}>
            <Sheet aria-labelledby={seatsId}>
              <SheetHead>
                <HStack xstyle={styles.seatsTitle}>
                  <Heading id={seatsId} level={2} xstyle={headingLooks.title}>
                    Seats
                  </Heading>
                  {isTeamMatch ? <TeamShape teams={teams} xstyle={styles.shape} /> : null}
                </HStack>
                <Text
                  color="inherit"
                  type="inherit"
                  xstyle={[figureStyles.figure, styles.seatCount]}
                >
                  {match.participants.length} / {match.maxPlayers}
                </Text>
              </SheetHead>
              <SheetRows>
                {slots.map((slotIndex) => {
                  const participant = participantsBySlot.get(slotIndex) ?? null;
                  const isMine = ownedSlots.includes(slotIndex);
                  return (
                    <SheetRow key={slotIndex}>
                      <SeatLine
                        canClaim={canClaim && !isLocked}
                        factionCode={factionCodeFor(slotIndex)}
                        isActive={isMine && ownedSlots.length > 1 && seatSlot === slotIndex}
                        isMine={isMine}
                        onChoose={
                          isMine && ownedSlots.length > 1
                            ? () => setActiveSlot(slotIndex)
                            : undefined
                        }
                        onClaim={() => claim(slotIndex)}
                        onFactionChange={
                          isMine &&
                          pendingAction === null &&
                          isLobby &&
                          participant?.ready === false
                            ? (factionId) =>
                                submitAction(
                                  { action: "updateParticipant", slotIndex, factionId, joinSlug },
                                  "faction",
                                )
                            : undefined
                        }
                        participant={
                          participant && seat?.slotIndex === slotIndex
                            ? { ...participant, coId: shownCoId }
                            : participant
                        }
                        slotIndex={slotIndex}
                        team={
                          !isTeamMatch ? null : isHost ? (
                            <TeamKey
                              factionCodes={slots.map(factionCodeFor)}
                              isDisabled={isLocked}
                              onChange={(team) =>
                                void submitAction({ action: "setTeam", slotIndex, team }, "team")
                              }
                              slotIndex={slotIndex}
                              teams={teams}
                              viewerTeam={viewerTeam}
                            />
                          ) : (
                            <TeamTag
                              isMine={teams[slotIndex] === viewerTeam}
                              team={teams[slotIndex]!}
                            />
                          )
                        }
                      />
                    </SheetRow>
                  );
                })}
              </SheetRows>
              {canEditBans || !areBansAlike ? (
                <VStack aria-label="Banned" role="group" xstyle={styles.bans}>
                  <HStack xstyle={styles.bansHead}>
                    <Text color="inherit" type="inherit" xstyle={styles.bansLabel}>
                      Banned
                    </Text>
                    {canEditBans ? (
                      <Button
                        isDisabled={isLocked}
                        isLoading={pendingAction === "bans"}
                        label="Edit"
                        onClick={() => {
                          const opened = banDraft(match.settings, match.maxPlayers);
                          setBanEdit({ opened, draft: opened });
                        }}
                        size="sm"
                        xstyle={styles.bansEdit}
                      />
                    ) : null}
                  </HStack>
                  <BanLines
                    factionCodes={slots.map(factionCodeFor)}
                    lists={match.settings}
                    teams={teams}
                    tierList={tierList}
                    viewerSlots={ownedSlots}
                    viewerTeam={viewerTeam}
                  />
                </VStack>
              ) : null}
              {shareUrl ? (
                <SheetFoot>
                  <HStack as="span" xstyle={styles.share}>
                    <Icon icon={IconLink} size="sm" xstyle={styles.shareIcon} />
                    <Text
                      color="inherit"
                      type="inherit"
                      xstyle={[figureStyles.figure, styles.shareUrl]}
                    >
                      {shareUrl}
                    </Text>
                  </HStack>
                  <CopyLinkKey url={shareUrl} />
                </SheetFoot>
              ) : null}
            </Sheet>
          </VStack>

          <VStack xstyle={[setupStyles.after, setupStyles.phoneHidden]}>
            <Sheet aria-label="Battlefield">
              <SheetBody xstyle={setupStyles.pickerBody}>
                <BattlefieldPicture mapId={match.mapId} revision={match.mapRevision} />
              </SheetBody>
            </Sheet>
          </VStack>
        </VStack>

        <VStack xstyle={setupStyles.picker}>
          <Sheet aria-label="Commander">
            <SheetBody xstyle={setupStyles.pickerBody}>
              <TierPicker
                bannedCoIds={bannedCoIds}
                chosenCoId={shownCoId}
                factionCode={faction?.code ?? null}
                isDisabled={
                  seat === null || seat.ready || pendingAction !== null || !isLobby || isCoSaving
                }
                mode="pick"
                onPick={(coId) => {
                  if (seat) {
                    void submitAction(
                      { action: "updateParticipant", slotIndex: seat.slotIndex, coId, joinSlug },
                      "co",
                    );
                  }
                }}
                onPreview={seat ? setPreviewCoId : undefined}
                tierList={tierList}
              />
            </SheetBody>
          </Sheet>
        </VStack>
      </VStack>

      {seat && isLobby ? (
        <TaskDock
          keys={
            <>
              {leaveKey(true)}
              {readyKey(true)}
            </>
          }
          label="Your seat"
          line={
            <>
              <SeatStamp coId={shownCoId} factionCode={faction?.code ?? null} />
              {faction ? (
                <HStack as="span" xstyle={styles.dockArmy}>
                  <b>{faction.displayName}</b>
                  {viewerTeam !== null ? <TeamTag isMine size={18} team={viewerTeam} /> : null}
                </HStack>
              ) : null}
            </>
          }
        />
      ) : !seat && isLobby && claimKey(true) ? (
        <TaskDock keys={claimKey(true)} label="Take a seat" line={<b>{mapName}</b>} />
      ) : null}

      <ModalSheet
        isOpen={banEdit !== null}
        onOpenChange={(isOpen) => {
          if (isOpen || banEdit === null) return;
          setBanEdit(null);
          void saveSeatBans(banEdit.opened, banEdit.draft);
        }}
        title="Banned"
        width={760}
      >
        {banEdit ? (
          <BanEditor
            draft={banEdit.draft}
            factionCodes={slots.map(factionCodeFor)}
            isAllLocked
            lockedSlots={lockedBanSlots}
            names={slots.map((slotIndex) => participantsBySlot.get(slotIndex)?.userName ?? null)}
            onChange={(draft) => setBanEdit({ ...banEdit, draft })}
            teams={teams}
            tierList={tierList}
          />
        ) : null}
      </ModalSheet>

      <ModalSheet
        footer={
          <>
            <Button label="Keep seat" onClick={() => setLeavingSlot(null)} />
            <Button
              label="Leave"
              onClick={() => {
                const slotIndex = leavingSlot;
                setLeavingSlot(null);
                if (slotIndex !== null) void submitAction({ action: "leave", slotIndex }, "leave");
              }}
              variant="destructive"
              xstyle={styles.armed}
            />
          </>
        }
        isOpen={leavingSlot !== null}
        onOpenChange={(isOpen) => {
          if (!isOpen) setLeavingSlot(null);
        }}
        purpose="form"
        title="Leave this seat?"
      >
        <Text as="p" color="secondary" xstyle={styles.modalBody}>
          Someone else may take it.
        </Text>
      </ModalSheet>
    </Page>
  );
}

/** The viewer's own crest is where the army is chosen. */
function SeatLine({
  canClaim,
  factionCode,
  isActive,
  isMine,
  onChoose,
  onClaim,
  onFactionChange,
  participant,
  slotIndex,
  team,
}: {
  canClaim: boolean;
  factionCode: string;
  isActive: boolean;
  isMine: boolean;
  /** Present on a hotseat player's own seats: brief and pick for this one. */
  onChoose?: () => void;
  onClaim: () => void;
  onFactionChange?: (factionId: number) => void | Promise<void>;
  participant: MatchParticipantSnapshot | null;
  slotIndex: number;
  /** The seat's side: the host's key, or its tag. Null in a free-for-all. */
  team: ReactNode;
}) {
  const who = participant ? (
    <HStack as="span" xstyle={styles.who}>
      {participant.isCoSealed || participant.coId !== null || participant.userId !== null ? (
        <SeatStamp coId={participant.coId} factionCode={factionCode} />
      ) : null}
      {participant.userId !== null ? (
        <PlayerLink userId={participant.userId}>
          <b {...stylex.props(styles.name)}>{participant.userName}</b>
        </PlayerLink>
      ) : (
        <b {...stylex.props(styles.name)}>{participant.userName}</b>
      )}
      {isMine ? (
        <HStack as="span" xstyle={styles.you}>
          YOU
        </HStack>
      ) : null}
    </HStack>
  ) : (
    <HStack as="span" xstyle={styles.who}>
      <CoStamp factionCode={factionCode} isOpenSeat />
      <Text color="inherit" type="inherit" xstyle={styles.open}>
        Open
      </Text>
    </HStack>
  );

  return (
    <VStack
      xstyle={[styles.seat, team !== null && styles.seatTeams, isActive && styles.seatActive]}
    >
      <Text color="inherit" type="inherit" xstyle={[figureStyles.figure, styles.number]}>
        {slotIndex + 1}
      </Text>
      <HStack as="span" xstyle={styles.crest}>
        {onFactionChange ? (
          <FactionCrest factionCode={factionCode} onChange={onFactionChange} size={14} />
        ) : (
          <FactionLogo factionCode={factionCode} size={14} />
        )}
      </HStack>
      {onChoose ? (
        <button
          aria-pressed={isActive}
          onClick={onChoose}
          type="button"
          {...stylex.props(styles.choose)}
        >
          {who}
        </button>
      ) : (
        who
      )}
      <HStack as="span" xstyle={styles.state}>
        {participant === null ? (
          canClaim ? (
            <Button label={`Claim seat ${slotIndex + 1}`} onClick={onClaim} size="sm">
              Claim
            </Button>
          ) : null
        ) : participant.ready ? (
          // Beside a team key the check alone says it; its name stays Ready.
          <HStack
            aria-label={team !== null ? "Ready" : undefined}
            as="span"
            role={team !== null ? "img" : undefined}
            xstyle={styles.ready}
          >
            <Icon icon={IconCheck} size="sm" />
            {team !== null ? null : "Ready"}
          </HStack>
        ) : null}
      </HStack>
      {team}
    </VStack>
  );
}

function CopyLinkKey({ url }: { url: string }) {
  const [isCopied, setIsCopied] = useState(false);
  useEffect(() => {
    if (!isCopied) return;
    const timer = setTimeout(() => setIsCopied(false), 2000);
    return () => clearTimeout(timer);
  }, [isCopied]);
  return (
    <Button
      clickAction={async () => {
        await navigator.clipboard.writeText(url);
        setIsCopied(true);
      }}
      label={isCopied ? "Copied" : "Copy link"}
      size="sm"
    />
  );
}

const styles = stylex.create({
  // One column for every seat: a plain crest sits where a crest key draws its
  // own, inside the key's border and padding.
  crest: {
    paddingInlineStart: {
      default: "7px",
      ":has(button)": 0,
    },
  },
  output: {
    display: "grid",
    gap: "12px",
  },
  seatCount: {
    color: colorVars["--color-text-secondary"],
    fontSize: textSizeVars["--font-size-sm"],
    fontWeight: 600,
  },
  seat: {
    display: "grid",
    gridTemplateColumns: "16px 44px minmax(0, 1fr) auto",
    alignItems: "center",
    gap: "12px",
    minBlockSize: "56px",
    paddingBlock: "8px",
    paddingInline: {
      default: "20px",
      [pageLayout.phoneMedia]: "16px",
    },
  },
  seatTeams: {
    gridTemplateColumns: "16px 44px minmax(0, 1fr) auto auto",
  },
  seatsFirst: {
    order: {
      default: null,
      [pageLayout.phoneMedia]: 1,
    },
  },
  seatsTitle: {
    alignItems: "baseline",
    gap: "10px",
  },
  shape: {
    color: colorVars["--color-text-primary"],
    fontSize: awbrnVars.text14,
    fontWeight: 700,
  },
  bans: {
    display: "grid",
    gap: "10px",
    paddingBlock: "14px",
    paddingInline: {
      default: "20px",
      [pageLayout.phoneMedia]: "16px",
    },
    borderBlockStartColor: awbrnVars.rule,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: "1px",
  },
  bansHead: {
    alignItems: "center",
    justifyContent: "space-between",
    minBlockSize: "32px",
  },
  bansLabel: {
    fontSize: awbrnVars.text14,
    fontWeight: 650,
  },
  bansEdit: {
    minBlockSize: {
      default: null,
      [pageLayout.phoneMedia]: "44px",
    },
  },
  dockArmy: {
    display: "inline-flex",
    alignItems: "center",
    gap: "6px",
    whiteSpace: "nowrap",
  },
  seatActive: {
    backgroundColor: awbrnVars.ink100,
  },
  number: {
    color: awbrnVars.textTertiary,
    fontSize: textSizeVars["--font-size-sm"],
  },
  who: {
    display: "inline-flex",
    alignItems: "center",
    gap: "8px",
    minInlineSize: 0,
  },
  choose: {
    display: "flex",
    minInlineSize: 0,
    minBlockSize: "44px",
    padding: 0,
    borderStyle: "none",
    backgroundColor: "transparent",
    color: "inherit",
    font: "inherit",
    textAlign: "start",
    cursor: "pointer",
  },
  name: {
    display: "block",
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  open: {
    color: colorVars["--color-text-secondary"],
  },
  you: {
    display: "inline-flex",
    alignItems: "center",
    blockSize: "18px",
    paddingInline: "6px",
    borderRadius: "4px",
    backgroundColor: awbrnVars.ink100,
    color: colorVars["--color-text-secondary"],
    fontSize: awbrnVars.textMicro,
    fontWeight: 750,
    letterSpacing: "0.14em",
  },
  state: {
    display: "inline-flex",
    justifyContent: "flex-end",
  },
  ready: {
    display: "inline-flex",
    alignItems: "center",
    gap: "4px",
    color: awbrnVars.ok,
    fontSize: awbrnVars.text13_5,
    fontWeight: 650,
  },
  share: {
    display: "inline-flex",
    alignItems: "center",
    gap: "8px",
    flexGrow: 1,
    flexBasis: 0,
    minInlineSize: 0,
  },
  shareIcon: {
    flexShrink: 0,
    color: colorVars["--color-text-secondary"],
  },
  shareUrl: {
    overflow: "hidden",
    textOverflow: "ellipsis",
    color: colorVars["--color-text-secondary"],
  },
  modalBody: {
    paddingBlock: "16px",
    paddingInline: "22px",
  },
  armed: {
    backgroundColor: "var(--color-danger)",
    borderColor: "var(--color-danger)",
    color: colorVars["--color-on-error"],
  },
});
