import { integer, sqliteTable, text } from "drizzle-orm/sqlite-core";

export const matchEventsTable = sqliteTable("events", {
  seq: integer("seq").primaryKey({ autoIncrement: true }),
  kind: text("kind").notNull(),
  payload: text("payload", { mode: "json" }).notNull(),
  createdAt: integer("createdAt", { mode: "timestamp" }).notNull(),
});
