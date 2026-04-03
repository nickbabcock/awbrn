import init, {
  type CanvasDisplay,
  type CanvasSize,
  type KeyboardEvent,
  type MouseButtonEvent,
  BevyApp,
} from "../wasm/awbrn_wasm";
import wasmPath from "../wasm/awbrn_wasm_bg.wasm?url";
import { proxy } from "comlink";

const initialized = init({ module_or_path: wasmPath });

export type GameDisplay = CanvasDisplay;
export type GameSize = CanvasSize;
export type GameKeyboardInput = KeyboardEvent;

export interface GamePointerInput {
  button: number;
  x: number;
  y: number;
}

export interface GamePointerPosition {
  x: number;
  y: number;
}

export interface GameInstance {
  pause: () => void;
  resume: () => void;
  resize: (size: GameSize) => void;
  handleKeyDown: (event: GameKeyboardInput) => void;
  handleKeyUp: (event: GameKeyboardInput) => void;
  handlePointerMove: (position: GamePointerPosition) => void;
  handlePointerDown: (event: GamePointerInput) => void;
  handlePointerUp: (event: GamePointerInput) => void;
  handlePointerLeave: () => void;
  handleBlur: () => void;
  newReplay: (file: File | FileSystemFileHandle) => Promise<void>;
  loadMapPreview: (mapId: number) => Promise<void>;
}

const toMouseButtonEvent = (event: GamePointerInput): MouseButtonEvent => ({
  button: event.button,
});

export const createGame = async (...args: ConstructorParameters<typeof BevyApp>) => {
  await initialized;
  const app = new BevyApp(...args);

  let animationId: number;

  function update() {
    app.update();
    animationId = requestAnimationFrame(update);
  }
  animationId = requestAnimationFrame(update);

  return proxy<GameInstance>({
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
    handlePointerMove: ({ x, y }: GamePointerPosition) => {
      app.handle_mouse_move(x, y);
    },
    handlePointerDown: (event: GamePointerInput) => {
      app.handle_mouse_move(event.x, event.y);
      app.handle_mouse_down(toMouseButtonEvent(event));
    },
    handlePointerUp: (event: GamePointerInput) => {
      app.handle_mouse_move(event.x, event.y);
      app.handle_mouse_up(toMouseButtonEvent(event));
    },
    handlePointerLeave: () => {
      app.handle_mouse_leave();
    },
    handleBlur: () => {
      app.handle_canvas_blur();
    },
    newReplay: async (file: File | FileSystemFileHandle) => {
      const fileHandle = file instanceof FileSystemFileHandle ? await file.getFile() : file;
      const fileData = await fileHandle.arrayBuffer();
      const fileBuffer = new Uint8Array(fileData);
      app.new_replay(fileBuffer);
    },
    loadMapPreview: async (mapId: number) => {
      app.preview_map(mapId);
    },
  });
};
