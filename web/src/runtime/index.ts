import { isTauri } from "@tauri-apps/api/core";
import type { GameRuntime } from "./types";

type RuntimeFlavor = "desktop" | "web";
const FORCED_RUNTIME = import.meta.env.VITE_APP_RUNTIME;

function runtimeFlavor(): RuntimeFlavor | null {
  if (FORCED_RUNTIME === "desktop" || FORCED_RUNTIME === "web") {
    return FORCED_RUNTIME;
  }

  return null;
}

export function isDesktopRuntime(): boolean {
  const flavor = runtimeFlavor();
  if (flavor) {
    return flavor === "desktop";
  }

  return Boolean(import.meta.env.TAURI_ENV_PLATFORM) && isTauri();
}

export async function createRuntime(): Promise<GameRuntime> {
  const flavor = runtimeFlavor();
  if (flavor === "desktop") {
    const { TauriRuntime } = await import("./tauri");
    return new TauriRuntime();
  }

  if (flavor === "web") {
    const { WebRuntime } = await import("./web");
    return new WebRuntime();
  }

  if (isDesktopRuntime()) {
    const { TauriRuntime } = await import("./tauri");
    return new TauriRuntime();
  }

  const { WebRuntime } = await import("./web");
  return new WebRuntime();
}

export type { GameRuntime } from "./types";
