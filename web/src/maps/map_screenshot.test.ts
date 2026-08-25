/// <reference types="node" />

import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import map178597 from "../../../assets/maps/178597.json";
import { awbwMapDataSchema } from "#/awbw/schemas.ts";
import {
  importAwbwMapDocument,
  initSync,
  MapRenderer,
  renderSmallMapScreenshot,
} from "#/wasm/awbrn_server.js";
import { importedMapDocumentSchema } from "./map_document.ts";
import { mapScreenshotKey } from "./map_screenshot.ts";

const PNG_SIGNATURE = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

function readAsset(path: string): Uint8Array {
  return new Uint8Array(readFileSync(new URL(`../../../${path}`, import.meta.url)));
}

/** Width and height, as the PNG header states them. */
function pngSize(png: Uint8Array): { width: number; height: number } {
  const header = new DataView(png.buffer, png.byteOffset, png.byteLength);
  return { width: header.getUint32(16), height: header.getUint32(20) };
}

describe("map screenshots", () => {
  initSync({
    module: readFileSync(new URL("../wasm/awbrn_server_bg.wasm", import.meta.url)),
  });

  /** The map an import would have stored. */
  const { document } = importedMapDocumentSchema.parse(
    importAwbwMapDocument(awbwMapDataSchema.parse(map178597)),
  );

  // No MapRenderer is built here, which is what says the smallmap needs no
  // atlas.
  it("draws the smallmap without the atlases", () => {
    const png = renderSmallMapScreenshot(document);

    expect(png.subarray(0, PNG_SIGNATURE.length)).toEqual(PNG_SIGNATURE);
    // A smallmap tile is four pixels, and it has no overhang row.
    expect(pngSize(png)).toEqual({
      width: document.width * 4,
      height: document.height * 4,
    });
  });

  it("draws the full map from the atlases the renderer holds", () => {
    using renderer = new MapRenderer(
      readAsset("assets/textures/tiles.png"),
      readAsset("assets/textures/units.png"),
      readAsset("assets/textures/ui.png"),
      readAsset("assets/data/ui_atlas.json"),
    );

    const png = renderer.renderFull(document);

    expect(png.subarray(0, PNG_SIGNATURE.length)).toEqual(PNG_SIGNATURE);
    // A tile is 16px, and the top row of the picture is the terrain overhang.
    expect(pngSize(png)).toEqual({
      width: document.width * 16,
      height: (document.height + 1) * 16,
    });
  });

  it("says where a picture is kept", () => {
    expect(mapScreenshotKey("abc123", "full")).toBe("maps/img/v1/abc123/full.png");
    expect(mapScreenshotKey("abc123", "small")).toBe("maps/img/v1/abc123/small.png");
  });
});
