import { defineConfig } from "drizzle-kit";

export default defineConfig({
  schema: "./src/db/player.ts",
  out: "./drizzle/player",
  dialect: "sqlite",
  driver: "durable-sqlite",
});
