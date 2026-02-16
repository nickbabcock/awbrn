/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly TAURI_ENV_PLATFORM?: string;
  readonly TAURI_ENV_DEBUG?: string;
  readonly VITE_APP_RUNTIME?: "desktop" | "web";
}
