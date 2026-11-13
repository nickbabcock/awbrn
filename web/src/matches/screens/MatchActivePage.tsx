import { useQuery, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { Grid } from "@astryxdesign/core/Grid";
import { useMediaQuery } from "@astryxdesign/core/hooks";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Icon } from "@astryxdesign/core/Icon";
import { borderVars, radiusVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useCanvasCourierSurface, pressBoardKey } from "#/canvas_courier/index.ts";
import { TileDetails, TileLine, TileWindow } from "#/components/TileInfoBar.tsx";
import { ImmersiveNoticeLine, useImmersiveNotice } from "#/components/GameFullscreen.tsx";
import { ReviewFoot, PhoneReviewBar, type ReviewModel } from "#/components/StepControls.tsx";
import { sessionQueryOptions } from "#/auth/auth.queries.ts";
import { useAppSession } from "#/auth/useAppSession.ts";
import { useActiveMatchRunner } from "#/engine/runtime_context.tsx";
import type { GameRunner } from "#/engine/game_runner.ts";
import { useGameActions, useGameStore } from "#/engine/store.ts";
import type { LiveMatchPlayer } from "#/engine/worker_module.ts";
import { getFactionByCode } from "#/factions.ts";
import { mapCatalogEntryQueryOptions } from "#/maps/maps.queries.ts";
import { useMatchWebSocket, type MatchWebSocketStatus } from "#/matches/match_websocket.ts";
import type {
  InitialBoardMessage,
  MatchResults,
  LiveTransition,
  MatchClockMessage,
  MatchWebSocketMessage,
  ReviewBoundary,
} from "#/matches/match_protocol.ts";
import { nextTurnStart, previousTurnStart, stepTarget } from "#/matches/match_review.ts";
import {
  clockPressure,
  findClockPreset,
  formatClockDuration,
  formatClockTerms,
  seatRemainingMs,
} from "#/matches/match_clock.ts";
import { useClockNow } from "#/matches/components/SeatClock.tsx";
import { buildArmies, type MatchArmy, recordedSeat } from "#/matches/match_armies.ts";
import { matchKeys } from "#/matches/matches.keys.ts";
import { matchDetailQueryOptions } from "#/matches/matches.queries.ts";
import type { MatchSnapshot } from "#/matches/schemas.ts";
import { MatchOverDialog } from "#/matches/components/MatchOverDialog.tsx";
import {
  BATTLE_CALCULATOR_SHEET_MEDIA,
  BattleCalculator,
} from "#/matches/components/BattleCalculator.tsx";
import { BuildMenu } from "#/matches/components/BuildMenu.tsx";
import { buildMenuMount } from "#/matches/components/board_menu_mount.ts";
import { useLeaving } from "#/matches/components/useLeaving.ts";
import { plateView as pickPlateView } from "#/matches/components/plate_view.ts";
import { MatchMenu } from "#/matches/components/MatchMenu.tsx";
import { DockedAttackPreview } from "#/matches/components/AttackPreview.tsx";
import { UnitActionMenu } from "#/matches/components/UnitActionMenu.tsx";
import {
  ArmedPower,
  ArmyCard,
  ArmyPlate,
  ArmySheet,
  Ledger,
  PowerKey,
  powerName,
  readyPowers,
  TurnOrderRow,
  type ArmyClock,
  type ArmyView,
  type PowerControl,
} from "#/matches/components/Armies.tsx";
import {
  BoardKeys,
  Locator,
  useTableLayout,
  useWholeMap,
  ViewKey,
  WholeMapOutline,
  formatScale,
  useBoardView,
  FirstTouchHint,
} from "#/matches/components/BoardControls.tsx";
import { BoardTable, WaitingLine } from "#/matches/components/BoardTable.tsx";
import { TurnLog } from "#/matches/components/TurnLog.tsx";
import { dayStarts, type LogBoundary } from "#/matches/components/turn_log.ts";
import { fallDayFromOutline, recordFalls, type ArmyFall } from "#/matches/components/army_falls.ts";
import { EndTurnDialog, EndTurnKey } from "#/matches/components/EndTurn.tsx";
import {
  isAsyncClock,
  TurnAlertsOffer,
  useTurnAlertsOffer,
} from "#/matches/components/TurnAlertsOffer.tsx";
import {
  ConnectionState,
  DockKey,
  DockRow,
  FullscreenKey,
  InkIconKey,
  MatchFrame,
  MatchNotice,
  ReconnectKey,
  SignInKey,
  type MatchConnection,
  StripClock,
  TurnLine,
  TurnStrip,
  WaitCell,
  WatchingTag,
  useFullscreenEnabled,
} from "#/matches/components/MatchFrame.tsx";
import { canActivatePower } from "#/replay/power_meter.ts";
import { describeTurnResidue } from "#/matches/turn_residue.ts";
import type { ActivatablePowerLevel } from "#/replay/power_meter.ts";
import type { UnitKind } from "#/wasm/awbrn_wasm.js";
import { matchLayout, matchType } from "#/matches/components/matchLayout.stylex.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { BackKey } from "#/ui/BackKey.tsx";
import { Button } from "#/ui/Button.tsx";
import { IconCalculator, IconClose, IconMore, IconRefresh, IconReplay } from "#/ui/icons.tsx";
import { ModalSheet } from "#/ui/ModalSheet.tsx";

/** A phone: the board full-bleed, the dock under the thumb. */
const PHONE_MEDIA = "(max-width: 720px)";

/** Said when a question about the past could not be put to the host at all. */
const REVIEW_UNREACHABLE = "The match could not be read because the connection is not open.";
/** Said when the board could not be put back after a reading; a reconnect rebuilds it. */
const REVIEW_RESTORE_FAILED = "The live board could not be restored. Reconnect to catch up.";
/** The host's answer when a match's log cannot be opened for reading. */
const REVIEW_REFUSED = "this match cannot be reviewed";

export function MatchActivePage({
  joinSlug = null,
  matchId,
}: {
  joinSlug?: string | null;
  matchId: string;
}) {
  const { data: match } = useSuspenseQuery(matchDetailQueryOptions(matchId, joinSlug));
  const session = useAppSession();
  const queryClient = useQueryClient();
  const runner = useActiveMatchRunner();
  const playerRoster = useGameStore((state) => state.playerRoster);
  const setProductionOptions = useGameStore((state) => state.actions.setProductionOptions);
  const setUnitActions = useGameStore((state) => state.actions.setUnitActions);
  const livePlayers = useMemo<LiveMatchPlayer[]>(
    () =>
      match.participants.map((participant) => ({
        playerId: participant.slotIndex,
        factionId: participant.factionId,
      })),
    [match.participants],
  );
  // Reading an earlier moment of the match. The board is the engine's either
  // way; what changes is who says what goes on it. While the viewer is
  // reading, every position comes from the host, because only the host holds
  // the log an earlier board is rebuilt from — and only the host can rebuild
  // it without showing the viewer more than the fog allows.
  const [reviewOutline, setReviewOutline] = useState<ReviewBoundary[] | null>(null);
  const [reviewPosition, setReviewPosition] = useState<{ index: number; total: number } | null>(
    null,
  );
  const [isReviewing, setIsReviewing] = useState(false);
  const [isReviewBusy, setIsReviewBusy] = useState(false);
  const [reviewError, setReviewError] = useState<string | null>(null);
  /** True while the answer on its way back is the one that ends the reading. */
  const returningToLiveRef = useRef(false);
  /**
   * A turn step asked for before the outline arrived.
   *
   * Where a turn begins is a fact about the whole match, and the outline is
   * what holds it. A viewer who reaches for a turn before anything has been
   * asked of the host gets the outline fetched under them and the step taken
   * when it lands, rather than a key that does nothing the first time.
   */
  const pendingStepRef = useRef<{ kind: "action" | "turn"; delta: number } | null>(null);
  /**
   * The outline is asked for as soon as the match is open, so the scrubber has
   * its day ticks and the rail its log before anybody reaches back. Each move
   * the match makes bumps the revision, and the outline held is fresh only
   * while it answers the latest one: a step is never spelled against an
   * outline that ends before the match does.
   */
  const [outlineRevision, setOutlineRevision] = useState(0);
  const [outlineAnswered, setOutlineAnswered] = useState<number | null>(null);
  const outlineAskedRef = useRef<number | null>(null);
  /** What the engine has been told, read from inside callbacks and effects. */
  const isReviewingRef = useRef(false);
  const [initialBoard, setInitialBoard] = useState<InitialBoardMessage | null>(null);
  // Funds are in the match state the host opens with, so every army shows
  // them from the first paint rather than waiting on the engine to report.
  const openingFunds = initialBoard?.gameState?.players;
  const armies = useMemo(
    () => withOpeningFunds(buildArmies(match.participants, playerRoster), openingFunds),
    [match.participants, openingFunds, playerRoster],
  );
  const [matchError, setMatchError] = useState<string | null>(null);
  const [boardError, setBoardError] = useState<string | null>(null);
  // `undefined` until the server says which seat, if any, this viewer holds.
  const [viewerSlotIndex, setViewerSlotIndex] = useState<number | null | undefined>(undefined);
  // The seat the match record gives this account, while the connection has
  // not said yet: the rail keeps YOU on it from the first paint. A hotseat
  // account holds several, and only the server can say which one is up.
  const userId = session?.user.id ?? null;
  const recordSeat = useMemo(
    () => recordedSeat(match.participants, userId),
    [match.participants, userId],
  );
  const seatSlot = viewerSlotIndex === undefined ? recordSeat : viewerSlotIndex;
  // How each fallen army left, as the observations this page was handed say.
  const [falls, setFalls] = useState<ReadonlyMap<number, ArmyFall>>(() => new Map());
  const [spectatorFogActive, setSpectatorFogActive] = useState<boolean | null>(null);
  /**
   * How the match ended, once it has. Held rather than acted on: the result is
   * read over the board that produced it, and the page becomes a replay only
   * when the player says they are done looking at it.
   */
  const [matchResults, setMatchResults] = useState<MatchResults | null>(null);
  const [isResultOpen, setIsResultOpen] = useState(false);
  const [activePlayerSlot, setActivePlayerSlot] = useState<number | null>(null);
  // Null until the server sends the first clock message.
  const [clock, setClock] = useState<MatchClockMessage | null>(null);
  // The nav badge counts the matches waiting on this player, and the server
  // publishes the seat on the move a moment after the action that moved it.
  // The tab that made the move does not wait for the poll to read it back.
  const [isEndingTurn, setIsEndingTurn] = useState(false);
  const [isResigning, setIsResigning] = useState(false);
  const [activatingPower, setActivatingPower] = useState<ActivatablePowerLevel>();
  const isEndingTurnRef = useRef(false);
  const isResigningRef = useRef(false);
  const isActivatingPowerRef = useRef(false);
  const finishEndingTurn = useCallback(() => {
    isEndingTurnRef.current = false;
    setIsEndingTurn(false);
  }, []);
  const finishResigning = useCallback(() => {
    isResigningRef.current = false;
    setIsResigning(false);
  }, []);
  const finishActivatingPower = useCallback(() => {
    isActivatingPowerRef.current = false;
    setActivatingPower(undefined);
  }, []);
  // A command the server refuses, or one that never leaves, puts the board back
  // the way it was: the unit selected at its origin with its route intact, so a
  // retry is one press rather than the whole move again.
  const restoreAfterRefusal = useCallback(() => {
    void runner.rejectPendingCommand().catch((error) => {
      console.error("Error restoring the board after a refused command:", error);
    });
  }, [runner]);

  const handleMatchMessage = useCallback(
    (message: MatchWebSocketMessage) => {
      switch (message.type) {
        case "initialBoard": {
          setBoardError(null);
          setInitialBoard(message);
          setActivePlayerSlot(message.gameState?.activePlayerSlot ?? null);
          const opening = message.gameState?.observation;
          if (opening) setFalls((known) => recordFalls(known, opening, true));
          return;
        }
        case "connected": {
          // A fresh connection supersedes whatever went wrong on the last one,
          // and opens on the live board.
          setMatchError(null);
          setReviewError(null);
          // A hotseat handoff to another seat of the same viewer closes the
          // turn with this frame and not with a player update, so the end turn
          // command must stop here too.
          finishEndingTurn();
          finishResigning();
          finishActivatingPower();
          setViewerSlotIndex(message.slotIndex);
          // The notice arrives before this frame and only ever for a spectator,
          // so holding a seat here means an earlier notice has gone stale.
          if (message.slotIndex !== null) {
            setSpectatorFogActive(null);
          }
          return;
        }
        case "error": {
          // The outline the page asks for on its own is a convenience; a match
          // that cannot be read back says so only when somebody reaches back.
          if (message.message === REVIEW_REFUSED && !isReviewingRef.current) {
            outlineAskedRef.current = null;
            return;
          }
          finishEndingTurn();
          finishResigning();
          finishActivatingPower();
          // A question about the past that was refused leaves the board where
          // it is. The keys come back rather than staying pressed.
          setIsReviewBusy(false);
          returningToLiveRef.current = false;
          setMatchError(message.message);
          restoreAfterRefusal();
          return;
        }
        case "spectatorNotice": {
          setSpectatorFogActive(message.fogActive);
          return;
        }
        case "matchOver": {
          // Nothing is invalidated here. Re-reading the match would flip its
          // phase and swap this page for the replay under the player, which is
          // the jump the result is meant to replace.
          finishEndingTurn();
          finishResigning();
          finishActivatingPower();
          setMatchResults(message.results);
          setIsResultOpen(true);
          return;
        }
        case "clock": {
          setClock(message);
          return;
        }
        case "playerUpdate": {
          setOutlineRevision((revision) => revision + 1);
          setActivePlayerSlot(message.activePlayerSlot);
          setFalls((known) => recordFalls(known, message.transition.post, false));
          finishEndingTurn();
          void runner.applyLiveTransition(message.transition).catch((error) => {
            console.error("Error applying live player transition:", error);
          });
          return;
        }
        case "reviewOutline": {
          setReviewOutline(message.boundaries);
          setOutlineAnswered(outlineAskedRef.current);
          return;
        }
        case "reviewState": {
          setIsReviewBusy(false);
          if (message.observation === null) {
            // A fogged match has no public board, so somebody holding no seat
            // in it has no earlier board to be shown either.
            returningToLiveRef.current = false;
            setReviewError("A fogged match can only be read back by the seats playing it.");
            return;
          }

          if (returningToLiveRef.current) {
            returningToLiveRef.current = false;
            isReviewingRef.current = false;
            setIsReviewing(false);
            setReviewPosition(null);
            // The board is handed back first, which drops the transitions
            // held at the edge, and the match as it stands is put on it
            // second. Anything the match does after this answer arrives the
            // ordinary way.
            void runner
              .exitBoardReview()
              .then(() =>
                runner.applyLiveTransition({
                  post: message.observation,
                  events: [],
                } as LiveTransition),
              )
              .catch((error) => {
                console.error("Error returning to the live match:", error);
                setReviewError(REVIEW_RESTORE_FAILED);
              });
            return;
          }

          setReviewPosition({ index: message.index, total: message.latestIndex });
          void runner
            .applyReviewState(message.transition ?? { post: message.observation, events: [] })
            .catch((error) => {
              console.error("Error showing a reviewed position:", error);
              setReviewError("That moment of the match could not be shown.");
            });
          return;
        }
        case "spectatorState": {
          if (!message.transition) return;
          const post = message.transition.post;
          setOutlineRevision((revision) => revision + 1);
          setFalls((known) => recordFalls(known, post, false));
          void runner.applyLiveTransition(message.transition).catch((error) => {
            console.error("Error applying live spectator transition:", error);
          });
          return;
        }
        default: {
          return;
        }
      }
    },
    [finishActivatingPower, finishEndingTurn, finishResigning, restoreAfterRefusal, runner],
  );
  // The server lets only a signed-in viewer onto the match, so a signed-out
  // one is asked to sign in rather than shown a connection that cannot open.
  const isSignedOut = session === null;
  const { reconnect, sendMessage, status } = useMatchWebSocket(
    matchId,
    handleMatchMessage,
    !isSignedOut,
  );
  // Until a connection has opened, waiting on one is not a problem: the first
  // connection reads in Text on ink 2, and warn means a connection that
  // dropped. A first connection that fails was refused, not dropped.
  const [hasConnected, setHasConnected] = useState(false);
  const [isRefused, setIsRefused] = useState(false);
  useEffect(() => {
    if (status === "connected") {
      setHasConnected(true);
      setIsRefused(false);
    } else if (status === "disconnected" || status === "error") {
      setIsRefused(true);
    }
  }, [status]);
  // A session that ended elsewhere still reads as signed in here, and the
  // server refuses it: ask again, so the page can say to sign in.
  useEffect(() => {
    if (isRefused && !hasConnected) {
      void queryClient.invalidateQueries({ queryKey: sessionQueryOptions().queryKey });
    }
  }, [hasConnected, isRefused, queryClient]);
  const connection: MatchConnection = isSignedOut
    ? "signedOut"
    : status === "connected"
      ? "connected"
      : hasConnected
        ? "reconnecting"
        : isRefused
          ? "refused"
          : "connecting";

  useEffect(() => {
    runner.setLiveCommandHandler((command) => {
      setMatchError(null);
      if (!sendMessage(command)) {
        setMatchError("The order could not be sent because the match connection is not open.");
        restoreAfterRefusal();
      }
    });
    return () => runner.setLiveCommandHandler(undefined);
  }, [restoreAfterRefusal, runner, sendMessage]);
  // The army the build menu speaks for. Its treasury pays for the order, and
  // its sprites are the ones the menu shows, in whatever depiction the viewer
  // has chosen for it.
  const viewerArmy = armies.find((army) => army.entry.playerId === seatSlot);

  useEffect(() => {
    if (activatingPower !== undefined && viewerArmy?.entry.activePower !== undefined) {
      finishActivatingPower();
    }
  }, [activatingPower, finishActivatingPower, viewerArmy?.entry.activePower]);

  // A seat resigns while any seat holds the turn, so the updates that follow
  // the order speak for other players as often as for this one. The order is
  // done when the board says this army has left, and not before.
  useEffect(() => {
    if (isResigning && viewerArmy?.entry.eliminated === true) {
      finishResigning();
    }
  }, [finishResigning, isResigning, viewerArmy?.entry.eliminated]);

  useEffect(() => {
    if (status !== "connected") {
      finishEndingTurn();
      finishResigning();
      finishActivatingPower();
    }
  }, [finishActivatingPower, finishEndingTurn, finishResigning, status]);

  const handleActivatePower = useCallback(
    (level: ActivatablePowerLevel) => {
      if (
        isActivatingPowerRef.current ||
        status !== "connected" ||
        viewerSlotIndex === null ||
        viewerSlotIndex === undefined ||
        viewerSlotIndex !== activePlayerSlot
      ) {
        return;
      }

      isActivatingPowerRef.current = true;
      setActivatingPower(level);
      setMatchError(null);
      if (!sendMessage({ type: "activatePower", level })) {
        finishActivatingPower();
        setMatchError("The CO power could not be activated because the connection is not open.");
        return;
      }
      setProductionOptions(null);
    },
    [
      activePlayerSlot,
      finishActivatingPower,
      sendMessage,
      setProductionOptions,
      status,
      viewerSlotIndex,
    ],
  );

  /**
   * Give the match up, from wherever the player is standing in it.
   *
   * The one order a seat may send while another seat holds the turn, so it
   * asks nothing of the clock and nothing of the board: whoever is playing
   * keeps playing, and this army leaves. The menu has already asked the
   * question, so this sends the order.
   */
  const handleResign = useCallback(() => {
    if (
      isResigningRef.current ||
      status !== "connected" ||
      viewerSlotIndex === null ||
      viewerSlotIndex === undefined
    ) {
      return;
    }

    isResigningRef.current = true;
    setIsResigning(true);
    setMatchError(null);
    if (!sendMessage({ type: "resign" })) {
      finishResigning();
      setMatchError("The match could not be resigned because the connection is not open.");
      return;
    }
    setProductionOptions(null);
  }, [finishResigning, sendMessage, setProductionOptions, status, viewerSlotIndex]);

  const turnReadiness = useGameStore((state) => state.turnReadiness);
  const isPowerReady =
    viewerArmy !== undefined &&
    (canActivatePower(viewerArmy.entry, "cop") || canActivatePower(viewerArmy.entry, "scop"));
  const turnResidue = describeTurnResidue(turnReadiness, isPowerReady);
  const [isConfirmingEndTurn, setIsConfirmingEndTurn] = useState(false);
  const alertsOffer = useTurnAlertsOffer(isAsyncClock(match.settings.clock));
  const offerAlerts = alertsOffer.offer;

  const handleEndTurn = useCallback(() => {
    if (
      isEndingTurnRef.current ||
      turnReadiness === undefined ||
      status !== "connected" ||
      viewerSlotIndex === null ||
      viewerSlotIndex === undefined ||
      viewerSlotIndex !== activePlayerSlot
    ) {
      return;
    }

    isEndingTurnRef.current = true;
    setIsEndingTurn(true);
    setMatchError(null);
    if (!sendMessage({ type: "endTurn" })) {
      finishEndingTurn();
      setMatchError("The command could not be sent because the match connection is not open.");
      return;
    }
    setProductionOptions(null);
    offerAlerts();
  }, [
    activePlayerSlot,
    finishEndingTurn,
    offerAlerts,
    sendMessage,
    setProductionOptions,
    status,
    turnReadiness,
    viewerSlotIndex,
  ]);

  /**
   * Put the question, unless there is nothing to ask about.
   *
   * Both the button and the board's chord come through here, so the board never
   * ends a turn the button would have asked about first.
   */
  const requestEndTurn = useCallback(() => {
    if (turnReadiness === undefined) {
      return;
    }
    if (turnResidue === null) {
      handleEndTurn();
      return;
    }
    setIsConfirmingEndTurn(true);
  }, [handleEndTurn, turnReadiness, turnResidue]);

  // The board keeps moving under an open question — a power activated from the
  // meter, a unit spent from the board. A question with nothing left to ask
  // about is one the player has already answered.
  useEffect(() => {
    if (turnReadiness === undefined || turnResidue === null) {
      setIsConfirmingEndTurn(false);
    }
  }, [turnReadiness, turnResidue]);

  const requestEndTurnRef = useRef(requestEndTurn);
  requestEndTurnRef.current = requestEndTurn;
  useEffect(() => {
    runner.setEndTurnRequestHandler(() => requestEndTurnRef.current());
    return () => runner.setEndTurnRequestHandler(undefined);
  }, [runner]);

  const handleBuildUnit = useCallback(
    (unit: UnitKind, x: number, y: number) => {
      setMatchError(null);
      if (
        !sendMessage({
          type: "build",
          position: { x, y },
          unit_type: unit,
        })
      ) {
        setMatchError("The unit could not be built because the match connection is not open.");
        return;
      }
      // The server remains authoritative. Close the advisory menu immediately;
      // a rejection arrives through the normal websocket error message.
      setProductionOptions(null);
    },
    [sendMessage, setProductionOptions],
  );

  const handleDisplayFactionChange = useCallback(
    (playerId: number, factionId: number | null) => {
      void runner.setPlayerDisplayFaction(playerId, factionId).catch((error) => {
        console.error("Error updating match faction depiction:", error);
      });
    },
    [runner],
  );

  /**
   * Where the viewer is standing, counted in actions taken.
   *
   * Null until the match has said how long it is, which it is only asked once
   * somebody reaches for an earlier moment.
   */
  const currentReviewIndex =
    reviewPosition?.index ?? (reviewOutline === null ? null : reviewOutline.length - 1);

  /**
   * Take the board out of play.
   *
   * Orders are spelled against the units on the board, and the units on a
   * board being read moved long ago. So the menus close, the engine stops
   * taking orders, and the live match waits at the edge until the viewer comes
   * back to it.
   */
  const beginReview = useCallback(() => {
    if (isReviewingRef.current) return;
    isReviewingRef.current = true;
    setIsReviewing(true);
    setProductionOptions(null);
    setUnitActions(null);
    void runner.enterBoardReview().catch((error) => {
      console.error("Error taking the board out of play:", error);
    });
  }, [runner, setProductionOptions, setUnitActions]);

  /** Ask for one boundary, and report whether the question got out. */
  const requestReviewSeek = useCallback(
    (index: number | null) => {
      setReviewError(null);
      if (!sendMessage({ type: "reviewSeek", index })) {
        setReviewError(REVIEW_UNREACHABLE);
        return false;
      }
      setIsReviewBusy(true);
      return true;
    },
    [sendMessage],
  );

  /**
   * Step by actions or by turns.
   *
   * A step needs to know where it is starting from, and a viewer who has asked
   * the match nothing yet does not know how long it is. The step is held until
   * the outline answers that, so the first press does what every later one
   * does rather than nothing.
   */
  const isOutlineFresh = reviewOutline !== null && outlineAnswered === outlineRevision;
  const requestStep = useCallback(
    (kind: "action" | "turn", delta: number) => {
      beginReview();
      const boundaries = reviewOutline;
      if (boundaries === null || currentReviewIndex === null || !isOutlineFresh) {
        // A step held for an outline that was never asked for would wait for
        // an answer nothing is coming back with, under a key that stays busy.
        if (outlineAskedRef.current !== outlineRevision) {
          if (!sendMessage({ type: "reviewOutline" })) {
            setReviewError(REVIEW_UNREACHABLE);
            return;
          }
          outlineAskedRef.current = outlineRevision;
        }
        pendingStepRef.current = { kind, delta };
        setIsReviewBusy(true);
        return;
      }

      const target = stepTarget(boundaries, currentReviewIndex, kind, delta);
      if (target === null) {
        setIsReviewBusy(false);
        return;
      }
      requestReviewSeek(target);
    },
    [
      beginReview,
      currentReviewIndex,
      isOutlineFresh,
      outlineRevision,
      requestReviewSeek,
      reviewOutline,
      sendMessage,
    ],
  );

  // The outline, asked for once the host has said who is watching, and again
  // whenever the match moves on. A watcher of a fogged match is shown no
  // earlier board, so they are not given its outline either.
  useEffect(() => {
    if (status !== "connected" || viewerSlotIndex === undefined || spectatorFogActive === true) {
      return;
    }
    if (outlineAskedRef.current === outlineRevision) return;
    if (sendMessage({ type: "reviewOutline" })) outlineAskedRef.current = outlineRevision;
  }, [outlineRevision, sendMessage, spectatorFogActive, status, viewerSlotIndex]);

  // A step that was waiting on the outline, taken now that it has one.
  useEffect(() => {
    const pending = pendingStepRef.current;
    if (pending === null || reviewOutline === null || !isOutlineFresh) return;
    pendingStepRef.current = null;

    const target = stepTarget(
      reviewOutline,
      reviewPosition?.index ?? reviewOutline.length - 1,
      pending.kind,
      pending.delta,
    );
    if (target === null) {
      setIsReviewBusy(false);
      return;
    }
    requestReviewSeek(target);
  }, [isOutlineFresh, requestReviewSeek, reviewOutline, reviewPosition]);

  /** Come back to the match as it stands. */
  const returnToLiveMatch = useCallback(() => {
    if (!isReviewingRef.current) return;
    // The end of a match still being played moves, so it is named rather than
    // counted to: an index that was the end a moment ago may be behind by the
    // time the question arrives. The board is only marked as coming back once
    // the question is away: a mark left standing over a question that never
    // left would read the next answer as this one's.
    returningToLiveRef.current = requestReviewSeek(null);
  }, [requestReviewSeek]);

  // A connection that dropped took the reading with it: what the viewer was
  // shown came from the host, and the board is rebuilt from the snapshot the
  // next connection opens with.
  useEffect(() => {
    if (status === "connected") return;
    isReviewingRef.current = false;
    returningToLiveRef.current = false;
    pendingStepRef.current = null;
    setIsReviewing(false);
    setIsReviewBusy(false);
    setReviewPosition(null);
    setReviewOutline(null);
    setOutlineAnswered(null);
    outlineAskedRef.current = null;
    // The engine is told as well as the page. A board left under review holds
    // the live match at the edge, so the next connection's own board would
    // wait there rather than being played on.
    void runner.exitBoardReview().catch((error: unknown) => {
      console.error("Error giving the board back after a dropped connection:", error);
    });
  }, [runner, status]);

  // The seat spending its bank is the only one that can run out while the page
  // is open, so it sets how often every readout redraws.
  const runningRemaining = clock === null ? null : Math.max(0, clock.deadlineAt - Date.now());
  const now = useClockNow(runningRemaining);

  const day = playerRoster?.day ?? null;
  const landingDay = useLandingDay(day, !isReviewing && status === "connected");
  const isPlayer = seatSlot !== null;
  const isViewerOut = viewerArmy?.entry.eliminated ?? false;
  const clockInitialMs = match.settings.clock.initialMs;

  const turnOrder = useMemo(() => armies.map((army) => army.entry.playerId), [armies]);
  const views = useMemo<ArmyView[]>(
    () =>
      armies.map((army) => ({
        army,
        isViewer: seatSlot === army.entry.playerId,
        clock: readArmyClock(clock, army.entry.playerId, now, clockInitialMs),
        fall: army.entry.eliminated
          ? readFall(falls.get(army.entry.playerId), reviewOutline, army.entry.playerId, turnOrder)
          : null,
        onFactionChange: army.hasLiveStats
          ? (factionId: number) =>
              handleDisplayFactionChange(
                army.entry.playerId,
                factionId === getFactionByCode(army.entry.actualFactionCode)?.id ? null : factionId,
              )
          : undefined,
        isRecordOnly: isSignedOut,
      })),
    [
      armies,
      clock,
      clockInitialMs,
      falls,
      handleDisplayFactionChange,
      isSignedOut,
      now,
      reviewOutline,
      seatSlot,
      turnOrder,
    ],
  );

  // A fogged match has no board a watcher may read, so it offers them no
  // controls for reading one either; nor does a match a signed-out viewer
  // cannot open.
  const review: ReviewModel | null =
    spectatorFogActive === true || isSignedOut
      ? null
      : {
          position: isReviewing
            ? reviewPosition
            : reviewOutline === null
              ? null
              : { index: reviewOutline.length - 1, total: reviewOutline.length - 1 },
          isAtLatest: !isReviewing,
          canStepBack: isReviewing
            ? (reviewPosition?.index ?? 1) > 0
            : // A match nobody has played yet has nothing behind it.
              reviewOutline === null || reviewOutline.length > 1,
          canStepForward:
            isReviewing && (reviewPosition === null || reviewPosition.index < reviewPosition.total),
          canStepTurnBack:
            reviewOutline === null || currentReviewIndex === null
              ? !isReviewing
              : previousTurnStart(reviewOutline, currentReviewIndex) !== null,
          canStepTurnForward:
            isReviewing &&
            (reviewOutline === null ||
              currentReviewIndex === null ||
              nextTurnStart(reviewOutline, currentReviewIndex) !== null),
          latestLabel: "Live",
          isBusy: isReviewBusy || status !== "connected",
          onSeekStart: () => {
            beginReview();
            requestReviewSeek(0);
          },
          onStep: (delta: number) => requestStep("action", delta),
          onTurnStep: (delta: number) => requestStep("turn", delta),
          onSeekLatest: returnToLiveMatch,
          onSeek:
            reviewOutline === null
              ? undefined
              : (index: number) => {
                  if (index >= reviewOutline.length - 1 && !isReviewing) return;
                  beginReview();
                  requestReviewSeek(index);
                },
          dayStarts: reviewOutline === null ? undefined : dayStarts(reviewOutline),
          hasLastKey: false,
        };

  return (
    <>
      <ActiveMatchScreen
        activatingPower={activatingPower}
        activePlayerSlot={activePlayerSlot}
        armies={armies}
        alertsOffer={alertsOffer}
        boardError={boardError}
        connection={connection}
        day={day}
        initialBoard={initialBoard}
        isDayNew={landingDay !== null && landingDay === day && !isReviewing}
        isEndingTurn={isEndingTurn}
        isPlayer={isPlayer}
        isResigning={isResigning}
        isReviewing={isReviewing}
        isTurnReadinessKnown={turnReadiness !== undefined}
        isTurnSpent={turnResidue === null}
        isViewerOut={isViewerOut}
        log={
          review === null || reviewOutline === null || currentReviewIndex === null
            ? null
            : { boundaries: reviewOutline, currentIndex: currentReviewIndex }
        }
        match={match}
        matchError={matchError}
        onActivatePower={handleActivatePower}
        onBoardError={setBoardError}
        onBuildUnit={handleBuildUnit}
        onEndTurn={requestEndTurn}
        onResign={handleResign}
        onDismissMatchError={() => setMatchError(null)}
        onReturnToLive={() => {
          setReviewError(null);
          if (isReviewingRef.current) returnToLiveMatch();
        }}
        players={livePlayers}
        reconnect={reconnect}
        review={review}
        reviewError={reviewError}
        runner={runner}
        spectatorFogActive={spectatorFogActive}
        status={status}
        turnReadiness={turnReadiness}
        viewerArmy={viewerArmy ?? null}
        viewerSlotIndex={seatSlot}
        views={views}
      />

      {/* The one move in the game that cannot be taken back, and the only one
          the page asks about. It asks only when the turn has something left in
          it, so the question is never a habit. */}
      <EndTurnDialog
        factionCode={viewerArmy?.entry.displayFactionCode ?? "os"}
        isEnding={isEndingTurn}
        isOpen={isConfirmingEndTurn}
        isPowerReady={isPowerReady}
        onConfirm={() => {
          setIsConfirmingEndTurn(false);
          handleEndTurn();
        }}
        onOpenChange={setIsConfirmingEndTurn}
        readiness={turnReadiness}
      />

      {/* How the match ended, over the board it ended on. Watching the replay
          is what re-reads the match, and the phase it comes back with is what
          hands this page over to the review one. */}
      {matchResults ? (
        <MatchOverDialog
          armies={armies}
          day={day}
          isOpen={isResultOpen}
          isRated={match.pool !== null && match.void === null}
          matchId={matchId}
          onOpenChange={setIsResultOpen}
          onWatchReplay={() => {
            setIsResultOpen(false);
            void queryClient.invalidateQueries({
              queryKey: matchKeys.detail(matchId, joinSlug),
            });
          }}
          results={matchResults}
          viewerSlotIndex={viewerSlotIndex ?? null}
        />
      ) : null}
    </>
  );
}

/** How long a landing postmark keeps its landing: its two frames, and a margin. */
const POSTMARK_LANDING_MS = 400;

/**
 * The live day that just opened, while its postmark lands.
 *
 * The day the page opened on never lands, and neither does a day read in
 * review: only a later live day than any seen before (an End turn that opens
 * a day). The landing is let go once it has played, so coming back from review
 * or reconnecting does not play it again.
 */
function useLandingDay(day: number | null, isLive: boolean): number | null {
  const latestRef = useRef<number | null>(null);
  const [landing, setLanding] = useState<number | null>(null);
  useEffect(() => {
    if (!isLive || day === null) return;
    const latest = latestRef.current;
    latestRef.current = latest === null ? day : Math.max(latest, day);
    if (latest !== null && day > latest) setLanding(day);
  }, [day, isLive]);
  useEffect(() => {
    if (landing === null) return;
    const timer = window.setTimeout(() => setLanding(null), POSTMARK_LANDING_MS);
    return () => window.clearTimeout(timer);
  }, [landing]);
  return landing;
}

/** The clock as terms: `Live 5m +2m`, `Async 7d +2d`, or the terms alone. */
function clockTerms(clock: MatchSnapshot["settings"]["clock"]): string {
  const preset = findClockPreset(clock);
  const terms = formatClockTerms(clock);
  return preset ? `${preset.name} ${terms}` : terms;
}

/**
 * How a fallen army left. The day comes from the observation the fall was
 * first seen in, or else from the outline when the outline alone can say it.
 */
function readFall(
  seen: ArmyFall | undefined,
  outline: readonly ReviewBoundary[] | null,
  playerId: number,
  turnOrder: readonly number[],
): ArmyFall {
  const day = seen?.day ?? (outline ? fallDayFromOutline(outline, playerId, turnOrder) : null);
  return { day, reason: seen?.reason ?? null };
}

/** One seat's clock, as its army shows it. */
function readArmyClock(
  clock: MatchClockMessage | null,
  playerId: number,
  now: number,
  initialMs: number,
): ArmyClock | null {
  if (clock === null) return null;
  const remaining = seatRemainingMs(clock, playerId, now);
  const value = formatClockDuration(remaining);
  const isRunning = playerId === clock.activeSlot;
  return {
    value,
    isRunning,
    pressure: clockPressure(remaining, initialMs),
    label: remaining <= 0 ? "Out of time" : `${value} left${isRunning ? ", running" : ""}`,
  };
}

function ActiveMatchScreen({
  activatingPower,
  activePlayerSlot,
  alertsOffer,
  armies,
  boardError,
  connection,
  day,
  initialBoard,
  isDayNew,
  isEndingTurn,
  isPlayer,
  isResigning,
  isReviewing,
  isTurnReadinessKnown,
  isTurnSpent,
  isViewerOut,
  log,
  match,
  matchError,
  onActivatePower,
  onBoardError,
  onBuildUnit,
  onEndTurn,
  onResign,
  onDismissMatchError,
  onReturnToLive,
  players,
  reconnect,
  review,
  reviewError,
  runner,
  spectatorFogActive,
  status,
  turnReadiness,
  viewerArmy,
  viewerSlotIndex,
  views,
}: {
  activatingPower: ActivatablePowerLevel | undefined;
  activePlayerSlot: number | null;
  alertsOffer: ReturnType<typeof useTurnAlertsOffer>;
  armies: MatchArmy[];
  boardError: string | null;
  connection: MatchConnection;
  day: number | null;
  initialBoard: InitialBoardMessage | null;
  isDayNew: boolean;
  isEndingTurn: boolean;
  isPlayer: boolean;
  isResigning: boolean;
  isReviewing: boolean;
  isTurnReadinessKnown: boolean;
  isTurnSpent: boolean;
  isViewerOut: boolean;
  /** The match's outline and where the viewer stands in it, once the host has said. */
  log: { boundaries: readonly LogBoundary[]; currentIndex: number } | null;
  match: MatchSnapshot;
  matchError: string | null;
  onActivatePower: (level: ActivatablePowerLevel) => void;
  onBoardError: (message: string | null) => void;
  onBuildUnit: (unit: UnitKind, x: number, y: number) => void;
  onEndTurn: () => void;
  onResign: () => void;
  /** Leave a reading of the past that failed, back to the match as it stands. */
  onReturnToLive: () => void;
  /** Clear a refused order's notice once the board is back where it was. */
  onDismissMatchError: () => void;
  players: LiveMatchPlayer[];
  reconnect: () => void;
  review: ReviewModel | null;
  reviewError: string | null;
  runner: GameRunner;
  spectatorFogActive: boolean | null;
  status: MatchWebSocketStatus;
  turnReadiness: ReturnType<typeof useGameStore.getState>["turnReadiness"];
  viewerArmy: MatchArmy | null;
  viewerSlotIndex: number | null | undefined;
  views: ArmyView[];
}) {
  const {
    canvasContainerRef,
    enterFullscreen,
    exitFullscreen,
    focus,
    fullscreenMode,
    isFullscreen,
    surfaceRef,
  } = useCanvasCourierSurface({
    host: runner.host,
    canvasClassName: stylex.props(styles.gameCanvas).className ?? "",
  });
  const isImmersiveNotice = useImmersiveNotice(fullscreenMode);
  const playersRef = useRef(players);
  playersRef.current = players;
  const onBoardErrorRef = useRef(onBoardError);
  onBoardErrorRef.current = onBoardError;
  const productionOptions = useGameStore((state) => state.productionOptions);
  const unitActions = useGameStore((state) => state.unitActions);
  const inspectedUnit = useGameStore((state) => state.inspectedUnit);
  const setProductionOptions = useGameActions().setProductionOptions;
  const isPhone = useMediaQuery(PHONE_MEDIA);
  const isCalculatorCompact = useMediaQuery(BATTLE_CALCULATOR_SHEET_MEDIA);
  const isFullscreenEnabled = useFullscreenEnabled();
  const tableRef = useRef<HTMLDivElement>(null);
  const [isCalculatorOpen, setIsCalculatorOpen] = useState(false);
  const [isMatchMenuOpen, setIsMatchMenuOpen] = useState(false);
  const [isTileSheetOpen, setIsTileSheetOpen] = useState(false);
  const [sheetArmyId, setSheetArmyId] = useState<number | null>(null);
  // The army stays in the sheet while it closes, so its exit can play.
  const [isArmySheetOpen, setIsArmySheetOpen] = useState(false);
  const openArmySheet = (playerId: number) => {
    setSheetArmyId(playerId);
    setIsArmySheetOpen(true);
  };
  const [armedPower, setArmedPower] = useState<ActivatablePowerLevel | null>(null);
  // The power that is on is said once, in the tile row, until it is read.
  const [seenPowerDay, setSeenPowerDay] = useState<number | null>(null);
  const hoveredTile = useGameStore((state) => state.hoveredTile);
  const productionSite = productionOptions?.site;
  const mapDims = initialBoard
    ? { width: initialBoard.map.width, height: initialBoard.map.height }
    : null;
  const layout = useTableLayout(tableRef, mapDims, isPhone);
  const whole = useWholeMap(runner);
  const boardView = useBoardView();
  const { data: catalogEntry } = useQuery(
    mapCatalogEntryQueryOptions(match.mapId, match.mapRevision),
  );
  // Each Try again loads the board afresh.
  const [loadAttempt, setLoadAttempt] = useState(0);

  useEffect(() => {
    if (!initialBoard) return;

    let canceled = false;
    void Promise.resolve()
      .then(async () => {
        if (canceled) return;
        if (initialBoard.gameState) {
          await runner.loadLiveMatch(
            initialBoard.map,
            playersRef.current,
            initialBoard.gameState.observation,
          );
        } else {
          await runner.loadMatchMap(initialBoard.map);
        }
      })
      .catch((error: unknown) => {
        console.error("Error loading match map:", error);
        if (canceled) return;
        onBoardErrorRef.current(
          error instanceof Error ? error.message : "The engine did not report a reason.",
        );
      });

    return () => {
      canceled = true;
    };
  }, [initialBoard, loadAttempt, runner]);

  const dismissBuildMenu = useCallback(() => {
    setProductionOptions(null);
  }, [setProductionOptions]);
  // The engine owns the selection, so a dismissal is reported to it rather than
  // acted on here; it answers by closing the menu and stepping back to the unit.
  const chooseUnitAction = useCallback(
    (index: number) => {
      void runner.chooseUnitAction(index).catch((error) => {
        console.error("Error sending the chosen order:", error);
      });
    },
    [runner],
  );
  const dismissUnitActions = useCallback(() => {
    void runner.dismissUnitAction().catch((error) => {
      console.error("Error dismissing the destination menu:", error);
    });
  }, [runner]);

  const mapName = initialBoard?.map.metadata.name ?? catalogEntry?.name ?? null;
  const isViewerTurn = isPlayer && viewerSlotIndex === activePlayerSlot;
  const isOwed = isViewerTurn && !isReviewing && !isViewerOut;
  const isConnected = status === "connected";
  const mover = armies.find((army) => army.entry.playerId === activePlayerSlot) ?? null;
  const moverView = views.find((view) => view.army.entry.playerId === activePlayerSlot) ?? null;
  const title = isOwed ? "Your turn" : mover ? `${mover.name}'s turn` : (mapName ?? match.name);
  const factionCode = viewerArmy?.entry.displayFactionCode ?? "os";
  const endTurnTooltip = !isConnected
    ? "Reconnect to the match before ending your turn."
    : isReviewing
      ? "Return to live to play on"
      : !isViewerTurn
        ? "You can end the turn when your army is active."
        : !isTurnReadinessKnown
          ? "Waiting for the board to report turn readiness."
          : "End turn (Ctrl+Enter)";
  const isEndTurnDisabled = !isConnected || !isViewerTurn || !isTurnReadinessKnown || isReviewing;
  const buildBlockedReason = isConnected ? undefined : "Reconnect to the match to send this order.";
  const power: PowerControl = {
    activating: activatingPower,
    onActivate: isConnected && isViewerTurn && !isReviewing ? onActivatePower : undefined,
  };
  const viewerPowers = viewerArmy && power.onActivate ? readyPowers(viewerArmy.entry) : [];
  const strongestPower = viewerPowers.at(-1);
  const hasOrders = unitActions !== null && unitActions.destination !== undefined;
  const hasBuild = productionOptions !== null && productionSite !== undefined;
  const isSelecting = inspectedUnit !== null && isViewerTurn && !isReviewing;

  // An armed power is disarmed by anything that takes the dock away from it.
  useEffect(() => {
    if (viewerPowers.length === 0) setArmedPower(null);
  }, [viewerPowers.length]);

  const moverClock = moverView?.clock ?? null;
  const stripClock =
    moverClock === null ? null : (
      <StripClock
        label={`${mover?.name ?? "The mover"}: ${moverClock.label}`}
        pressure={moverClock.pressure}
        value={moverClock.value}
      />
    );

  const tryAgainKey = (onPress: () => void) => (
    <Button
      icon={<Icon icon={IconRefresh} />}
      label="Try again"
      onClick={onPress}
      variant="on-ink"
    />
  );
  // Each notice carries the way out of what failed, and stays until its cause
  // clears: there is no close key.
  const notices = (
    <>
      {boardError ? (
        <MatchNotice
          action={tryAgainKey(() => {
            onBoardError(null);
            setLoadAttempt((attempt) => attempt + 1);
          })}
          description={boardError}
          title="The board could not be loaded"
        />
      ) : null}
      {connection === "signedOut" ? (
        // On a phone its key is the dock's Sign in, so the notice says it
        // without a second one.
        <MatchNotice
          action={isPhone ? undefined : <SignInKey size="notice" />}
          title="Sign in to watch"
          tone="info"
        />
      ) : null}
      {connection === "refused" ? (
        <MatchNotice action={tryAgainKey(reconnect)} title="The match could not be reached" />
      ) : null}
      {matchError ? (
        <MatchNotice
          action={
            isConnected ? (
              // The board is already back where it was before the order; OK
              // gives the row back to it.
              <Button label="OK" onClick={onDismissMatchError} variant="on-ink" />
            ) : (
              <ReconnectKey onPress={reconnect} />
            )
          }
          description={matchError}
          title="The order was not taken"
        />
      ) : null}
      {reviewError ? (
        <MatchNotice
          action={
            reviewError === REVIEW_RESTORE_FAILED || !isConnected ? (
              <ReconnectKey onPress={reconnect} />
            ) : (
              <Button label="Back to live" onClick={onReturnToLive} variant="on-ink" />
            )
          }
          description={reviewError}
          title="The match could not be read"
        />
      ) : null}
    </>
  );
  const hasNotice =
    connection === "signedOut" ||
    connection === "refused" ||
    boardError !== null ||
    matchError !== null ||
    reviewError !== null;
  const columnNotices = hasNotice ? <VStack xstyle={styles.columnNotice}>{notices}</VStack> : null;

  // A new selection is a new menu: it lands (90ms, stepped) and takes the
  // caret afresh.
  const ordersMenu = hasOrders ? (
    <UnitActionMenu
      key={`${unitActions.destination!.x},${unitActions.destination!.y}:${unitActions.options
        .map((option) => option.name)
        .join("|")}`}
      anchor={null}
      attacker={unitActions.attacker ?? undefined}
      destination={unitActions.destination!}
      disabledReason={buildBlockedReason}
      isEnabled={buildBlockedReason === undefined}
      onChoose={chooseUnitAction}
      onDismiss={dismissUnitActions}
      onRestoreFocus={focus}
      options={unitActions.options}
      preselected={unitActions.preselected ?? undefined}
      presentation={isPhone ? "dock" : "docked"}
    />
  ) : null;
  // Mounted once per width: beside the board, in the rail, or as a phone sheet.
  const buildMount = hasBuild ? buildMenuMount(layout.mode) : null;
  // The phone sheet keeps its last site while it leaves, so its exit plays.
  const buildSheet = useLeaving(buildMount === "sheet" ? productionOptions : null);
  const renderBuildMenu = (
    production: NonNullable<typeof productionOptions>,
    presentation: "sheet" | "docked",
    isOpen = true,
  ) => {
    const site = production.site!;
    return (
      <BuildMenu
        key={`${site.x},${site.y}`}
        anchor={null}
        disabledReason={buildBlockedReason}
        factionCode={factionCode}
        funds={viewerArmy?.entry.stats.funds ?? null}
        isEnabled={buildBlockedReason === undefined}
        isOpen={isOpen}
        // A leaving sheet's site is gone from the engine; it takes no order.
        onBuild={(unit) => {
          if (isOpen) onBuildUnit(unit, site.x, site.y);
        }}
        onDismiss={dismissBuildMenu}
        onRestoreFocus={focus}
        options={production.options}
        presentation={presentation}
        site={site}
      />
    );
  };
  const buildMenu =
    hasBuild && (buildMount === "column" || buildMount === "rail")
      ? renderBuildMenu(productionOptions, "docked")
      : null;
  const locator =
    catalogEntry && mapName ? (
      <Locator mapName={mapName} runner={runner} src={catalogEntry.screenshot.small} />
    ) : null;
  // A notice, or the Turn alerts offer, takes the tile window's place beside
  // the orders, as it takes the tile row on a phone.
  const tileMessage =
    columnNotices ??
    (alertsOffer.isOffering ? <TurnAlertsOffer onDismiss={alertsOffer.dismiss} /> : null);
  const side = (
    <>
      <BoardKeys runner={runner} />
      {tileMessage ?? <TileWindow />}
      <DockedAttackPreview />
      {isPhone ? null : ordersMenu}
      {buildMount === "column" ? buildMenu : null}
    </>
  );

  // The phone sheet stays while it leaves, so its exit plays.
  const calculatorSheet = useLeaving(isCalculatorOpen && isCalculatorCompact ? true : null);
  const calculator =
    isCalculatorOpen || calculatorSheet.value ? (
      <BattleCalculator
        isOpen={isCalculatorOpen}
        onDismiss={() => setIsCalculatorOpen(false)}
        onRestoreFocus={focus}
        presentation={isCalculatorCompact ? "sheet" : "board"}
        roster={useGameStore.getState().playerRoster}
        runner={runner}
        viewerSlotIndex={viewerSlotIndex ?? null}
      />
    ) : null;

  // The connection is said once: in the desktop strip, a dot and its word;
  // on a phone the dock says the word, so the turn line keeps only the dot.
  const subLine = (
    <>
      {title === match.name ? null : <b {...stylex.props(styles.matchName)}>{match.name}</b>}
      {spectatorFogActive === null ? (
        <ConnectionState status={connection} />
      ) : (
        <WatchingTag hasFog={spectatorFogActive} />
      )}
    </>
  );

  // Rail: the armies, then the view block when the table has no column. The
  // ledger scrolls inside the rail, so the view block (and the orders docked
  // in it) never falls below the fold; while orders are open there, they take
  // the log's place.
  const isOrderingInRail = layout.mode === "none" && (hasOrders || hasBuild);
  const rail = (
    <>
      {views.length <= 2 ? (
        views.map((view) => (
          <ArmyCard
            isOwed={isOwed && view.isViewer}
            key={view.army.entry.playerId}
            onOpen={() => openArmySheet(view.army.entry.playerId)}
            power={view.isViewer ? power : undefined}
            view={view}
          />
        ))
      ) : (
        <Ledger isOwed={isOwed} onOpen={openArmySheet} power={power} views={views} />
      )}
      {layout.mode === "none" ? (
        <section aria-label="View" {...stylex.props(styles.viewBlock)}>
          {tileMessage ?? <TileLine hasNames />}
          <BoardKeys runner={runner} />
          {locator}
          <DockedAttackPreview />
          {ordersMenu}
          {buildMount === "rail" ? buildMenu : null}
        </section>
      ) : null}
      {log && review?.onSeek && !isOrderingInRail ? (
        <TurnLog
          armies={armies}
          boundaries={log.boundaries}
          currentIndex={log.currentIndex}
          isDisabled={review.isBusy}
          onSeek={review.onSeek}
        />
      ) : null}
    </>
  );

  const railFoot = (
    <>
      <InkIconKey
        icon={IconCalculator}
        isDisabled={isCalculatorOpen}
        label="Calculator"
        onPress={() => setIsCalculatorOpen(true)}
      />
      {isPlayer && !isViewerOut ? (
        <>
          <InkIconKey
            icon={IconMore}
            isDisabled={isMatchMenuOpen}
            label="Match"
            onPress={() => setIsMatchMenuOpen(true)}
          />
          <EndTurnKey
            factionCode={factionCode}
            isDisabled={isEndTurnDisabled}
            isLoading={isEndingTurn}
            isSpent={isTurnSpent && isViewerTurn}
            onPress={onEndTurn}
            readiness={turnReadiness}
            size="rail"
            tooltip={endTurnTooltip}
          />
        </>
      ) : null}
    </>
  );

  // Phone: the army plates, or the turn-order row over one plate.
  const showUnits = isReviewing;
  const plateView = pickPlateView(views, { isReviewing, moverSlot: activePlayerSlot });
  const phoneArmies =
    views.length <= 2 ? (
      <Grid columns={Math.max(views.length, 1)} gap={0} xstyle={styles.plates}>
        {views.map((view) => (
          <ArmyPlate
            hasYouTag
            isOwed={isOwed && view.isViewer}
            key={view.army.entry.playerId}
            onOpen={() => openArmySheet(view.army.entry.playerId)}
            showUnits={showUnits}
            view={view}
          />
        ))}
      </Grid>
    ) : (
      <>
        <TurnOrderRow isOwed={isOwed} onOpen={openArmySheet} views={views} />
        {/* A record-only plate would repeat the stamp and name above it. */}
        {plateView && !plateView.isRecordOnly ? (
          <ArmyPlate
            hasYouTag={false}
            isOwed={false}
            isWide
            onOpen={() => openArmySheet(plateView.army.entry.playerId)}
            showUnits={showUnits}
            view={plateView}
          />
        ) : null}
      </>
    );

  const viewerEntry = viewerArmy?.entry ?? null;
  // The board is waited on only once the match has a connection to send it;
  // until then the connection itself is what is waited on, and the strip (or
  // the phone dock) already says so.
  const waiting =
    boardError === null && (initialBoard !== null || connection === "connected")
      ? "Loading the board"
      : null;
  const tileRow = hasOrders ? null : (
    <HStack align="center" gap={2} xstyle={styles.tileRow}>
      {hasNotice ? (
        <VStack xstyle={styles.tileRowNotice}>{notices}</VStack>
      ) : isImmersiveNotice ? (
        <ImmersiveNoticeLine xstyle={styles.wholeLine} />
      ) : alertsOffer.isOffering ? (
        <TurnAlertsOffer onDismiss={alertsOffer.dismiss} />
      ) : boardView.bv === null ? (
        <>
          {waiting === null ? (
            <HStack as="span" xstyle={[styles.wholeLine, styles.idleLine]} />
          ) : (
            <WaitingLine label={waiting} />
          )}
          <ViewKey runner={runner} />
        </>
      ) : viewerEntry?.activePower !== undefined && isViewerTurn && seenPowerDay !== day ? (
        <>
          <HStack as="span" role="status" xstyle={[styles.wholeLine, styles.powerOn]}>
            <b>{powerName(viewerEntry, viewerEntry.activePower === "scop" ? "scop" : "cop")} on</b>
          </HStack>
          <Button
            label="OK"
            onClick={() => setSeenPowerDay(day)}
            variant="on-ink"
            xstyle={styles.okKey}
          />
        </>
      ) : armedPower !== null && viewerEntry ? (
        <ArmedPower
          entry={viewerEntry}
          level={armedPower}
          onCancel={() => setArmedPower(null)}
          onUse={() => {
            onActivatePower(armedPower);
            setArmedPower(null);
          }}
        />
      ) : whole.isWhole ? (
        <>
          <HStack as="span" xstyle={styles.wholeLine}>
            <b {...stylex.props(styles.wholeScale)}>{formatScale(boardView.bv?.fitScale ?? 1)}</b>
            Tap to jump
          </HStack>
          <ViewKey runner={runner} />
        </>
      ) : (
        <>
          <FirstTouchHint>
            <TileLine onOpen={() => setIsTileSheetOpen(true)} />
          </FirstTouchHint>
          <ViewKey runner={runner} />
        </>
      )}
    </HStack>
  );

  const dock = hasOrders ? (
    ordersMenu
  ) : isReviewing && review ? (
    <PhoneReviewBar review={review} />
  ) : (
    <DockRow>
      {viewerEntry && strongestPower ? (
        <PowerKey
          isArmed={armedPower !== null}
          isLoading={activatingPower !== undefined}
          label={`CO power: ${powerName(viewerEntry, strongestPower)}`}
          level={strongestPower}
          onPress={() => setArmedPower((armed) => (armed === null ? strongestPower : null))}
          size="dock"
        />
      ) : null}
      <DockKey
        icon={IconCalculator}
        isDisabled={isCalculatorOpen}
        label="Calculator"
        onPress={() => setIsCalculatorOpen(true)}
      />
      {isPlayer && !isViewerOut ? (
        <DockKey icon={IconMore} label="Match" onPress={() => setIsMatchMenuOpen(true)} />
      ) : null}
      {connection === "signedOut" ? (
        <SignInKey size="dock" />
      ) : !isConnected ? (
        <>
          <WaitCell
            line={connection === "reconnecting" ? "reconnecting" : "connecting"}
            name={connection === "reconnecting" ? "Reconnecting" : "Connecting"}
          />
          {/* A dock key, so the word "Reconnecting" keeps its room at 390px. */}
          {connection === "reconnecting" ? (
            <DockKey icon={IconRefresh} label="Reconnect" onPress={reconnect} />
          ) : null}
        </>
      ) : isPlayer && !isViewerOut && isViewerTurn ? (
        isSelecting && !hasBuild ? (
          <DockKey
            icon={IconClose}
            label="Deselect"
            onPress={() => {
              const canvas = runner.host.canvas;
              if (canvas) pressBoardKey(canvas, "Escape");
            }}
          />
        ) : (
          <EndTurnKey
            factionCode={factionCode}
            isDisabled={isEndTurnDisabled}
            isLoading={isEndingTurn}
            isSpent={isTurnSpent}
            onPress={onEndTurn}
            readiness={turnReadiness}
            size="dock"
            tooltip={endTurnTooltip}
          />
        )
      ) : mover ? (
        <WaitCell
          clock={
            moverClock ? (
              <StripClock
                label={moverClock.label}
                pressure={moverClock.pressure}
                value={moverClock.value}
              />
            ) : undefined
          }
          name={mover.name}
        />
      ) : null}
    </DockRow>
  );

  const sheetView = views.find((view) => view.army.entry.playerId === sheetArmyId) ?? null;
  const fullscreenKey = (
    <FullscreenKey isFullscreen={isFullscreen} onEnter={enterFullscreen} onExit={exitFullscreen} />
  );

  return (
    <>
      <MatchFrame
        dock={dock}
        foot={review ? <ReviewFoot review={review} /> : undefined}
        fullscreenMode={fullscreenMode}
        label={match.name}
        phoneArmies={phoneArmies}
        rail={rail}
        railFoot={railFoot}
        strip={
          <TurnStrip
            clock={stripClock}
            day={day}
            isDayNew={isDayNew}
            isLive={!isReviewing}
            isOwed={isOwed}
            keys={
              <>
                {connection === "reconnecting" ? <ReconnectKey onPress={reconnect} /> : null}
                {fullscreenKey}
              </>
            }
            sub={
              <>
                {subLine}
                {isImmersiveNotice ? <ImmersiveNoticeLine /> : null}
              </>
            }
            title={title}
          />
        }
        surfaceRef={surfaceRef}
        table={
          <BoardTable
            canvasContainerRef={canvasContainerRef}
            layout={layout}
            locator={locator}
            map={mapDims}
            floating={calculator}
            notice={columnNotices}
            overlay={<WholeMapOutline runner={runner} />}
            side={side}
            tableRef={tableRef}
            waiting={waiting}
          />
        }
        tileRow={tileRow}
        turnLine={
          <TurnLine
            back={<BackKey label="Back to Play" to="/matches" />}
            day={day}
            isDayNew={isDayNew}
            isTitleDim={!isOwed && isPlayer}
            keys={
              <>
                {review && !isReviewing ? (
                  <InkIconKey
                    icon={IconReplay}
                    isDisabled={review.isBusy || !review.canStepBack}
                    label="Review earlier moments"
                    onPress={() => review.onStep(-1)}
                  />
                ) : null}
                {isFullscreenEnabled ? fullscreenKey : null}
              </>
            }
            sub={subLine}
            title={title}
          />
        }
      />
      {buildSheet.value?.site
        ? renderBuildMenu(buildSheet.value, "sheet", buildSheet.isOpen)
        : null}

      <MatchMenu
        canResign={isPlayer && !isViewerOut}
        clockTerms={clockTerms(match.settings.clock)}
        disabledReason={isConnected ? undefined : "Reconnect to the match before resigning."}
        funds={match.settings.startingFunds}
        hasFog={match.settings.fogEnabled}
        isOpen={isMatchMenuOpen}
        isResigning={isResigning}
        mapId={match.mapId}
        mapName={mapName}
        onOpenChange={setIsMatchMenuOpen}
        onResign={onResign}
      />

      <ArmySheet
        isOpen={isArmySheetOpen && sheetView !== null}
        onOpenChange={setIsArmySheetOpen}
        power={sheetView?.isViewer ? power : undefined}
        view={sheetView}
      />

      <ModalSheet
        isOpen={isTileSheetOpen}
        onOpenChange={setIsTileSheetOpen}
        title={hoveredTile?.unit?.name ?? hoveredTile?.terrainName ?? "Tile"}
      >
        <TileDetails />
      </ModalSheet>
    </>
  );
}

/**
 * Fill the funds of armies the engine has not reported on yet from the match
 * state the host opened with.
 */
function withOpeningFunds(
  armies: MatchArmy[],
  players: readonly { slotIndex: number; funds: number }[] | undefined,
): MatchArmy[] {
  if (!players) return armies;
  return armies.map((army) => {
    if (army.hasLiveStats || army.entry.stats.funds !== undefined) return army;
    const funds = players.find((player) => player.slotIndex === army.entry.playerId)?.funds;
    if (funds === undefined) return army;
    return { ...army, entry: { ...army.entry, stats: { ...army.entry.stats, funds } } };
  });
}

const styles = stylex.create({
  gameCanvas: {
    display: "block",
    width: "100%",
    height: "100%",
    imageRendering: "pixelated",
    outline: "none",
    touchAction: "none",
  },
  matchName: {
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    color: awbrnVars.textOnInk,
    fontWeight: 650,
  },
  viewBlock: {
    display: "grid",
    flexShrink: 0,
    gap: spacingVars["--spacing-3"],
    paddingBlock: awbrnVars.spacing3_5,
    paddingInline: spacingVars["--spacing-4"],
    borderBlockEndColor: awbrnVars.ink800,
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: borderVars["--border-width"],
  },
  plates: {
    display: "grid",
    backgroundColor: awbrnVars.ink900,
    borderBlockEndColor: awbrnVars.ink800,
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: borderVars["--border-width"],
  },
  tileRow: {
    display: "flex",
    alignItems: "center",
    gap: spacingVars["--spacing-2"],
    minBlockSize: matchLayout.footHeight,
    paddingBlock: spacingVars["--spacing-1-5"],
    paddingInline: spacingVars["--spacing-2"],
    backgroundColor: awbrnVars.ink900,
    borderBlockStartColor: awbrnVars.ink800,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: borderVars["--border-width"],
  },
  /** A notice in the tile window's place: the window's frame, on ink. */
  columnNotice: {
    overflow: "hidden",
    borderRadius: radiusVars["--radius-container"],
    boxShadow: `inset 0 0 0 1px ${awbrnVars.ink700}`,
  },
  tileRowNotice: {
    flexGrow: 1,
    minInlineSize: 0,
    marginBlock: `calc(-1 * ${spacingVars["--spacing-1-5"]})`,
    marginInline: `calc(-1 * ${spacingVars["--spacing-2"]})`,
  },
  /** The tile row's place, held empty while the match is not connected. */
  idleLine: {
    backgroundColor: "transparent",
  },
  wholeLine: {
    display: "flex",
    alignItems: "center",
    gap: awbrnVars.spacing2_5,
    flexGrow: 1,
    minInlineSize: 0,
    blockSize: matchLayout.target,
    paddingInline: spacingVars["--spacing-3"],
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: awbrnVars.ink850,
    color: awbrnVars.textOnInkSecondary,
    fontSize: matchType.small,
  },
  powerOn: {
    color: awbrnVars.gold,
    boxShadow: `inset 0 0 0 1.5px ${awbrnVars.gold}`,
    backgroundColor: awbrnVars.ink950,
  },
  okKey: {
    flexShrink: 0,
    blockSize: matchLayout.target,
    minInlineSize: matchLayout.dockKeyWidth,
  },
  wholeScale: {
    color: awbrnVars.textOnInkStrong,
    fontFamily: awbrnVars.fontFigure,
    fontSize: matchType.body,
    fontWeight: 600,
  },
});
