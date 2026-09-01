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

  async loadReplay(file: File | FileSystemFileHandle): Promise<void> {
    const game = await this.requireGame();
    await game.newReplay(file);
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

  dispose(): void {
    this.surfaceVersion += 1;
    this.activeSurface = undefined;
    this.battleCatalogPromise = undefined;
    this.liveCommandHandler = undefined;
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
