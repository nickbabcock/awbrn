import { describe, expect, it } from "vitest";
import map61748 from "../../../assets/maps/61748.json";
import { awbwMapDataSchema } from "#/awbw/schemas.ts";
import { aiProfiles, importAwbwMapDocument, initSync, WasmMatch } from "#/wasm/awbrn_server.js";
import type { StoredActionEvent } from "#/wasm/awbrn_server.js";
import serverWasmModule from "#/wasm/awbrn_server_bg.wasm";

initSync({ module: serverWasmModule });

/** Turns played, both seats counted, as in the native test. */
const TURNS = 16;

/**
 * The digest of the game, which `crates/awbrn-server/tests/recorded_game.rs`
 * pins for the native build. Change both together.
 */
const DIGEST = "98488491ef8f823d";

/** Compact JSON with sorted keys, the bytes serde_json writes for a `Value`. */
function canonicalJson(value: unknown): string {
  if (Array.isArray(value)) {
    return `[${value.map(canonicalJson).join(",")}]`;
  }
  if (value !== null && typeof value === "object") {
    const object = value as Record<string, unknown>;
    const keys = Object.keys(object).sort();
    return `{${keys.map((key) => `${JSON.stringify(key)}:${canonicalJson(object[key])}`).join(",")}}`;
  }
  // A Rust `None` arrives as `undefined` and serializes as `null`.
  return JSON.stringify(value ?? null);
}

/** The 64-bit FNV-1a hash of the UTF-8 bytes of `text`, in hex. */
function fnv1a(text: string): string {
  let hash = 0xcbf29ce484222325n;
  for (const byte of new TextEncoder().encode(text)) {
    hash = BigInt.asUintN(64, (hash ^ BigInt(byte)) * 0x100000001b3n);
  }
  return hash.toString(16).padStart(16, "0");
}

/**
 * The AI is measured on native builds, and the wasm build under test takes
 * `ln`, `exp`, `asinh` and `powf` from another libm.
 */
describe("the Hard opponent in the wasm build under test", () => {
  it("plays the game the native build plays", { timeout: 300_000 }, () => {
    // The engine's own Hard tier, which is what the native test seats.
    const hard = aiProfiles().profiles.find((profile) => profile.tier === "hard");
    expect(hard).toBeDefined();
    const { document } = importAwbwMapDocument(awbwMapDataSchema.parse(map61748));
    const match = new WasmMatch({
      map: document,
      players: [1, 2].map((factionId) => ({
        factionId,
        team: null,
        startingFunds: 0,
        coId: 1,
        aiProfileId: hard!.id,
      })),
      fogEnabled: false,
      startingFunds: 0,
      rngSeed: 1,
    });

    const events: StoredActionEvent[] = [];
    for (let turn = 0; turn < TURNS; turn += 1) {
      const slot = match.pendingAiSlot();
      if (slot === undefined) {
        break;
      }
      events.push(...match.runAiTurn(slot).actions.map((action) => action.storedActionEvent));
    }

    expect(fnv1a(canonicalJson(events))).toBe(DIGEST);
  });
});
