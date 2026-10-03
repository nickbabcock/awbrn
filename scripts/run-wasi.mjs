// Run a wasm32-wasip1 program under Node WASI, which uses V8 like a Worker.
//
// A Rust crate that depends on `awbrn-protocol` imports wasm-bindgen
// placeholder functions. The program does not call them, so this runner
// gives each one a stub that throws.
//
// Usage: node scripts/run-wasi.mjs <program.wasm> <directory> [arguments...]
// The program sees <directory> as `/w`.

import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";

const [wasmPath, directory, ...args] = process.argv.slice(2);
if (wasmPath === undefined || directory === undefined) {
  console.error("usage: node scripts/run-wasi.mjs <program.wasm> <directory> [arguments...]");
  process.exit(2);
}

const wasi = new WASI({
  version: "preview1",
  args: ["program", ...args],
  env: {},
  preopens: { "/w": directory },
});
const module = await WebAssembly.compile(readFileSync(wasmPath));
const imports = { wasi_snapshot_preview1: wasi.wasiImport };
for (const entry of WebAssembly.Module.imports(module)) {
  if (entry.module === "wasi_snapshot_preview1" || entry.kind !== "function") {
    continue;
  }
  imports[entry.module] ??= {};
  imports[entry.module][entry.name] = () => {
    throw new Error(`the program called the stub ${entry.module}.${entry.name}`);
  };
}
const instance = await WebAssembly.instantiate(module, imports);
process.exitCode = wasi.start(instance);
