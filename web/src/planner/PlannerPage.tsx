import { Badge } from "@astryxdesign/core/Badge";
import { Banner } from "@astryxdesign/core/Banner";
import { Card } from "@astryxdesign/core/Card";
import { Center } from "@astryxdesign/core/Center";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { FileInput } from "@astryxdesign/core/FileInput";
import { Grid } from "@astryxdesign/core/Grid";
import { useMediaQuery } from "@astryxdesign/core/hooks";
import { List, ListItem } from "@astryxdesign/core/List";
import { MetadataList, MetadataListItem } from "@astryxdesign/core/MetadataList";
import { RadioList, RadioListItem } from "@astryxdesign/core/RadioList";
import { Selector } from "@astryxdesign/core/Selector";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Switch } from "@astryxdesign/core/Switch";
import { Text } from "@astryxdesign/core/Text";
import { TextInput } from "@astryxdesign/core/TextInput";
import { borderVars, colorVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getAwbwGameFn } from "#/awbw/awbw.functions.ts";
import type { AwbwGameSnapshot } from "#/awbw/schemas.ts";
import { parseAwbwGameId } from "#/awbw/parsers.ts";
import { useCanvasCourierSurface } from "#/canvas_courier/index.ts";
import { coRoster } from "#/co_roster.ts";
import { loadCoPortraitCatalog } from "#/components/co_portraits.ts";
import { BoardFullscreenExit, GameFullscreenButton } from "#/components/GameFullscreen.tsx";
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
import { Button } from "#/ui/Button.tsx";
import { Page, PageHeader } from "#/ui/Page.tsx";
import { rosterLayout } from "#/ui/rosterLayout.stylex.ts";
import {
  DEFAULT_SEAT,
  NO_CO_ID,
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

/** Below this width, the board menus open as a sheet on the bottom edge. */
const SHEET_MEDIA = "(max-width: 767px)";

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
  /** A game read before its planner file was chosen, to apply to the file. */
  const [pendingGame, setPendingGame] = useState<AwbwGameSnapshot | null>(null);
  const [firstPlayerId, setFirstPlayerId] = useState("");
  /** The army whose missiles the board previews, or "" for none. */
  const [previewedPlayerId, setPreviewedPlayerId] = useState("");
  const [seats, setSeats] = useState<Record<number, SeatInput>>({});
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
    setError(null);
    setPlanner(null);
    setFirstPlayerId("");
    setPreviewedPlayerId("");
    setSeats({});
    setFilledFrom(null);
    if (!file) return;

    try {
      const data = new Uint8Array(await file.arrayBuffer());
      const summary = await runner.readMovePlanner(data);
      setPlanner({ data, summary });
      if (pendingGame) {
        applyGame(pendingGame, summary, {});
        setPendingGame(null);
      } else if (gameLink.trim() === "" && summary.gameId) {
        // The file names the game that it was saved from, which is most often
        // the game to fill the setup from.
        setGameLink(`https://awbw.amarriner.com/game.php?games_id=${summary.gameId}`);
      }
    } catch (cause) {
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
    setFilledFrom(
      game.armies.some((army) => army.funds === null)
        ? `Filled from game ${game.gameId}. Fog hides some funds, so enter them.`
        : `Filled from game ${game.gameId}.`,
    );
  }

  async function fillFromAwbwGame() {
    if (gameId === null) return;
    setError(null);
    setIsFillingFromGame(true);
    try {
      const game = await readGame(gameId);
      if (planner) {
        applyGame(game, planner.summary, seats);
      } else {
        setPendingGame(game);
        setFilledFrom(`Read game ${gameId}. Choose a move planner file from it to fill the setup.`);
      }
    } catch (cause) {
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

  return (
    <Page width="full">
      <VStack gap={6}>
        <PageHeader
          description="Load an AWBW move planner file and play the turn out with the actions of the game: move, attack, capture, build, and activate CO powers. Every edit can be undone. The file stays on your device."
          title="Move planner"
        />

        {error ? <Banner description={error} status="error" title="Planner error" /> : null}

        <Grid align="start" gap={4} xstyle={styles.layout}>
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
              <VStack ref={canvasContainerRef} gap={0} width="100%" height="100%" />
              {isFullscreen ? (
                <BoardFullscreenExit mode={fullscreenMode} onExit={exitFullscreen} />
              ) : null}
              {!isBoardRequested ? (
                <Center height="100%" width="100%" xstyle={styles.placeholder}>
                  <EmptyState
                    description="Choose a planner file, its map, each army's CO, and the army that plays first."
                    headingLevel={3}
                    isCompact
                    title="No board yet"
                  />
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
              <HStack align="center" gap={2} wrap="wrap">
                {playerRoster ? <Badge label={`Day ${playerRoster.day}`} variant="info" /> : null}
                <Text type="supporting">
                  {planner && isBoardRequested
                    ? `${planner.summary.name || "Planner"} · Map ${parsedMapId}`
                    : "No planner loaded"}
                </Text>
                {activeArmy ? (
                  <Text type="supporting">{activeArmy.displayFactionName} to move</Text>
                ) : null}
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
                {isBoardRequested && !isFullscreen ? (
                  <GameFullscreenButton onEnter={enterFullscreen} />
                ) : null}
              </HStack>
            </HStack>
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
                <Banner description={areaStrikes.error} status="error" title="No board" />
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
                      <Text type="label">{previewedName} missiles, in firing order</Text>
                      <Text type="supporting">
                        {isPreviewedTurn
                          ? `Where the missiles land if ${previewedName} starts the super power now.`
                          : `Where the missiles would land if ${previewedName} started the super power now. They aim again after each order, so plan this turn around them.`}{" "}
                        Each missile removes 3 HP from every unit within 2 tiles of its center, and
                        leaves at least 0.1 HP.
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
                    description="Fills the map, each army's CO, power meter and team, and the army to move. The planner file does not record them."
                    label="AWBW game"
                    onChange={(link) => {
                      setGameLink(link);
                      setFilledFrom(null);
                      setPendingGame(null);
                    }}
                    onEnter={() => void fillFromAwbwGame()}
                    placeholder="https://awbw.amarriner.com/game.php?games_id=…"
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
                    size="sm"
                    variant="secondary"
                  />
                  {filledFrom ? <Text type="supporting">{filledFrom}</Text> : null}
                </VStack>
                <FileInput
                  accept=".json"
                  changeAction={handlePlannerFileChange}
                  description="The JSON file that the AWBW move planner saves"
                  label="Move planner file"
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
                        description="The planner does not record its map."
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
                      description="This army plays first. The others follow in turn order."
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
                    <VStack gap={1}>
                      <Text type="label">Armies</Text>
                      <Text type="supporting">
                        The planner does not record COs, funds, or power meters.
                      </Text>
                    </VStack>
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

/** A star count, such as 3 or 2.5. */
function isStarCount(value: string): boolean {
  return /^\d+(\.\d+)?$/.test(value.trim());
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
        <Text type="supporting">These edits change the board outside the rules, for any army.</Text>
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
    imageRendering: "pixelated",
  },
  gameCanvasHidden: {
    visibility: "hidden",
  },
  placeholder: {
    position: "absolute",
    inset: 0,
    padding: spacingVars["--spacing-4"],
    backgroundColor: colorVars["--color-background-muted"],
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
    backgroundColor: "#16181d",
    color: "#ffffff",
    lineHeight: 1,
  },
  infantry: {
    borderColor: "#ffe14d",
  },
  funds: {
    borderColor: "#ff2fd1",
  },
  hp: {
    borderColor: "#ffffff",
    outlineWidth: 1,
    outlineStyle: "solid",
    outlineColor: "#16181d",
  },
  boardHud: {
    borderTopWidth: borderVars["--border-width"],
    borderTopStyle: "solid",
    borderTopColor: colorVars["--color-border-emphasized"],
    backgroundColor: colorVars["--color-background-surface"],
  },
});
