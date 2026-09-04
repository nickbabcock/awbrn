import { Banner } from "@astryxdesign/core/Banner";
import { Card } from "@astryxdesign/core/Card";
import { Center } from "@astryxdesign/core/Center";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Grid } from "@astryxdesign/core/Grid";
import { Heading } from "@astryxdesign/core/Heading";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Spinner } from "@astryxdesign/core/Spinner";
import { Text } from "@astryxdesign/core/Text";
import { borderVars, colorVars } from "@astryxdesign/core/theme/tokens.stylex";
import { useMediaQuery } from "@astryxdesign/core/hooks";
import { useSuspenseQuery } from "@tanstack/react-query";
import { Calculator as CalculatorIcon } from "pixelarticons/react/Calculator";
import { Download as DownloadIcon } from "pixelarticons/react/Download";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useMemo, useState } from "react";
import { useCanvasCourierSurface } from "#/canvas_courier/index.ts";
import { loadCoPortraitCatalog } from "#/components/co_portraits.ts";
import { BoardFullscreenExit, GameFullscreenButton } from "#/components/GameFullscreen.tsx";
import { StepControls } from "#/components/StepControls.tsx";
import { TileInfoBar } from "#/components/TileInfoBar.tsx";
import type { GameRunner } from "#/engine/game_runner.ts";
import { useActiveMatchRunner } from "#/engine/runtime_context.tsx";
import { useGameStore } from "#/engine/store.ts";
import { getFactionByCode } from "#/factions.ts";
import {
  BATTLE_CALCULATOR_SHEET_MEDIA,
  BattleCalculator,
} from "#/matches/components/BattleCalculator.tsx";
import { ViewpointSelector } from "#/matches/components/ViewpointSelector.tsx";
import { buildArmies } from "#/matches/match_armies.ts";
import { matchDetailQueryOptions } from "#/matches/matches.queries.ts";
import { fetchMatchReplayBytes, matchReplayDownloadPath } from "#/matches/replay_archive.ts";
import { RosterList, RosterRow } from "#/replay/RosterRow.tsx";
import { Button } from "#/ui/Button.tsx";
import { rosterLayout } from "#/ui/rosterLayout.stylex.ts";

/**
 * A finished match, read back.
 *
 * The match keeps its own address once it ends: a link handed out while it was
 * being played still opens the same board afterwards, and nothing has to be
 * downloaded and handed back to a second page to watch it.
 *
 * What is watched is the stored archive rather than the host. The host answers
 * a live match one seat at a time, because a board it handed to the wrong seat
 * would be a board that seat is not entitled to; a match that is over has no
 * such seat, and the archive carries every action, so the engine in this page
 * can project any of them. That is what makes the viewpoint control possible,
 * and it is the reason a fogged match is worth opening again.
 */
export function MatchReplayPage({
  joinSlug = null,
  matchId,
}: {
  joinSlug?: string | null;
  matchId: string;
}) {
  const { data: match } = useSuspenseQuery(matchDetailQueryOptions(matchId, joinSlug));
  const runner = useActiveMatchRunner();
  const playerRoster = useGameStore((state) => state.playerRoster);
  const replayPosition = useGameStore((state) => state.replayPosition);
  const replayViewpoint = useGameStore((state) => state.replayViewpoint);
  const portraitCatalog = useMemo(() => loadCoPortraitCatalog(), []);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isCalculatorOpen, setIsCalculatorOpen] = useState(false);
  const isCalculatorCompact = useMediaQuery(BATTLE_CALCULATOR_SHEET_MEDIA);
  const armies = useMemo(
    () => buildArmies(match.participants, playerRoster),
    [match.participants, playerRoster],
  );
  const {
    canvasRef,
    enterFullscreen,
    exitFullscreen,
    focus,
    fullscreenMode,
    isFullscreen,
    surfaceRef,
  } = useCanvasCourierSurface({ controller: runner });

  // The archive is immutable and served with a year's cache, so it is fetched
  // once for the life of the page and never revalidated.
  useEffect(() => {
    let cancelled = false;

    setIsLoading(true);
    setLoadError(null);
    void fetchMatchReplayBytes(matchId)
      .then((bytes) => (cancelled ? undefined : runner.loadReplay(bytes, () => !cancelled)))
      .then(() => {
        if (cancelled) return;
        setIsLoading(false);
        focus();
      })
      .catch((error: unknown) => {
        if (cancelled) return;
        setIsLoading(false);
        setLoadError(error instanceof Error ? error.message : "the replay could not be read");
        console.error("Failed to open a finished match for review:", error);
      });

    return () => {
      cancelled = true;
    };
  }, [focus, matchId, runner]);

  /**
   * Walk the archive.
   *
   * Every step goes to the engine, which holds the archive and is the only
   * thing that can say what the board looked like at a boundary.
   */
  function stepReplay(step: (runner: GameRunner) => Promise<void>) {
    void step(runner).catch((error: unknown) => {
      setLoadError(error instanceof Error ? error.message : "the replay could not be stepped");
      console.error("Error stepping a finished match:", error);
    });
  }

  function changeViewpoint(playerId: number | null, followsActivePlayer: boolean) {
    void runner.setReplayViewpoint(playerId, followsActivePlayer).catch((error: unknown) => {
      console.error("Error changing the replay viewpoint:", error);
    });
  }

  const turnHolder =
    replayPosition === null || replayPosition.activePlayerId === null
      ? null
      : (armies.find((army) => army.entry.playerId === replayPosition.activePlayerId)?.name ??
        null);

  return (
    <VStack gap={4} padding={4}>
      <VStack gap={1}>
        <Heading level={1}>{match.name}</Heading>
        <Text color="secondary">This match is over. Every seat can be watched through.</Text>
      </VStack>

      {loadError ? (
        <Banner description={loadError} status="error" title="The replay could not be opened" />
      ) : null}

      <Grid align="start" gap={4} xstyle={styles.reviewLayout}>
        <Card padding={0} variant="muted" xstyle={styles.boardPanel}>
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
              {...stylex.props(styles.gameCanvas, !playerRoster && styles.gameCanvasHidden)}
            />
            {isFullscreen ? (
              <BoardFullscreenExit mode={fullscreenMode} onExit={exitFullscreen} />
            ) : null}

            {/* One overlay for the two ways there is no board: the archive is
                still being read, or there is none to read. */}
            {playerRoster ? null : (
              <Center height="100%" width="100%">
                {isLoading ? (
                  <Spinner label="Opening the replay" />
                ) : (
                  <VStack gap={0} maxWidth={420} padding={3}>
                    <EmptyState
                      description={
                        loadError ??
                        "No archive was stored for this match, so there is nothing to watch."
                      }
                      headingLevel={2}
                      isCompact
                      title="No replay to watch"
                    />
                  </VStack>
                )}
              </Center>
            )}

            {isCalculatorOpen ? (
              <BattleCalculator
                onDismiss={() => setIsCalculatorOpen(false)}
                onRestoreFocus={focus}
                presentation={isCalculatorCompact ? "sheet" : "board"}
                roster={playerRoster}
                runner={runner}
              />
            ) : null}

            {playerRoster ? <TileInfoBar /> : null}
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
            {/* Whose eyes the board is drawn through is the control this page
                exists for, so it leads the status line rather than sitting
                among the commands on the other end. */}
            {playerRoster ? (
              <ViewpointSelector
                armies={armies}
                onChange={changeViewpoint}
                viewpoint={replayViewpoint}
              />
            ) : (
              <Text type="supporting">Map {match.mapId}</Text>
            )}

            <HStack align="center" gap={2} wrap="wrap">
              <Button
                clickAction={() => setIsCalculatorOpen(true)}
                icon={<CalculatorIcon aria-hidden height={16} width={16} />}
                isDisabled={isCalculatorOpen}
                label="Calculator"
                size="sm"
                variant="secondary"
              />
              <Button
                as="a"
                href={matchReplayDownloadPath(matchId)}
                icon={<DownloadIcon aria-hidden height={16} width={16} />}
                label="Download the replay archive"
                size="sm"
                variant="secondary"
              >
                Archive
              </Button>
              {playerRoster && !isFullscreen ? (
                <GameFullscreenButton onEnter={enterFullscreen} />
              ) : null}
            </HStack>
          </HStack>

          {playerRoster && replayPosition ? (
            <VStack gap={0} paddingBlock={2} paddingInline={3} xstyle={styles.stepControls}>
              <StepControls
                canStepBack={replayPosition.index > 0}
                canStepForward={replayPosition.index < replayPosition.total}
                canStepTurnBack={replayPosition.previousTurnIndex !== null}
                canStepTurnForward={replayPosition.nextTurnIndex !== null}
                day={replayPosition.day}
                isAtLatest={replayPosition.index >= replayPosition.total}
                latestLabel="End"
                onSeekLatest={() => stepReplay((runner) => runner.replaySeekEnd())}
                onSeekStart={() => stepReplay((runner) => runner.replaySeek(0))}
                onStep={(delta) => stepReplay((runner) => runner.replayStep(delta))}
                onTurnStep={(delta) => stepReplay((runner) => runner.replayStepTurn(delta))}
                position={{ index: replayPosition.index, total: replayPosition.total }}
                turnHolder={turnHolder}
              />
            </VStack>
          ) : null}
        </Card>

        <VStack as="section" aria-label="Armies" gap={0} xstyle={styles.rosterSection}>
          <Card padding={0} xstyle={styles.rosterPanel}>
            <RosterList>
              {armies.map((army) => (
                <RosterRow
                  isActive={army.isActive}
                  key={army.entry.playerId}
                  name={army.name}
                  onFactionChange={(factionId) =>
                    runner.setPlayerDisplayFaction(
                      army.entry.playerId,
                      factionId === getFactionByCode(army.entry.actualFactionCode)?.id
                        ? null
                        : factionId,
                    )
                  }
                  player={army.entry}
                  portraitCatalog={portraitCatalog}
                />
              ))}
            </RosterList>
          </Card>
        </VStack>
      </Grid>
    </VStack>
  );
}

const styles = stylex.create({
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
  boardPanel: {
    overflow: "hidden",
    justifySelf: "start",
    inlineSize: "100%",
    maxInlineSize: rosterLayout.boardMaxInlineSize,
  },
  gameSurface: {
    position: "relative",
    aspectRatio: "3 / 2",
    backgroundColor: colorVars["--color-background-inverted"],
  },
  gameSurfaceFullscreen: {
    alignItems: "center",
    aspectRatio: "auto",
    blockSize: "100%",
    inlineSize: "100%",
    justifyContent: "center",
  },
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
    outline: "none",
  },
  gameCanvasHidden: {
    visibility: "hidden",
  },
  boardHud: {
    borderTopWidth: borderVars["--border-width"],
    borderTopStyle: "solid",
    borderTopColor: colorVars["--color-border-emphasized"],
    backgroundColor: colorVars["--color-background-surface"],
  },
  stepControls: {
    borderTopWidth: borderVars["--border-width"],
    borderTopStyle: "solid",
    borderTopColor: colorVars["--color-border"],
    backgroundColor: colorVars["--color-background-surface"],
  },
  rosterSection: {
    minInlineSize: 0,
  },
  rosterPanel: {
    overflow: "hidden",
  },
});
