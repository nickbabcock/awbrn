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
import { Grid } from "@astryxdesign/core/Grid";
import { Heading } from "@astryxdesign/core/Heading";
import { MetadataList, MetadataListItem } from "@astryxdesign/core/MetadataList";
import { Section } from "@astryxdesign/core/Section";
import { Skeleton } from "@astryxdesign/core/Skeleton";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { colorVars, durationVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { useEffect, useMemo, useRef, useState } from "react";
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
  DEFAULT_CO_PORTRAIT_KEY,
  getCoPortraitByAwbwId,
  loadCoPortraitCatalog,
  type CoPortraitCatalog,
} from "#/components/co_portraits.ts";
import { PlayerHeader } from "#/components/PlayerHeader.tsx";
import { defaultFactionIdForSlot, getFactionById, mapSlotFactionIds } from "#/factions.ts";
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
import { TWO_COLUMN_GRID_MIN_WIDTH } from "#/ui/layout.ts";
import { formatClockSummary } from "#/matches/match_clock.ts";

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
  const slotFactionIds = useMemo(
    () => (mapData ? mapSlotFactionIds(mapData, match.maxPlayers) : null),
    [mapData, match.maxPlayers],
  );
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

  return (
    <Section padding={6} variant="transparent">
      <VStack gap={6}>
        <Grid align="end" columns={{ minWidth: 320, max: 2, repeat: "fit" }} gap={5}>
          <VStack gap={2}>
            <Text color="accent" type="supporting" weight="bold">
              {formatPhaseLabel(match.phase)}
            </Text>
            {/* A match name is free text, so it may arrive as one unbroken run. */}
            <HStack align="center" gap={2} wrap="wrap">
              <Heading level={1} type="display-2" xstyle={styles.breakAnywhere}>
                {match.name}
              </Heading>
              {match.settings.hotseatEnabled ? <Badge label="Hotseat" variant="blue" /> : null}
            </HStack>
            <Text color="secondary" type="large">
              {mapName} · {match.maxPlayers} players ·{" "}
              {match.isPrivate ? "Private invite" : "Open lobby"}
            </Text>
          </VStack>
          <MetadataList columns="single" label={{ position: "start", width: 120 }}>
            <MetadataListItem label="Creator">{match.creatorName}</MetadataListItem>
            <MetadataListItem label="Match rules">
              {match.settings.fogEnabled ? "Fog on" : "Fog off"} ·{" "}
              {match.settings.startingFunds.toLocaleString()} funds
            </MetadataListItem>
            <MetadataListItem label="Clock">
              {formatClockSummary(match.settings.clock)}
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

        <Grid
          align="start"
          columns={{ minWidth: TWO_COLUMN_GRID_MIN_WIDTH, max: 2, repeat: "fit" }}
          gap={6}
        >
          <Card padding={5}>
            <VStack gap={4}>
              <VStack gap={1}>
                <Heading level={2}>{mapName}</Heading>
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
                <Section height={280} padding={0} variant="muted" xstyle={styles.pictureWell}>
                  <Skeleton height="100%" radius="none" />
                </Section>
              )}

              <MetadataList columns={3} label={{ position: "top" }}>
                <MetadataListItem label="Layout">
                  {mapData
                    ? `${mapData.width} × ${mapData.height}`
                    : `${match.maxPlayers} player map`}
                </MetadataListItem>
                <MetadataListItem label="Visibility">
                  {match.settings.fogEnabled ? "Fog enabled" : "Clear vision"}
                </MetadataListItem>
                <MetadataListItem label="Economy">
                  {match.settings.startingFunds.toLocaleString()} starting funds
                </MetadataListItem>
              </MetadataList>

              <VStack gap={2}>
                <Heading level={3}>Banned COs</Heading>
                <BannedCoList bannedCoIds={match.settings.bannedCoIds} />
              </VStack>

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

          <Card padding={5}>
            <VStack gap={4}>
              <VStack gap={1}>
                <Heading level={2}>Seats</Heading>
                <Text color="secondary" type="supporting">
                  {match.participants.length} of {match.maxPlayers} claimed
                </Text>
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
                  const slotFactionId = slotFactionIds?.[slotIndex] ?? null;
                  const factionId =
                    participant?.factionId ?? slotFactionId ?? defaultFactionIdForSlot(slotIndex);

                  return (
                    <SeatCard
                      bannedCoIds={bannedCoIds}
                      catalog={portraitCatalog}
                      isLocked={isLocked}
                      isMine={participant?.userId === currentUserId && currentUserId !== null}
                      canClaim={
                        session !== null &&
                        slotFactionId !== null &&
                        (match.settings.hotseatEnabled || !hasOwnedSeat)
                      }
                      factionCode={getFactionById(factionId)?.code ?? "os"}
                      isLeaveConfirming={leaveConfirmingSlot === slotIndex}
                      key={slotIndex}
                      onClaim={() =>
                        slotFactionId === null
                          ? undefined
                          : void submitAction(
                              { action: "join", slotIndex, factionId: slotFactionId, joinSlug },
                              `join-${slotIndex}`,
                            )
                      }
                      onFactionChange={
                        participant === null || match.phase !== "lobby"
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
                      onPickCo={(coId) =>
                        void submitAction(
                          { action: "updateParticipant", slotIndex, coId, joinSlug },
                          "co",
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
                      onReadyChange={(ready) =>
                        void submitAction(
                          { action: "updateParticipant", slotIndex, ready, joinSlug },
                          "ready",
                        )
                      }
                      participant={participant}
                      phase={match.phase}
                      slotIndex={slotIndex}
                    />
                  );
                })}
              </VStack>
            </VStack>
          </Card>
        </Grid>
      </VStack>
    </Section>
  );
}

/**
 * One seat, drawn the same whoever holds it.
 *
 * An open seat and a claimed one are the same card with the same parts in the
 * same places: the army above, the CO and the state beside each other, the
 * commands at the foot. A roster whose rows change shape as players arrive is
 * a roster nobody can read at a glance, which is the whole job of this panel.
 */
function SeatCard({
  bannedCoIds,
  canClaim,
  catalog,
  factionCode,
  isLeaveConfirming,
  isLocked,
  isMine,
  onClaim,
  onFactionChange,
  onLeave,
  onPickCo,
  onReadyChange,
  participant,
  phase,
  slotIndex,
}: {
  bannedCoIds: ReadonlySet<number>;
  canClaim: boolean;
  catalog: CoPortraitCatalog;
  factionCode: string;
  isLeaveConfirming: boolean;
  isLocked: boolean;
  isMine: boolean;
  onClaim: () => void;
  onFactionChange?: (factionId: number) => void | Promise<void>;
  onLeave: () => void;
  onPickCo: (coId: number) => void;
  onReadyChange: (ready: boolean) => void;
  participant: MatchParticipantSnapshot | null;
  phase: MatchSnapshot["phase"];
  slotIndex: number;
}) {
  const portrait = getCoPortraitByAwbwId(participant?.coId ?? null);
  const coName = participant === null ? "—" : coDisplayName(participant.coId);
  const status = seatStatus(participant, phase);

  // The board opens by itself on the seat that has no CO yet, which is the
  // seat the player just claimed, and closes once they have chosen. After that
  // it is theirs to open again. Deriving the default from the seat rather than
  // holding it in state is what makes it open on the claim: the card was
  // already mounted as an open seat when the player pressed it.
  const [isBoardOpen, setBoardOpen] = useState<boolean | null>(null);
  const canPick = isMine && phase === "lobby";
  const showBoard = canPick && (isBoardOpen ?? participant?.coId == null);
  const boardRef = useRef<HTMLDivElement>(null);

  // A board that opens below the fold is the same as no board, so it brings
  // itself to the player rather than waiting to be scrolled to.
  useEffect(() => {
    if (showBoard) boardRef.current?.scrollIntoView({ block: "nearest" });
  }, [showBoard]);

  return (
    <Section padding={0} variant="muted">
      <VStack gap={2}>
        <PlayerHeader
          factionCode={factionCode}
          isFactionLocked={!isMine || isLocked}
          name={participant ? participant.userName : `Seat ${slotIndex + 1} · open`}
          onFactionChange={isMine ? onFactionChange : undefined}
        />

        <Section padding={3} variant="transparent">
          <VStack gap={3}>
            <HStack align="center" gap={3}>
              <SeatPortrait
                catalog={catalog}
                coKey={portrait?.key ?? DEFAULT_CO_PORTRAIT_KEY}
                coName={coName}
                isEmpty={participant === null}
                isOpen={showBoard}
                onToggle={canPick && !isLocked ? () => setBoardOpen(!showBoard) : undefined}
              />
              <VStack gap={0.5} xstyle={styles.seatIdentity}>
                <Text maxLines={1} weight="bold">
                  {coName}
                </Text>
                <Text color={status.tone} type="label">
                  {status.label}
                </Text>
              </VStack>
            </HStack>

            {showBoard && participant !== null ? (
              <VStack gap={2} ref={boardRef}>
                <Text color="secondary" type="supporting">
                  {participant.coId === null
                    ? "Choose the commander for this seat. You cannot ready up until you do."
                    : "Press another face to change commander. Changing stands you down until you ready again."}
                </Text>
                <CoBoard
                  bannedCoIds={bannedCoIds}
                  isDisabled={isLocked}
                  mode="pick"
                  onPick={(coId) => {
                    setBoardOpen(false);
                    onPickCo(coId);
                  }}
                  selectedCoId={participant.coId}
                  size="sm"
                />
              </VStack>
            ) : null}

            {participant === null ? (
              <Button
                clickAction={onClaim}
                isDisabled={isLocked || !canClaim}
                label="Claim seat"
                size="sm"
                variant="primary"
                width="100%"
              />
            ) : isMine ? (
              <HStack gap={2} wrap="wrap">
                <Button
                  clickAction={() => onReadyChange(!participant.ready)}
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
                {/* Leaving forfeits the seat and cannot be undone if someone
                    else claims it, and this button sits 8px from Ready on a
                    phone. It takes two presses, and the second one says what
                    it does. */}
                <Button
                  clickAction={onLeave}
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
      </VStack>
    </Section>
  );
}

/**
 * The face in a seat, and the fastest way to change it.
 *
 * On the viewer's own seat the portrait is the key that opens the CO board: a
 * player who wants a different commander reaches for the face they want to
 * replace, not for a control beside it. On every other seat it is the same
 * cell without the behaviour, so the roster still reads as one row repeated.
 *
 * It wears the cursor the rest of the system uses for a chosen thing, the
 * accent outline with the accent ring inside it, while the board it opened is
 * on screen.
 */
function SeatPortrait({
  catalog,
  coKey,
  coName,
  isEmpty,
  isOpen,
  onToggle,
}: {
  catalog: CoPortraitCatalog;
  coKey: string;
  coName: string;
  isEmpty: boolean;
  isOpen: boolean;
  /** Left out on a seat the viewer cannot change, which makes it a plain cell. */
  onToggle?: () => void;
}) {
  const face = (
    <CoPortrait
      catalog={catalog}
      coKey={coKey}
      fallbackLabel={coName}
      hasFrame={false}
      size={SEAT_PORTRAIT_SIZE}
    />
  );

  if (!onToggle) {
    return (
      <Section
        padding={0}
        variant="muted"
        xstyle={[styles.seatPortrait, isEmpty && styles.seatPortraitEmpty]}
      >
        {face}
      </Section>
    );
  }

  return (
    // The label is set on the key rather than beside it, because the portrait
    // inside already names the CO and a key that reads "Change CO, now Andy
    // Andy" is what happens when both are left to be read.
    <button
      aria-expanded={isOpen}
      aria-label={isOpen ? "Close the CO board" : `Change CO, now ${coName}`}
      onClick={onToggle}
      title={isOpen ? "Close the CO board" : "Change CO"}
      type="button"
      {...stylex.props(
        styles.seatPortrait,
        styles.portraitKey,
        isOpen && styles.portraitKeyOpen,
        styles.portraitKeyReducedMotion,
      )}
    >
      {face}
    </button>
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
  // A command in this system is a key on a menu, so the portrait that opens
  // the CO board is one: the ink outline, the cast shadow, and the 2px it
  // moves into that shadow when pressed. A control that only appears on hover
  // is a control nobody knows is there.
  portraitKey: {
    borderColor: {
      default: colorVars["--color-border-emphasized"],
      ":hover": "var(--color-accent)",
      ":focus-visible": "var(--color-accent)",
    },
    borderStyle: "solid",
    borderWidth: "var(--border-width)",
    boxShadow: {
      default: "var(--shadow-low)",
      ":active": "none",
    },
    cursor: "pointer",
    display: "block",
    outline: "none",
    padding: 0,
    transform: {
      default: null,
      ":active": `translate(${awbrnVars.offsetControlPressed}, ${awbrnVars.offsetControlPressed})`,
    },
    transitionDuration: {
      default: null,
      ":active": durationVars["--duration-fast-min"],
    },
  },
  // While the board it opened is on screen the key stays down: the accent
  // outline with the accent ring inside it, flush on the panel rather than
  // above it, which is the same cursor a chosen map plate wears.
  portraitKeyOpen: {
    borderColor: "var(--color-accent)",
    boxShadow: "var(--shadow-inset-selected)",
    transform: `translate(${awbrnVars.offsetControlPressed}, ${awbrnVars.offsetControlPressed})`,
  },
  // The key still loses its shadow and keeps its accent outline, so the state
  // is legible without the 2px of travel. Applied last so it wins over both.
  portraitKeyReducedMotion: {
    transform: {
      default: null,
      "@media (prefers-reduced-motion: reduce)": "none",
    },
  },
  // An open seat shows the same portrait cell the claimed seats show, held
  // back so the row reads as waiting rather than as a player with no face.
  seatPortraitEmpty: {
    opacity: 0.4,
  },
  seatIdentity: {
    minWidth: 0,
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
