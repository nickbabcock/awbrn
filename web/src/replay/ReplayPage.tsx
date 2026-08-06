import { Badge } from "@astryxdesign/core/Badge";
import { Banner } from "@astryxdesign/core/Banner";
import { Card } from "@astryxdesign/core/Card";
import { Center } from "@astryxdesign/core/Center";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { FileInput } from "@astryxdesign/core/FileInput";
import { Grid } from "@astryxdesign/core/Grid";
import { Heading } from "@astryxdesign/core/Heading";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { borderVars, colorVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useState } from "react";
import { resolveAwbwUsername } from "#/awbw/api.ts";
import { useCanvasCourierSurface } from "#/canvas_courier/index.ts";
import { loadCoPortraitCatalog, type CoPortraitCatalog } from "#/components/co_portraits.ts";
import { BoardFullscreenExit, GameFullscreenButton } from "#/components/GameFullscreen.tsx";
import { TileInfoBar } from "#/components/TileInfoBar.tsx";
import { useReplayRunner } from "#/engine/runtime_context.tsx";
import { useGameActions, useGameStore } from "#/engine/store.ts";
import { getFactionByCode } from "#/factions.ts";
import { rosterLayout } from "#/ui/rosterLayout.stylex.ts";
import { RosterList, RosterRow } from "./RosterRow";

export function ReplayPage() {
  const currentDay = useGameStore((state) => state.currentDay);
  const playerRoster = useGameStore((state) => state.playerRoster);
  const gameActions = useGameActions();
  const [portraitCatalog] = useState<CoPortraitCatalog>(() => loadCoPortraitCatalog());
  const [playerNames, setPlayerNames] = useState<Record<number, string>>({});
  const [replayFile, setReplayFile] = useState<File | null>(null);
  const [replayError, setReplayError] = useState<string | null>(null);
  const runner = useReplayRunner();
  const {
    canvasRef,
    enterFullscreen,
    exitFullscreen,
    focus,
    fullscreenMode,
    isFullscreen,
    surfaceRef,
  } = useCanvasCourierSurface({ controller: runner });

  async function handleReplayFileChange(files: File | File[] | null) {
    const file = Array.isArray(files) ? files[0] : files;
    if (!file) {
      setReplayFile(null);
      gameActions.setPlayerRoster(null);
      setReplayError(null);
      return;
    }

    gameActions.setPlayerRoster(null);
    setReplayError(null);
    try {
      if (!runner) throw new Error("Game runner was not initialized.");
      await runner.loadReplay(file);
      focus();
    } catch (error) {
      gameActions.setPlayerRoster(null);
      setReplayError(error instanceof Error ? error.message : "Replay failed to load.");
      console.error("Error loading replay:", error);
    }
  }

  useEffect(() => {
    let cancelled = false;

    if (!playerRoster) {
      setPlayerNames({});
      return () => {
        cancelled = true;
      };
    }

    const activeUserIds = new Set(playerRoster.players.map((player) => player.userId));
    setPlayerNames((previous) =>
      Object.fromEntries(
        Object.entries(previous).filter(([userId]) => activeUserIds.has(Number(userId))),
      ),
    );

    void Promise.all(
      playerRoster.players.map(async (player) => {
        const username = await resolveAwbwUsername(player.userId);
        if (!username || cancelled) return;

        setPlayerNames((previous) =>
          previous[player.userId] === username
            ? previous
            : { ...previous, [player.userId]: username },
        );
      }),
    );

    return () => {
      cancelled = true;
    };
  }, [playerRoster]);

  async function handlePlayerDisplayFactionChange(playerId: number, factionId: number | null) {
    setReplayError(null);
    try {
      if (!runner) throw new Error("Game runner was not initialized.");
      await runner.setPlayerDisplayFaction(playerId, factionId);
    } catch (error) {
      setReplayError(
        error instanceof Error ? error.message : "Faction depiction failed to update.",
      );
      console.error("Error updating replay faction depiction:", error);
    }
  }

  const replayLoader = (
    <FileInput
      accept=".zip"
      changeAction={handleReplayFileChange}
      description={playerRoster ? undefined : "AWBW replay archives, in .zip format"}
      isLabelHidden={Boolean(playerRoster)}
      label="Load a replay"
      mode={playerRoster ? "input" : "dropzone"}
      onChange={(files) => setReplayFile(Array.isArray(files) ? (files[0] ?? null) : files)}
      value={replayFile}
      width={playerRoster ? 260 : "100%"}
    />
  );

  return (
    <VStack gap={4} padding={4}>
      <Heading level={1}>Battle review</Heading>

      {replayError ? (
        <Banner description={replayError} status="error" title="Replay failed to load" />
      ) : null}

      {/* The board leads: it takes the width the viewport has and the armies
          keep a fixed rail beside it, so the map is the largest thing on the
          page rather than one half of a split. */}
      <Grid align="start" gap={4} xstyle={styles.reviewLayout}>
        {/* The board is the artifact; everything else frames it. */}
        <Card padding={0} variant="muted" xstyle={styles.boardPanel}>
          <VStack
            gap={0}
            ref={surfaceRef}
            xstyle={[
              styles.gameSurface,
              !playerRoster && styles.gameSurfaceEmpty,
              isFullscreen && styles.gameSurfaceFullscreen,
              fullscreenMode === "immersive" && styles.gameSurfaceImmersive,
            ]}
          >
            <canvas
              ref={canvasRef}
              width={960}
              height={640}
              tabIndex={0}
              {...stylex.props(styles.gameCanvas, !playerRoster && styles.gameCanvasHidden)}
            />
            {isFullscreen ? (
              <BoardFullscreenExit mode={fullscreenMode} onExit={exitFullscreen} />
            ) : null}
            {!playerRoster ? (
              <Center height="100%" width="100%" xstyle={styles.loaderOverlay}>
                <VStack gap={3} maxWidth={420} width="100%">
                  {replayLoader}
                </VStack>
              </Center>
            ) : null}

            {/* The terrain window stands on the battlefield itself, so reading
                a tile costs the review no height. */}
            {playerRoster ? <TileInfoBar /> : null}
          </VStack>

          {/* The game's own status line, kept on the board it describes. */}
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
              <Badge label={`Day ${currentDay}`} variant="info" />
              {playerRoster ? (
                <Text type="supporting">
                  Game {playerRoster.matchId} · Map {playerRoster.mapId}
                </Text>
              ) : (
                <Text type="supporting">No replay loaded</Text>
              )}
            </HStack>
            {/* Full screen is only an offer once there is a battle to watch,
                and while the board holds the screen the way back out is the key
                on the board itself. */}
            {playerRoster ? (
              <HStack align="center" gap={2} wrap="wrap">
                {isFullscreen ? null : <GameFullscreenButton onEnter={enterFullscreen} />}
                {replayLoader}
              </HStack>
            ) : null}
          </HStack>
        </Card>

        {/* The armies need no visible title: the board names the battle, and
            each row names its own army. */}
        <VStack as="section" aria-label="Armies" gap={0} xstyle={styles.rosterSection}>
          <Card padding={0} xstyle={styles.rosterPanel}>
            {playerRoster ? (
              <RosterList>
                {playerRoster.players.map((player) => (
                  <RosterRow
                    isActive={playerRoster.activePlayerId === player.playerId}
                    key={player.playerId}
                    name={playerNames[player.userId] ?? `Player ${player.turnOrder}`}
                    onFactionChange={(factionId) =>
                      handlePlayerDisplayFactionChange(
                        player.playerId,
                        factionId === getFactionByCode(player.actualFactionCode)?.id
                          ? null
                          : factionId,
                      )
                    }
                    player={player}
                    portraitCatalog={portraitCatalog}
                  />
                ))}
              </RosterList>
            ) : (
              <VStack gap={0} padding={3}>
                <EmptyState
                  description="Once a replay is loaded, every army lists its CO, funds, and unit strength here."
                  headingLevel={3}
                  isCompact
                  title="No armies yet"
                />
              </VStack>
            )}
          </Card>
        </VStack>
      </Grid>
    </VStack>
  );
}

const styles = stylex.create({
  // One column until the rail and a board wide enough to read both fit; two
  // from there. The board takes the width its height allows and the rail stays
  // against it, so a wide window puts the spare space around the pair instead
  // of between the map and the armies that describe it.
  reviewLayout: {
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
  boardPanel: {
    overflow: "hidden",
    justifySelf: "start",
    inlineSize: "100%",
    maxInlineSize: rosterLayout.boardMaxInlineSize,
  },
  gameSurface: {
    position: "relative",
    // The engine draws 960x640; the frame keeps that ratio at every width so
    // the map is never stretched on a phone.
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
  // With no replay the frame only has to say where the board goes, so it stops
  // reserving a board's worth of empty ground.
  gameSurfaceEmpty: {
    aspectRatio: "auto",
    height: 360,
  },
  gameCanvas: {
    display: "block",
    width: "100%",
    height: "100%",
    outline: "none",
    imageRendering: "pixelated",
  },
  gameCanvasHidden: {
    visibility: "hidden",
  },
  loaderOverlay: {
    position: "absolute",
    inset: 0,
    padding: spacingVars["--spacing-4"],
    backgroundColor: colorVars["--color-background-muted"],
  },
  boardHud: {
    borderTopWidth: borderVars["--border-width"],
    borderTopStyle: "solid",
    borderTopColor: colorVars["--color-border-emphasized"],
    backgroundColor: colorVars["--color-background-surface"],
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
