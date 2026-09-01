import { describe, expect, it } from "vitest";
import map178597 from "../../../assets/maps/178597.json";
import { awbwMapDataSchema } from "#/awbw/schemas.ts";
import { importAwbwMapDocument, initSync, WasmMatch } from "#/wasm/awbrn_server.js";
import serverWasmModule from "#/wasm/awbrn_server_bg.wasm";

initSync({ module: serverWasmModule });

describe("AI match action responses", () => {
  it("includes the human recipient when the server plays a turn", () => {
    const { document } = importAwbwMapDocument(awbwMapDataSchema.parse(map178597));
    const match = new WasmMatch({
      map: document,
      players: [
        { factionId: 1, team: null, startingFunds: 10_000, coId: 1 },
        {
          factionId: 2,
          team: null,
          startingFunds: 10_000,
          coId: 2,
          aiProfileId: "ai-easy-v1",
        },
      ],
      fogEnabled: false,
      startingFunds: 10_000,
      rngSeed: 1,
    });

    match.process_action(0, { type: "endTurn" });
    const aiTurn = match.runAiTurn(1);

    expect(aiTurn.actions.length).toBeGreaterThan(0);
    for (const response of aiTurn.actions) {
      expect(response.playerMessagesBySlot).not.toBeInstanceOf(Map);
      expect(response.playerMessagesBySlot["0"]).toMatchObject({
        type: "playerUpdate",
      });
    }
  });
});
