import { importAwbwMapDocument, initSync } from "#/wasm/awbrn_server.js";
import type { AwbwMapDataWire, ImportedMapDocument } from "#/wasm/awbrn_server.js";
import serverWasmModule from "#/wasm/awbrn_server_bg.wasm";

initSync({ module: serverWasmModule });

export function canonicalizeAwbwMap(source: AwbwMapDataWire): ImportedMapDocument {
  return importAwbwMapDocument(source);
}
