import { defineConfig } from "drizzle-kit";

export default defineConfig({
  schema: "./src/db/match.ts",
  out: "./drizzle/match",
  dialect: "sqlite",
});
