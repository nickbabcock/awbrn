import { cloudflare } from "@cloudflare/vite-plugin";
import { tanstackStart } from "@tanstack/react-start/plugin/vite";
import stylex from "@stylexjs/unplugin";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

const srcDir = fileURLToPath(new URL("./src", import.meta.url));

export default defineConfig({
  resolve: {
    tsconfigPaths: true,
  },
  plugins: [
    stylex.vite({
      // Temporary workaround for StyleX theme-file alias resolution.
      // Upstream: https://github.com/facebook/stylex/issues/40
      aliases: {
        "#/*": `${srcDir}/*`,
      },
      useCSSLayers: true,
      // Astryx ships its own precompiled atomic classes ("x" + hash). StyleX
      // hashes by property and value, so an app rule identical to one of them
      // would share its class name, land in the app's later layers and outrank
      // the theme's component overrides. A distinct prefix keeps the two sets
      // of classes apart.
      classNamePrefix: "a",
      dev: process.env.NODE_ENV === "development",
      runtimeInjection: false,
    }),
    cloudflare({ viteEnvironment: { name: "ssr" } }),
    tanstackStart(),
    react(),
  ],
  server: {
    fs: {
      allow: [".."],
    },
    headers: {
      "Cross-Origin-Embedder-Policy": "require-corp",
      "Cross-Origin-Opener-Policy": "same-origin",
    },
  },
});
