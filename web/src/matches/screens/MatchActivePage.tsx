import { useSuspenseQuery } from "@tanstack/react-query";
import { Badge } from "@astryxdesign/core/Badge";
import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { Card } from "@astryxdesign/core/Card";
import { Grid } from "@astryxdesign/core/Grid";
import { Heading } from "@astryxdesign/core/Heading";
import { Section } from "@astryxdesign/core/Section";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { StatusDot } from "@astryxdesign/core/StatusDot";
import { Text } from "@astryxdesign/core/Text";
import * as stylex from "@stylexjs/stylex";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useCanvasCourierSurface } from "#/canvas_courier/index.ts";
import { getCoPortraitByAwbwId, loadCoPortraitCatalog } from "#/components/co_portraits.ts";
import { useActiveMatchRunner } from "#/engine/runtime_context.tsx";
import type { GameRunner } from "#/engine/game_runner.ts";
import { useGameStore } from "#/engine/store.ts";
import type { LiveMatchPlayer } from "#/engine/worker_module.ts";
import { getFactionByCode, getFactionById } from "#/factions.ts";
import { rosterLayout } from "#/ui/rosterLayout.stylex.ts";
import { useMatchWebSocket, type MatchWebSocketStatus } from "#/matches/match_websocket.ts";
import type { InitialBoardMessage, MatchWebSocketMessage } from "#/matches/match_protocol.ts";
import { matchDetailQueryOptions } from "#/matches/matches.queries.ts";
import type { MatchParticipantSnapshot } from "#/matches/schemas.ts";
import { RosterList, RosterRow } from "#/replay/RosterRow.tsx";
import type { PlayerRosterEntry, PlayerRosterSnapshot } from "#/wasm/awbrn_wasm.js";

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
  const isEndingTurnRef = useRef(false);
  const finishEndingTurn = useCallback(() => {
    isEndingTurnRef.current = false;
    setIsEndingTurn(false);
  }, []);
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
          setMatchError(message.message);
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
    [finishEndingTurn, runner],
  );
  const { reconnect, sendMessage, status } = useMatchWebSocket(matchId, handleMatchMessage);

  useEffect(() => {
    if (status !== "connected") {
      finishEndingTurn();
    }
  }, [finishEndingTurn, status]);

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
    }
  }, [activePlayerSlot, finishEndingTurn, sendMessage, status, viewerSlotIndex]);

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
        {/* A match name is free text, so it may arrive as one unbroken run. */}
        <Heading level={1} type="display-2" xstyle={styles.matchName}>
          {match.name}
        </Heading>

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
            onEndTurn={handleEndTurn}
            players={livePlayers}
            reconnect={reconnect}
            runner={runner}
            status={status}
            activePlayerSlot={activePlayerSlot}
            isEndingTurn={isEndingTurn}
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
                      isViewer={viewerSlotIndex === army.entry.playerId}
                      key={army.entry.playerId}
                      name={army.name}
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
  onEndTurn,
  players,
  reconnect,
  runner,
  status,
  viewerSlotIndex,
}: {
  activePlayerSlot: number | null;
  day: number | null;
  initialBoard: InitialBoardMessage | null;
  isEndingTurn: boolean;
  match: { mapId: number; maxPlayers: number; settings: { fogEnabled: boolean } };
  onBoardError: (message: string | null) => void;
  onEndTurn: () => void;
  players: LiveMatchPlayer[];
  reconnect: () => void;
  runner: GameRunner;
  status: MatchWebSocketStatus;
  viewerSlotIndex: number | null | undefined;
}) {
  const { canvasRef, surfaceRef } = useCanvasCourierSurface({ controller: runner });
  const playersRef = useRef(players);
  playersRef.current = players;
  const onBoardErrorRef = useRef(onBoardError);
  onBoardErrorRef.current = onBoardError;

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

  const statusText = initialBoard
    ? `${initialBoard.map.Name} loaded from match state`
    : status === "connected"
      ? "Waiting for board state…"
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

  return (
    <Card padding={0} variant="muted" xstyle={styles.boardPanel}>
      {/* The engine draws 960x640; the frame keeps that ratio at every width so
          the map is never stretched or dropped into a tall slot on a phone. */}
      <VStack gap={0} ref={surfaceRef} xstyle={styles.gameSurface}>
        <canvas
          ref={canvasRef}
          width={960}
          height={640}
          tabIndex={0}
          {...stylex.props(styles.gameCanvas)}
        />
      </VStack>

      <HStack
        align="center"
        gap={3}
        justify="between"
        paddingBlock={2}
        paddingInline={3}
        wrap="wrap"
        xstyle={styles.boardHud}
      >
        <HStack align="center" gap={2} wrap="wrap">
          {/* Before the engine reports, there is no day to report. */}
          {day === null ? null : <Badge label={`Day ${day}`} variant="info" />}
          <Text type="supporting">
            Map {match.mapId} · {match.maxPlayers} players ·{" "}
            {match.settings.fogEnabled ? "Fog on" : "Fog off"}
          </Text>
        </HStack>
        {/* An <output> is a live region by default, so a drop is announced
            rather than only recolored. The dot repeats what the text beside it
            already says, so it is hidden from assistive technology. */}
        <HStack align="center" gap={3} wrap="wrap">
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
          <HStack align="center" as="output" gap={2}>
            <StatusDot
              aria-hidden="true"
              isPulsing={status === "connecting"}
              label={statusText}
              variant={statusVariant(status)}
            />
            <Text color={status === "connected" ? "primary" : "secondary"} type="supporting">
              {statusText}
            </Text>
            {status === "disconnected" || status === "error" ? (
              <Button clickAction={reconnect} label="Reconnect" size="sm" variant="secondary" />
            ) : null}
          </HStack>
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
    stats: {
      funds: undefined,
      income: undefined,
      unitCount: undefined,
      unitValue: undefined,
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
  matchName: {
    overflowWrap: "anywhere",
    // The title names the battle once; it must not take the viewport the board
    // needs. The signage face is still the largest thing on the page in weight,
    // not in height.
    fontSize: "clamp(var(--font-size-3xl), 4vw, var(--font-size-5xl))",
  },
  // One column until the rail and a board wide enough to read both fit; two
  // from there, with every extra pixel going to the board.
  matchLayout: {
    gridTemplateColumns: {
      default: "minmax(0, 1fr)",
      [rosterLayout.desktopMedia]: rosterLayout.railColumns,
    },
  },
  // A 3:2 board that grows without limit runs off a laptop screen, and a map
  // you have to scroll to see is not the focal point. The width is capped at
  // whatever keeps the whole board in the viewport, with a floor so a short
  // window shrinks the board rather than making it unreadable.
  boardPanel: {
    overflow: "hidden",
    justifySelf: "start",
    inlineSize: "100%",
    maxInlineSize: rosterLayout.boardMaxInlineSize,
  },
  gameSurface: {
    position: "relative",
    aspectRatio: "3 / 2",
    backgroundColor: "var(--color-background-inverted)",
  },
  gameCanvas: {
    display: "block",
    width: "100%",
    height: "100%",
    imageRendering: "pixelated",
    outline: "none",
  },
  boardHud: {
    borderTopWidth: "var(--border-width)",
    borderTopStyle: "solid",
    borderTopColor: "var(--color-border-emphasized)",
    backgroundColor: "var(--color-background-surface)",
  },
  rosterPanel: {
    overflow: "hidden",
    alignSelf: "start",
  },
  // The armies stay with the board on a tall screen rather than scrolling away
  // from the thing they describe.
  rosterSection: {
    position: {
      default: "static",
      [rosterLayout.desktopMedia]: "sticky",
    },
    top: "var(--spacing-4)",
  },
});
