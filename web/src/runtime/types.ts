import type { GameEvent } from "../game_events";

export interface RuntimeBootstrapArgs {
  container: HTMLElement;
  canvas: HTMLCanvasElement | null;
  onEvent: (event: GameEvent) => void;
}

export interface GameRuntime {
  bootstrap(args: RuntimeBootstrapArgs): Promise<void>;
  loadReplay(data: Uint8Array): Promise<void>;
  dispose(): void;
}
