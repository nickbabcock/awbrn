import { Banner } from "@astryxdesign/core/Banner";
import { Card } from "@astryxdesign/core/Card";
import { Center } from "@astryxdesign/core/Center";
import { FileInput } from "@astryxdesign/core/FileInput";
import { Grid } from "@astryxdesign/core/Grid";
import { Heading } from "@astryxdesign/core/Heading";
import { useAnnounce, useMediaQuery } from "@astryxdesign/core/hooks";
import { InfoTip } from "@astryxdesign/core/InfoTip";
import { List, ListItem } from "@astryxdesign/core/List";
import { MetadataList, MetadataListItem } from "@astryxdesign/core/MetadataList";
import { RadioList, RadioListItem } from "@astryxdesign/core/RadioList";
import { Selector } from "@astryxdesign/core/Selector";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Switch } from "@astryxdesign/core/Switch";
import { Text } from "@astryxdesign/core/Text";
import { TextInput } from "@astryxdesign/core/TextInput";
import { compactDefinition } from "@astryxdesign/core/theme";
import {
  borderVars,
  colorVars,
  radiusVars,
  spacingVars,
  typeScaleVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getAwbwGameFn } from "#/awbw/awbw.functions.ts";
import type { AwbwGameSnapshot } from "#/awbw/schemas.ts";
import { parseAwbwGameId } from "#/awbw/parsers.ts";
import { useCanvasCourierSurface } from "#/canvas_courier/index.ts";
import { coRoster } from "#/co_roster.ts";
import { loadCoPortraitCatalog } from "#/components/co_portraits.ts";
import {
  GameFullscreenButton,
  ImmersiveNoticeLine,
  useImmersiveNotice,
} from "#/components/GameFullscreen.tsx";
import { TileInfoBar } from "#/components/TileInfoBar.tsx";
import { useReplayRunner } from "#/engine/runtime_context.tsx";
import { useGameStore } from "#/engine/store.ts";
import { AttackPreview } from "#/matches/components/AttackPreview.tsx";
import { BuildMenu } from "#/matches/components/BuildMenu.tsx";
import { UnitActionMenu } from "#/matches/components/UnitActionMenu.tsx";
import { describeTurnResidue } from "#/matches/turn_residue.ts";
import {
  canActivatePower,
  readPowerMeter,
  type ActivatablePowerLevel,
} from "#/replay/power_meter.ts";
import { RosterList, RosterRow } from "#/replay/RosterRow.tsx";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { Figure } from "#/ui/Figure.tsx";
import { Page, PageHeader } from "#/ui/Page.tsx";
import { rosterLayout } from "#/ui/rosterLayout.stylex.ts";
import { missileVars } from "./missileColors.stylex.ts";
import {
  DEFAULT_SEAT,
  NO_CO_ID,
  parseStars,
  seatSetup,
  setupFromAwbwGame,
  type SeatInput,
} from "./planner_seats.ts";
import type { GameRunner } from "#/engine/game_runner.ts";
import type { PlannerLuck } from "#/engine/worker_module.ts";
import type { PlayerCommand } from "#/matches/match_protocol.ts";
import type {
  MovePlannerSummary,
  PlannerSelectedUnit,
  PlayerRosterEntry,
  PreviewedAreaStrike,
  StruckUnit,
  UnitKind,
} from "#/wasm/awbrn_wasm.js";

/**
 * The name of each Covering Fire missile, by its aim. The swatch copies the
 * badge on the board, so the colors are the board's missile colors and not
 * theme colors.
 */
const STRIKE_POLICIES = {
  "infantry-hp": { label: "Infantry missile", swatch: "infantry" },
  "unit-value": { label: "Funds missile", swatch: "funds" },
  "unit-hp": { label: "HP missile", swatch: "hp" },
} as const;

type PlannerEdit = Parameters<GameRunner["editPlanner"]>[0];

/** The luck that planner attacks roll, by its label. */
const LUCK_OPTIONS: { label: string; value: PlannerLuck }[] = [
  { label: "Low luck", value: "low" },
  { label: "Average luck", value: "average" },
  { label: "High luck", value: "high" },
];

/** The visible HP a unit can be given, highest first. */
const HP_OPTIONS = Array.from({ length: 10 }, (_, index) => String(10 - index));

const NO_CO_LABEL = "No CO";
const NO_PREVIEW_LABEL = "None";
const CO_OPTIONS = [NO_CO_LABEL, ...coRoster.map((co) => co.displayName)];

/**
 * Below this width, the board menus open as a sheet on the bottom edge. The
 * width is the one at which Astryx makes its own sheets compact. The height
 * part of the Astryx rule is not used, so a short, wide window keeps the menus
 * on the board.
 */
const SHEET_MEDIA = `(width < ${compactDefinition().widthBelow}px)`;

/** How long a press on the board can still be the one that opened a menu. */
const BOARD_PRESS_MAX_AGE_MS = 2000;

interface LoadedPlanner {
  data: Uint8Array;
  summary: MovePlannerSummary;
}

interface BoardPress {
  at: number;
  isCoarse: boolean;
  x: number;
  y: number;
}

export function PlannerPage() {
  const runner = useReplayRunner();
  const portraitCatalog = useMemo(() => loadCoPortraitCatalog(), []);
  const playerRoster = useGameStore((state) => state.playerRoster);
  const areaStrikes = useGameStore((state) => state.areaStrikes);
  const plannerSelection = useGameStore((state) => state.plannerSelection);
  const productionOptions = useGameStore((state) => state.productionOptions);
  const unitActions = useGameStore((state) => state.unitActions);
  const turnReadiness = useGameStore((state) => state.turnReadiness);
  const setProductionOptions = useGameStore((state) => state.actions.setProductionOptions);
  const [plannerFile, setPlannerFile] = useState<File | null>(null);
  const [planner, setPlanner] = useState<LoadedPlanner | null>(null);
  const [mapId, setMapId] = useState("");
  /** The AWBW game link, or game ID, that the setup is filled from. */
  const [gameLink, setGameLink] = useState("");
  const [isFillingFromGame, setIsFillingFromGame] = useState(false);
  const [filledFrom, setFilledFrom] = useState<string | null>(null);
  const announce = useAnnounce();
  /**
   * Count the changes to the game link and to the planner file. A read keeps
   * the counts from when it started, so that it can find out when it finishes
   * whether the inputs that it read are still there.
   */
  const gameLinkChanges = useRef(0);
  const plannerFileChanges = useRef(0);

  /** Show the result of a fill, and tell a screen reader, which cannot see it appear. */
  function reportFilled(message: string) {
    setFilledFrom(message);
    announce(message);
  }
  /** A game read before its planner file was chosen, to apply to the file. */
  const pendingGame = useRef<AwbwGameSnapshot | null>(null);
  const [firstPlayerId, setFirstPlayerId] = useState("");
  /** The army whose missiles the board previews, or "" for none. */
  const [previewedPlayerId, setPreviewedPlayerId] = useState("");
  const [seats, setSeats] = useState<Record<number, SeatInput>>({});
  // The reads below finish after later renders, so they read the planner and
  // the seats from here, not from the render that started them.
  const loadedPlanner = useRef<LoadedPlanner | null>(null);
  const seatsRef = useRef(seats);
  seatsRef.current = seats;
  const [error, setError] = useState<string | null>(null);
  const [isBoardRequested, setIsBoardRequested] = useState(false);
  const [isAfterStrikes, setIsAfterStrikes] = useState(false);
  const [luck, setLuck] = useState<PlannerLuck>("average");
  const isCompactViewport = useMediaQuery(SHEET_MEDIA);
  const {
    canvasContainerRef,
    enterFullscreen,
    exitFullscreen,
    fullscreenMode,
    isFullscreen,
    focus,
    surfaceRef,
  } = useCanvasCourierSurface({
    host: runner.host,
    canvasClassName:
      stylex.props(styles.gameCanvas, !isBoardRequested && styles.gameCanvasHidden).className ?? "",
  });

  const reportError = useCallback((cause: unknown, fallback: string) => {
    setError(cause instanceof Error ? cause.message : fallback);
  }, []);

  const order = useCallback(
    (command: PlayerCommand) => {
      void runner.plannerOrder(command).catch((cause) => {
        reportError(cause, "The order could not be given.");
      });
    },
    [reportError, runner],
  );

  // The board asks the host to carry each order, as in a match. Here the host
  // gives the order straight back to the engine, which plays it locally.
  useEffect(() => {
    if (!isBoardRequested) return;
    runner.setLiveCommandHandler(order);
    runner.setEndTurnRequestHandler(() => order({ type: "endTurn" }));
    return () => {
      runner.setLiveCommandHandler(undefined);
      runner.setEndTurnRequestHandler(undefined);
    };
  }, [isBoardRequested, order, runner]);

  async function handlePlannerFileChange(files: File | File[] | null) {
    const file = Array.isArray(files) ? files[0] : files;
    // Only a newer file makes this read stale. A game link changed during the
    // read makes the game that was waiting stale, not the file.
    const fileChange = ++plannerFileChanges.current;
    const linkChange = gameLinkChanges.current;
    setError(null);
    loadedPlanner.current = null;
    setPlanner(null);
    setFirstPlayerId("");
    setPreviewedPlayerId("");
    setSeats({});
    setFilledFrom(null);
    if (!file) return;

    try {
      const data = new Uint8Array(await file.arrayBuffer());
      const summary = await runner.readMovePlanner(data);
      if (fileChange !== plannerFileChanges.current) return;
      loadedPlanner.current = { data, summary };
      setPlanner(loadedPlanner.current);
      // Both branches read the game link from when the read started.
      if (linkChange !== gameLinkChanges.current) return;
      const game = pendingGame.current;
      if (game) {
        pendingGame.current = null;
        applyGame(game, summary, {});
      } else if (gameLink.trim() === "" && summary.gameId) {
        // The file names the game that it was saved from, which is most often
        // the game to fill the setup from.
        setGameLink(`https://awbw.amarriner.com/game.php?games_id=${summary.gameId}`);
      }
    } catch (cause) {
      if (fileChange !== plannerFileChanges.current) return;
      reportError(cause, "The move planner file could not be read.");
    }
  }

  const seatFor = (playerId: number) => seats[playerId] ?? DEFAULT_SEAT;
  const updateSeat = (playerId: number, change: Partial<SeatInput>) =>
    setSeats((current) => ({
      ...current,
      [playerId]: { ...(current[playerId] ?? DEFAULT_SEAT), ...change },
    }));

  const firesStrikes = (coId: number) => planner?.summary.areaStrikeCoIds.includes(coId) ?? false;
  // The armies whose CO has missiles to preview.
  const strikeArmies =
    planner?.summary.players.filter((player) => firesStrikes(seatFor(player.playerId).coId)) ?? [];

  function changeCo(playerId: number, coId: number) {
    updateSeat(playerId, { coId });
    // The first army with missiles is the one to watch, until the player
    // picks another.
    if (firesStrikes(coId) && previewedPlayerId === "") {
      setPreviewedPlayerId(String(playerId));
    } else if (!firesStrikes(coId) && previewedPlayerId === String(playerId)) {
      setPreviewedPlayerId("");
    }
  }

  function changePreview(playerId: string) {
    setPreviewedPlayerId(playerId);
    if (!isBoardRequested) return;
    void runner
      .setPlannerPreviewedPlayer(playerId === "" ? null : Number(playerId))
      .catch((cause) => reportError(cause, "The board could not preview those missiles."));
  }

  const previewSelector =
    strikeArmies.length > 0 ? (
      <Selector
        label="Preview missiles of"
        labelTooltip="The board shows where this army's missiles would land if its super power started now, on every army's turn."
        onChange={(value) =>
          changePreview(
            String(strikeArmies.find((player) => player.factionName === value)?.playerId ?? ""),
          )
        }
        options={[NO_PREVIEW_LABEL, ...strikeArmies.map((player) => player.factionName)]}
        size="sm"
        value={
          strikeArmies.find((player) => String(player.playerId) === previewedPlayerId)
            ?.factionName ?? NO_PREVIEW_LABEL
        }
      />
    ) : null;

  const gameId = parseAwbwGameId(gameLink);

  /** Fill the setup of the planner file `summary` from `game`. */
  function applyGame(
    game: AwbwGameSnapshot,
    summary: MovePlannerSummary,
    currentSeats: Record<number, SeatInput>,
  ) {
    const setup = setupFromAwbwGame(
      game,
      summary.players.map((player) => player.playerId),
      currentSeats,
      summary.powerStarCharges,
      summary.areaStrikeCoIds,
    );
    if (setup === null) {
      setFilledFrom(null);
      setError(`AWBW game ${game.gameId} has none of the armies in this planner file.`);
      return;
    }
    if (setup.mapId !== null) setMapId(setup.mapId);
    if (setup.firstPlayerId !== null) setFirstPlayerId(setup.firstPlayerId);
    setSeats(setup.seats);
    changePreview(setup.previewedPlayerId);
    reportFilled(
      game.armies.some((army) => army.funds === null)
        ? `Filled from game ${game.gameId}. Fog hides some funds, so enter them.`
        : `Filled from game ${game.gameId}.`,
    );
  }

  async function fillFromAwbwGame() {
    if (gameId === null) return;
    // A changed game link makes this read stale. A changed planner file does
    // too, when the game was to fill the file that was open. With no file
    // open, the game waits for the next file, so a file chosen during the read
    // is the one to fill.
    const fileChange = plannerFileChanges.current;
    const linkChange = gameLinkChanges.current;
    const hadPlanner = loadedPlanner.current !== null;
    const isStale = () =>
      linkChange !== gameLinkChanges.current ||
      (hadPlanner && fileChange !== plannerFileChanges.current);
    setError(null);
    setIsFillingFromGame(true);
    try {
      const game = await readGame(gameId);
      if (isStale()) return;
      const current = loadedPlanner.current;
      if (current) {
        applyGame(game, current.summary, seatsRef.current);
      } else {
        pendingGame.current = game;
        reportFilled(`Read game ${gameId}. Choose a move planner file from it to fill the setup.`);
      }
    } catch (cause) {
      if (isStale()) return;
      reportError(cause, "The AWBW game could not be read.");
    } finally {
      setIsFillingFromGame(false);
    }
  }

  const parsedMapId = Number.parseInt(mapId, 10);
  const isMapIdValid = /^\d+$/.test(mapId.trim()) && parsedMapId > 0;
  const invalidSeat = planner?.summary.players.find((player) => {
    const seat = seatFor(player.playerId);
    return !isWholeNumber(seat.funds) || !isStarCount(seat.powerStars);
  });
  const canOpenBoard =
    planner !== null && isMapIdValid && firstPlayerId !== "" && invalidSeat === undefined;

  async function openBoard() {
    if (!planner || !canOpenBoard) return;
    setError(null);
    try {
      setIsBoardRequested(true);
      await runner.loadMovePlanner(planner.data, {
        mapId: parsedMapId,
        firstPlayerId: Number(firstPlayerId),
        previewedPlayerId: previewedPlayerId === "" ? undefined : Number(previewedPlayerId),
        seats: planner.summary.players.map((player) =>
          seatSetup(player.playerId, seatFor(player.playerId), planner.summary.powerStarCharges),
        ),
      });
      focus();
    } catch (cause) {
      reportError(cause, "The board could not be loaded.");
    }
  }

  function editPlanner(edit: PlannerEdit) {
    void runner.editPlanner(edit).catch((cause) => {
      reportError(cause, "The planner board could not be changed.");
    });
  }

  async function showAfterStrikes(on: boolean) {
    setIsAfterStrikes(on);
    try {
      await runner.setPlannerAfterStrikes(on);
    } catch (cause) {
      reportError(cause, "The board could not show that view.");
    }
  }

  async function changeLuck(next: PlannerLuck) {
    setLuck(next);
    try {
      await runner.setPlannerLuck(next);
    } catch (cause) {
      reportError(cause, "The planner luck could not be set.");
    }
  }

  const canUndo = areaStrikes?.canUndo ?? false;
  const canRedo = areaStrikes?.canRedo ?? false;

  // Undo and redo keys work anywhere on the page once a board is open, except
  // in a text field, where they edit the text.
  useEffect(() => {
    if (!isBoardRequested) return;
    function handleKeyDown(event: KeyboardEvent) {
      if (!(event.ctrlKey || event.metaKey) || event.altKey) return;
      const target = event.target;
      if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) return;
      const key = event.key.toLowerCase();
      const edit: PlannerEdit | null =
        key === "z"
          ? { type: event.shiftKey ? "redo" : "undo" }
          : key === "y"
            ? { type: "redo" }
            : null;
      if (!edit) return;
      event.preventDefault();
      void runner.editPlanner(edit).catch((cause) => {
        reportError(cause, "The planner board could not be changed.");
      });
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isBoardRequested, reportError, runner]);

  // The last press on the board, so that a menu opens beside it and in the
  // shape that the input expects.
  const pressRef = useRef<BoardPress | null>(null);
  const [press, setPress] = useState<BoardPress | null>(null);
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
  const openMenu = productionOptions ?? unitActions;
  useEffect(() => {
    if (openMenu === null) {
      setPress(null);
      return;
    }
    const recent = pressRef.current;
    setPress(recent && performance.now() - recent.at < BOARD_PRESS_MAX_AGE_MS ? recent : null);
  }, [openMenu]);
  const menuPresentation = press?.isCoarse || isCompactViewport ? "sheet" : "board";

  const factionNames = new Map(
    planner?.summary.players.map((player) => [player.playerId, player.factionName]) ?? [],
  );
  const activeArmy = playerRoster?.players.find(
    (player) => player.playerId === playerRoster.activePlayerId,
  );
  const isPowerReady =
    activeArmy !== undefined &&
    (canActivatePower(activeArmy, "cop") || canActivatePower(activeArmy, "scop"));
  const isTurnSpent = describeTurnResidue(turnReadiness, isPowerReady) === null;
  const reportedPreviewId = areaStrikes?.playerId;
  // The engine reports the teams that the board plays with, which can come
  // from an AWBW game and not from the planner file.
  const teamOf = (playerId: number | undefined) =>
    playerRoster?.players.find((player) => player.playerId === playerId)?.team ?? null;
  const previewedTeam = teamOf(reportedPreviewId);
  const isAlly = (unit: StruckUnit) =>
    unit.playerId === reportedPreviewId ||
    (previewedTeam !== null && teamOf(unit.playerId) === previewedTeam);
  const hasStrikes = (areaStrikes?.strikes.length ?? 0) > 0;
  const previewedArmy = playerRoster?.players.find(
    (player) => player.playerId === reportedPreviewId,
  );
  const previewedName =
    reportedPreviewId === undefined ? "Army" : (factionNames.get(reportedPreviewId) ?? "Army");
  const isPreviewedTurn =
    reportedPreviewId !== undefined && playerRoster?.activePlayerId === reportedPreviewId;
  const productionSite = productionOptions?.site;

  function activatePower(level: ActivatablePowerLevel) {
    order({ type: "activatePower", level });
    setProductionOptions(null);
  }

  function buildUnit(unit: UnitKind, x: number, y: number) {
    order({ type: "build", position: { x, y }, unit_type: unit });
    setProductionOptions(null);
  }

  // The strip under the board holds the day, the army to move and the
  // planner's keys; with no board it holds nothing, so the frame stays ink.
  const hasBoardHud = isBoardRequested || playerRoster != null || areaStrikes != null;
  const isImmersiveNotice = useImmersiveNotice(fullscreenMode);

  // The strip goes into full screen with the board, so its keys, the way out
  // and the notice of it stay beside the board rather than over a tile.
  const boardHud = hasBoardHud ? (
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
        {playerRoster ? <Figure size="md">Day {playerRoster.day}</Figure> : null}
        {/* The notice holds the line for a moment, then gives it back. */}
        {isImmersiveNotice ? (
          <ImmersiveNoticeLine xstyle={styles.noticeLine} />
        ) : (
          <>
            {planner && isBoardRequested ? (
              <Text type="supporting">
                {`${planner.summary.name || "Planner"} · Map ${parsedMapId}`}
              </Text>
            ) : null}
            {activeArmy ? (
              <Text type="supporting">{activeArmy.displayFactionName} to move</Text>
            ) : null}
          </>
        )}
      </HStack>
      <HStack align="center" gap={2} wrap="wrap">
        {areaStrikes ? (
          <>
            {hasStrikes ? (
              <Switch
                label="After strikes"
                labelTooltip="Each unit shows the HP it keeps after all the missiles."
                onChange={(on) => void showAfterStrikes(on)}
                size="sm"
                value={isAfterStrikes}
              />
            ) : null}
            <Selector
              label="Attack luck"
              labelTooltip="Each luck roll of an attack and its counter lands here in its range."
              onChange={(value) => {
                const option = LUCK_OPTIONS.find((item) => item.label === value);
                if (option) void changeLuck(option.value);
              }}
              options={LUCK_OPTIONS.map((item) => item.label)}
              size="sm"
              value={LUCK_OPTIONS.find((item) => item.value === luck)?.label ?? ""}
            />
            <Button
              clickAction={() => editPlanner({ type: "undo" })}
              isDisabled={!canUndo}
              label="Undo"
              size="sm"
              variant="secondary"
            />
            <Button
              clickAction={() => editPlanner({ type: "redo" })}
              isDisabled={!canRedo}
              label="Redo"
              size="sm"
              variant="secondary"
            />
            <Button
              clickAction={() => editPlanner({ type: "reset" })}
              isDisabled={!canUndo && !canRedo}
              label="Reset"
              size="sm"
              variant="secondary"
            />
            <Button
              clickAction={() => order({ type: "endTurn" })}
              isDisabled={activeArmy === undefined}
              label="End turn"
              size="sm"
              tooltip="Give the turn to the next army (Ctrl+Enter)."
              variant={isTurnSpent ? "primary" : "secondary"}
            />
          </>
        ) : null}
        {isFullscreen ? (
          <Button
            clickAction={exitFullscreen}
            label="Exit full screen"
            size="sm"
            variant="secondary"
          />
        ) : isBoardRequested ? (
          <GameFullscreenButton onEnter={enterFullscreen} />
        ) : null}
      </HStack>
    </HStack>
  ) : null;

  return (
    <Page>
      <VStack gap={6}>
        <PageHeader title="Plan a turn" />

        {error ? (
          <Banner description={error} role="alert" status="error" title="Planner error" />
        ) : null}

        <Grid align="start" gap={4} xstyle={styles.layout}>
          <Card padding={0} variant="ink" xstyle={styles.boardPanel}>
            <VStack
              gap={0}
              ref={surfaceRef}
              xstyle={[
                styles.gameSurface,
                isFullscreen && styles.gameSurfaceFullscreen,
                fullscreenMode === "immersive" && styles.gameSurfaceImmersive,
              ]}
            >
              {/* The board's own box, which its readout sits in; in full screen
                  the strip takes a row below it. */}
              <VStack gap={0} xstyle={styles.boardArea}>
                <VStack ref={canvasContainerRef} gap={0} width="100%" height="100%" />
                {playerRoster ? <TileInfoBar /> : null}
              </VStack>
              {!isBoardRequested ? (
                <Center height="100%" width="100%" xstyle={styles.placeholder}>
                  {/* The Replays empty state's ink well; the setup beside it loads a board. */}
                  <Center height="100%" width="100%" xstyle={styles.emptyWell}>
                    <Heading level={2} xstyle={styles.emptyTitle}>
                      No board yet
                    </Heading>
                  </Center>
                </Center>
              ) : null}
              {productionOptions === null || productionSite === undefined ? null : (
                <BuildMenu
                  anchor={productionOptions.anchor ?? press}
                  factionCode={activeArmy?.displayFactionCode ?? "os"}
                  funds={activeArmy?.stats.funds ?? null}
                  isEnabled
                  onBuild={(unit) => buildUnit(unit, productionSite.x, productionSite.y)}
                  onDismiss={() => setProductionOptions(null)}
                  onRestoreFocus={focus}
                  options={productionOptions.options}
                  presentation={menuPresentation}
                  site={productionSite}
                />
              )}
              {unitActions === null || unitActions.destination === undefined ? null : (
                <UnitActionMenu
                  anchor={unitActions.anchor ?? press}
                  attacker={unitActions.attacker ?? undefined}
                  destination={unitActions.destination}
                  isEnabled
                  onChoose={(index) => {
                    void runner.chooseUnitAction(index).catch((cause) => {
                      reportError(cause, "The order could not be given.");
                    });
                  }}
                  onDismiss={() => {
                    void runner.dismissUnitAction().catch((cause) => {
                      reportError(cause, "The menu could not be closed.");
                    });
                  }}
                  onRestoreFocus={focus}
                  options={unitActions.options}
                  preselected={unitActions.preselected ?? undefined}
                  presentation={menuPresentation}
                />
              )}
              <AttackPreview surfaceRef={surfaceRef} />
              {isFullscreen ? boardHud : null}
            </VStack>
            {isFullscreen ? null : boardHud}
          </Card>

          <VStack as="section" aria-label="Planner setup" gap={4}>
            {playerRoster && isBoardRequested ? (
              <VStack as="section" aria-label="Armies" gap={0}>
                <Card padding={0} xstyle={styles.rosterPanel}>
                  <RosterList>
                    {playerRoster.players.map((player) => {
                      const isActive = playerRoster.activePlayerId === player.playerId;
                      return (
                        <RosterRow
                          isActive={isActive}
                          isViewer={isActive}
                          key={player.playerId}
                          name={player.displayFactionName}
                          onActivatePower={isActive ? activatePower : undefined}
                          player={player}
                          portraitCatalog={portraitCatalog}
                        />
                      );
                    })}
                  </RosterList>
                </Card>
              </VStack>
            ) : null}

            {plannerSelection ? (
              <SelectedUnitCard
                factionName={factionNames.get(plannerSelection.playerId) ?? "Unknown army"}
                onDelete={() => editPlanner({ type: "delete", unitId: plannerSelection.unitId })}
                onHpChange={(hp) =>
                  editPlanner({ type: "setHp", unitId: plannerSelection.unitId, hp })
                }
                unit={plannerSelection}
              />
            ) : null}

            {isBoardRequested && previewSelector ? (
              <Card padding={3}>{previewSelector}</Card>
            ) : null}

            {areaStrikes?.error ? (
              <Card padding={3}>
                <Banner
                  description={areaStrikes.error}
                  role="alert"
                  status="error"
                  title="No board"
                />
              </Card>
            ) : !hasStrikes && isPreviewedTurn && previewedArmy?.activePower !== undefined ? (
              <Card padding={3}>
                <Text type="supporting">
                  {previewedName}&apos;s power is on, so it has no missiles to fire this turn. The
                  preview comes back on the next army&apos;s turn.
                </Text>
              </Card>
            ) : hasStrikes && areaStrikes ? (
              <Card padding={0}>
                <List
                  hasDividers
                  header={
                    <VStack gap={1} padding={3}>
                      {/* Each strike row shows the HP it removes, so the damage
                          rule is reference text behind the info button. */}
                      <HStack align="center" gap={1}>
                        <Text type="label">{previewedName} missiles, in firing order</Text>
                        <InfoTip
                          content="Each missile removes 3 HP from every unit within 2 tiles of its center, and leaves at least 0.1 HP."
                          label="How missiles do damage"
                        />
                      </HStack>
                      <Text type="supporting">
                        {isPreviewedTurn
                          ? `Where the missiles land if ${previewedName} starts the super power now.`
                          : `Where the missiles would land if ${previewedName} started the super power now. They aim again after each order, so plan this turn around them.`}
                      </Text>
                      {previewedArmy ? (
                        <Text type="supporting">{superPowerCharge(previewedArmy)}</Text>
                      ) : null}
                    </VStack>
                  }
                >
                  {areaStrikes.strikes.map((strike, index) => (
                    <StrikeItem
                      factionNames={factionNames}
                      index={index}
                      isAlly={isAlly}
                      key={strike.policy}
                      strike={strike}
                    />
                  ))}
                </List>
              </Card>
            ) : null}

            <Card padding={4}>
              <VStack gap={4}>
                <VStack gap={2}>
                  <TextInput
                    autoCapitalize="none"
                    enterKeyHint="go"
                    hasAutoCorrect={false}
                    inputMode="url"
                    label="AWBW game"
                    onChange={(link) => {
                      gameLinkChanges.current += 1;
                      setGameLink(link);
                      setFilledFrom(null);
                      pendingGame.current = null;
                    }}
                    onEnter={() => void fillFromAwbwGame()}
                    placeholder="Game link or ID"
                    status={
                      gameLink.trim() !== "" && gameId === null
                        ? { type: "error", message: "Enter an AWBW game link or game ID." }
                        : undefined
                    }
                    value={gameLink}
                  />
                  <Button
                    clickAction={fillFromAwbwGame}
                    isDisabled={gameId === null}
                    isLoading={isFillingFromGame}
                    label="Fill from AWBW game"
                    variant="secondary"
                  />
                  {filledFrom ? <Text type="supporting">{filledFrom}</Text> : null}
                </VStack>
                <FileInput
                  accept=".json"
                  changeAction={handlePlannerFileChange}
                  label="Planner file"
                  onChange={(files) =>
                    setPlannerFile(Array.isArray(files) ? (files[0] ?? null) : files)
                  }
                  value={plannerFile}
                  width="100%"
                />
                {planner ? (
                  <>
                    <VStack gap={1}>
                      <TextInput
                        enterKeyHint="go"
                        inputMode="numeric"
                        isRequired
                        label="AWBW map ID"
                        onChange={setMapId}
                        onEnter={() => void openBoard()}
                        status={
                          mapId !== "" && !isMapIdValid
                            ? { type: "error", message: "Enter a whole number." }
                            : undefined
                        }
                        value={mapId}
                      />
                    </VStack>
                    <RadioList
                      isRequired
                      label="Army to plan"
                      onChange={setFirstPlayerId}
                      value={firstPlayerId}
                    >
                      {planner.summary.players.map((player) => (
                        <RadioListItem
                          description={player.team ? `Team ${player.team}` : "No team"}
                          key={player.playerId}
                          label={player.factionName}
                          value={String(player.playerId)}
                        />
                      ))}
                    </RadioList>
                    <Heading level={5}>Armies</Heading>
                    {planner.summary.players.map((player) => {
                      const seat = seatFor(player.playerId);
                      const coName =
                        coRoster.find((co) => co.awbwId === seat.coId)?.displayName ?? NO_CO_LABEL;
                      return (
                        <VStack gap={2} key={player.playerId}>
                          <Text type="label">{player.factionName}</Text>
                          <Selector
                            label="CO"
                            onChange={(value) =>
                              changeCo(
                                player.playerId,
                                coRoster.find((co) => co.displayName === value)?.awbwId ?? NO_CO_ID,
                              )
                            }
                            options={CO_OPTIONS}
                            size="sm"
                            value={coName}
                          />
                          <HStack gap={2}>
                            <TextInput
                              inputMode="numeric"
                              label="Funds"
                              onChange={(funds) => updateSeat(player.playerId, { funds })}
                              size="sm"
                              status={
                                isWholeNumber(seat.funds)
                                  ? undefined
                                  : { type: "error", message: "Enter a whole number." }
                              }
                              value={seat.funds}
                            />
                            <TextInput
                              inputMode="decimal"
                              label="Power stars"
                              onChange={(powerStars) =>
                                updateSeat(player.playerId, {
                                  powerStars,
                                  importedCharge: undefined,
                                })
                              }
                              size="sm"
                              status={
                                isStarCount(seat.powerStars)
                                  ? undefined
                                  : { type: "error", message: "Enter a number of stars." }
                              }
                              value={seat.powerStars}
                            />
                          </HStack>
                          {seat.powerUses > 0 || seat.powerOn !== "none" ? (
                            <Text type="supporting">{powerHistory(seat)}</Text>
                          ) : null}
                        </VStack>
                      );
                    })}
                    {previewSelector}
                    <Button
                      clickAction={openBoard}
                      isDisabled={!canOpenBoard}
                      label={isBoardRequested ? "Open the board again" : "Open the board"}
                      variant="primary"
                    />
                  </>
                ) : null}
              </VStack>
            </Card>
          </VStack>
        </Grid>
      </VStack>
    </Page>
  );
}

/** How far an army's meter is from its super power, in words. */
/**
 * The armies, map and turn of an AWBW game.
 *
 * Throws an error with a message for the player when the game cannot be read.
 */
async function readGame(gameId: number): Promise<AwbwGameSnapshot> {
  const result = await getAwbwGameFn({ data: { gameId } });
  switch (result.status) {
    case "found":
      return result.game;
    case "notFound":
      throw new Error(`AWBW has no game ${gameId}.`);
    case "rateLimited":
      throw new Error("Too many AWBW requests. Try again in a moment.");
    case "unavailable":
      throw new Error("AWBW did not answer. Try again later.");
  }
}

function superPowerCharge(army: PlayerRosterEntry): string {
  const name = army.displayFactionName;
  if (army.activePower !== undefined) return `${name} has a power on now.`;
  const scop = readPowerMeter(army)?.scop;
  if (!scop) return `${name} has no super power.`;
  if (scop.remaining === 0) return `${name} has the charge for the super power.`;
  const stars = Math.ceil((scop.remaining / (scop.charge / scop.stars)) * 10) / 10;
  return `${name} needs ${stars} more stars for the super power.`;
}

/** The powers that an army used, and the one that is on. */
function powerHistory(seat: SeatInput): string {
  const used = seat.powerUses === 1 ? "Used 1 power" : `Used ${seat.powerUses} powers`;
  const on =
    seat.powerOn === "cop"
      ? "normal power on"
      : seat.powerOn === "scop"
        ? "super power on"
        : "no power on";
  return `${used}, ${on}.`;
}

function isWholeNumber(value: string): boolean {
  return /^\d+$/.test(value.trim());
}

/** A star count, such as 3, 2.5 or 2,5. */
function isStarCount(value: string): boolean {
  return parseStars(value) !== null;
}

function SelectedUnitCard({
  factionName,
  onDelete,
  onHpChange,
  unit,
}: {
  factionName: string;
  onDelete: () => void;
  onHpChange: (hp: number) => void;
  unit: PlannerSelectedUnit;
}) {
  return (
    <Card padding={4}>
      <VStack gap={3}>
        <MetadataList title={<Text type="label">Selected unit</Text>}>
          <MetadataListItem label="Army">{factionName}</MetadataListItem>
          <MetadataListItem label="Unit">{unit.unitName}</MetadataListItem>
          <MetadataListItem label="Tile">
            ({unit.x}, {unit.y})
          </MetadataListItem>
        </MetadataList>
        <Selector
          label="HP"
          onChange={(value) => onHpChange(Number(value))}
          options={HP_OPTIONS}
          value={String(unit.hp)}
        />
        <Button clickAction={onDelete} label="Remove unit" size="sm" variant="destructive" />
      </VStack>
    </Card>
  );
}

function StrikeItem({
  factionNames,
  index,
  isAlly,
  strike,
}: {
  factionNames: Map<number, string>;
  index: number;
  isAlly: (unit: StruckUnit) => boolean;
  strike: PreviewedAreaStrike;
}) {
  const policy = STRIKE_POLICIES[strike.policy];
  const enemies = strike.units.filter((unit) => !isAlly(unit));
  const allies = strike.units.filter(isAlly);
  const hpLost = (units: StruckUnit[]) =>
    units.reduce((total, unit) => total + unit.hpBefore - unit.hpAfter, 0) / 10;

  return (
    <ListItem
      description={
        <VStack gap={1}>
          <Text type="supporting">
            Center ({strike.x}, {strike.y}) · {enemies.length} enemy units lose {hpLost(enemies)} HP
            · {allies.length} allied units lose {hpLost(allies)} HP
          </Text>
          {strike.units.map((unit) => (
            <Text key={`${unit.x},${unit.y}`} type="supporting">
              {isAlly(unit) ? "Ally" : "Enemy"} {factionNames.get(unit.playerId) ?? ""}{" "}
              {unit.unitName} at ({unit.x}, {unit.y}): {formatHp(unit.hpBefore)} →{" "}
              {formatHp(unit.hpAfter)} HP
            </Text>
          ))}
        </VStack>
      }
      label={`${index + 1}. ${policy.label}`}
      startContent={
        <Center
          aria-hidden
          height={20}
          isInline
          width={20}
          xstyle={[styles.strikeSwatch, styles[policy.swatch]]}
        >
          <Text color="inherit" size="sm" type="inherit" weight="bold">
            {String(index + 1)}
          </Text>
        </Center>
      }
    />
  );
}

/** Exact HP in tenths, as the visible HP that AWBW rounds it up to. */
function formatHp(exactHp: number): number {
  return Math.ceil(exactHp / 10);
}

const styles = stylex.create({
  layout: {
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
    backgroundColor: awbrnVars.ink950,
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
  // The board takes the surface, less the strip's row in full screen.
  boardArea: {
    position: "relative",
    alignSelf: "stretch",
    flexBasis: 0,
    flexGrow: 1,
    minBlockSize: 0,
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
  placeholder: {
    position: "absolute",
    inset: 0,
    padding: spacingVars["--spacing-4"],
    backgroundColor: awbrnVars.ink950,
  },
  // The Replays empty state on the board's ink: a dashed well and a title.
  emptyWell: {
    borderColor: awbrnVars.ink600,
    borderRadius: radiusVars["--radius-container"],
    borderStyle: "dashed",
    borderWidth: "1.5px",
  },
  emptyTitle: {
    color: awbrnVars.textOnInkStrong,
    fontSize: awbrnVars.text26,
    fontStretch: "68%",
    fontWeight: 800,
    lineHeight: 1.05,
    wordSpacing: "0.1em",
  },
  rosterPanel: {
    overflow: "hidden",
  },
  // The badge on the board: the number on dark ink, in a ring of the
  // missile's color.
  strikeSwatch: {
    flexShrink: 0,
    borderWidth: 2,
    borderStyle: "solid",
    backgroundColor: awbrnVars.ink950,
    color: awbrnVars.textOnInkStrong,
    lineHeight: 1,
  },
  infantry: {
    borderColor: missileVars.infantry,
  },
  funds: {
    borderColor: missileVars.funds,
  },
  hp: {
    borderColor: awbrnVars.textOnInkStrong,
    outlineWidth: 1,
    outlineStyle: "solid",
    outlineColor: awbrnVars.ink950,
  },
  boardHud: {
    alignSelf: "stretch",
    flexShrink: 0,
    borderTopWidth: borderVars["--border-width"],
    borderTopStyle: "solid",
    borderTopColor: awbrnVars.rule,
    backgroundColor: colorVars["--color-background-surface"],
  },
  // The notice takes the strip's ground and the line it stands in for, so the
  // strip keeps its height when the notice cuts.
  noticeLine: {
    color: colorVars["--color-text-primary"],
    fontSize: typeScaleVars["--text-supporting-size"],
    lineHeight: typeScaleVars["--text-supporting-leading"],
  },
});
