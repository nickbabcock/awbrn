import { sql } from "drizzle-orm";
import { integer, sqliteTable, text } from "drizzle-orm/sqlite-core";

/**
 * The browsers a player has allowed notifications on.
 *
 * A player reads on more than one device, and each one subscribes separately,
 * so a notification goes to every row here. The push service is the authority
 * on whether a row is still good: it reports a subscription the browser has
 * dropped, and that row is deleted rather than retried.
 */
export const pushSubscriptionsTable = sqliteTable("push_subscriptions", {
  endpoint: text("endpoint").primaryKey(),
  p256dh: text("p256dh").notNull(),
  auth: text("auth").notNull(),
  /** How the player named this browser, for a page that lists them. */
  label: text("label"),
  createdAt: integer("createdAt", { mode: "timestamp" })
    .notNull()
    .default(sql`(unixepoch())`),
  /**
   * Failed deliveries in a row. A push service that is having a bad minute is
   * retried; one that keeps refusing eventually costs the subscription.
   */
  failureCount: integer("failureCount").notNull().default(0),
});

/**
 * The turns waiting to be announced to a player who is not watching.
 *
 * A player whose matches all move at once should hear once, so turns are
 * collected here and an alarm sends them as a single notification. The row is
 * keyed by match, which is what stops a match that moves twice before the
 * alarm from being announced twice.
 */
export const pendingTurnsTable = sqliteTable("pending_turns", {
  matchId: text("matchId").primaryKey(),
  matchName: text("matchName").notNull(),
  deadlineAt: integer("deadlineAt", { mode: "timestamp" }),
  queuedAt: integer("queuedAt", { mode: "timestamp" })
    .notNull()
    .default(sql`(unixepoch())`),
});
