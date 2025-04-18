import init, { BevyApp } from "awbrn-wasm";
import wasmPath from "awbrn-wasm/awbrn_wasm_bg.wasm?url";
import { proxy } from "comlink";

const initialized = init({ module_or_path: wasmPath });

export const createGame = async (
  ...args: ConstructorParameters<typeof BevyApp>
) => {
  await initialized;
  const app = new BevyApp(...args);

  function update() {
    app.update();
    requestAnimationFrame(update);
  }
  requestAnimationFrame(update);

  return proxy({
    resize: (...args: Parameters<BevyApp["resize"]>) => {
      app.resize(...args);
    }
  })
};
