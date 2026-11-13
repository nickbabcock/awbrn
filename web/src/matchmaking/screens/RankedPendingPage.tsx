/*
 * A ranked pairing to confirm. The server chose the map, the army and the
 * turn order; the player chooses a commander.
 *
 * The pairing shows the match and not the person. The server withholds the
 * other player's name, rating and commander until both are ready, so nobody
 * can decline the opponents they do not want and take the rest.
 */

import { Heading } from "@astryxdesign/core/Heading";
import { Icon } from "@astryxdesign/core/Icon";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { colorVars } from "@astryxdesign/core/theme/tokens.stylex";
import { useMutation, useQuery, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import * as stylex from "@stylexjs/stylex";
import { useNavigate } from "@tanstack/react-router";
import { useEffect, useId, useMemo, useState } from "react";
import { useAppSession } from "#/auth/useAppSession.ts";
import { coTierListFor } from "#/co_tiers.ts";
import { FactionLogo } from "#/components/FactionLogo.tsx";
import { getFactionById } from "#/factions.ts";
import { mapCatalogEntryQueryOptions, mapRevisionQueryOptions } from "#/maps/maps.queries.ts";
import { matchKeys } from "#/matches/matches.keys.ts";
import { matchDetailQueryOptions } from "#/matches/matches.queries.ts";
import type { MatchSnapshot, RankedConfirmationRequest } from "#/matches/schemas.ts";
import { projectSeatChanges } from "#/matches/seat_changes.ts";
import { useSeatChanges } from "#/matches/useSeatChanges.ts";
import { Countdown } from "#/matchmaking/components/Countdown.tsx";
import { updateRankedConfirmationFn } from "#/matchmaking/matchmaking.functions.ts";
import { rankedKeys } from "#/matchmaking/matchmaking.keys.ts";
import { rankedOverviewQueryOptions } from "#/matchmaking/matchmaking.queries.ts";
import { rankedPoolCopy } from "#/matchmaking/ranked_display.ts";
import { CoBrief } from "#/play/CoBrief.tsx";
import { useOpeningCo } from "#/play/opening_co.ts";
import { SeatStamp } from "#/play/Opponents.tsx";
import {
  BattlefieldPeek,
  BattlefieldPicture,
  Fact,
  SetupClock,
  setupStyles,
} from "#/play/setup.tsx";
import { TaskDock } from "#/play/TaskDock.tsx";
import { TierPicker } from "#/play/TierPicker.tsx";
import { Button } from "#/ui/Button.tsx";
import { RouterButton } from "#/ui/astryx-links.tsx";
import { BackLink } from "#/ui/BackKey.tsx";
import { ProblemBanner } from "#/ui/EmptySheet.tsx";
import { IconCheck } from "#/ui/icons.tsx";
import { ModalSheet } from "#/ui/ModalSheet.tsx";
import { Page } from "#/ui/Page.tsx";
import { Sheet, SheetBody, SheetFoot } from "#/ui/Sheet.tsx";

/** How often the page asks whether the other player has readied. */
const CONFIRMATION_POLL_INTERVAL_MS = 15_000;

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
  const briefId = useId();
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
  const mapName = mapQuery.data?.metadata.name ?? mapEntryQuery.data?.name ?? `Map ${match.mapId}`;
  const { data: overview } = useQuery(rankedOverviewQueryOptions());
  const pool = overview?.pools.find((entry) =>
    entry.pending.some((pairing) => pairing.matchId === matchId),
  )?.pool;

  const currentUserId = session?.user.id ?? null;
  const seatChanges = useSeatChanges(
    JSON.stringify([matchId, joinSlug, currentUserId]),
    async (_slotIndex, patch) => {
      if (patch.coId === undefined || patch.coId === null) return;
      await updateRankedConfirmationFn({
        data: { matchId, action: { action: "selectCommander", coId: patch.coId } },
      });
      await queryClient.cancelQueries({ queryKey: detailQueryOptions.queryKey });
      const confirmed = await queryClient.fetchQuery({ ...detailQueryOptions, staleTime: 0 });
      const saved = confirmed.participants.find(
        (participant) => participant.userId === currentUserId,
      );
      if (saved?.coId !== patch.coId) throw new Error("The server did not confirm your choice.");
    },
    () =>
      Promise.all([
        queryClient.invalidateQueries({ queryKey: detailQueryOptions.queryKey }),
        queryClient.invalidateQueries({ queryKey: rankedKeys.overview() }),
        queryClient.invalidateQueries({ queryKey: matchKeys.mine() }),
        queryClient.invalidateQueries({ queryKey: matchKeys.awaiting() }),
      ]),
  );
  useEffect(() => {
    seatChanges.changes.reconcile(match, currentUserId);
  }, [match, currentUserId, seatChanges.changes, seatChanges.intents]);
  const displayMatch = projectSeatChanges(match, currentUserId, seatChanges.intents);
  const seat = displayMatch.participants.find(
    (participant) => participant.userId === currentUserId,
  );
  const seatError = Array.from(seatChanges.intents.values()).find((intent) => intent.error)?.error;
  const faction = seat ? getFactionById(seat.factionId) : null;
  const bannedCoIds = useMemo(() => new Set(match.settings.bannedCoIds), [match.settings]);
  const tierList = coTierListFor({
    fogEnabled: match.settings.fogEnabled,
    playerCount: match.maxPlayers,
  });

  const [error, setError] = useState<string | null>(null);
  const [isDeclineOpen, setIsDeclineOpen] = useState(false);
  const [previewCoId, setPreviewCoId] = useState<number | null>(null);

  // The briefing opens on a commander (the player's last, else Andy) rather
  // than an empty box, so Ready is the red key from the first look. It is
  // only shown until the player acts: Ready saves it first, and a tap on
  // another stamp replaces it. Looking at a pairing never writes to it.
  const needsOpeningCo =
    seat !== undefined && seat.coId === null && !seat.ready && match.phase === "pending";
  const openingCo = useOpeningCo(bannedCoIds, matchId, { isEnabled: needsOpeningCo });
  const shownCoId = seat?.coId ?? (needsOpeningCo ? (openingCo ?? null) : null);

  // One press can send two requests: the opening commander, then Ready.
  const confirmation = useMutation({
    mutationFn: async (actions: RankedConfirmationRequest[]) => {
      for (const action of actions) {
        await updateRankedConfirmationFn({ data: { matchId, action } });
      }
    },
    onError: (mutationError: Error) => {
      setError(mutationError.message);
    },
    onSuccess: async (_result, actions) => {
      setError(null);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: detailQueryOptions.queryKey }),
        queryClient.invalidateQueries({ queryKey: rankedKeys.overview() }),
        queryClient.invalidateQueries({ queryKey: matchKeys.mine() }),
        queryClient.invalidateQueries({ queryKey: matchKeys.awaiting() }),
      ]);
      if (actions.some((action) => action.action === "refuse")) {
        await queryClient.invalidateQueries({ queryKey: rankedOverviewQueryOptions().queryKey });
        await navigate({ to: "/ranked" });
      }
    },
  });

  // The window ran out or the other player declined: nothing is owed now.
  if (match.phase === "cancelled") return <ClosedPairing match={match} />;

  if (!seat) {
    return (
      <Page>
        <ProblemBanner
          action={<BackLink label="Ranked" to="/ranked" />}
          title="This pairing is not yours"
        />
      </Page>
    );
  }

  const deadlineAt = match.confirmationDeadlineAt;
  // At the deadline the server closes the window; ask at once rather than at
  // the next poll, so the page does not offer Ready for up to 15 seconds more.
  const recheck = () => {
    void queryClient.invalidateQueries({ queryKey: detailQueryOptions.queryKey });
  };
  const isReady = seat.ready;
  const isOpen = match.phase === "pending";
  const isBusy = confirmation.isPending || seatChanges.isSaving;
  const hasUnsavedChoice = seatChanges.intents.size > 0;
  const canReady = shownCoId !== null && !hasUnsavedChoice && !isBusy && isOpen;

  function ready() {
    if (seatChanges.changes.getSnapshot().intents.size > 0 || shownCoId === null) return;
    confirmation.mutate(
      seat?.coId === null
        ? [{ action: "selectCommander", coId: shownCoId }, { action: "ready" }]
        : [{ action: "ready" }],
    );
  }

  function askDecline() {
    if (seatChanges.changes.getSnapshot().isSaving || confirmation.isPending) return;
    setIsDeclineOpen(true);
  }

  // Until a commander is chosen, Ready waits as a plain key on the well.
  const isWaitingForCo = shownCoId === null && !isBusy;
  const readyKey = (dockStyle?: boolean) => (
    <Button
      clickAction={ready}
      isDisabled={!canReady}
      isLoading={isBusy}
      label="Ready"
      size="lg"
      variant={isWaitingForCo ? "secondary" : "primary"}
      xstyle={[
        dockStyle && [setupStyles.dockKey, setupStyles.dockKeyMain],
        isWaitingForCo && setupStyles.waitingKey,
      ]}
    />
  );
  const declineKey = (dockStyle?: boolean) => (
    <Button
      isDisabled={isBusy || !isOpen}
      label="Decline"
      onClick={askDecline}
      size="lg"
      variant="destructive"
      xstyle={dockStyle ? setupStyles.dockKey : undefined}
    />
  );

  return (
    <Page>
      <header>
        <BackLink label="Ranked" to="/ranked" />
        <HStack xstyle={setupStyles.head}>
          <Heading level={1} xstyle={setupStyles.title}>
            {mapName}
          </Heading>
          {deadlineAt && !isReady ? (
            <VStack xstyle={setupStyles.phoneHidden}>
              <SetupClock>
                <Countdown
                  deadlineAt={deadlineAt}
                  isOnInk
                  onExpire={recheck}
                  size="lead"
                  tone="urgent"
                />
              </SetupClock>
            </VStack>
          ) : null}
        </HStack>
      </header>

      {error ? <ProblemBanner title={error} /> : null}
      {seatError ? (
        <ProblemBanner
          action={
            <Button
              isDisabled={isBusy || !isOpen}
              label="Retry"
              onClick={() => seatChanges.changes.retry()}
              size="sm"
            />
          }
          title="Your commander was not saved"
        />
      ) : null}
      {isReady ? (
        <ProblemBanner
          icon={<Icon icon={IconCheck} />}
          status="success"
          title={
            <HStack as="span" xstyle={styles.readyLine}>
              Ready
              {deadlineAt ? (
                <Countdown deadlineAt={deadlineAt} onExpire={recheck} tone="theirs" />
              ) : null}
            </HStack>
          }
        />
      ) : null}

      <VStack xstyle={setupStyles.grid}>
        <VStack xstyle={setupStyles.side}>
          <VStack xstyle={setupStyles.first}>
            <Sheet aria-labelledby={briefId}>
              <BattlefieldPeek
                facts={
                  <>
                    <Fact isStrong>
                      {faction ? (
                        <FactionLogo factionCode={faction.code} isLabelHidden size={14} />
                      ) : null}
                      {faction?.displayName ?? "Unknown army"}
                    </Fact>
                    <Fact>{seat.slotIndex === 0 ? "Moves first" : "Moves second"}</Fact>
                    <Fact>
                      {pool
                        ? rankedPoolCopy(pool).name
                        : match.settings.fogEnabled
                          ? "Fog"
                          : "Ranked"}
                    </Fact>
                  </>
                }
                mapId={match.mapId}
                mapName={mapName}
                revision={match.mapRevision}
              />
              <SheetBody xstyle={setupStyles.pickerBody}>
                <CoBrief
                  coId={previewCoId ?? shownCoId}
                  factionCode={faction?.code ?? null}
                  headingId={briefId}
                />
              </SheetBody>
              {isReady ? null : (
                <VStack xstyle={setupStyles.phoneHidden}>
                  <SheetFoot>
                    {declineKey()}
                    {readyKey()}
                  </SheetFoot>
                </VStack>
              )}
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
                isDisabled={isReady || confirmation.isPending || !isOpen}
                mode="pick"
                onPick={(coId) => {
                  if (!confirmation.isPending) {
                    seatChanges.changes.select(seat.slotIndex, { coId, ready: false });
                  }
                }}
                onPreview={setPreviewCoId}
                tierList={tierList}
              />
            </SheetBody>
          </Sheet>
        </VStack>
      </VStack>

      {isReady ? null : (
        <TaskDock
          isOwed
          keys={
            <>
              {declineKey(true)}
              {readyKey(true)}
            </>
          }
          label="Your pick"
          line={
            <>
              <SeatStamp coId={shownCoId} factionCode={faction?.code ?? null} />
              {deadlineAt ? <Countdown deadlineAt={deadlineAt} tone="urgent" /> : null}
            </>
          }
        />
      )}

      <ModalSheet
        footer={
          <>
            <Button label="Keep pairing" onClick={() => setIsDeclineOpen(false)} />
            <Button
              clickAction={async () => {
                setIsDeclineOpen(false);
                confirmation.mutate([{ action: "refuse" }]);
              }}
              label="Confirm decline"
              variant="destructive"
              xstyle={styles.armed}
            />
          </>
        }
        isOpen={isDeclineOpen}
        onOpenChange={setIsDeclineOpen}
        purpose="form"
        title="Void this pairing?"
      >
        <Text as="p" color="secondary" xstyle={styles.declineBody}>
          Your rating is unchanged.
        </Text>
      </ModalSheet>
    </Page>
  );
}

/**
 * A pairing whose window has closed, run out or declined. The band, Ready and
 * Decline go with it, and the page is a banner with the way back to Ranked.
 */
export function ClosedPairing({ match }: { match: MatchSnapshot }) {
  const mapQuery = useQuery(mapRevisionQueryOptions(match.mapId, match.mapRevision));
  const mapEntryQuery = useQuery(mapCatalogEntryQueryOptions(match.mapId, match.mapRevision));
  const mapName = mapQuery.data?.metadata.name ?? mapEntryQuery.data?.name ?? `Map ${match.mapId}`;
  return (
    <Page>
      <header>
        <HStack xstyle={setupStyles.head}>
          <Heading level={1} xstyle={setupStyles.title}>
            {mapName}
          </Heading>
        </HStack>
      </header>
      <ProblemBanner
        action={<RouterButton label="Ranked" size="sm" to="/ranked" />}
        status="warning"
        title="This pairing has closed"
      />
    </Page>
  );
}

const styles = stylex.create({
  readyLine: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: "16px",
    inlineSize: "100%",
  },
  declineBody: {
    paddingBlock: "16px",
    paddingInline: "22px",
    color: colorVars["--color-text-secondary"],
  },
  // Armed: the destructive key fills with danger.
  armed: {
    backgroundColor: "var(--color-danger)",
    borderColor: "var(--color-danger)",
    color: colorVars["--color-on-error"],
  },
});
