import { useSuspenseQuery } from "@tanstack/react-query";
import * as stylex from "@stylexjs/stylex";
import { useCallback, useEffect, useMemo, useState } from "react";
import { CoPortrait } from "#/components/CoPortrait.tsx";
import {
  DEFAULT_CO_PORTRAIT_KEY,
  getCoPortraitByAwbwId,
  loadCoPortraitCatalog,
} from "#/components/co_portraits.ts";
import { getFactionById } from "#/factions.ts";
import { getFactionVisual } from "#/faction_visuals.ts";
import { PlayerHeader } from "#/components/PlayerHeader.tsx";
import { Frame, Heading, Kicker, Page, Section, Text } from "#/ui/primitives.tsx";
import { tokens } from "#/ui/theme.stylex.ts";
import { matchDetailQueryOptions } from "#/matches/matches.queries.ts";
import { useMatchWebSocket } from "#/matches/match_websocket.ts";
import type { InitialBoardMessage, MatchWebSocketMessage } from "#/matches/match_protocol.ts";
import { useCanvasCourierSurface } from "#/canvas_courier/index.ts";
import { useActiveMatchRunner } from "#/engine/runtime_context.tsx";
import type { GameRunner } from "#/engine/game_runner.ts";

export function MatchActivePage({ matchId }: { matchId: string }) {
  const { data: match } = useSuspenseQuery(matchDetailQueryOptions(matchId, null));
  const portraitCatalog = useMemo(() => loadCoPortraitCatalog(), []);
  const runner = useActiveMatchRunner();
  const [initialBoard, setInitialBoard] = useState<InitialBoardMessage | null>(null);
  const handleMatchMessage = useCallback((msg: MatchWebSocketMessage) => {
    if (msg.type === "initialBoard") {
      setInitialBoard(msg);
    }
  }, []);
  const { status } = useMatchWebSocket(matchId, handleMatchMessage);

  return (
    <Page width="wide">
      <Section>
        <div {...stylex.props(styles.layout)}>
          <header {...stylex.props(styles.header)}>
            <div {...stylex.props(styles.headerCopy)}>
              <Kicker xstyle={styles.headerKicker}>Match Active</Kicker>
              <Heading size="display">{match.name}</Heading>
              <Text size="lg" tone="strong">
                Map {match.mapId} · {match.maxPlayers} players ·{" "}
                {match.settings.fogEnabled ? "Fog on" : "Fog off"}
              </Text>
            </div>
          </header>

          <div {...stylex.props(styles.mainGrid)}>
            <Frame as="section" surface="panel" padding="none" xstyle={styles.gameSection}>
              <ActiveMatchBoard runner={runner} initialBoard={initialBoard} status={status} />
            </Frame>

            <Frame as="section" surface="panel" padding="none" xstyle={styles.rosterSection}>
              <div {...stylex.props(styles.rosterInner)}>
                <div {...stylex.props(styles.sectionHeader)}>
                  <Kicker>Roster</Kicker>
                  <Heading size="lg">Players</Heading>
                </div>

                <div {...stylex.props(styles.participantList)}>
                  {match.participants.map((participant) => {
                    const faction = getFactionById(participant.factionId);
                    const factionVisual = getFactionVisual(faction?.code ?? "os");
                    const portrait = getCoPortraitByAwbwId(participant.coId);

                    return (
                      <div
                        key={participant.slotIndex}
                        {...stylex.props(styles.participantCard(factionVisual.accent))}
                      >
                        <PlayerHeader
                          factionCode={faction?.code ?? "os"}
                          name={participant.userName}
                        />
                        <div {...stylex.props(styles.participantBody)}>
                          <CoPortrait
                            catalog={portraitCatalog}
                            coKey={portrait?.key ?? DEFAULT_CO_PORTRAIT_KEY}
                            fallbackLabel={portrait?.displayName ?? "No CO"}
                          />
                          <Text size="sm" tone="muted">
                            {portrait?.displayName ?? "No CO"}
                          </Text>
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            </Frame>
          </div>
        </div>
      </Section>
    </Page>
  );
}

function ActiveMatchBoard({
  runner,
  initialBoard,
  status,
}: {
  runner: GameRunner;
  initialBoard: InitialBoardMessage | null;
  status: string;
}) {
  const { canvasRef, surfaceRef } = useCanvasCourierSurface({
    controller: runner,
  });

  useEffect(() => {
    if (!initialBoard) {
      return;
    }

    let cancelled = false;

    void Promise.resolve()
      .then(async () => {
        if (!cancelled) {
          await runner.loadMatchMap(initialBoard.map);
        }
      })
      .catch((error) => {
        console.error("Error loading match map:", error);
      });

    return () => {
      cancelled = true;
    };
  }, [initialBoard, runner]);

  return (
    <div {...stylex.props(styles.gameBoardShell)}>
      <div ref={surfaceRef} {...stylex.props(styles.gameSurface)}>
        <canvas
          ref={canvasRef}
          width={960}
          height={640}
          tabIndex={0}
          {...stylex.props(styles.gameCanvas)}
        />
      </div>
      <div {...stylex.props(styles.boardStatus)}>
        <div {...stylex.props(styles.statusDot(status))} />
        <Text size="sm" tone={status === "connected" ? "strong" : "muted"}>
          {initialBoard
            ? `${initialBoard.map.Name} loaded from match state`
            : status === "connected"
              ? "Waiting for board state..."
              : status === "connecting"
                ? "Connecting to match..."
                : status === "error"
                  ? "Connection error — retrying."
                  : "Disconnected — reconnecting."}
        </Text>
      </div>
    </div>
  );
}

const STATUS_COLORS: Record<string, string> = {
  connected: "#4ade80",
  connecting: "#facc15",
  disconnected: "#94a3b8",
  error: "#f87171",
};

const styles = stylex.create({
  layout: {
    display: "grid",
    gap: tokens.space6,
  },
  header: {
    paddingBottom: tokens.space5,
    borderBottomWidth: 3,
    borderBottomStyle: "solid",
    borderBottomColor: tokens.chromeBorderSoft,
  },
  headerCopy: {
    display: "grid",
    gap: tokens.space2,
  },
  headerKicker: {
    color: tokens.brandHover,
  },
  mainGrid: {
    display: "grid",
    gap: tokens.space8,
    gridTemplateColumns: {
      default: "minmax(0, 1fr) minmax(360px, 460px)",
      "@media (max-width: 980px)": "1fr",
    },
    alignItems: "start",
  },
  gameSection: {
    overflow: "hidden",
    minHeight: 520,
  },
  gameBoardShell: {
    position: "relative",
    display: "grid",
    minHeight: 520,
    backgroundColor: "#0b1020",
  },
  gameSurface: {
    width: "100%",
    height: 520,
    overflow: "hidden",
  },
  gameCanvas: {
    display: "block",
    width: "100%",
    height: "100%",
    imageRendering: "pixelated",
    outline: "none",
  },
  boardStatus: {
    position: "absolute",
    left: tokens.space4,
    bottom: tokens.space4,
    display: "flex",
    alignItems: "center",
    gap: tokens.space2,
    maxWidth: "calc(100% - 32px)",
    paddingTop: tokens.space2,
    paddingRight: tokens.space3,
    paddingBottom: tokens.space2,
    paddingLeft: tokens.space3,
    borderWidth: 2,
    borderStyle: "solid",
    borderColor: tokens.strokeHeavy,
    borderRadius: tokens.radius2,
    backgroundColor: "rgba(11, 16, 32, 0.88)",
  },
  statusDot: (status: string) => ({
    width: 10,
    height: 10,
    borderRadius: "50%",
    backgroundColor: STATUS_COLORS[status] ?? STATUS_COLORS.disconnected,
  }),
  rosterSection: {
    overflow: "visible",
  },
  rosterInner: {
    display: "grid",
    gap: tokens.space4,
    alignContent: "start",
    padding: tokens.space6,
  },
  sectionHeader: {
    display: "grid",
    gap: tokens.space1,
  },
  participantList: {
    display: "grid",
    gap: tokens.space3,
  },
  participantCard: (accent: string) => ({
    display: "grid",
    borderWidth: 2,
    borderStyle: "solid",
    borderColor: accent,
    borderRadius: tokens.radius2,
    overflow: "hidden",
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardSm}`,
  }),
  participantBody: {
    display: "flex",
    alignItems: "center",
    gap: tokens.space3,
    padding: tokens.space3,
  },
});
