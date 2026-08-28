import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    projects: ["./vitest.worker.config.ts", "./vitest.node.config.ts"],
  },
});
