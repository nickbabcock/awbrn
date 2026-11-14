import { Grid } from "@astryxdesign/core/Grid";
import { Icon } from "@astryxdesign/core/Icon";
import { useMediaQuery } from "@astryxdesign/core/hooks";
import { HStack, Stack, VStack } from "@astryxdesign/core/Stack";
import { borderVars, radiusVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { useLeaving } from "#/matches/components/useLeaving.ts";
import { useCanvasCourierSurface } from "#/canvas_courier/index.ts";
import { ImmersiveNoticeLine, useImmersiveNotice } from "#/components/GameFullscreen.tsx";
import {
  PhoneReplayBar,
  ReviewBarKey,
  ReviewFoot,
  type ReviewModel,
} from "#/components/StepControls.tsx";
import { TileDetails, TileLine, TileWindow } from "#/components/TileInfoBar.tsx";
import type { GameRunner } from "#/engine/game_runner.ts";
import { useGameStore } from "#/engine/store.ts";
import {
  ArmyCard,
  ArmyPlate,
  ArmySheet,
  Ledger,
  TurnOrderRow,
  type ArmyView,
} from "#/matches/components/Armies.tsx";
import {
  BATTLE_CALCULATOR_SHEET_MEDIA,
  BattleCalculator,
} from "#/matches/components/BattleCalculator.tsx";
import {
  BoardKeys,
  FirstTouchHint,
  formatScale,
  Locator,
  useBoardView,
  useTableLayout,
  useWholeMap,
  ViewKey,
  WholeMapOutline,
} from "#/matches/components/BoardControls.tsx";
import { BoardTable, WaitingLine } from "#/matches/components/BoardTable.tsx";
import { LogMoment, TurnLog, TurnLogSheet } from "#/matches/components/TurnLog.tsx";
import { dayStarts, type LogBoundary } from "#/matches/components/turn_log.ts";
import {
  DockKey,
  FullscreenKey,
  InkIconKey,
  MatchFrame,
  MatchNotice,
  TurnLine,
  TurnStrip,
  useFullscreenEnabled,
} from "#/matches/components/MatchFrame.tsx";
import { matchLayout, matchType } from "#/matches/components/matchLayout.stylex.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { IconCalculator, IconLog, IconRefresh } from "#/ui/icons.tsx";
import { ModalSheet } from "#/ui/ModalSheet.tsx";

const PHONE_MEDIA = "(max-width: 720px)";

/**
 * A replay, read back: a finished match at its own address, or an archive
 * loaded from a file.
 */
export function ReplayScreen({
  archiveKey,
  back,
  error,
  locatorSrc,
  map,
  onRetry,
  outline,
  runner,
  title,
  viewpoint,
  views,
}: {
  /** Download (a finished match) or Load (an archive from a file). */
  archiveKey: ReactNode;
  back: ReactNode;
  error: string | null;
  locatorSrc: string | null;
  map: { width: number; height: number } | null;
  /** Reads the archive again after it could not be read. */
  onRetry?: () => void;
  /** The archive's outline, one boundary per moment, read from the file as it loads. */
  outline: readonly LogBoundary[] | null;
  runner: GameRunner;
  title: string;
  viewpoint?: ReactNode;
  views: ArmyView[];
}) {
  const replayPosition = useGameStore((state) => state.replayPosition);
  const hoveredTile = useGameStore((state) => state.hoveredTile);
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
  // The board takes the keys once an archive opens, so the arrows step it
  // without a click on the board first.
  const isOpen = replayPosition !== null;
  useEffect(() => {
    if (isOpen) focus();
  }, [focus, isOpen]);
  const tableRef = useRef<HTMLDivElement>(null);
  const isPhone = useMediaQuery(PHONE_MEDIA);
  const isCalculatorCompact = useMediaQuery(BATTLE_CALCULATOR_SHEET_MEDIA);
  const isFullscreenEnabled = useFullscreenEnabled();
  const layout = useTableLayout(tableRef, map, isPhone);
  const whole = useWholeMap(runner);
  const boardView = useBoardView();
  const [isCalculatorOpen, setIsCalculatorOpen] = useState(false);
  const [isTileSheetOpen, setIsTileSheetOpen] = useState(false);
  const [isLogOpen, setIsLogOpen] = useState(false);
  // The army stays in the sheet while it closes, so its exit can play.
  const [sheetArmyId, setSheetArmyId] = useState<number | null>(null);
  const [isArmySheetOpen, setIsArmySheetOpen] = useState(false);
  const openArmySheet = (playerId: number) => {
    setSheetArmyId(playerId);
    setIsArmySheetOpen(true);
  };
  const [stepError, setStepError] = useState<string | null>(null);

  const lastStepRef = useRef<((runner: GameRunner) => Promise<void>) | null>(null);
  function stepReplay(step: (runner: GameRunner) => Promise<void>) {
    lastStepRef.current = step;
    setStepError(null);
    void step(runner).catch((stepFailure: unknown) => {
      setStepError(
        stepFailure instanceof Error ? stepFailure.message : "The replay could not be stepped.",
      );
      console.error("Error stepping the replay:", stepFailure);
    });
  }

  // The outline is read from the file in the page; one that does not count
  // the engine's actions describes some other reading of the archive.
  const boundaries =
    outline !== null && (replayPosition === null || outline.length === replayPosition.total + 1)
      ? outline
      : null;
  // Until the engine reports where the archive opens, the bar stands at the
  // opening with its keys held, so the screen does not reflow when it does.
  const position =
    replayPosition ??
    (boundaries === null
      ? null
      : {
          index: 0,
          total: boundaries.length - 1,
          day: boundaries[0]?.day ?? 1,
          previousTurnIndex: null,
          nextTurnIndex: null,
        });
  const review: ReviewModel | null =
    position === null
      ? null
      : {
          position: { index: position.index, total: position.total },
          isAtLatest: position.index >= position.total,
          canStepBack: position.index > 0,
          canStepForward: position.index < position.total,
          canStepTurnBack: position.previousTurnIndex !== null,
          canStepTurnForward: position.nextTurnIndex !== null,
          latestLabel: "End",
          isBusy: replayPosition === null,
          onSeekStart: () => stepReplay((game) => game.replaySeek(0)),
          onStep: (delta) => stepReplay((game) => game.replayStep(delta)),
          onTurnStep: (delta) => stepReplay((game) => game.replayStepTurn(delta)),
          onSeekLatest: () => stepReplay((game) => game.replaySeekEnd()),
          onSeek: (index) => stepReplay((game) => game.replaySeek(index)),
          dayStarts: boundaries === null ? undefined : dayStarts(boundaries),
          hasLastKey: true,
        };
  const day = position?.day ?? null;
  const armies = views.map((view) => view.army);
  const log =
    boundaries !== null && position !== null
      ? { boundaries, currentIndex: position.index, isBusy: replayPosition === null }
      : null;
  const seekLine = (index: number) => stepReplay((game) => game.replaySeek(index));
  const mover = views.find((view) => view.army.isActive) ?? null;
  const locator = locatorSrc ? <Locator mapName={title} runner={runner} src={locatorSrc} /> : null;
  const shownError = error ?? stepError;
  // Read the file again, or retry the step that failed.
  const retry =
    error !== null
      ? onRetry
      : () => {
          const step = lastStepRef.current;
          if (step) stepReplay(step);
        };
  const notice = shownError ? (
    <MatchNotice
      action={
        retry ? (
          <Button
            icon={<Icon icon={IconRefresh} />}
            label="Try again"
            onClick={retry}
            variant="on-ink"
          />
        ) : undefined
      }
      description={shownError}
      title="The replay could not be read"
    />
  ) : null;
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
      />
    ) : null;

  // A notice takes the tile window's place, as it takes the tile row on a
  // phone. Before the table knows its mode there is no window to stand in, so
  // it takes the board's place.
  const tileNotice = notice ? <VStack xstyle={styles.columnNotice}>{notice}</VStack> : null;
  const rail = (
    <>
      {views.length <= 2 ? (
        views.map((view) => <ArmyCard isOwed={false} key={view.army.entry.playerId} view={view} />)
      ) : (
        <Ledger isOwed={false} onOpen={openArmySheet} views={views} />
      )}
      {viewpoint ? (
        <section aria-label="Viewpoint" {...stylex.props(styles.viewBlock)}>
          {viewpoint}
        </section>
      ) : null}
      {layout.mode === "none" ? (
        <section aria-label="View" {...stylex.props(styles.viewBlock)}>
          {tileNotice ?? <TileLine hasNames />}
          <BoardKeys runner={runner} />
          {locator}
        </section>
      ) : null}
      {log ? (
        <TurnLog
          armies={armies}
          boundaries={log.boundaries}
          currentIndex={log.currentIndex}
          isDisabled={log.isBusy}
          onSeek={seekLine}
        />
      ) : null}
    </>
  );

  const plateView = mover ?? views[0] ?? null;
  const phoneArmies =
    views.length <= 2 ? (
      <Grid columns={Math.max(views.length, 1)} gap={0} xstyle={styles.plates}>
        {views.map((view) => (
          <ArmyPlate
            hasYouTag={false}
            isOwed={false}
            key={view.army.entry.playerId}
            onOpen={() => openArmySheet(view.army.entry.playerId)}
            showUnits
            view={view}
          />
        ))}
      </Grid>
    ) : (
      <>
        <TurnOrderRow isOwed={false} onOpen={openArmySheet} views={views} />
        {plateView ? (
          <ArmyPlate
            hasYouTag={false}
            isOwed={false}
            isWide
            onOpen={() => openArmySheet(plateView.army.entry.playerId)}
            showUnits
            view={plateView}
          />
        ) : null}
      </>
    );

  const tileRow = (
    <HStack align="center" gap={2} xstyle={styles.tileRow}>
      {notice ? (
        <VStack xstyle={styles.tileRowNotice}>{notice}</VStack>
      ) : isImmersiveNotice ? (
        <ImmersiveNoticeLine xstyle={styles.wholeLine} />
      ) : boardView.bv === null ? (
        <>
          <WaitingLine label="Loading the replay" />
          <ViewKey runner={runner} />
        </>
      ) : whole.isWhole ? (
        <>
          <HStack as="span" xstyle={styles.wholeLine}>
            <b {...stylex.props(styles.wholeScale)}>{formatScale(boardView.bv.fitScale)}</b>
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

  const openCalculator = () => setIsCalculatorOpen(true);
  const fullscreenKey = (
    <FullscreenKey isFullscreen={isFullscreen} onEnter={enterFullscreen} onExit={exitFullscreen} />
  );
  const sheetView = views.find((view) => view.army.entry.playerId === sheetArmyId) ?? null;

  return (
    <>
      <MatchFrame
        dock={
          review ? (
            <>
              {viewpoint ? (
                <section aria-label="Viewpoint" {...stylex.props(styles.phoneViewpoint)}>
                  {viewpoint}
                </section>
              ) : null}
              <PhoneReplayBar
                extra={
                  <>
                    {log ? (
                      <ReviewBarKey
                        icon={IconLog}
                        isDisabled={isLogOpen}
                        label="The day's log"
                        onPress={() => setIsLogOpen(true)}
                      />
                    ) : null}
                    <ReviewBarKey
                      icon={IconCalculator}
                      isDisabled={isCalculatorOpen}
                      label="Calculator"
                      onPress={openCalculator}
                    />
                  </>
                }
                review={review}
                said={
                  log ? (
                    <LogMoment
                      armies={armies}
                      boundaries={log.boundaries}
                      currentIndex={log.currentIndex}
                    />
                  ) : mover ? (
                    <HStack as="span" xstyle={styles.said}>
                      <Stack as="span" aria-hidden="true" xstyle={styles.dot} />
                      {mover.army.name}
                    </HStack>
                  ) : undefined
                }
              />
            </>
          ) : (
            <HStack gap={2} xstyle={styles.emptyDock}>
              <DockKey
                icon={IconCalculator}
                isDisabled={isCalculatorOpen}
                label="Calculator"
                onPress={openCalculator}
              />
            </HStack>
          )
        }
        foot={review ? <ReviewFoot review={review} /> : undefined}
        fullscreenMode={fullscreenMode}
        label={title}
        phoneArmies={phoneArmies}
        rail={rail}
        railFoot={
          <>
            <InkIconKey
              icon={IconCalculator}
              isDisabled={isCalculatorOpen}
              label="Calculator"
              onPress={openCalculator}
            />
            {archiveKey}
          </>
        }
        strip={
          <TurnStrip
            day={day}
            isOwed={false}
            keys={fullscreenKey}
            sub={isImmersiveNotice ? <ImmersiveNoticeLine /> : undefined}
            title={title}
          />
        }
        surfaceRef={surfaceRef}
        table={
          <BoardTable
            canvasContainerRef={canvasContainerRef}
            layout={layout}
            locator={locator}
            map={map}
            floating={calculator}
            notice={tileNotice}
            overlay={<WholeMapOutline runner={runner} />}
            side={
              <>
                <BoardKeys runner={runner} />
                {tileNotice ?? <TileWindow />}
              </>
            }
            tableRef={tableRef}
            waiting={shownError ? null : "Loading the replay"}
          />
        }
        tileRow={tileRow}
        turnLine={
          <TurnLine
            back={back}
            day={day}
            keys={
              <>
                {archiveKey}
                {isFullscreenEnabled ? fullscreenKey : null}
              </>
            }
            title={title}
          />
        }
      />
      <ArmySheet
        isOpen={isArmySheetOpen && sheetView !== null}
        onOpenChange={setIsArmySheetOpen}
        view={sheetView}
      />
      {log ? (
        <TurnLogSheet
          armies={armies}
          boundaries={log.boundaries}
          currentIndex={log.currentIndex}
          isDisabled={log.isBusy}
          isOpen={isLogOpen}
          onOpenChange={setIsLogOpen}
          onSeek={seekLine}
        />
      ) : null}
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

const styles = stylex.create({
  gameCanvas: {
    display: "block",
    width: "100%",
    height: "100%",
    imageRendering: "pixelated",
    outline: "none",
    touchAction: "none",
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
    minBlockSize: matchLayout.footHeight,
    paddingBlock: spacingVars["--spacing-1-5"],
    paddingInline: spacingVars["--spacing-2"],
    backgroundColor: awbrnVars.ink900,
    borderBlockStartColor: awbrnVars.ink800,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: borderVars["--border-width"],
  },
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
  wholeScale: {
    color: awbrnVars.textOnInkStrong,
    fontFamily: awbrnVars.fontFigure,
    fontSize: matchType.body,
    fontWeight: 600,
  },
  said: {
    display: "inline-flex",
    alignItems: "center",
    gap: spacingVars["--spacing-2"],
  },
  dot: {
    display: "inline-block",
    inlineSize: matchLayout.movingDot,
    blockSize: matchLayout.movingDot,
    borderRadius: "50%",
    backgroundColor: awbrnVars.okOnInk,
  },
  phoneViewpoint: {
    paddingBlock: spacingVars["--spacing-2"],
    paddingInline: spacingVars["--spacing-3"],
    backgroundColor: awbrnVars.paper,
  },
  emptyDock: {
    paddingBlock: awbrnVars.spacing2_5,
    paddingInline: spacingVars["--spacing-3"],
    paddingBlockEnd: `calc(${spacingVars["--spacing-3"]} + env(safe-area-inset-bottom, 0px))`,
    backgroundColor: awbrnVars.ink900,
    borderBlockStartColor: awbrnVars.ink800,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: borderVars["--border-width"],
  },
});
