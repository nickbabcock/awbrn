import { importAwbwMapDocument, initSync } from "#/wasm/awbrn_server.js";
import type { AwbwMapDataWire, ImportedMapDocument } from "#/wasm/awbrn_server.js";
import serverWasmModule from "#/wasm/awbrn_server_bg.wasm";

let initialized = false;

export function ensureServerWasmInitialized(
  module: WebAssembly.Module | BufferSource = serverWasmModule,
): void {
  if (initialized) return;
  initSync({ module });
  initialized = true;
}

export function canonicalizeAwbwMap(source: AwbwMapDataWire): ImportedMapDocument {
  ensureServerWasmInitialized();
  return importAwbwMapDocument(source);
}
