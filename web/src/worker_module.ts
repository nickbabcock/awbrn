import init, { BevyApp } from "./wasm/awbrn_wasm";
import wasmPath from "./wasm/awbrn_wasm_bg.wasm?url";
import { proxy } from "comlink";

const initialized = init({ module_or_path: wasmPath });

export const createGame = async (
  ...args: ConstructorParameters<typeof BevyApp>
) => {
  await initialized;
  const app = new BevyApp(...args);

  let animationId: number;

  function update() {
    app.update();
    animationId = requestAnimationFrame(update);
  }
  animationId = requestAnimationFrame(update);

  return proxy({
    pause: () => {
      console.log("Pausing game");
      cancelAnimationFrame(animationId);
    },
    resume: () => {
      console.log("Resuming game");
      cancelAnimationFrame(animationId);
      update();
    },
    resize: (...args: Parameters<BevyApp["resize"]>) => {
      app.resize(...args);
    },
    handleKeyDown: (...args: Parameters<BevyApp["handle_key_down"]>) => {
      app.handle_key_down(...args);
    },
    handleKeyUp: (...args: Parameters<BevyApp["handle_key_up"]>) => {
      app.handle_key_up(...args);
    },
    newReplay: async (file: File | FileSystemFileHandle) => {
      const fileHandle =
        file instanceof FileSystemFileHandle ? await file.getFile() : file;
      const fileData = await fileHandle.arrayBuffer();
      const fileBuffer = new Uint8Array(fileData);
      app.new_replay(fileBuffer);
    },
  });
};
