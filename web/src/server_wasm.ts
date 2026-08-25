import { env } from "cloudflare:workers";
import {
  importAwbwMapDocument,
  initSync,
  MapRenderer,
  renderSmallMapScreenshot,
} from "#/wasm/awbrn_server.js";
import type {
  AwbrnMapDocumentWire,
  AwbwMapDataWire,
  ImportedMapDocument,
} from "#/wasm/awbrn_server.js";
import serverWasmModule from "#/wasm/awbrn_server_bg.wasm";
import uiAtlasUrl from "../../assets/data/ui_atlas.json?url";
import tilesTextureUrl from "../../assets/textures/tiles.png?url";
import uiTextureUrl from "../../assets/textures/ui.png?url";
import unitsTextureUrl from "../../assets/textures/units.png?url";

initSync({ module: serverWasmModule });

export function canonicalizeAwbwMap(source: AwbwMapDataWire): ImportedMapDocument {
  return importAwbwMapDocument(source);
}

/**
 * Draw a map at its starting position, at sprite size.
 *
 * The renderer that holds the decoded atlases is built the first time a map is
 * drawn and then kept, so an isolate reads and decodes them once however many
 * maps it goes on to draw.
 */
export async function renderFullMapScreenshotPng(
  document: AwbrnMapDocumentWire,
): Promise<Uint8Array> {
  return (await mapRenderer()).renderFull(document);
}

/**
 * Draw a map as a smallmap: four pixels for each tile, terrain only.
 *
 * It draws from a fixed palette, so it reads no atlas and needs nothing
 * loaded first.
 */
export function renderSmallMapScreenshotPng(document: AwbrnMapDocumentWire): Uint8Array {
  return renderSmallMapScreenshot(document);
}

let renderer: Promise<MapRenderer> | null = null;

function mapRenderer(): Promise<MapRenderer> {
  // A failed load is not remembered: the next map to be drawn tries again.
  renderer ??= buildRenderer().catch((error: unknown) => {
    renderer = null;
    throw error;
  });
  return renderer;
}

async function buildRenderer(): Promise<MapRenderer> {
  const [tiles, units, ui, uiAtlas] = await Promise.all([
    readAsset(tilesTextureUrl),
    readAsset(unitsTextureUrl),
    readAsset(uiTextureUrl),
    readAsset(uiAtlasUrl),
  ]);
  return new MapRenderer(tiles, units, ui, uiAtlas);
}

async function readAsset(url: string): Promise<Uint8Array> {
  // The binding wants an absolute URL and reads only the path from it. The
  // host has to be one the development server accepts, because there this
  // binding is that server, and it refuses a host it does not serve.
  const response = await env.ASSETS.fetch(new URL(url, "http://localhost"));
  if (!response.ok) {
    throw new Error(`the renderer could not read ${url} (${response.status})`);
  }

  return new Uint8Array(await response.arrayBuffer());
}
