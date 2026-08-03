import { useSuspenseQuery } from "@tanstack/react-query";
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
import { CoPortrait } from "#/components/CoPortrait.tsx";
import {
  DEFAULT_CO_PORTRAIT_KEY,
  getCoPortraitByAwbwId,
  loadCoPortraitCatalog,
} from "#/components/co_portraits.ts";
import { PlayerHeader } from "#/components/PlayerHeader.tsx";
import { useActiveMatchRunner } from "#/engine/runtime_context.tsx";
import type { GameRunner } from "#/engine/game_runner.ts";
import type { LiveMatchPlayer } from "#/engine/worker_module.ts";
import { getFactionById } from "#/factions.ts";
import { useMatchWebSocket } from "#/matches/match_websocket.ts";
import type { InitialBoardMessage, MatchWebSocketMessage } from "#/matches/match_protocol.ts";
import { matchDetailQueryOptions } from "#/matches/matches.queries.ts";

export function MatchActivePage({ matchId }: { matchId: string }) {
  const { data: match } = useSuspenseQuery(matchDetailQueryOptions(matchId, null));
  const portraitCatalog = useMemo(() => loadCoPortraitCatalog(), []);
  const runner = useActiveMatchRunner();
  const livePlayers = useMemo<LiveMatchPlayer[]>(
    () =>
      match.participants.map((participant) => ({
        playerId: participant.slotIndex,
        factionId: participant.factionId,
      })),
    [match.participants],
  );
  const [initialBoard, setInitialBoard] = useState<InitialBoardMessage | null>(null);
  const handleMatchMessage = useCallback(
    (message: MatchWebSocketMessage) => {
      if (message.type === "initialBoard") {
        setInitialBoard(message);
        return;
      }
      if (message.type === "playerUpdate") {
        void runner.applyLiveTransition(message.transition).catch((error) => {
          console.error("Error applying live player transition:", error);
        });
        return;
      }
      if (message.type === "spectatorState" && message.transition) {
        void runner.applyLiveTransition(message.transition).catch((error) => {
          console.error("Error applying live spectator transition:", error);
        });
      }
    },
    [runner],
  );
  const { status } = useMatchWebSocket(matchId, handleMatchMessage);

  return (
    <Section padding={6} variant="transparent">
      <VStack gap={6}>
        <VStack gap={2}>
          <Heading level={1} type="display-2">
            {match.name}
          </Heading>
          <Text color="secondary" type="large">
            Map {match.mapId} · {match.maxPlayers} players ·{" "}
            {match.settings.fogEnabled ? "Fog on" : "Fog off"}
          </Text>
        </VStack>

        <Grid align="start" columns={{ minWidth: 300, max: 2, repeat: "fit" }} gap={6}>
          <ActiveMatchBoard
            initialBoard={initialBoard}
            players={livePlayers}
            runner={runner}
            status={status}
          />

          <Section padding={5} variant="section">
            <VStack gap={4}>
              <Heading level={2}>Players</Heading>
              <VStack gap={3}>
                {match.participants.map((participant) => {
                  const faction = getFactionById(participant.factionId);
                  const portrait = getCoPortraitByAwbwId(participant.coId);

                  return (
                    <Section key={participant.slotIndex} padding={0} variant="muted">
                      <VStack gap={2}>
                        <PlayerHeader
                          factionCode={faction?.code ?? "os"}
                          name={participant.userName}
                        />
                        <Section padding={3} variant="transparent">
                          <HStack align="center" gap={3}>
                            <CoPortrait
                              catalog={portraitCatalog}
                              coKey={portrait?.key ?? DEFAULT_CO_PORTRAIT_KEY}
                              fallbackLabel={portrait?.displayName ?? "No CO"}
                            />
                            <Text color="secondary" type="supporting">
                              {portrait?.displayName ?? "No CO"}
                            </Text>
                          </HStack>
                        </Section>
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

function ActiveMatchBoard({
  runner,
  initialBoard,
  players,
  status,
}: {
  runner: GameRunner;
  initialBoard: InitialBoardMessage | null;
  players: LiveMatchPlayer[];
  status: string;
}) {
  const { canvasRef, surfaceRef } = useCanvasCourierSurface({ controller: runner });
  const playersRef = useRef(players);
  playersRef.current = players;

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
      .catch((error) => console.error("Error loading match map:", error));

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

  return (
    <Card padding={0} width="100%">
      <VStack gap={0}>
        <Section height={520} padding={0} ref={surfaceRef} variant="muted">
          <canvas
            ref={canvasRef}
            width={960}
            height={640}
            tabIndex={0}
            {...stylex.props(styles.canvas)}
          />
        </Section>
        <Section dividers={["top"]} padding={3} variant="section">
          <HStack align="center" gap={2}>
            <StatusDot
              isPulsing={status === "connecting"}
              label={statusText}
              variant={statusVariant(status)}
            />
            <Text color={status === "connected" ? "primary" : "secondary"} type="supporting">
              {statusText}
            </Text>
          </HStack>
        </Section>
      </VStack>
    </Card>
  );
}

function statusVariant(status: string): "success" | "warning" | "error" | "neutral" {
  if (status === "connected") return "success";
  if (status === "connecting") return "warning";
  if (status === "error") return "error";
  return "neutral";
}

const styles = stylex.create({
  canvas: {
    display: "block",
    width: "100%",
    height: "100%",
    imageRendering: "pixelated",
    outline: "none",
  },
});
