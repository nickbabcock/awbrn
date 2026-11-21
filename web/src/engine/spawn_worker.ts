// Kept in its own module and reached only through a client-only function.
// Vite shares worker bundles across environments and emits them into any
// build whose output contains the module that spawns the worker. If the
// server build included this module, it would carry the worker and the 25 MB
// browser wasm past Cloudflare's Worker size limit.
export function spawnGameWorker(): Worker {
  return new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
}
