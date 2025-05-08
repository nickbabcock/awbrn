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
    },
    handleKeyDown: (...args: Parameters<BevyApp["handle_key_down"]>) => {
      app.handle_key_down(...args);
    },
    handleKeyUp: (...args: Parameters<BevyApp["handle_key_up"]>) => {
      app.handle_key_up(...args);
    },
  });
};
