import { describe, expect, it } from "vitest";
import { aiProfiles, initSync } from "#/wasm/awbrn_server.js";
import serverWasmModule from "#/wasm/awbrn_server_bg.wasm";
import { aiProfileDisplay, aiProfileDisplays, aiSeatName } from "./ai_profiles.ts";
import { aiProfileIds } from "./schemas.ts";

initSync({ module: serverWasmModule });

describe("the opponent roster", () => {
  /**
   * The engine decides who the opponents are. Everything on this side is a
   * copy made so a menu, a zod schema and a check constraint can read the
   * roster without loading a wasm module, and a copy is only worth having
   * while it is the same as the original.
   */
  it("is the engine's roster, written out", () => {
    expect(aiProfileDisplays).toEqual(aiProfiles().profiles);
  });

  it("stores identifiers the database will accept", () => {
    expect(aiProfileIds).toEqual(aiProfiles().profiles.map((profile) => profile.id));
  });

  it("names every seat something a person can read", () => {
    expect(aiSeatName("ai-easy-v1")).toBe("Easy CPU");
    expect(aiSeatName("ai-hard-v1")).toBe("Hard CPU");
  });

  it("does not resolve an opponent it has no profile for", () => {
    expect(aiProfileDisplay("ai-nonesuch")).toBeNull();
    expect(aiProfileDisplay(null)).toBeNull();
  });
});
