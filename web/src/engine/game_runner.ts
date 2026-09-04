import { proxy, transfer, wrap } from "comlink";
import {
  CanvasCourierTransport,
  type CanvasCourierController,
  type CanvasCourierSurface,
  type CanvasSize,
} from "#/canvas_courier/index.ts";
import type { AwbrnMapDocument } from "#/maps/map_document.ts";
import type { ObservedTransition } from "#/wasm/awbrn_server.js";
import type {
  BattleCatalog,
  BattleForecastResponse,
  BattleRequestWire,
  DeleteUnitCommandRequested,
  EndTurnRequested,
  GameEvent,
  MoveCommandRequested,
  UnloadCommandRequested,
} from "#/wasm/awbrn_wasm.js";
import type { PlayerCommand } from "#/matches/match_protocol.ts";
import type { LiveMatchPlayer } from "./worker_module";
import { gameAssetConfig } from "./asset_manifest";
import { useGameStore } from "./store";
import type { GameWorker } from "./worker_types";

type GameInstance = Awaited<ReturnType<GameWorker["createGame"]>>;

export interface GameSurface extends CanvasCourierSurface {}

export class GameRunner implements CanvasCourierController {
  private activeSurface: GameSurface | undefined;
  private battleCatalogPromise: Promise<BattleCatalog> | undefined;
  private createGamePromise: Promise<GameInstance> | undefined;
  private game: GameInstance | undefined;
  private pendingLiveTransitions: ObservedTransition[] = [];
  /** Keep live updates behind the first match snapshot. */
  private liveBaselinePending = true;
  private liveCommandHandler: ((command: PlayerCommand) => void) | undefined;
  private endTurnRequestHandler: ((request: EndTurnRequested) => void) | undefined;
  private rawWorker: Worker | undefined;
  private surfaceVersion = 0;
  private readonly transport = new CanvasCourierTransport();
  private transferredCanvas: HTMLCanvasElement | undefined;
  private worker: GameWorker | undefined;

  attachSurface(surface: GameSurface): void {
    if (this.activeSurface?.canvas === surface.canvas) {
      this.activeSurface = surface;
      return;
    }

    const version = ++this.surfaceVersion;
    this.activeSurface = surface;

    const measuredSize = this.transport.measureSurface(surface);
    this.prepareCanvasForAttachment(surface, measuredSize);
    this.transport.attachSurface(surface);

    void this.ensureGame(surface, measuredSize).catch((error) => {
      if (version === this.surfaceVersion) {
        console.error("GameRunner failed to initialize:", error);
      }
    });
  }

  /**
   * Put an archive on the board.
   *
   * `isCurrent` is asked again once the game is ready, because the wait for it
   * is long enough for the page to have moved on. One runner serves every
   * match a viewer walks through, so an archive that arrives after the viewer
   * has left would be loaded over the one they are reading now.
   */
  async loadReplay(
    source: File | FileSystemFileHandle | Uint8Array,
    isCurrent?: () => boolean,
  ): Promise<void> {
    const game = await this.requireGame();
    if (isCurrent && !isCurrent()) return;
    await game.newReplay(source);
  }

  /**
   * Watch a loaded archive through one seat's eyes, or through none.
   *
   * `playerId` is the seat and `followsActivePlayer` puts the view on whoever
   * holds the turn. What each seat could see is already held beside the board,
   * so a change here re-selects a projection rather than replaying anything.
   */
  async setReplayViewpoint(playerId: number | null, followsActivePlayer: boolean): Promise<void> {
    const game = await this.requireGame();
    await game.setReplayViewpoint(playerId, followsActivePlayer);
  }

  async loadMapPreview(mapId: number): Promise<void> {
    const game = await this.requireGame();
    await game.loadMapPreview(mapId);
  }

  async loadMatchMap(map: AwbrnMapDocument): Promise<void> {
    this.liveBaselinePending = true;
    const game = await this.requireGame();
    await game.loadMatchMap(map);
    await this.applyPendingLiveTransitions(game);
    this.liveBaselinePending = false;
  }

  async loadLiveMatch(
    map: AwbrnMapDocument,
    players: LiveMatchPlayer[],
    observation: unknown,
  ): Promise<void> {
    this.liveBaselinePending = true;
    const game = await this.requireGame();
    await game.loadLiveMatch(map, players, observation);
    await this.applyPendingLiveTransitions(game);
    this.liveBaselinePending = false;
  }

  async applyLiveTransition(transition: ObservedTransition): Promise<void> {
    if (this.liveBaselinePending || (!this.game && !this.createGamePromise)) {
      this.pendingLiveTransitions.push(transition);
      return;
    }
    const game = await this.requireGame();
    await game.applyLiveTransition(transition);
  }

  async setPlayerDisplayFaction(playerId: number, factionId: number | null): Promise<void> {
    const game = await this.requireGame();
    await game.setPlayerDisplayFaction(playerId, factionId);
  }

  /**
   * Score a hypothetical engagement.
   *
   * It goes to the worker rather than to the game, so it answers whether or not
   * a board has loaded and never waits on one. The worker is the only place the
   * engine lives, and asking the rules from anywhere else would mean a second
   * copy of them in the browser.
   */
  async forecastBattle(request: BattleRequestWire): Promise<BattleForecastResponse> {
    return this.getWorker().forecastBattle(request);
  }

  /** The units, terrain and commanders the rules define. Fetched once. */
  loadBattleCatalog(): Promise<BattleCatalog> {
    this.battleCatalogPromise ??= this.getWorker()
      .loadBattleCatalog()
      .catch((error) => {
        this.battleCatalogPromise = undefined;
        throw error;
      });
    return this.battleCatalogPromise;
  }

  setLiveCommandHandler(handler: ((command: PlayerCommand) => void) | undefined): void {
    this.liveCommandHandler = handler;
  }

  /**
   * Who is asked when the board asks to end the turn.
   *
   * The board never ends one. It asks, and the page decides whether the
   * question needs putting to the player first.
   */
  setEndTurnRequestHandler(handler: ((request: EndTurnRequested) => void) | undefined): void {
    this.endTurnRequestHandler = handler;
  }

  dispose(): void {
    this.surfaceVersion += 1;
    this.activeSurface = undefined;
    this.battleCatalogPromise = undefined;
    this.liveCommandHandler = undefined;
    this.endTurnRequestHandler = undefined;
    this.transport.dispose();
    this.game = undefined;
    this.pendingLiveTransitions = [];
    this.liveBaselinePending = true;
    this.createGamePromise = undefined;
    this.transferredCanvas = undefined;
    this.worker = undefined;
    this.rawWorker?.terminate();
    this.rawWorker = undefined;
  }

  private async ensureGame(surface: GameSurface, size: CanvasSize): Promise<GameInstance> {
    if (this.game) {
      return this.game;
    }

    if (!this.createGamePromise) {
      this.assertCanvasTransferable(surface.canvas);
      this.transferredCanvas = surface.canvas;

      this.createGamePromise = this.getWorker()
        .createGame(
          transfer(surface.offscreen, [surface.offscreen]),
          size,
          gameAssetConfig,
          this.transport.inputConfig,
          proxy((event: GameEvent) => {
            this.handleGameEvent(event);
          }),
        )
        .then((game) => {
          this.game = game;
          return game;
        })
        .catch((error) => {
          this.createGamePromise = undefined;
          throw error;
        });
    }

    return this.createGamePromise;
  }

  private async applyPendingLiveTransitions(game: GameInstance): Promise<void> {
    while (this.pendingLiveTransitions.length > 0) {
      const pendingTransitions = this.pendingLiveTransitions;
      this.pendingLiveTransitions = [];
      for (const transition of pendingTransitions) {
        await game.applyLiveTransition(transition);
      }
    }
  }

  private handleGameEvent(event: GameEvent): void {
    switch (event.type) {
      case "NewDay": {
        useGameStore.getState().actions.setCurrentDay(event.day);
        break;
      }
      case "MapDimensions": {
        break;
      }
      case "ReplayLoaded": {
        useGameStore.getState().actions.setReplayPosition(null);
        // A new archive is watched from outside it until somebody picks a
        // seat, and the seat the last archive was watched from is not one this
        // archive need have.
        useGameStore.getState().actions.setReplayViewpoint(null);
        break;
      }
      case "ReplayViewpointChanged": {
        useGameStore.getState().actions.setReplayViewpoint(event);
        break;
      }
      case "ReplayPositionChanged": {
        useGameStore.getState().actions.setReplayPosition(event);
        break;
      }
      case "PlayerRosterUpdated": {
        useGameStore.getState().actions.setPlayerRoster(event);
        useGameStore.getState().actions.setCurrentDay(event.day);
        break;
      }
      case "ProductionOptionsChanged": {
        useGameStore
          .getState()
          .actions.setProductionOptions(event.site === undefined ? null : event);
        break;
      }
      case "UnitInspectionChanged": {
        useGameStore.getState().actions.setInspectedUnit(event.unit ?? null);
        break;
      }
      case "TileHoverChanged": {
        useGameStore.getState().actions.setHoveredTile(event.tile ?? null);
        break;
      }
      case "MoveCommandRequested": {
        this.handleMoveCommandRequest(event);
        break;
      }
      case "UnloadCommandRequested": {
        this.handleUnloadCommandRequest(event);
        break;
      }
      case "DeleteUnitCommandRequested": {
        this.handleDeleteUnitCommandRequest(event);
        break;
      }
      case "AttackPreviewChanged": {
        useGameStore
          .getState()
          .actions.setAttackPreview(event.forecast === undefined ? null : event);
        break;
      }
      case "TurnReadinessChanged": {
        useGameStore.getState().actions.setTurnReadiness(event);
        break;
      }
      case "EndTurnRequested": {
        this.endTurnRequestHandler?.(event);
        break;
      }
      case "UnitActionsChanged": {
        useGameStore
          .getState()
          .actions.setUnitActions(event.destination === undefined ? null : event);
        break;
      }
      default: {
        break;
      }
    }
  }

  /**
   * The engine decided; the browser only carries it. The action arrives in the
   * server's own `PostMoveAction` shape, so nothing is reinterpreted here.
   */
  private handleMoveCommandRequest(request: MoveCommandRequested): void {
    if (!this.liveCommandHandler) {
      console.error("GameRunner received a move command without a live command handler");
      return;
    }
    this.liveCommandHandler({
      type: "moveUnit",
      unit_id: request.unitId,
      path: request.path,
      action: request.action,
    });
  }

  private handleUnloadCommandRequest(request: UnloadCommandRequested): void {
    if (!this.liveCommandHandler) {
      console.error("GameRunner received an unload command without a live command handler");
      return;
    }
    this.liveCommandHandler({
      type: "unload",
      transport_id: request.transportId,
      cargo_id: request.cargoId,
      position: request.position,
    });
  }

  private handleDeleteUnitCommandRequest(request: DeleteUnitCommandRequested): void {
    if (!this.liveCommandHandler) {
      console.error("GameRunner received a delete-unit command without a live command handler");
      return;
    }
    this.liveCommandHandler({
      type: "deleteUnit",
      unit_id: request.unitId,
    });
  }

  /** Send the order at this index on the menu the engine last offered. */
  async chooseUnitAction(index: number): Promise<void> {
    const game = await this.requireGame();
    await game.chooseUnitAction(index);
  }

  /**
   * Read an earlier moment of the match instead of playing on it.
   *
   * The board stops taking orders and the live match waits at the edge, so
   * what the viewer is reading is not written over while they read it.
   * Nothing moves until a position arrives through
   * {@link applyReviewState}: only the host can say what an earlier board
   * looked like to this viewer.
   */
  async enterBoardReview(): Promise<void> {
    const game = await this.requireGame();
    await game.enterBoardReview();
  }

  /** Come back to the match as it stands, catching up on what it did. */
  async exitBoardReview(): Promise<void> {
    const game = await this.requireGame();
    await game.exitBoardReview();
  }

  /** Show a position the host answered a review request with. */
  async applyReviewState(transition: unknown): Promise<void> {
    const game = await this.requireGame();
    await game.applyReviewState(transition);
  }

  /** Step through the loaded archive by actions. */
  async replayStep(delta: number): Promise<void> {
    const game = await this.requireGame();
    await game.replayStep(delta);
  }

  /** Step through the loaded archive by whole turns. */
  async replayStepTurn(delta: number): Promise<void> {
    const game = await this.requireGame();
    await game.replayStepTurn(delta);
  }

  /** Stand at one boundary of the loaded archive. */
  async replaySeek(index: number): Promise<void> {
    const game = await this.requireGame();
    await game.replaySeek(index);
  }

  /** Stand at the end of the loaded archive. */
  async replaySeekEnd(): Promise<void> {
    const game = await this.requireGame();
    await game.replaySeekEnd();
  }

  /** Dismiss the destination menu, stepping back to the selected unit. */
  async dismissUnitAction(): Promise<void> {
    const game = await this.requireGame();
    await game.dismissUnitAction();
  }

  /**
   * Put the board back after the server refused the command that was sent, so
   * the player adjusts and retries rather than starting the move again.
   */
  async rejectPendingCommand(): Promise<void> {
    const game = await this.requireGame();
    await game.rejectPendingCommand();
  }

  private prepareCanvasForAttachment(surface: GameSurface, size: CanvasSize): void {
    if (this.transferredCanvas === undefined) {
      this.applyInitialCanvasSize(surface.offscreen, size);
      return;
    }

    this.assertCanvasTransferable(surface.canvas);
  }

  private assertCanvasTransferable(canvas: HTMLCanvasElement): void {
    if (this.transferredCanvas && this.transferredCanvas !== canvas) {
      throw new Error(
        "GameRunner cannot attach a different canvas after transferring to OffscreenCanvas.",
      );
    }
  }

  private applyInitialCanvasSize(offscreen: OffscreenCanvas, size: CanvasSize): void {
    offscreen.width = size.width;
    offscreen.height = size.height;
  }

  private async requireGame(): Promise<GameInstance> {
    if (this.game) {
      return this.game;
    }

    if (this.createGamePromise) {
      return this.createGamePromise;
    }

    throw new Error("GameRunner is not initialized yet.");
  }

  private getWorker(): GameWorker {
    if (!this.worker) {
      this.rawWorker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
      this.worker = wrap<GameWorker>(this.rawWorker);
    }

    return this.worker;
  }
}
