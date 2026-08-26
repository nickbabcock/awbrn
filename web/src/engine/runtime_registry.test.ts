import { describe, expect, it, vi } from "vitest";
import { GameRuntimeRegistry } from "./runtime_registry";

class TestRunner {
  dispose = vi.fn();
}

describe("GameRuntimeRegistry", () => {
  it("keeps the replay runner alive across same-route syncs", () => {
    const registry = new GameRuntimeRegistry(() => new TestRunner());
    const runner = registry.getReplayRunner();

    registry.syncPathname("/");
    registry.syncPathname("/");

    expect(registry.getReplayRunner()).toBe(runner);
    expect(runner.dispose).not.toHaveBeenCalled();
  });

  it("disposes only the replay runner when leaving the home route", () => {
    const onDisposeGameState = vi.fn();
    const registry = new GameRuntimeRegistry(() => new TestRunner(), { onDisposeGameState });
    const replayRunner = registry.getReplayRunner();
    const activeRunner = registry.getActiveMatchRunner();

    registry.syncPathname("/");
    registry.syncPathname("/matches/new");

    expect(replayRunner.dispose).toHaveBeenCalledTimes(1);
    expect(onDisposeGameState).toHaveBeenCalledTimes(1);
    expect(activeRunner.dispose).not.toHaveBeenCalled();
  });

  // The lobby draws the map from the picture the import rendered, so no runner
  // is started for it and the match route is the only one that holds one.
  it("keeps the active match runner while moving between match routes", () => {
    const registry = new GameRuntimeRegistry(() => new TestRunner());
    const activeRunner = registry.getActiveMatchRunner();

    registry.syncPathname("/matches/new");
    registry.syncPathname("/matches/abc123");

    expect(activeRunner.dispose).not.toHaveBeenCalled();
    expect(registry.getActiveMatchRunner()).toBe(activeRunner);
  });

  it("disposes the active match runner when leaving a match route", () => {
    const onDisposeGameState = vi.fn();
    const registry = new GameRuntimeRegistry(() => new TestRunner(), { onDisposeGameState });
    const activeRunner = registry.getActiveMatchRunner();

    registry.syncPathname("/matches/abc123");
    registry.syncPathname("/matches");

    expect(activeRunner.dispose).toHaveBeenCalledTimes(1);
    // The roster outlives the runner unless it is cleared with it, so the next
    // match would open showing the previous match's armies.
    expect(onDisposeGameState).toHaveBeenCalledTimes(1);
  });
});
