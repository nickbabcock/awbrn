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
    const onDisposeReplay = vi.fn();
    const registry = new GameRuntimeRegistry(() => new TestRunner(), { onDisposeReplay });
    const replayRunner = registry.getReplayRunner();
    const previewRunner = registry.getPreviewRunner("matches-new");

    registry.syncPathname("/");
    registry.syncPathname("/matches/new");

    expect(replayRunner.dispose).toHaveBeenCalledTimes(1);
    expect(onDisposeReplay).toHaveBeenCalledTimes(1);
    expect(previewRunner.dispose).not.toHaveBeenCalled();
  });

  it("disposes only the matches-new preview runner when leaving that route", () => {
    const registry = new GameRuntimeRegistry(() => new TestRunner());
    const matchesNewRunner = registry.getPreviewRunner("matches-new");
    const lobbyRunner = registry.getPreviewRunner("match-lobby");

    registry.syncPathname("/matches/new");
    registry.syncPathname("/matches/abc123");

    expect(matchesNewRunner.dispose).toHaveBeenCalledTimes(1);
    expect(lobbyRunner.dispose).not.toHaveBeenCalled();
  });

  it("disposes only the match-lobby preview runner when leaving that route", () => {
    const registry = new GameRuntimeRegistry(() => new TestRunner());
    const matchesNewRunner = registry.getPreviewRunner("matches-new");
    const lobbyRunner = registry.getPreviewRunner("match-lobby");

    registry.syncPathname("/matches/abc123");
    registry.syncPathname("/matches");

    expect(lobbyRunner.dispose).toHaveBeenCalledTimes(1);
    expect(matchesNewRunner.dispose).not.toHaveBeenCalled();
  });
});
