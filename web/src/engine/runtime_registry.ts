import { GameRunner } from "./game_runner";

export type PreviewRunnerScope = "match-lobby" | "matches-new";

interface RunnerLike {
  dispose(): void;
}

interface RuntimeRegistryOptions {
  onDisposeReplay?: () => void;
}

const MATCH_LOBBY_PATH_PATTERN = /^\/matches\/[^/]+$/;

function isReplayPath(pathname: string): boolean {
  return pathname === "/";
}

function isMatchesNewPath(pathname: string): boolean {
  return pathname === "/matches/new";
}

function isMatchLobbyPath(pathname: string): boolean {
  return pathname !== "/matches/new" && MATCH_LOBBY_PATH_PATTERN.test(pathname);
}

export class GameRuntimeRegistry<TRunner extends RunnerLike = GameRunner> {
  private currentPathname: string | undefined;
  private previewRunners = new Map<PreviewRunnerScope, TRunner>();
  private replayRunner: TRunner | undefined;

  constructor(
    private readonly createRunner: () => TRunner = () => new GameRunner() as unknown as TRunner,
    private readonly options: RuntimeRegistryOptions = {},
  ) {}

  getPreviewRunner(scope: PreviewRunnerScope): TRunner {
    let runner = this.previewRunners.get(scope);
    if (!runner) {
      runner = this.createRunner();
      this.previewRunners.set(scope, runner);
    }

    return runner;
  }

  getReplayRunner(): TRunner {
    this.replayRunner ??= this.createRunner();
    return this.replayRunner;
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

    if (isMatchesNewPath(previousPathname) && !isMatchesNewPath(pathname)) {
      this.disposePreviewRunner("matches-new");
    }

    if (isMatchLobbyPath(previousPathname) && !isMatchLobbyPath(pathname)) {
      this.disposePreviewRunner("match-lobby");
    }
  }

  disposeAll(): void {
    this.disposeReplayRunner();

    for (const scope of this.previewRunners.keys()) {
      this.disposePreviewRunner(scope);
    }
  }

  private disposePreviewRunner(scope: PreviewRunnerScope): void {
    const runner = this.previewRunners.get(scope);
    if (!runner) {
      return;
    }

    this.previewRunners.delete(scope);
    runner.dispose();
  }

  private disposeReplayRunner(): void {
    if (!this.replayRunner) {
      return;
    }

    this.replayRunner.dispose();
    this.replayRunner = undefined;
    this.options.onDisposeReplay?.();
  }
}
