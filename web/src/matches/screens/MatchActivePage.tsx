import { useSuspenseQuery } from "@tanstack/react-query";
import { Badge } from "@astryxdesign/core/Badge";
import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "#/ui/Button.tsx";
import { Card } from "@astryxdesign/core/Card";
import { Grid } from "@astryxdesign/core/Grid";
import { Heading } from "@astryxdesign/core/Heading";
import { Section } from "@astryxdesign/core/Section";
import { Link } from "@astryxdesign/core/Link";
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
import * as stylex from "@stylexjs/stylex";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useCanvasCourierSurface } from "#/canvas_courier/index.ts";
import { getCoPortraitByAwbwId, loadCoPortraitCatalog } from "#/components/co_portraits.ts";
import { BoardFullscreenExit, GameFullscreenButton } from "#/components/GameFullscreen.tsx";
import { TileInfoBar } from "#/components/TileInfoBar.tsx";
import { useActiveMatchRunner } from "#/engine/runtime_context.tsx";
import type { GameRunner } from "#/engine/game_runner.ts";
import { useGameActions, useGameStore } from "#/engine/store.ts";
import type { LiveMatchPlayer } from "#/engine/worker_module.ts";
import { getFactionByCode, getFactionById } from "#/factions.ts";
import { rosterLayout } from "#/ui/rosterLayout.stylex.ts";
import { useMatchWebSocket, type MatchWebSocketStatus } from "#/matches/match_websocket.ts";
import type { InitialBoardMessage, MatchWebSocketMessage } from "#/matches/match_protocol.ts";
import { matchDetailQueryOptions } from "#/matches/matches.queries.ts";
import type { MatchParticipantSnapshot } from "#/matches/schemas.ts";
import { BuildMenu } from "#/matches/components/BuildMenu.tsx";
import { UnitActionMenu } from "#/matches/components/UnitActionMenu.tsx";
import { RosterList, RosterRow } from "#/replay/RosterRow.tsx";
import type { ActivatablePowerLevel } from "#/replay/power_meter.ts";
import type {
  PlayerRosterEntry,
  PlayerRosterSnapshot,
  ProductionOptionsChanged,
  UnitActionsChanged,
  UnitKind,
} from "#/wasm/awbrn_wasm.js";

/**
 * A seat, and whatever the engine currently knows about it.
 *
 * The seats are known from the match record before the board is running, so the
 * armies list is built from the record and the engine's statistics are merged in
 * as they arrive. The list therefore never starts empty and never reflows.
 */
interface MatchArmy {
  entry: PlayerRosterEntry;
  hasLiveStats: boolean;
  isActive: boolean;
  name: string;
}

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
const AWBW_MAP_URL = "https://awbw.amarriner.com/prevmaps.php?maps_id=";

/**
 * How long a press can still be the one that opened a menu. A selection made
 * from the keyboard has no press behind it, and a menu placed at a pointer that
 * has not moved since the last turn would be placed nowhere in particular.
 */
const BOARD_PRESS_MAX_AGE_MS = 2000;

export function MatchActivePage({
  joinSlug = null,
  matchId,
}: {
  joinSlug?: string | null;
  matchId: string;
}) {
  const { data: match } = useSuspenseQuery(matchDetailQueryOptions(matchId, joinSlug));
  const portraitCatalog = useMemo(() => loadCoPortraitCatalog(), []);
  const runner = useActiveMatchRunner();
  const playerRoster = useGameStore((state) => state.playerRoster);
  const productionOptions = useGameStore((state) => state.productionOptions);
  const unitActions = useGameStore((state) => state.unitActions);
  const setProductionOptions = useGameStore((state) => state.actions.setProductionOptions);
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
  const [initialBoard, setInitialBoard] = useState<InitialBoardMessage | null>(null);
  const [matchError, setMatchError] = useState<string | null>(null);
  const [boardError, setBoardError] = useState<string | null>(null);
  // `undefined` until the server says which seat, if any, this viewer holds.
  const [viewerSlotIndex, setViewerSlotIndex] = useState<number | null | undefined>(undefined);
  const [spectatorFogActive, setSpectatorFogActive] = useState<boolean | null>(null);
  const [activePlayerSlot, setActivePlayerSlot] = useState<number | null>(null);
  const [isEndingTurn, setIsEndingTurn] = useState(false);
  const [activatingPower, setActivatingPower] = useState<ActivatablePowerLevel>();
  const isEndingTurnRef = useRef(false);
  const isActivatingPowerRef = useRef(false);
  const finishEndingTurn = useCallback(() => {
    isEndingTurnRef.current = false;
    setIsEndingTurn(false);
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
          finishActivatingPower();
          setMatchError(message.message);
          restoreAfterRefusal();
          return;
        }
        case "spectatorNotice": {
          setSpectatorFogActive(message.fogActive);
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
    [finishActivatingPower, finishEndingTurn, restoreAfterRefusal, runner],
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

  useEffect(() => {
    if (status !== "connected") {
      finishEndingTurn();
      finishActivatingPower();
    }
  }, [finishActivatingPower, finishEndingTurn, status]);

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

  const handleEndTurn = useCallback(() => {
    if (
      isEndingTurnRef.current ||
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
    viewerSlotIndex,
  ]);

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

  return (
    <Section padding={6} variant="transparent">
      <VStack gap={6}>
        {matchError ? <Banner description={matchError} status="error" title="Match error" /> : null}
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
            match={match}
            onBoardError={setBoardError}
            onBuildUnit={handleBuildUnit}
            onEndTurn={handleEndTurn}
            players={livePlayers}
            productionOptions={productionOptions}
            unitActions={unitActions}
            reconnect={reconnect}
            runner={runner}
            status={status}
            activePlayerSlot={activePlayerSlot}
            isEndingTurn={isEndingTurn}
            viewerFactionCode={viewerArmy?.entry.displayFactionCode ?? null}
            viewerFunds={viewerArmy?.entry.stats.funds ?? null}
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
    </Section>
  );
}

/**
 * The board, with the game's own readouts on the strip beneath it.
 *
 * Day, map, and connection belong to the board rather than to the page, so they
 * stay in the same viewport as the thing they describe at every width.
 */
function ActiveMatchBoard({
  activePlayerSlot,
  day,
  initialBoard,
  isEndingTurn,
  match,
  onBoardError,
  onBuildUnit,
  onEndTurn,
  players,
  productionOptions,
  unitActions,
  reconnect,
  runner,
  status,
  viewerFactionCode,
  viewerFunds,
  viewerSlotIndex,
}: {
  activePlayerSlot: number | null;
  day: number | null;
  initialBoard: InitialBoardMessage | null;
  isEndingTurn: boolean;
  match: { mapId: number; maxPlayers: number; name: string; settings: { fogEnabled: boolean } };
  onBoardError: (message: string | null) => void;
  onBuildUnit: (unit: UnitKind, x: number, y: number) => void;
  onEndTurn: () => void;
  players: LiveMatchPlayer[];
  productionOptions: ProductionOptionsChanged | null;
  unitActions: UnitActionsChanged | null;
  reconnect: () => void;
  runner: GameRunner;
  status: MatchWebSocketStatus;
  viewerFactionCode: string | null;
  viewerFunds: number | null;
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
  const productionSite = productionOptions?.site;

  // The last press on the board, so the menu can open where the player pointed
  // and in the shape the input they used expects. A press is only kept for as
  // long as it can still plausibly be the one that opened a menu.
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
  const mapName = initialBoard?.map.Name ?? null;
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
        : undefined;
  const buildBlockedReason =
    status !== "connected" ? "Reconnect to the match to send this order." : undefined;

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
            anchor={press}
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
            anchor={press}
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
          <Link
            href={`${AWBW_MAP_URL}${match.mapId}`}
            isExternalLink
            type="supporting"
            xstyle={styles.hudMap}
          >
            {mapName ?? `Map ${match.mapId}`}
          </Link>
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
        {/* Every control the strip offers, and nothing that only reports. */}
        <HStack align="center" gap={2} wrap="wrap">
          {/* While the board holds the screen this strip is behind it, and the
              way back out is the key on the board itself. */}
          {isFullscreen ? null : <GameFullscreenButton onEnter={enterFullscreen} />}
          {status === "disconnected" || status === "error" ? (
            <Button clickAction={reconnect} label="Reconnect" size="sm" variant="secondary" />
          ) : null}
          {isPlayer ? (
            <Button
              clickAction={onEndTurn}
              isDisabled={status !== "connected" || !isViewerTurn}
              isLoading={isEndingTurn}
              label="End turn"
              size="sm"
              tooltip={endTurnTooltip}
              variant="primary"
            />
          ) : null}
        </HStack>
      </HStack>
    </Card>
  );
}

function buildArmies(
  participants: MatchParticipantSnapshot[],
  playerRoster: PlayerRosterSnapshot | null,
): MatchArmy[] {
  const liveEntries = new Map(
    (playerRoster?.players ?? []).map((player) => [player.playerId, player]),
  );

  return participants.map((participant, index) => {
    const liveEntry = liveEntries.get(participant.slotIndex);

    return {
      entry: liveEntry ?? seatEntry(participant, index),
      hasLiveStats: liveEntry !== undefined,
      isActive: playerRoster?.activePlayerId === participant.slotIndex,
      name: participant.userName,
    };
  });
}

/**
 * A seat as the match record describes it, with every statistic still unknown.
 * The readouts render their own "--" until the engine reports real values.
 */
function seatEntry(participant: MatchParticipantSnapshot, index: number): PlayerRosterEntry {
  const faction = getFactionById(participant.factionId);
  const factionCode = faction?.code ?? "os";
  const factionName = faction?.displayName ?? "Orange Star";
  const portrait = getCoPortraitByAwbwId(participant.coId);

  return {
    playerId: participant.slotIndex,
    userId: 0,
    turnOrder: index,
    team: undefined,
    eliminated: false,
    actualFactionCode: factionCode,
    actualFactionName: factionName,
    displayFactionCode: factionCode,
    displayFactionName: factionName,
    factionCode,
    factionName,
    coKey: portrait?.key,
    coName: portrait?.displayName,
    tagCoKey: undefined,
    tagCoName: undefined,
    powerCharge: undefined,
    copCost: undefined,
    scopCost: undefined,
    powerStarCharge: undefined,
    activePower: undefined,
    stats: {
      funds: undefined,
      income: undefined,
      unitCount: undefined,
      unitValue: undefined,
      properties: undefined,
      comTowers: undefined,
    },
  };
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
