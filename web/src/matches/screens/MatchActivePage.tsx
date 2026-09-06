import { useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { AlertDialog } from "@astryxdesign/core/AlertDialog";
import { Badge } from "@astryxdesign/core/Badge";
import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "#/ui/Button.tsx";
import { Card } from "@astryxdesign/core/Card";
import { Grid } from "@astryxdesign/core/Grid";
import { Heading } from "@astryxdesign/core/Heading";
import { Section } from "@astryxdesign/core/Section";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { useMediaQuery } from "@astryxdesign/core/hooks";
import { StatusDot } from "@astryxdesign/core/StatusDot";
import { Text } from "@astryxdesign/core/Text";
import {
  borderVars,
  colorVars,
  fontWeightVars,
  spacingVars,
  typeScaleVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import { Calculator as CalculatorIcon } from "pixelarticons/react/Calculator";
import { MenuSquare as MatchMenuIcon } from "pixelarticons/react/MenuSquare";
import * as stylex from "@stylexjs/stylex";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useCanvasCourierSurface } from "#/canvas_courier/index.ts";
import { loadCoPortraitCatalog } from "#/components/co_portraits.ts";
import { BoardFullscreenExit, GameFullscreenButton } from "#/components/GameFullscreen.tsx";
import { TileInfoBar } from "#/components/TileInfoBar.tsx";
import { useActiveMatchRunner } from "#/engine/runtime_context.tsx";
import type { GameRunner } from "#/engine/game_runner.ts";
import { useGameActions, useGameStore } from "#/engine/store.ts";
import type { LiveMatchPlayer } from "#/engine/worker_module.ts";
import { getFactionByCode } from "#/factions.ts";
import { rosterLayout } from "#/ui/rosterLayout.stylex.ts";
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
import { StepControls } from "#/components/StepControls.tsx";
import { clockPressure, formatClockDuration, seatRemainingMs } from "#/matches/match_clock.ts";
import { SeatClock, useClockNow } from "#/matches/components/SeatClock.tsx";
import { buildArmies } from "#/matches/match_armies.ts";
import { matchKeys } from "#/matches/matches.keys.ts";
import { matchDetailQueryOptions } from "#/matches/matches.queries.ts";
import { MatchOverDialog } from "#/matches/components/MatchOverDialog.tsx";
import type { MatchClock } from "#/matches/schemas.ts";
import {
  BATTLE_CALCULATOR_SHEET_MEDIA,
  BattleCalculator,
} from "#/matches/components/BattleCalculator.tsx";
import { BuildMenu } from "#/matches/components/BuildMenu.tsx";
import { MatchMenu } from "#/matches/components/MatchMenu.tsx";
import { AttackPreview } from "#/matches/components/AttackPreview.tsx";
import { UnitActionMenu } from "#/matches/components/UnitActionMenu.tsx";
import { RosterList, RosterRow } from "#/replay/RosterRow.tsx";
import { canActivatePower } from "#/replay/power_meter.ts";
import { describeTurnResidue } from "#/matches/turn_residue.ts";
import type { ActivatablePowerLevel } from "#/replay/power_meter.ts";
import type {
  PlayerRosterSnapshot,
  ProductionOptionsChanged,
  UnitActionsChanged,
  UnitKind,
} from "#/wasm/awbrn_wasm.js";

/** The press that opened a menu: where it landed, and what pressed. */
interface BoardPress {
  at: number;
  isCoarse: boolean;
  x: number;
  y: number;
}

/**
 * Below this width the board has no room beside it for a menu, so the build
 * order becomes a sheet on the bottom edge whatever pressed the board.
 */
const BUILD_SHEET_MEDIA = "(max-width: 767px)";

/**
 * A device whose primary input is a finger. The board's own menus read this
 * from the press that opened them; a menu opened from the strip has no press
 * on the board to read, so it asks the device instead.
 */
const COARSE_POINTER_MEDIA = "(pointer: coarse)";

/**
 * Below this width the board gives up the page inset and runs to both edges.
 * This is the same breakpoint the build order uses to become a sheet: the width
 * at which the phone, not the desk, is the machine being designed for.
 */
const BOARD_BLEED_MEDIA = "@media (max-width: 767px)";

/**
 * The AWBW page for a map, so a player can read the map the match is played on
 * where it is published. AWBRN is independent of AWBW; this is a reference to
 * the map, not a claim of any relationship.
 */

/**
 * How long a press can still be the one that opened a menu. A selection made
 * from the keyboard has no press behind it, and a menu placed at a pointer that
 * has not moved since the last turn would be placed nowhere in particular.
 */
const BOARD_PRESS_MAX_AGE_MS = 2000;

/** Said when a question about the past could not be put to the host at all. */
const REVIEW_UNREACHABLE = "The match could not be read because the connection is not open.";

export function MatchActivePage({
  joinSlug = null,
  matchId,
}: {
  joinSlug?: string | null;
  matchId: string;
}) {
  const { data: match } = useSuspenseQuery(matchDetailQueryOptions(matchId, joinSlug));
  const queryClient = useQueryClient();
  const portraitCatalog = useMemo(() => loadCoPortraitCatalog(), []);
  const runner = useActiveMatchRunner();
  const playerRoster = useGameStore((state) => state.playerRoster);
  const productionOptions = useGameStore((state) => state.productionOptions);
  const unitActions = useGameStore((state) => state.unitActions);
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
  const armies = useMemo(
    () => buildArmies(match.participants, playerRoster),
    [match.participants, playerRoster],
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
  /** What the engine has been told, read from inside callbacks and effects. */
  const isReviewingRef = useRef(false);
  const [initialBoard, setInitialBoard] = useState<InitialBoardMessage | null>(null);
  const [matchError, setMatchError] = useState<string | null>(null);
  const [boardError, setBoardError] = useState<string | null>(null);
  // `undefined` until the server says which seat, if any, this viewer holds.
  const [viewerSlotIndex, setViewerSlotIndex] = useState<number | null | undefined>(undefined);
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
          return;
        }
        case "connected": {
          // A fresh connection supersedes whatever went wrong on the last one.
          setMatchError(null);
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
          setActivePlayerSlot(message.activePlayerSlot);
          finishEndingTurn();
          void runner.applyLiveTransition(message.transition).catch((error) => {
            console.error("Error applying live player transition:", error);
          });
          return;
        }
        case "reviewOutline": {
          setReviewOutline(message.boundaries);
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
                setReviewError("The live board could not be restored. Reconnect to catch up.");
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
  const { reconnect, sendMessage, status } = useMatchWebSocket(matchId, handleMatchMessage);

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
  const viewerArmy = armies.find((army) => army.entry.playerId === viewerSlotIndex);

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
  }, [
    activePlayerSlot,
    finishEndingTurn,
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
  const requestStep = useCallback(
    (kind: "action" | "turn", delta: number) => {
      beginReview();
      const boundaries = reviewOutline;
      if (boundaries === null || currentReviewIndex === null) {
        // A step held for an outline that was never asked for would wait for
        // an answer nothing is coming back with, under a key that stays busy.
        if (!sendMessage({ type: "reviewOutline" })) {
          setReviewError(REVIEW_UNREACHABLE);
          return;
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
    [beginReview, currentReviewIndex, requestReviewSeek, reviewOutline, sendMessage],
  );

  // A step that was waiting on the outline, taken now that it has one.
  useEffect(() => {
    const pending = pendingStepRef.current;
    if (pending === null || reviewOutline === null) return;
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
  }, [requestReviewSeek, reviewOutline, reviewPosition]);

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
  const viewerRemainingMs =
    clock === null || viewerSlotIndex === null || viewerSlotIndex === undefined
      ? null
      : seatRemainingMs(clock, viewerSlotIndex, now);

  return (
    <Section padding={6} variant="transparent">
      <VStack gap={6}>
        {matchError ? <Banner description={matchError} status="error" title="Match error" /> : null}
        {reviewError ? (
          <Banner description={reviewError} status="error" title="The match could not be read" />
        ) : null}
        {boardError ? (
          <Banner description={boardError} status="error" title="The board could not be loaded." />
        ) : null}
        {spectatorFogActive === null ? null : (
          <Banner
            status="info"
            title={
              spectatorFogActive
                ? "Spectating — you see only what is public."
                : "Spectating — the full board is visible."
            }
          />
        )}

        {/* The board leads. It takes whatever width the viewport has and the
            roster keeps a fixed rail beside it, so the map is the largest thing
            on the page at every size instead of splitting the page with a
            readout. Below the breakpoint the rail drops under the board rather
            than beside it, and the board still comes first. */}
        <Grid align="start" gap={4} xstyle={styles.matchLayout}>
          <ActiveMatchBoard
            day={playerRoster?.day ?? null}
            initialBoard={initialBoard}
            review={
              // A fogged match has no board a watcher may read, so it offers
              // them no controls for reading one either.
              spectatorFogActive === true
                ? null
                : {
                    canStepBack: isReviewing
                      ? (reviewPosition?.index ?? 1) > 0
                      : // A match nobody has played yet has nothing behind it.
                        reviewOutline === null || reviewOutline.length > 1,
                    canStepForward:
                      isReviewing &&
                      (reviewPosition === null || reviewPosition.index < reviewPosition.total),
                    canStepTurnBack:
                      reviewOutline === null || currentReviewIndex === null
                        ? !isReviewing
                        : previousTurnStart(reviewOutline, currentReviewIndex) !== null,
                    canStepTurnForward:
                      isReviewing &&
                      (reviewOutline === null ||
                        currentReviewIndex === null ||
                        nextTurnStart(reviewOutline, currentReviewIndex) !== null),
                    isBusy: isReviewBusy || status !== "connected",
                    isReviewing,
                    onSeekLatest: returnToLiveMatch,
                    onSeekStart: () => {
                      beginReview();
                      requestReviewSeek(0);
                    },
                    onStep: (delta: number) => requestStep("action", delta),
                    onTurnStep: (delta: number) => requestStep("turn", delta),
                    position: isReviewing ? reviewPosition : null,
                    turnHolder: armies.find((army) => army.isActive)?.name ?? null,
                  }
            }
            match={match}
            onBoardError={setBoardError}
            onBuildUnit={handleBuildUnit}
            onEndTurn={requestEndTurn}
            onResign={handleResign}
            isResigning={isResigning}
            isViewerOut={viewerArmy?.entry.eliminated ?? false}
            isTurnReadinessKnown={turnReadiness !== undefined}
            isTurnSpent={turnResidue === null}
            players={livePlayers}
            productionOptions={productionOptions}
            unitActions={unitActions}
            playerRoster={playerRoster}
            reconnect={reconnect}
            runner={runner}
            status={status}
            activePlayerSlot={activePlayerSlot}
            isEndingTurn={isEndingTurn}
            matchClock={match.settings.clock}
            viewerFactionCode={viewerArmy?.entry.displayFactionCode ?? null}
            viewerFunds={viewerArmy?.entry.stats.funds ?? null}
            viewerRemainingMs={viewerRemainingMs}
            viewerSlotIndex={viewerSlotIndex}
          />

          {/* Each row names its own army through its crest, so the panel needs
              no visible heading of its own. An active match always has seats;
              if the record says otherwise, an empty outlined box would be the
              wrong thing to draw. */}
          {armies.length === 0 ? null : (
            <VStack as="section" aria-label="Armies" gap={0} xstyle={styles.rosterSection}>
              <Card padding={0} xstyle={styles.rosterPanel}>
                <RosterList>
                  {armies.map((army) => (
                    <RosterRow
                      isActive={army.isActive}
                      activatingPower={activatingPower}
                      clock={
                        clock === null ? undefined : (
                          <SeatClock
                            clock={match.settings.clock}
                            expiresAt={
                              army.entry.playerId === clock.activeSlot ? clock.deadlineAt : null
                            }
                            isRunning={army.entry.playerId === clock.activeSlot}
                            name={army.name}
                            remaining={seatRemainingMs(clock, army.entry.playerId, now)}
                          />
                        )
                      }
                      isViewer={viewerSlotIndex === army.entry.playerId}
                      key={army.entry.playerId}
                      name={army.name}
                      onActivatePower={
                        status === "connected" &&
                        army.isActive &&
                        viewerSlotIndex === army.entry.playerId
                          ? handleActivatePower
                          : undefined
                      }
                      onFactionChange={
                        army.hasLiveStats
                          ? (factionId) =>
                              handleDisplayFactionChange(
                                army.entry.playerId,
                                factionId === getFactionByCode(army.entry.actualFactionCode)?.id
                                  ? null
                                  : factionId,
                              )
                          : undefined
                      }
                      player={army.entry}
                      portraitCatalog={portraitCatalog}
                    />
                  ))}
                </RosterList>
              </Card>
            </VStack>
          )}
        </Grid>
      </VStack>

      {/* The one move in the game that cannot be taken back, and the only one
          the page asks about. It asks only when the turn has something left in
          it, so the question is never a habit. */}
      <AlertDialog
        actionLabel="End turn"
        actionVariant="primary"
        cancelLabel="Keep playing"
        description={turnResidue ?? ""}
        isActionLoading={isEndingTurn}
        isOpen={isConfirmingEndTurn}
        onAction={() => {
          setIsConfirmingEndTurn(false);
          handleEndTurn();
        }}
        onOpenChange={setIsConfirmingEndTurn}
        title="End your turn?"
      />

      {/* How the match ended, over the board it ended on. Watching the replay
          is what re-reads the match, and the phase it comes back with is what
          hands this page over to the review one. */}
      {matchResults ? (
        <MatchOverDialog
          armies={armies}
          isOpen={isResultOpen}
          matchId={matchId}
          onOpenChange={setIsResultOpen}
          onWatchReplay={() => {
            setIsResultOpen(false);
            void queryClient.invalidateQueries({
              queryKey: matchKeys.detail(matchId, joinSlug),
            });
          }}
          portraitCatalog={portraitCatalog}
          results={matchResults}
          viewerSlotIndex={viewerSlotIndex ?? null}
        />
      ) : null}
    </Section>
  );
}

/**
 * The board, with the game's own readouts on the strip beneath it.
 *
 * Day, map, and connection belong to the board rather than to the page, so they
 * stay in the same viewport as the thing they describe at every width.
 */
/**
 * The keys that walk a match, and everything they need to draw themselves.
 *
 * Null for a viewer with nothing to walk: a fogged match keeps its history
 * from anybody holding no seat in it.
 */
interface BoardReviewControls {
  /** True while the board is showing a moment the match has moved on from. */
  isReviewing: boolean;
  isBusy: boolean;
  position: { index: number; total: number } | null;
  canStepBack: boolean;
  canStepForward: boolean;
  canStepTurnBack: boolean;
  canStepTurnForward: boolean;
  turnHolder: string | null;
  onSeekStart: () => void;
  onStep: (delta: number) => void;
  onTurnStep: (delta: number) => void;
  onSeekLatest: () => void;
}

function ActiveMatchBoard({
  activePlayerSlot,
  day,
  initialBoard,
  review,
  isEndingTurn,
  isResigning,
  isTurnReadinessKnown,
  isTurnSpent,
  isViewerOut,
  match,
  matchClock,
  onBoardError,
  onBuildUnit,
  onEndTurn,
  onResign,
  players,
  playerRoster,
  productionOptions,
  unitActions,
  reconnect,
  runner,
  status,
  viewerFactionCode,
  viewerFunds,
  viewerRemainingMs,
  viewerSlotIndex,
}: {
  activePlayerSlot: number | null;
  day: number | null;
  initialBoard: InitialBoardMessage | null;
  isEndingTurn: boolean;
  /** True while the viewer's resignation is on its way to the server. */
  isResigning: boolean;
  isTurnReadinessKnown: boolean;
  isTurnSpent: boolean;
  /** True once the viewer's own army has left the match. */
  isViewerOut: boolean;
  match: { mapId: string; maxPlayers: number; name: string; settings: { fogEnabled: boolean } };
  onBoardError: (message: string | null) => void;
  onBuildUnit: (unit: UnitKind, x: number, y: number) => void;
  onEndTurn: () => void;
  onResign: () => void;
  players: LiveMatchPlayer[];
  playerRoster: PlayerRosterSnapshot | null;
  review: BoardReviewControls | null;
  productionOptions: ProductionOptionsChanged | null;
  unitActions: UnitActionsChanged | null;
  reconnect: () => void;
  runner: GameRunner;
  status: MatchWebSocketStatus;
  matchClock: MatchClock;
  viewerFactionCode: string | null;
  viewerFunds: number | null;
  /** What the viewer's own bank holds, or null when they hold no seat. */
  viewerRemainingMs: number | null;
  viewerSlotIndex: number | null | undefined;
}) {
  const {
    canvasRef,
    enterFullscreen,
    exitFullscreen,
    focus,
    fullscreenMode,
    isFullscreen,
    surfaceRef,
  } = useCanvasCourierSurface({ controller: runner });
  const playersRef = useRef(players);
  playersRef.current = players;
  const onBoardErrorRef = useRef(onBoardError);
  onBoardErrorRef.current = onBoardError;
  const setProductionOptions = useGameActions().setProductionOptions;
  const isCompactViewport = useMediaQuery(BUILD_SHEET_MEDIA);
  const pressRef = useRef<BoardPress | null>(null);
  const [press, setPress] = useState<BoardPress | null>(null);
  const [isCalculatorOpen, setIsCalculatorOpen] = useState(false);
  const [isMatchMenuOpen, setIsMatchMenuOpen] = useState(false);
  const dismissMatchMenu = useCallback(() => setIsMatchMenuOpen(false), []);
  // The calculator's own breakpoint. It is not the build menu's: a sheet under
  // a thumb is one question, and a panel that outgrows the board frame is
  // another.
  const isCalculatorCompact = useMediaQuery(BATTLE_CALCULATOR_SHEET_MEDIA);
  const isCoarsePointer = useMediaQuery(COARSE_POINTER_MEDIA);
  const productionSite = productionOptions?.site;

  // The last press on the board, so the menu opens in the shape the input the
  // player used expects, and beside the press on a board too old to say where
  // the tile is. A press is only kept for as long as it can still plausibly be
  // the one that opened a menu.
  useEffect(() => {
    const surface = surfaceRef.current;
    if (!surface) return;

    const onPointerDown = (event: PointerEvent) => {
      const bounds = surface.getBoundingClientRect();
      pressRef.current = {
        at: event.timeStamp,
        isCoarse: event.pointerType !== "mouse",
        x: event.clientX - bounds.left,
        y: event.clientY - bounds.top,
      };
    };

    surface.addEventListener("pointerdown", onPointerDown, { passive: true });
    return () => surface.removeEventListener("pointerdown", onPointerDown);
  }, [surfaceRef]);

  // A menu belongs to the selection that opened it, so the press is read once,
  // when the engine reports the site, and then held still while the menu is up.
  const openMenu = productionOptions ?? unitActions;
  useEffect(() => {
    if (openMenu === null) {
      setPress(null);
      return;
    }

    const recent = pressRef.current;
    setPress(recent && performance.now() - recent.at < BOARD_PRESS_MAX_AGE_MS ? recent : null);
  }, [openMenu]);

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

  useEffect(() => {
    if (!initialBoard) return;

    let cancelled = false;
    void Promise.resolve()
      .then(async () => {
        if (cancelled) return;
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
        if (cancelled) return;
        onBoardErrorRef.current(
          error instanceof Error ? error.message : "The engine did not report a reason.",
        );
      });

    return () => {
      cancelled = true;
    };
  }, [initialBoard, runner]);

  // The map names itself in the readout, so the status line reports the
  // connection and nothing else. Naming the map here as well made a socket
  // state and a match identity share one sentence, and neither one read.
  const mapName = initialBoard?.map.metadata.name ?? null;
  const statusText =
    status === "connected"
      ? initialBoard
        ? "Live"
        : "Waiting for board state…"
      : status === "connecting"
        ? "Connecting to match…"
        : status === "error"
          ? "Connection error — retrying."
          : "Disconnected — reconnecting.";
  const isPlayer = viewerSlotIndex !== null && viewerSlotIndex !== undefined;
  const isViewerTurn = isPlayer && viewerSlotIndex === activePlayerSlot;
  const endTurnTooltip =
    status !== "connected"
      ? "Reconnect to the match before ending your turn."
      : !isViewerTurn
        ? "You can end the turn when your army is active."
        : !isTurnReadinessKnown
          ? "Waiting for the board to report turn readiness."
          : isTurnSpent
            ? "Nothing left to do this turn (Ctrl+Enter)."
            : "Ctrl+Enter";
  const buildBlockedReason =
    status !== "connected" ? "Reconnect to the match to send this order." : undefined;
  // A resignation is not an order to the board, so it says what it needs in its
  // own words rather than borrowing the board's.
  const matchBlockedReason =
    status !== "connected" ? "Reconnect to the match before resigning." : undefined;

  return (
    <Card padding={0} variant="muted" xstyle={styles.boardPanel}>
      {/* The engine draws 960x640; the frame keeps that ratio at every width so
          the map is never stretched or dropped into a tall slot on a phone.
          Full screen is the one place that ratio is dropped: the engine redraws
          at whatever size the canvas is given, so filling the screen buys the
          player board rather than stretching what was already there. */}
      <VStack
        gap={0}
        ref={surfaceRef}
        xstyle={[
          styles.gameSurface,
          isFullscreen && styles.gameSurfaceFullscreen,
          fullscreenMode === "immersive" && styles.gameSurfaceImmersive,
        ]}
      >
        <canvas
          ref={canvasRef}
          width={960}
          height={640}
          tabIndex={0}
          {...stylex.props(styles.gameCanvas)}
        />

        {isFullscreen ? (
          <BoardFullscreenExit mode={fullscreenMode} onExit={exitFullscreen} />
        ) : null}

        {/* The order sits on the battlefield with the base it belongs to, not
            in a strip under the board that a phone would have to scroll to. */}
        {productionOptions === null || productionSite === undefined ? null : (
          <BuildMenu
            anchor={productionOptions.anchor ?? press}
            disabledReason={buildBlockedReason}
            factionCode={viewerFactionCode ?? "os"}
            funds={viewerFunds ?? null}
            isEnabled={buildBlockedReason === undefined}
            onBuild={(unit) => onBuildUnit(unit, productionSite.x, productionSite.y)}
            onDismiss={dismissBuildMenu}
            onRestoreFocus={focus}
            options={productionOptions.options}
            presentation={press?.isCoarse || isCompactViewport ? "sheet" : "board"}
            site={productionSite}
          />
        )}

        {/* What the unit does where it is going. This is the only thing on the
            board that commits an order, so it is the one place a move can be
            reconsidered before it becomes real. */}
        {unitActions === null || unitActions.destination === undefined ? null : (
          <UnitActionMenu
            anchor={unitActions.anchor ?? press}
            attacker={unitActions.attacker ?? undefined}
            destination={unitActions.destination}
            disabledReason={buildBlockedReason}
            isEnabled={buildBlockedReason === undefined}
            onChoose={chooseUnitAction}
            onDismiss={dismissUnitActions}
            onRestoreFocus={focus}
            options={unitActions.options}
            preselected={unitActions.preselected ?? undefined}
            presentation={press?.isCoarse || isCompactViewport ? "sheet" : "board"}
          />
        )}

        {/* The engagement a player is imagining, over the board they are
            imagining it on. It commits nothing: the action menu stays the only
            thing that can spend a unit. */}
        {isCalculatorOpen ? (
          <BattleCalculator
            onDismiss={() => setIsCalculatorOpen(false)}
            onRestoreFocus={focus}
            presentation={isCalculatorCompact ? "sheet" : "board"}
            roster={playerRoster}
            runner={runner}
            viewerSlotIndex={viewerSlotIndex ?? null}
          />
        ) : null}

        {/* What the player can do to the match rather than to a unit in it.
            It opens in the middle of the board because the strip it was opened
            from names no tile, and it is the same window every other order is
            chosen from. */}
        {isMatchMenuOpen ? (
          <MatchMenu
            day={day}
            disabledReason={matchBlockedReason}
            isEnabled={matchBlockedReason === undefined}
            onDismiss={dismissMatchMenu}
            onResign={() => {
              // The server is authoritative and answers on the socket, so the
              // menu closes on the order the way the production menu does.
              setIsMatchMenuOpen(false);
              onResign();
            }}
            onRestoreFocus={focus}
            presentation={isCoarsePointer || isCompactViewport ? "sheet" : "board"}
          />
        ) : null}

        {/* What the shot being aimed would cost, in the frame the order menu
            will arrive in. It stands beside the target rather than in a corner,
            and it takes no press: the shot is fired through it. */}
        <AttackPreview surfaceRef={surfaceRef} />

        {/* The terrain window stands on the battlefield, the way the game's own
            does, so reading a tile costs the page no height. */}
        <TileInfoBar />
      </VStack>

      {/* The readout carries no chrome of its own: inside a panel that already
          has an outline, a second frame would only add noise. The End turn key
          is the one object in the strip and the one place command orange
          appears, so a glance separates what the match is from what the player
          can do about it. */}
      <HStack
        align="center"
        gap={4}
        justify="between"
        paddingBlock={3}
        paddingInline={3}
        wrap="wrap"
        xstyle={styles.boardHud}
      >
        {/* An <output> is a live region by default, so a drop is announced
            rather than only recolored. The dot repeats what the text beside it
            already says, so it is hidden from assistive technology. */}
        <HStack align="center" as="output" gap={3} wrap="wrap" xstyle={styles.hudReadout}>
          {/* Before the engine reports, there is no day to report. */}
          {day === null ? null : <Badge label={`Day ${day}`} variant="info" />}
          {/* The day beside it is the day of the moment being read, not of the
              match, so the strip says which of the two a reader is looking
              at. */}
          {review?.isReviewing ? <Badge label="Reading back" variant="warning" /> : null}
          {/* The match names itself here rather than in a display heading above
              the board. A player inside a match already knows which match they
              are in, and on a phone that heading cost more board than the name
              was worth. It stays the page's heading; it just stops shouting.
              A match name is free text, so it may arrive as one unbroken run. */}
          <Heading level={1} xstyle={styles.hudMatchName}>
            {match.name}
          </Heading>
          {/* The map is the one thing here a player may want to leave the match
              for, so it names itself and links out. The id is the fallback for
              the window before the engine reports the board, not the label. */}
          <Text type="supporting" xstyle={styles.hudMap}>
            {mapName ?? `Map ${match.mapId}`}
          </Text>
          <HStack align="center" gap={2} xstyle={styles.hudState}>
            <StatusDot
              aria-hidden="true"
              isPulsing={status === "connecting"}
              label={statusText}
              variant={statusVariant(status)}
            />
            <Text color={status === "connected" ? "primary" : "secondary"} type="supporting">
              {statusText}
            </Text>
          </HStack>
        </HStack>
        {/* Every control the strip offers, and the one reading that belongs to
            a control: how much of the viewer's own bank the open turn is
            spending, beside the key that stops it spending. */}
        <HStack align="center" gap={3} wrap="wrap">
          {isViewerTurn && viewerRemainingMs !== null ? (
            <TurnClockNotice openingMs={matchClock.initialMs} remaining={viewerRemainingMs} />
          ) : null}
          {/* While the board holds the screen this strip is behind it, and the
              way back out is the key on the board itself. */}
          <Button
            clickAction={() => setIsCalculatorOpen(true)}
            icon={<CalculatorIcon aria-hidden height={16} width={16} />}
            isDisabled={isCalculatorOpen}
            label="Calculator"
            size="sm"
            variant="secondary"
          />
          {isFullscreen ? null : <GameFullscreenButton onEnter={enterFullscreen} />}
          {status === "disconnected" || status === "error" ? (
            <Button clickAction={reconnect} label="Reconnect" size="sm" variant="secondary" />
          ) : null}
          {/* Match commands stand apart from the board's own orders: a key
              that opens a menu, so nothing that ends a match is ever one press
              away from the key that ends a turn. A seat that has already left
              has no match commands, so the key goes with them. */}
          {isPlayer && !isViewerOut ? (
            <Button
              clickAction={() => setIsMatchMenuOpen(true)}
              icon={<MatchMenuIcon aria-hidden height={16} width={16} />}
              isDisabled={isMatchMenuOpen || isResigning}
              isLoading={isResigning}
              label="Match"
              size="sm"
              variant="secondary"
            />
          ) : null}
          {/* A seat that has left the match will never be on the move again,
              so the key that would hand its turn on goes with it. */}
          {isPlayer && !isViewerOut ? (
            <Button
              clickAction={onEndTurn}
              isDisabled={
                status !== "connected" ||
                !isViewerTurn ||
                !isTurnReadinessKnown ||
                review?.isReviewing === true
              }
              isLoading={isEndingTurn}
              label="End turn"
              size="sm"
              tooltip={review?.isReviewing === true ? "Return to live to play on" : endTurnTooltip}
              // Ending the turn is the primary thing to do only once there is
              // nothing else: a board with units still to move has its primary
              // action out on the board, not on the strip.
              variant={isTurnSpent && isViewerTurn ? "primary" : "secondary"}
            />
          ) : null}
        </HStack>
      </HStack>

      {/* The match is walked from under the board it is drawn on, so the keys
          are beside the thing they move. They sit below the strip rather than
          in it: the strip says what the match is, and these change which
          moment of it is on the board. */}
      {review === null ? null : (
        <VStack gap={0} paddingBlock={2} paddingInline={3} xstyle={styles.stepControls}>
          <StepControls
            canStepBack={review.canStepBack}
            canStepForward={review.canStepForward}
            canStepTurnBack={review.canStepTurnBack}
            canStepTurnForward={review.canStepTurnForward}
            day={day ?? 1}
            isAtLatest={!review.isReviewing}
            isBusy={review.isBusy}
            latestLabel="Live"
            onSeekLatest={review.onSeekLatest}
            onSeekStart={review.onSeekStart}
            onStep={review.onStep}
            onTurnStep={review.onTurnStep}
            position={review.position}
            turnHolder={review.turnHolder}
          />
        </VStack>
      )}
    </Card>
  );
}

/**
 * The viewer's own bank, on the strip, once it is worth acting on.
 *
 * The roster already reports every army's clock, this one included, so a
 * healthy bank has nothing to add here and says nothing. What the roster
 * cannot do is put the warning against the key that answers it, so the line
 * appears when the bank turns short and names what to do about it.
 */
function TurnClockNotice({ openingMs, remaining }: { openingMs: number; remaining: number }) {
  const pressure = clockPressure(remaining, openingMs);
  if (pressure === "steady") return null;
  const span = formatClockDuration(remaining);

  return (
    <HStack
      align="center"
      xstyle={[styles.turnClock, pressure === "critical" && styles.turnClockCritical]}
    >
      <Text as="p" color="inherit" hasTabularNumbers type="supporting">
        {pressure === "critical" ? `End your turn — ${span} left` : `${span} left on your clock`}
      </Text>
    </HStack>
  );
}

/**
 * A dropped live match is a warning, not a resting state. Neutral is the
 * quietest color in the set and it read as "nothing is wrong" on the one event
 * a player most needs to notice.
 */
function statusVariant(status: MatchWebSocketStatus): "success" | "warning" | "error" | "neutral" {
  switch (status) {
    case "connected":
      return "success";
    case "connecting":
      return "warning";
    case "error":
      return "error";
    case "disconnected":
      return "warning";
  }
}

const styles = stylex.create({
  // The reading sits on the strip with no chrome of its own. Its color is the
  // whole signal, so it moves through the game's own low supply and damage
  // rather than through a severity scale, and it never blinks: the system has
  // one gesture and this is not it.
  turnClock: {
    flex: "0 0 auto",
    color: colorVars["--color-warning"],
  },
  turnClockCritical: {
    color: colorVars["--color-error"],
  },
  // One column until the rail and a board wide enough to read both fit; two
  // from there. The board takes the width its height allows and the rail stays
  // against it, so a wide window puts the spare space around the pair instead
  // of between the map and the armies that describe it.
  matchLayout: {
    gridTemplateColumns: {
      default: "minmax(0, 1fr)",
      [rosterLayout.desktopMedia]: rosterLayout.railColumns,
    },
    justifyContent: {
      default: "stretch",
      [rosterLayout.desktopMedia]: "center",
    },
  },
  // A 3:2 board that grows without limit runs off a laptop screen, and a map
  // you have to scroll to see is not the focal point. The width is capped at
  // whatever keeps the whole board in the viewport, with a floor so a short
  // window shrinks the board rather than making it unreadable.
  // On a phone the board takes the whole viewport width, pulling out through
  // the page inset. The inset is worth 12% of a 390px screen, and the board is
  // 3:2, so that width buys height as well. Everything else on the page keeps
  // its inset; only the board bleeds.
  //
  // A negative margin alone only moves the panel: `inlineSize` and
  // `maxInlineSize` both resolve against the inset column, so the panel keeps
  // the column's width and slides left, leaving the gap on one side. The width
  // has to grow by the same amount the margins pull out.
  //
  // The amount is the page inset, which is this screen's `Section padding={6}`
  // and nothing else, because the app shell is mounted with `contentPadding={0}`.
  // Measuring against the viewport with `vw` instead would ignore the scrollbar
  // and scroll the body sideways on a narrow desktop window.
  boardPanel: {
    overflow: "hidden",
    justifySelf: "start",
    inlineSize: {
      default: "100%",
      [BOARD_BLEED_MEDIA]: `calc(100% + ${spacingVars["--spacing-6"]} * 2)`,
    },
    maxInlineSize: {
      default: rosterLayout.boardMaxInlineSize,
      [BOARD_BLEED_MEDIA]: "none",
    },
    marginInline: {
      default: 0,
      [BOARD_BLEED_MEDIA]: `calc(${spacingVars["--spacing-6"]} * -1)`,
    },
  },
  gameSurface: {
    position: "relative",
    aspectRatio: "3 / 2",
    backgroundColor: colorVars["--color-background-inverted"],
  },
  // The board gives up its 3:2 frame and takes the screen's own shape. The
  // engine is told the new canvas size and redraws to it, so the extra room is
  // more map rather than a stretched one.
  gameSurfaceFullscreen: {
    alignItems: "center",
    aspectRatio: "auto",
    blockSize: "100%",
    inlineSize: "100%",
    justifyContent: "center",
  },
  // The fallback for browsers that will not hand an element the screen, iOS
  // Safari above all. It has to reach the same result the browser would: over
  // everything, including the app shell's own bar, and out to the edges the
  // notch leaves usable.
  gameSurfaceImmersive: {
    position: "fixed",
    inset: 0,
    zIndex: 100,
    blockSize: "100dvh",
    inlineSize: "100dvw",
    paddingBlock: "env(safe-area-inset-top) env(safe-area-inset-bottom)",
    paddingInline: "env(safe-area-inset-left) env(safe-area-inset-right)",
  },
  gameCanvas: {
    display: "block",
    width: "100%",
    height: "100%",
    imageRendering: "pixelated",
    outline: "none",
  },
  // The keys sit under the status strip and share its ground, separated from
  // it by the same rule the strip is separated from the board by.
  stepControls: {
    borderTopWidth: borderVars["--border-width"],
    borderTopStyle: "solid",
    borderTopColor: colorVars["--color-border"],
    backgroundColor: colorVars["--color-background-surface"],
  },
  boardHud: {
    borderTopWidth: borderVars["--border-width"],
    borderTopStyle: "solid",
    borderTopColor: colorVars["--color-border-emphasized"],
    backgroundColor: colorVars["--color-background-surface"],
  },
  // The readout takes whatever the commands leave, and gives it back to the map
  // name, which is the only part of it that can be long.
  hudReadout: {
    flex: "1 1 auto",
    minInlineSize: 0,
  },
  // The heading keeps its rank in the document and gives up its display voice:
  // it reads as one more fact on the readout line, at the size of the text
  // beside it.
  hudMatchName: {
    fontSize: typeScaleVars["--text-body-size"],
    fontWeight: fontWeightVars["--font-weight-bold"],
    lineHeight: typeScaleVars["--text-body-leading"],
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  // A map name runs to about forty characters. It shortens rather than pushing
  // the End turn key onto a line of its own.
  hudMap: {
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  hudState: {
    flex: "none",
  },
  // The panel keeps the whole rail. Its section is a column, where `align-self`
  // is the width rather than the height, so asking it to sit at the start once
  // shrank the armies to the width of their own sprites. The board's height is
  // not imposed on it either way: the grid places both columns at the start.
  rosterPanel: {
    overflow: "hidden",
  },
  // The armies stay with the board on a tall screen rather than scrolling away
  // from the thing they describe.
  rosterSection: {
    position: {
      default: "static",
      [rosterLayout.desktopMedia]: "sticky",
    },
    top: spacingVars["--spacing-4"],
  },
});
