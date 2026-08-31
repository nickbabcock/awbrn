import path from "node:path";
import {
  buildPagesASSETSBinding,
  cloudflareTest,
  readD1Migrations,
} from "@cloudflare/vitest-plugin";
import { tanstackStart } from "@tanstack/react-start/plugin/vite";
import { defineProject } from "vitest/config";

// The worker requires this secret, so a checkout with no .dev.vars (CI, for
// example) gets a test value instead of a warning about the missing one.
process.env.AUTH_SECRET ??= "test-auth-secret";

/**
 * The identity notifications are signed with, for the tests that send one.
 *
 * It is a real P-256 pair because the delivery path signs with it and a
 * placeholder would only ever produce a key import error. It reaches no push
 * service: the tests that use it intercept the request.
 */
const testVapidKeys = {
  VAPID_PUBLIC_KEY:
    "BN-xxXsX4H7ZKs7eaM9FolY_usx0VtpdFGB9LEcGX75L-vgSt4nn75A5syoL1Dccx90A9_Y7Ocq0FuXsF54bguI",
  VAPID_PRIVATE_KEY: "h44BRwsnOowZg7S4mIzhMVjZBuX4ImmVstH5bbtt8ng",
  VAPID_SUBJECT: "mailto:test@example.com",
};

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
          bindings: { TEST_MIGRATIONS: migrations, ...testVapidKeys },
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
