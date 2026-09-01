import { describe, expect, it, vi } from "vitest";
import type { AwbrnMapDocument } from "#/maps/map_document.ts";
import type { ObservedTransition } from "#/wasm/awbrn_server.js";
import { GameRunner } from "./game_runner.ts";

describe("GameRunner live transitions", () => {
  it("applies updates received during the live baseline after the baseline", async () => {
    const game = {
      applyLiveTransition: vi.fn().mockResolvedValue(undefined),
      loadLiveMatch: vi.fn().mockResolvedValue(undefined),
    };
    const runner = new GameRunner();
    const internals = runner as unknown as {
      game: typeof game;
    };
    internals.game = game;

    const first = {} as ObservedTransition;
    const second = {} as ObservedTransition;
    await runner.applyLiveTransition(first);
    await runner.applyLiveTransition(second);
    await runner.loadLiveMatch({} as AwbrnMapDocument, [], {});

    expect(game.loadLiveMatch).toHaveBeenCalledOnce();
    expect(game.applyLiveTransition).toHaveBeenNthCalledWith(1, first);
    expect(game.applyLiveTransition).toHaveBeenNthCalledWith(2, second);
    runner.dispose();
  });
});
