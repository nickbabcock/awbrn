import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { GameEvent } from "../game_events";
import type { GameRuntime, RuntimeBootstrapArgs } from "./types";

export class TauriRuntime implements GameRuntime {
  private abortController = new AbortController();
  private unlistenGameEvent: UnlistenFn | undefined;
  private resizeObserver: ResizeObserver | undefined;
  private queuedCursor: { x: number; y: number } | null = null;
  private cursorRafId = 0;
  private pressedKeys = new Set<string>();

  async bootstrap({ container, onEvent }: RuntimeBootstrapArgs): Promise<void> {
    this.unlistenGameEvent = await listen<GameEvent>("game-event", (event) => {
      onEvent(event.payload);
    });

    await this.sendWindowMetrics(container);

    this.resizeObserver = new ResizeObserver(() => {
      void this.sendWindowMetrics(container);
    });
    this.resizeObserver.observe(container);

    window.addEventListener(
      "resize",
      () => {
        void this.sendWindowMetrics(container);
      },
      { signal: this.abortController.signal },
    );

    const queueCursor = (x: number, y: number) => {
      this.queuedCursor = { x, y };
      if (this.cursorRafId !== 0) {
        return;
      }

      this.cursorRafId = requestAnimationFrame(() => {
        this.cursorRafId = 0;
        if (!this.queuedCursor) {
          return;
        }

        const { x: cursorX, y: cursorY } = this.queuedCursor;
        this.queuedCursor = null;
        void invoke<void>("interaction_cursor_moved", {
          x: cursorX,
          y: cursorY,
        });
      });
    };

    window.addEventListener(
      "mousemove",
      (event) => {
        queueCursor(event.clientX, event.clientY);
      },
      { signal: this.abortController.signal },
    );

    window.addEventListener(
      "mousedown",
      (event) => {
        if (isInteractiveTarget(event.target)) {
          return;
        }

        const button = normalizeMouseButton(event.button);
        if (button === null) {
          return;
        }

        void invoke<void>("interaction_mouse_button", {
          button,
          pressed: true,
        });
      },
      { signal: this.abortController.signal },
    );

    window.addEventListener(
      "mouseup",
      (event) => {
        const button = normalizeMouseButton(event.button);
        if (button === null) {
          return;
        }

        void invoke<void>("interaction_mouse_button", {
          button,
          pressed: false,
        });
      },
      { signal: this.abortController.signal },
    );

    window.addEventListener(
      "wheel",
      (event) => {
        if (isInteractiveTarget(event.target)) {
          return;
        }

        const lines = wheelEventToLines(event);
        if (Math.abs(lines) < Number.EPSILON) {
          return;
        }

        void invoke<void>("interaction_mouse_wheel", { lines });
        event.preventDefault();
      },
      { signal: this.abortController.signal, passive: false },
    );

    window.addEventListener(
      "keydown",
      (event) => {
        if (event.repeat || this.pressedKeys.has(event.code)) {
          return;
        }

        this.pressedKeys.add(event.code);
        void invoke<void>("interaction_key", {
          code: event.code,
          pressed: true,
        });
      },
      { signal: this.abortController.signal },
    );

    window.addEventListener(
      "keyup",
      (event) => {
        if (!this.pressedKeys.delete(event.code)) {
          return;
        }

        void invoke<void>("interaction_key", {
          code: event.code,
          pressed: false,
        });
      },
      { signal: this.abortController.signal },
    );

    window.addEventListener(
      "contextmenu",
      (event) => {
        event.preventDefault();
      },
      { signal: this.abortController.signal },
    );

    window.addEventListener("blur", () => this.releaseAllInputs(), {
      signal: this.abortController.signal,
    });
  }

  async loadReplay(data: Uint8Array): Promise<void> {
    await invoke<void>("new_replay", { data: Array.from(data) });
  }

  dispose(): void {
    this.abortController.abort();
    this.abortController = new AbortController();

    if (this.cursorRafId !== 0) {
      cancelAnimationFrame(this.cursorRafId);
      this.cursorRafId = 0;
    }

    this.queuedCursor = null;
    this.resizeObserver?.disconnect();
    this.resizeObserver = undefined;

    this.releaseAllInputs();

    if (this.unlistenGameEvent) {
      void this.unlistenGameEvent();
      this.unlistenGameEvent = undefined;
    }
  }

  private async sendWindowMetrics(container: HTMLElement): Promise<void> {
    const bounds = container.getBoundingClientRect();

    await invoke<void>("set_window_metrics", {
      width: Math.max(1, bounds.width),
      height: Math.max(1, bounds.height),
      scaleFactor: window.devicePixelRatio,
    });
  }

  private releaseAllInputs(): void {
    this.queuedCursor = null;

    void invoke<void>("interaction_mouse_button", {
      button: 0,
      pressed: false,
    });
    void invoke<void>("interaction_mouse_button", {
      button: 1,
      pressed: false,
    });
    void invoke<void>("interaction_mouse_button", {
      button: 2,
      pressed: false,
    });

    for (const code of this.pressedKeys) {
      void invoke<void>("interaction_key", { code, pressed: false });
    }

    this.pressedKeys.clear();
  }
}

function normalizeMouseButton(button: number): number | null {
  return button >= 0 && button <= 2 ? button : null;
}

function wheelEventToLines(event: WheelEvent): number {
  if (Math.abs(event.deltaY) < Number.EPSILON) {
    return 0;
  }

  const zoomDelta = event.deltaY > 0 ? 0.9 : 1.1;
  if (zoomDelta > 1.0) {
    return Math.log(zoomDelta) / Math.log(1.1);
  }

  return -Math.log(1.0 / zoomDelta) / Math.log(1.1);
}

function isInteractiveTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) {
    return false;
  }

  return target.closest("[data-map-input-stop='true']") !== null;
}
