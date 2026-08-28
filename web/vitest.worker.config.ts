import path from "node:path";
import {
  buildPagesASSETSBinding,
  cloudflareTest,
  readD1Migrations,
} from "@cloudflare/vitest-plugin";
import { tanstackStart } from "@tanstack/react-start/plugin/vite";
import { defineProject } from "vitest/config";

export default defineProject(async () => {
  const migrations = await readD1Migrations(path.join(import.meta.dirname, "drizzle/global"));
  const assets = await buildPagesASSETSBinding(path.join(import.meta.dirname, "../assets"));

  return {
    plugins: [
      tanstackStart(),
      cloudflareTest({
        wrangler: {
          configPath: "./wrangler.jsonc",
        },
        miniflare: {
          bindings: { TEST_MIGRATIONS: migrations },
          compatibilityFlags: ["enable_nodejs_sqlite_module"],
          serviceBindings: { ASSETS: assets },
        },
      }),
    ],
    test: {
      name: "worker",
      exclude: ["src/canvas_courier/ring_buffer.test.ts"],
      include: ["src/**/*.test.ts"],
      setupFiles: ["./test/apply-migrations.ts"],
    },
  };
});
