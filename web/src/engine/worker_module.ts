import init, { type CanvasDisplay, BevyApp } from "#/wasm/awbrn_wasm.js";
import wasmPath from "#/wasm/awbrn_wasm_bg.wasm?url";
import { proxy } from "comlink";
import {
  SharedCanvasEventAction,
  SharedCanvasEventType,
  SharedCanvasInputReader,
  SharedCanvasPointerKind,
  SharedCanvasWheelDeltaMode,
  type SharedCanvasInputConfig,
} from "#/canvas_courier/index.ts";

const initialized = init({ module_or_path: wasmPath });

export type GameDisplay = CanvasDisplay;

export interface GameInstance {
  newReplay: (file: File | FileSystemFileHandle) => Promise<void>;
  loadMapPreview: (mapId: number) => Promise<void>;
  setPlayerDisplayFaction: (playerId: number, factionId: number | null) => Promise<void>;
}

class WorkerInputBridge {
  private readonly reader: SharedCanvasInputReader;
  private paused = false;

  constructor(
    config: SharedCanvasInputConfig,
    private readonly app: BevyApp,
  ) {
    this.reader = new SharedCanvasInputReader(config);
  }

  drain() {
    this.reader.drain((event) => {
      switch (event.type) {
        case SharedCanvasEventType.Keyboard:
          if (event.action === SharedCanvasEventAction.Down) {
            this.app.handle_key_down_code(event.keyCode, event.repeat);
            return;
          }

          this.app.handle_key_up_code(event.keyCode, event.repeat);
          return;
        case SharedCanvasEventType.Pointer:
          if (event.pointerKind === SharedCanvasPointerKind.Touch) {
            switch (event.action) {
              case SharedCanvasEventAction.Move:
                this.app.handle_touch_move(event.pointerId, event.x, event.y);
                return;
              case SharedCanvasEventAction.Down:
                this.app.handle_touch_start(event.pointerId, event.x, event.y);
                return;
              case SharedCanvasEventAction.Up:
                this.app.handle_touch_end(event.pointerId, event.x, event.y);
                return;
              case SharedCanvasEventAction.Leave:
                this.app.handle_touch_cancel(event.pointerId, event.x, event.y);
                return;
              default:
                return;
            }
          }

          switch (event.action) {
            case SharedCanvasEventAction.Move:
              this.app.handle_mouse_move(event.x, event.y);
              return;
            case SharedCanvasEventAction.Down:
              this.app.handle_mouse_move(event.x, event.y);
              this.app.handle_mouse_down({ button: event.button });
              return;
            case SharedCanvasEventAction.Up:
              this.app.handle_mouse_move(event.x, event.y);
              this.app.handle_mouse_up({ button: event.button });
              return;
            case SharedCanvasEventAction.Leave:
              this.app.handle_mouse_leave();
              return;
            default:
              return;
          }
        case SharedCanvasEventType.Wheel:
          this.app.handle_mouse_wheel(
            event.deltaX,
            event.deltaY,
            event.deltaMode !== SharedCanvasWheelDeltaMode.Pixel,
          );
          return;
        case SharedCanvasEventType.FocusChange:
          if (event.action === SharedCanvasEventAction.Blur) {
            this.app.handle_canvas_blur();
          }
          return;
        case SharedCanvasEventType.Resize:
          this.app.resize({
            width: event.width,
            height: event.height,
            scaleFactor: event.scaleFactor,
          });
          return;
        case SharedCanvasEventType.Visibility:
          this.paused = event.action === SharedCanvasEventAction.Hidden;
          return;
        default:
          return;
      }
    });
  }

  shouldUpdate() {
    return !this.paused;
  }
}

export const createGame = async (
  canvas: OffscreenCanvas,
  display: CanvasDisplay,
  assetConfig: ConstructorParameters<typeof BevyApp>[2],
  inputConfig: SharedCanvasInputConfig,
  eventCallback: ConstructorParameters<typeof BevyApp>[3],
) => {
  await initialized;
  const app = new BevyApp(canvas, display, assetConfig, eventCallback);
  const inputBridge = new WorkerInputBridge(inputConfig, app);

  function update() {
    inputBridge.drain();
    if (inputBridge.shouldUpdate()) {
      app.update();
    }
    requestAnimationFrame(update);
  }
  requestAnimationFrame(update);

  return proxy<GameInstance>({
    newReplay: async (file: File | FileSystemFileHandle) => {
      const fileHandle = file instanceof FileSystemFileHandle ? await file.getFile() : file;
      const fileData = await fileHandle.arrayBuffer();
      const fileBuffer = new Uint8Array(fileData);
      app.new_replay(fileBuffer);
    },
    loadMapPreview: async (mapId: number) => {
      app.preview_map(mapId);
    },
    setPlayerDisplayFaction: async (playerId: number, factionId: number | null) => {
      app.set_player_display_faction(playerId, factionId);
    },
  });
};
