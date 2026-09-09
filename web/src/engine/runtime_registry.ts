import { GameRunner } from "./game_runner";

interface RunnerLike {
  dispose(): void;
}

interface RuntimeRegistryOptions {
  /**
   * Called when a runner that reports game state goes away. The roster and day
   * live outside the runner, so they have to be cleared with it; otherwise the
   * next board shows the previous one's armies until the engine reports.
   */
  onDisposeGameState?: () => void;
}

const MATCH_LOBBY_PATH_PATTERN = /^\/matches\/[^/]+$/;
const MAP_EDITOR_PATH_PATTERN = /^\/maps\/(new$|[^/]+\/edit$)/;

function isReplayPath(pathname: string): boolean {
  return pathname === "/";
}

function isMatchLobbyPath(pathname: string): boolean {
  return pathname !== "/matches/new" && MATCH_LOBBY_PATH_PATTERN.test(pathname);
}

/**
 * The two addresses the editor is open at: a blank board, and a map's own edit
 * screen. Both hold one runner, so moving between them keeps the engine warm
 * and forking a map does not pay for a second one.
 */
function isMapEditorPath(pathname: string): boolean {
  return MAP_EDITOR_PATH_PATTERN.test(pathname);
}

export class GameRuntimeRegistry<TRunner extends RunnerLike = GameRunner> {
  private activeMatchRunner: TRunner | undefined;
  private currentPathname: string | undefined;
  private mapEditorRunner: TRunner | undefined;
  private replayRunner: TRunner | undefined;

  constructor(
    private readonly createRunner: () => TRunner = () => new GameRunner() as unknown as TRunner,
    private readonly options: RuntimeRegistryOptions = {},
  ) {}

  getReplayRunner(): TRunner {
    this.replayRunner ??= this.createRunner();
    return this.replayRunner;
  }

  getActiveMatchRunner(): TRunner {
    this.activeMatchRunner ??= this.createRunner();
    return this.activeMatchRunner;
  }

  getMapEditorRunner(): TRunner {
    this.mapEditorRunner ??= this.createRunner();
    return this.mapEditorRunner;
  }

  syncPathname(pathname: string): void {
    const previousPathname = this.currentPathname;
    this.currentPathname = pathname;

    if (!previousPathname || previousPathname === pathname) {
      return;
    }

    if (isReplayPath(previousPathname) && !isReplayPath(pathname)) {
      this.disposeReplayRunner();
    }

    if (isMatchLobbyPath(previousPathname) && !isMatchLobbyPath(pathname)) {
      this.disposeActiveMatchRunner();
    }

    if (isMapEditorPath(previousPathname) && !isMapEditorPath(pathname)) {
      this.disposeMapEditorRunner();
    }
  }

  disposeAll(): void {
    this.disposeActiveMatchRunner();
    this.disposeMapEditorRunner();
    this.disposeReplayRunner();
  }

  private disposeMapEditorRunner(): void {
    if (!this.mapEditorRunner) {
      return;
    }

    this.mapEditorRunner.dispose();
    this.mapEditorRunner = undefined;
    this.options.onDisposeGameState?.();
  }

  private disposeReplayRunner(): void {
    if (!this.replayRunner) {
      return;
    }

    this.replayRunner.dispose();
    this.replayRunner = undefined;
    this.options.onDisposeGameState?.();
  }

  private disposeActiveMatchRunner(): void {
    if (!this.activeMatchRunner) {
      return;
    }

    this.activeMatchRunner.dispose();
    this.activeMatchRunner = undefined;
    this.options.onDisposeGameState?.();
  }
}
