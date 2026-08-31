import { DurableObject } from "cloudflare:workers";
import { drizzle, DrizzleSqliteDODatabase } from "drizzle-orm/durable-sqlite";
import { migrate } from "drizzle-orm/durable-sqlite/migrator";
import { asc, eq, inArray, sql } from "drizzle-orm";
import migrations from "../../drizzle/player/migrations";
import { pendingTurnsTable, pushSubscriptionsTable } from "#/db/player.ts";
import { getRequestSession } from "#/auth/auth.server.ts";
import { requireRateLimit } from "#/rate_limit.ts";
import { sendWebPush, type PushSubscription, type VapidKeys } from "./web_push.ts";
import {
  buildTurnDigest,
  parsePlayerClientMessage,
  type PlayerNotification,
  type PlayerSocketMessage,
} from "./player_protocol.ts";

interface PlayerSocketAttachment {
  /** Whether the tab holding this socket is the one the player is looking at. */
  visible: boolean;
}

const IDENTITY_KEY = "identity";
const PUSH_FAILURES_KEY = "pushFailures";

/**
 * How long turns are collected before a notification is sent.
 *
 * Several of a player's matches often move within moments of each other, and
 * one notification naming them all is worth more than three that interrupt in
 * a row. Nothing here is real time, so the wait costs the player nothing.
 */
const PUSH_DIGEST_DELAY_MS = 10_000;

/** How many refusals in a row cost a subscription its place. */
const MAX_PUSH_FAILURES = 5;

const PUSH_RETRY_BASE_MS = 60_000;
const MAX_PUSH_RETRY_MS = 60 * 60_000;

/**
 * One player's connection to the rest of the site.
 *
 * Whose turn it is lives in each match, and a player in ten matches cannot hold
 * ten sockets open to hear about them, so every match reports here instead and
 * this object is the one thing a tab connects to. It is also what remains when
 * no tab is open: the push subscriptions live in its own storage, so a match
 * that moves while the player is away still reaches them.
 */
export class PlayerDurableObject extends DurableObject<CloudflareBindings> {
  private readonly db: DrizzleSqliteDODatabase;

  constructor(ctx: DurableObjectState, env: CloudflareBindings) {
    super(ctx, env);
    this.db = drizzle(ctx.storage);
    ctx.blockConcurrencyWhile(async () => {
      await migrate(this.db, migrations);
    });
  }

  async fetch(request: Request): Promise<Response> {
    if (request.headers.get("Upgrade") !== "websocket") {
      return new Response("Not found", { status: 404 });
    }

    const session = await getRequestSession(request);
    if (!session) {
      return new Response("Unauthorized", { status: 401 });
    }

    // The worker addresses this object by the name it derives from the session
    // it just checked. Recording who that was, and refusing anyone else, means
    // a mistake in that derivation shows up as a refusal rather than as one
    // player reading another's notifications.
    const identity = await this.ctx.storage.get<string>(IDENTITY_KEY);
    if (identity === undefined) {
      await this.ctx.storage.put(IDENTITY_KEY, session.user.id);
    } else if (identity !== session.user.id) {
      return new Response("Forbidden", { status: 403 });
    }

    try {
      await requireRateLimit(this.env.WS_UPGRADE_RATE_LIMITER, `player:${session.user.id}`, 10);
    } catch (response) {
      if (response instanceof Response) return response;
      throw response;
    }

    const { 0: client, 1: server } = new WebSocketPair();
    this.ctx.acceptWebSocket(server);
    server.serializeAttachment({ visible: true } satisfies PlayerSocketAttachment);
    // A ping is answered without waking this object, so a tab may hold the
    // socket open for days at no cost.
    this.ctx.setWebSocketAutoResponse(new WebSocketRequestResponsePair("ping", "pong"));
    sendJson(server, { type: "ready" });

    return new Response(null, { status: 101, webSocket: client });
  }

  async webSocketMessage(ws: WebSocket, message: string | ArrayBuffer): Promise<void> {
    let parsed: unknown;
    try {
      const text = typeof message === "string" ? message : new TextDecoder().decode(message);
      parsed = JSON.parse(text);
    } catch {
      return;
    }

    const client = parsePlayerClientMessage(parsed);
    if (client === null) {
      return;
    }
    ws.serializeAttachment({ visible: client.visible } satisfies PlayerSocketAttachment);
  }

  async webSocketError(_ws: WebSocket, error: unknown): Promise<void> {
    console.error("WebSocket error in player DO:", error);
  }

  /**
   * Tell this player that a match has moved.
   *
   * Every open tab hears it either way, because a tab showing a stale count is
   * the thing this replaced polling to avoid. Whether the player is *told* is
   * the separate question: a turn that opened while no tab is being read is
   * collected for a notification, and one that merely closed never is.
   */
  async notify(notification: PlayerNotification): Promise<void> {
    this.broadcast(notification);

    if (notification.type !== "turnStarted" || this.hasVisibleTab()) {
      return;
    }

    // Collecting a turn nothing could ever carry would wake this object ten
    // seconds later only to throw the turn away, once for every turn of every
    // match, in a deployment that has no notifications configured at all.
    if (!this.canDeliverPush()) {
      return;
    }

    this.db
      .insert(pendingTurnsTable)
      .values({
        matchId: notification.matchId,
        matchName: notification.matchName,
        deadlineAt: notification.deadlineAt === null ? null : new Date(notification.deadlineAt),
        queuedAt: new Date(),
      })
      .onConflictDoUpdate({
        target: pendingTurnsTable.matchId,
        set: {
          matchName: notification.matchName,
          deadlineAt: notification.deadlineAt === null ? null : new Date(notification.deadlineAt),
        },
      })
      .run();

    // An alarm already set is one this turn joins rather than replaces, which
    // is what makes the wait a collecting window instead of a rolling delay.
    if ((await this.ctx.storage.getAlarm()) === null) {
      await this.ctx.storage.setAlarm(Date.now() + PUSH_DIGEST_DELAY_MS);
    }
  }

  /** Send whatever turns have collected, and retry the ones that would not go. */
  async alarm(): Promise<void> {
    const pending = this.db
      .select()
      .from(pendingTurnsTable)
      .orderBy(asc(pendingTurnsTable.queuedAt))
      .all();
    if (pending.length === 0) {
      return;
    }

    const batch = pending.map((turn) => turn.matchId);

    // A player who came back while the turns were collecting has already read
    // them on the socket, so the notification is dropped rather than sent late.
    if (this.hasVisibleTab()) {
      this.clearPending(batch);
      await this.ctx.storage.delete(PUSH_FAILURES_KEY);
      return;
    }

    // A browser that dropped its subscription, or a key withdrawn, between the
    // turn being collected and this alarm. Holding these would announce a turn
    // long since played the next time a browser subscribed.
    const keys = this.vapidKeys();
    if (keys === null || !this.canDeliverPush()) {
      this.clearPending(batch);
      await this.ctx.storage.delete(PUSH_FAILURES_KEY);
      return;
    }
    const subscriptions = this.db.select().from(pushSubscriptionsTable).all();

    const payload = buildTurnDigest(
      pending.map((turn) => ({ matchId: turn.matchId, matchName: turn.matchName })),
    );

    const results = await Promise.all(
      subscriptions.map(async (subscription) => {
        try {
          return await sendWebPush(subscription as PushSubscription, payload, keys);
        } catch (error) {
          console.error("Failed to reach a push service:", error);
          return { ok: false, status: 0, isGone: false };
        }
      }),
    );

    let delivered = false;
    for (const [index, result] of results.entries()) {
      const subscription = subscriptions[index]!;
      if (result.ok) {
        delivered = true;
        if (subscription.failureCount !== 0) {
          this.db
            .update(pushSubscriptionsTable)
            .set({ failureCount: 0 })
            .where(eq(pushSubscriptionsTable.endpoint, subscription.endpoint))
            .run();
        }
        continue;
      }

      // The push service reporting a subscription gone is the browser having
      // dropped it, which is an ordinary end and not a failure to retry.
      if (result.isGone || subscription.failureCount + 1 >= MAX_PUSH_FAILURES) {
        this.db
          .delete(pushSubscriptionsTable)
          .where(eq(pushSubscriptionsTable.endpoint, subscription.endpoint))
          .run();
        continue;
      }

      this.db
        .update(pushSubscriptionsTable)
        .set({ failureCount: sql`${pushSubscriptionsTable.failureCount} + 1` })
        .where(eq(pushSubscriptionsTable.endpoint, subscription.endpoint))
        .run();
    }

    // Reaching one of the player's browsers is reaching the player, so the
    // batch is done even if another browser has to be tried again later. What
    // is retried is that browser's next notification, never this one, because
    // a turn held for a slow device is announced after it has been played.
    if (delivered || this.db.select().from(pushSubscriptionsTable).all().length === 0) {
      this.clearPending(batch);
      await this.ctx.storage.delete(PUSH_FAILURES_KEY);
      return;
    }

    // Nothing got through and something is still worth trying, so the turns
    // stay and the next attempt waits longer than the last.
    const failures = ((await this.ctx.storage.get<number>(PUSH_FAILURES_KEY)) ?? 0) + 1;
    await this.ctx.storage.put(PUSH_FAILURES_KEY, failures);
    const delay = Math.min(PUSH_RETRY_BASE_MS * 2 ** (failures - 1), MAX_PUSH_RETRY_MS);
    await this.ctx.storage.setAlarm(Date.now() + delay);
  }

  /** Record a browser the player has allowed notifications on. */
  async addPushSubscription(
    subscription: PushSubscription & { label?: string | null },
  ): Promise<void> {
    this.db
      .insert(pushSubscriptionsTable)
      .values({
        endpoint: subscription.endpoint,
        p256dh: subscription.p256dh,
        auth: subscription.auth,
        label: subscription.label ?? null,
        createdAt: new Date(),
        failureCount: 0,
      })
      .onConflictDoUpdate({
        target: pushSubscriptionsTable.endpoint,
        // A browser that subscribes again has rotated its keys, and the row is
        // the same browser, so the keys are replaced and its failures forgiven.
        set: {
          p256dh: subscription.p256dh,
          auth: subscription.auth,
          label: subscription.label ?? null,
          failureCount: 0,
        },
      })
      .run();
  }

  /** Forget a browser, because the player turned notifications off on it. */
  async removePushSubscription(endpoint: string): Promise<void> {
    this.db
      .delete(pushSubscriptionsTable)
      .where(eq(pushSubscriptionsTable.endpoint, endpoint))
      .run();
  }

  /** Whether this player has any browser that notifications can reach. */
  async hasPushSubscription(): Promise<boolean> {
    return this.db.select().from(pushSubscriptionsTable).all().length > 0;
  }

  /**
   * Forget the turns that were just announced, and only those.
   *
   * A send is awaited, and a match that moves during it reaches this object
   * and collects a turn of its own. Naming the batch is what stops that turn
   * being thrown away with the ones it arrived behind, which would lose it
   * with nothing left to announce it.
   */
  private clearPending(matchIds: string[]): void {
    if (matchIds.length === 0) {
      return;
    }
    this.db.delete(pendingTurnsTable).where(inArray(pendingTurnsTable.matchId, matchIds)).run();
  }

  private broadcast(message: PlayerSocketMessage): void {
    for (const socket of this.ctx.getWebSockets()) {
      try {
        sendJson(socket, message);
      } catch {
        // Ignore closed connections.
      }
    }
  }

  /** True while at least one of the player's tabs is in front of them. */
  private hasVisibleTab(): boolean {
    return this.ctx.getWebSockets().some((socket) => {
      const attachment = socket.deserializeAttachment() as PlayerSocketAttachment | null;
      return attachment?.visible === true;
    });
  }

  /**
   * Whether a notification sent now could reach anything.
   *
   * It takes both a browser that has asked for them and a key to sign with,
   * and the same answer decides whether a turn is worth collecting and whether
   * what was collected is still worth sending. Asking it in one place is what
   * keeps those two from disagreeing.
   */
  private canDeliverPush(): boolean {
    if (this.vapidKeys() === null) {
      return false;
    }
    return this.db.select().from(pushSubscriptionsTable).all().length > 0;
  }

  /**
   * The sender identity notifications are signed with, or null without one.
   *
   * A deployment with no keys configured still runs: turns reach every open
   * tab, and only the notifications to a player who has closed them are given
   * up. That is what lets development run without the keys.
   */
  private vapidKeys(): VapidKeys | null {
    const publicKey = this.env.VAPID_PUBLIC_KEY;
    const privateKey = this.env.VAPID_PRIVATE_KEY;
    const subject = this.env.VAPID_SUBJECT;
    if (!publicKey || !privateKey || !subject) {
      return null;
    }
    return { publicKey, privateKey, subject };
  }
}

function sendJson(ws: WebSocket, message: PlayerSocketMessage): void {
  ws.send(JSON.stringify(message));
}
