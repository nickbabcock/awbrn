/*
 * THE BRIEFING ROOM
 *
 * THESIS: a lobby is a wait, and the two things worth doing during it are
 *   reading the ground and choosing who commands on it. So the screen is two
 *   panels that answer those, and the CO board opens inside the seat a player
 *   just claimed rather than anywhere they would have to go looking for it.
 * OWN-WORLD: the map arrives as the picture that was drawn at import rather
 *   than as a live engine surface, so the board reads at native pixels from
 *   the first paint and the lobby boots no renderer at all.
 * STORY: a player opens the link, reads the battlefield and what the match
 *   took away, claims a seat, picks a CO from the board that opens under it
 *   with the banned faces visibly struck, and readies.
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
import { colorVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useMemo, useRef, useState } from "react";
import { useMediaQuery } from "@astryxdesign/core/hooks";
import { Cancel as CancelIcon } from "pixelarticons/react/Cancel";
import { Check as CheckIcon } from "pixelarticons/react/Check";
import { Logout as LogoutIcon } from "pixelarticons/react/Logout";
import { useAppSession } from "#/auth/useAppSession.ts";
import { coDisplayName } from "#/co_roster.ts";
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
import { PlayerHeader } from "#/components/PlayerHeader.tsx";
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
const SEAT_PORTRAIT_SIZE = 48;

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

  const currentUserId = session?.user.id ?? null;
  const participantsBySlot = useMemo(
    () => new Map(match.participants.map((participant) => [participant.slotIndex, participant])),
    [match],
  );
  const hasOwnedSeat =
    currentUserId !== null &&
    match.participants.some((participant) => participant.userId === currentUserId);

  const matchMutation = useMutation({
    mutationFn: (action: MatchMutationRequest) => mutateMatchFn({ data: { matchId, action } }),
    onMutate: async (action) => {
      await queryClient.cancelQueries({ queryKey: detailQueryOptions.queryKey });
      const previousMatch = queryClient.getQueryData<MatchSnapshot>(detailQueryOptions.queryKey);

      if (action.action === "updateParticipant" && previousMatch && currentUserId !== null) {
        queryClient.setQueryData<MatchSnapshot>(detailQueryOptions.queryKey, {
          ...previousMatch,
          participants: previousMatch.participants.map((participant) =>
            participant.userId !== currentUserId || participant.slotIndex !== action.slotIndex
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
        queryClient.invalidateQueries({ queryKey: matchKeys.awaiting() }),
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
  const isLocked = pendingAction !== null || match.phase !== "lobby";
  const mapName = mapData?.metadata.name ?? mapEntry?.name ?? `Map ${match.mapId}`;

  const ownedSlots = Array.from({ length: match.maxPlayers }, (_, slotIndex) => slotIndex).filter(
    (slotIndex) =>
      currentUserId !== null && participantsBySlot.get(slotIndex)?.userId === currentUserId,
  );
  const openSlots = Array.from({ length: match.maxPlayers }, (_, slotIndex) => slotIndex).filter(
    (slotIndex) => !participantsBySlot.has(slotIndex),
  );
  const canClaim = session !== null && (match.settings.hotseatEnabled || !hasOwnedSeat);
  const readyCount = match.participants.filter((participant) => participant.ready).length;

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

  // The battlefield sits under the claim panel for a viewer without a seat,
  // where it is what they are deciding on, and beside the seat once they
  // have one, where the CO board is the decision.
  const battlefield = (
    <Card padding={0} xstyle={styles.panel}>
      <VStack gap={1} xstyle={styles.panelHead}>
        <Heading level={2}>Battlefield</Heading>
        <Text color="secondary" type="label">
          {[
            mapName === match.name ? null : mapName,
            mapData ? `${mapData.width}×${mapData.height}` : null,
            mapData ? `by ${mapData.metadata.author}` : null,
          ]
            .filter((part): part is string => part !== null)
            .join(" · ") || "Reading the battlefield"}
        </Text>
      </VStack>
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
      <VStack gap={4} xstyle={styles.panelBody}>
        <Text type="label">
          {[
            match.settings.fogEnabled ? "Fog" : "No fog",
            `${match.settings.startingFunds.toLocaleString()} funds`,
            formatClockSummary(match.settings.clock),
            `Host ${match.creatorName}`,
          ].join(" · ")}
        </Text>
        <VStack gap={2}>
          <Text type="label">Banned COs</Text>
          <BannedCoList bannedCoIds={match.settings.bannedCoIds} />
        </VStack>
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
    </Card>
  );

  return (
    <Page>
      <VStack gap={6}>
        <VStack as="header" gap={2}>
          {/* A match name is free text, so it may arrive as one unbroken run. */}
          <HStack align="center" gap={2} wrap="wrap">
            <Heading level={1} xstyle={styles.breakAnywhere}>
              {match.name}
            </Heading>
            <Badge label={formatPhaseLabel(match.phase)} variant="warning" />
            {match.settings.hotseatEnabled ? <Badge label="Hotseat" variant="blue" /> : null}
          </HStack>
          <Text color="secondary" type="large">
            {match.participants.length} of {match.maxPlayers} seats claimed · {readyCount} ready.
            The match starts when every seat is ready.
          </Text>
        </VStack>

        {/* These messages arrive without the viewer acting, now that the
            record is polled, so they are announced rather than only drawn. */}
        <VStack as="output" gap={3}>
          {actionError ? (
            <Banner description={actionError} status="error" title="Lobby update failed" />
          ) : null}
          {match.phase === "starting" ? (
            <Banner status="info" title="All players are ready. Starting the match…" />
          ) : null}
          {match.phase === "active" ? (
            <Banner status="info" title="The match is active. Lobby controls are locked." />
          ) : null}
        </VStack>

        <VStack gap={6} xstyle={styles.split}>
          <VStack gap={6} xstyle={styles.column}>
            {ownedSlots.length > 0 ? (
              ownedSlots.map((slotIndex) => {
                const participant = participantsBySlot.get(slotIndex)!;
                return (
                  <OwnSeatPanel
                    bannedCoIds={bannedCoIds}
                    factionCode={factionCodeFor(slotIndex)}
                    isLeaveConfirming={leaveConfirmingSlot === slotIndex}
                    isLocked={isLocked}
                    key={slotIndex}
                    onFactionChange={
                      match.phase !== "lobby"
                        ? undefined
                        : (nextValue) =>
                            submitAction(
                              {
                                action: "updateParticipant",
                                slotIndex,
                                factionId: nextValue,
                                joinSlug,
                              },
                              "faction",
                            )
                    }
                    onLeave={() => {
                      if (leaveConfirmingSlot !== slotIndex) {
                        setLeaveConfirmingSlot(slotIndex);
                        return;
                      }
                      setLeaveConfirmingSlot(null);
                      void submitAction({ action: "leave", slotIndex }, "leave");
                    }}
                    onPickCo={(coId) =>
                      void submitAction(
                        { action: "updateParticipant", slotIndex, coId, joinSlug },
                        "co",
                      )
                    }
                    onReadyChange={(ready) =>
                      void submitAction(
                        { action: "updateParticipant", slotIndex, ready, joinSlug },
                        "ready",
                      )
                    }
                    participant={participant}
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
            {ownedSlots.length === 0 ? battlefield : null}
          </VStack>

          <VStack gap={6} xstyle={styles.column}>
            <Card padding={0} xstyle={styles.panel}>
              <VStack gap={1} xstyle={styles.panelHead}>
                <Heading level={2}>Seats</Heading>
              </VStack>
              <VStack as="ul" gap={0} role="list" xstyle={styles.roster}>
                {Array.from({ length: match.maxPlayers }, (_, slotIndex) => (
                  <RosterRow
                    catalog={portraitCatalog}
                    factionCode={factionCodeFor(slotIndex)}
                    isMine={
                      currentUserId !== null &&
                      participantsBySlot.get(slotIndex)?.userId === currentUserId
                    }
                    key={slotIndex}
                    participant={participantsBySlot.get(slotIndex) ?? null}
                    phase={match.phase}
                    slotIndex={slotIndex}
                  />
                ))}
              </VStack>
            </Card>

            {ownedSlots.length > 0 ? battlefield : null}
          </VStack>
        </VStack>
      </VStack>
    </Page>
  );
}

/**
 * The viewer's own seat, which is the task the lobby exists for: choose the
 * commander, then ready. The board of faces is the panel's body at full size,
 * because it is the decision, and Ready is the one key that matters.
 */
function OwnSeatPanel({
  bannedCoIds,
  factionCode,
  isLeaveConfirming,
  isLocked,
  onFactionChange,
  onLeave,
  onPickCo,
  onReadyChange,
  participant,
}: {
  bannedCoIds: ReadonlySet<number>;
  factionCode: string;
  isLeaveConfirming: boolean;
  isLocked: boolean;
  onFactionChange?: (factionId: number) => void | Promise<void>;
  onLeave: () => void;
  onPickCo: (coId: number) => void;
  onReadyChange: (ready: boolean) => void;
  participant: MatchParticipantSnapshot;
}) {
  const hasCo = participant.coId !== null;
  const coName = coDisplayName(participant.coId);
  // A phone fits five small faces to a row and three large ones, and the
  // large board puts Ready ten rows down.
  const isWide = useMediaQuery("(min-width: 600px)");

  return (
    <Card padding={0} xstyle={styles.panel}>
      <VStack gap={3} xstyle={styles.panelHead}>
        <Heading level={2}>Your seat</Heading>
        <PlayerHeader
          factionCode={factionCode}
          isFactionLocked={isLocked || onFactionChange === undefined}
          name={participant.userName}
          onFactionChange={onFactionChange}
        />
      </VStack>
      <VStack gap={3} xstyle={styles.panelBody}>
        <HStack align="center" gap={2} justify="between" wrap="wrap">
          <Heading level={3}>Choose your commander</Heading>
          <Text color="secondary" type="label">
            {hasCo ? coName : "Not chosen"}
          </Text>
        </HStack>
        <CoBoard
          bannedCoIds={bannedCoIds}
          isDisabled={isLocked}
          mode="pick"
          onPick={onPickCo}
          selectedCoId={participant.coId}
          size={isWide ? "md" : "sm"}
        />
      </VStack>
      <HStack align="center" gap={3} justify="between" wrap="wrap" xstyle={styles.panelFoot}>
        <Text color="secondary">
          {participant.ready
            ? "You are ready. Changing your commander stands you down."
            : hasCo
              ? `Ready to command as ${coName}?`
              : "Pick a commander to ready up."}
        </Text>
        <HStack gap={2} wrap="wrap">
          {/* Leaving forfeits the seat and cannot be undone if someone else
              claims it. It takes two presses, and the second one says what it
              does. */}
          <Button
            clickAction={onLeave}
            icon={<LogoutIcon aria-hidden />}
            isDisabled={isLocked}
            label={isLeaveConfirming ? "Confirm leave" : "Leave"}
            variant={isLeaveConfirming ? "destructive" : "secondary"}
          />
          <Button
            clickAction={() => onReadyChange(!participant.ready)}
            icon={
              participant.ready ? (
                <CancelIcon aria-hidden height={14} width={14} />
              ) : (
                <CheckIcon aria-hidden height={14} width={14} />
              )
            }
            isDisabled={isLocked || (!hasCo && !participant.ready)}
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

/** One seat in the roster: army, face, name, commander, and where they stand. */
function RosterRow({
  catalog,
  factionCode,
  isMine,
  participant,
  phase,
  slotIndex,
}: {
  catalog: CoPortraitCatalog;
  factionCode: string;
  isMine: boolean;
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
    <HStack align="center" as="li" gap={3} xstyle={styles.rosterRow}>
      <FactionCrest factionCode={factionCode} size={24} />
      {portrait ? (
        <CoPortrait
          catalog={catalog}
          coKey={portrait.key}
          fallbackLabel={portrait.displayName}
          size={SEAT_PORTRAIT_SIZE}
        />
      ) : (
        <VStack gap={0} xstyle={styles.emptyPortrait} />
      )}
      <VStack gap={0.5} xstyle={styles.seatIdentity}>
        <HStack align="center" gap={2}>
          <Text maxLines={1} weight="bold">
            {participant ? participant.userName : `Seat ${slotIndex + 1}`}
          </Text>
          {isMine ? <Badge label="You" variant="neutral" /> : null}
        </HStack>
        <Text color="secondary" maxLines={1} type="supporting">
          {participant
            ? portrait
              ? portrait.displayName
              : "No commander yet"
            : "Waiting for a player"}
        </Text>
      </VStack>
      <Text color={status.tone} type="label">
        {status.label}
      </Text>
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
  return { label: "Waiting", tone: "secondary" };
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
  // Your seat takes seven columns of twelve and the roster and battlefield
  // the other five, so the decision leads and what it is about sits beside it.
  split: {
    display: {
      default: "flex",
      [pageLayout.desktopMedia]: "grid",
    },
    gridTemplateColumns: {
      default: null,
      [pageLayout.desktopMedia]: "minmax(0, 7fr) minmax(0, 5fr)",
    },
    alignItems: {
      default: "stretch",
      [pageLayout.desktopMedia]: "start",
    },
  },
  column: {
    minInlineSize: 0,
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
  panelFoot: {
    padding: spacingVars["--spacing-5"],
    backgroundColor: colorVars["--color-background-surface"],
    borderBlockStartColor: colorVars["--color-border-emphasized"],
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: "var(--border-width)",
  },
  roster: {
    margin: 0,
    padding: 0,
    listStyle: "none",
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
  // The map draws at its own size: large enough to read the ground, never
  // the largest thing on a screen whose job is a seat and a commander.
  mapCap: {
    inlineSize: "100%",
    maxInlineSize: "360px",
  },
  mapWell: {
    padding: spacingVars["--spacing-3"],
    backgroundColor: colorVars["--color-background-muted"],
    borderBlockEndColor: colorVars["--color-border-emphasized"],
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: "var(--border-width)",
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
