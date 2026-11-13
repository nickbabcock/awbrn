import { describe, expect, it, vi } from "vitest";

const manifest = vi.hoisted(() => ({ loaded: false }));

vi.mock("#/engine/asset_manifest.ts", () => {
  manifest.loaded = true;
  return {};
});

describe("co_portraits", () => {
  // Server-rendered pages show portraits, and the engine's asset manifest is
  // browser-only, so the portraits must not pull it in.
  it("does not load the engine asset manifest", async () => {
    const { DEFAULT_CO_PORTRAIT_KEY, getCoPortrait } = await import("./co_portraits");

    expect(getCoPortrait(DEFAULT_CO_PORTRAIT_KEY)?.sheetUrl).toEqual(expect.any(String));
    expect(manifest.loaded).toBe(false);
  });
});
