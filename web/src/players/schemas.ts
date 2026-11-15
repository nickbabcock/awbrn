import { z } from "zod";
import { matchHistoryRequestSchema, type MatchClock, type RankedPool } from "#/matches/schemas.ts";

/** Base64url, as the browser writes the keys it hands out. */
const base64UrlSchema = z
  .string()
  .min(1)
  .max(512)
  .regex(/^[A-Za-z0-9_-]+$/, "expected a base64url value");

/**
 * A browser's push subscription, as the site is willing to store it.
 *
 * The endpoint is checked to be an `https` URL because it is a location this
 * site later makes a request to, and a subscription is supplied by the page
 * rather than read from anywhere trusted.
 */
export const pushSubscriptionSchema = z.object({
  endpoint: z
    .url()
    .max(2048)
    .refine((value) => value.startsWith("https://"), "a push endpoint must be https"),
  p256dh: base64UrlSchema,
  auth: base64UrlSchema,
  label: z.string().trim().min(1).max(80).nullable().default(null),
});

export type PushSubscriptionInput = z.infer<typeof pushSubscriptionSchema>;

export const pushUnsubscribeSchema = z.object({
  endpoint: z.url().max(2048),
});

/** A user id as the profile address carries it. */
export const playerIdSchema = z.string().min(1).max(64);

export const playerProfileRequestSchema = z.object({ userId: playerIdSchema });

export const playerHistoryRequestSchema = matchHistoryRequestSchema.extend({
  userId: playerIdSchema,
});

/** One season, as the profile's time axis reads it. Times are epoch ms. */
export interface ProfileSeason {
  number: number;
  startsAt: number;
  endsAt: number;
}

/** Where the player stood in one pool at one season's close, or now for the open season. */
export interface ProfileSeasonPlace {
  season: number;
  /** Rated matches that season. */
  games: number;
  /** Null when the player held no place: no standings row, or off the ladder now. */
  place: number | null;
  rating: number | null;
  isProvisional: boolean;
}

/** One ranked pool the player holds a rating in. */
export interface ProfilePool {
  pool: RankedPool;
  rating: number;
  isProvisional: boolean;
  /** The live ladder place, or null when the rating is too old to hold one. */
  place: number | null;
  /** The first rated match: when, and the rating it started from. */
  start: { at: number; rating: number };
  /** The rating after each rated match (`[at, rating]`), thinned to what a plot can draw. */
  points: [number, number][];
  /** The highest rating the player reached. */
  peak: { at: number; rating: number };
  /** One entry per season from the pool's first rated match to the open season. */
  seasons: ProfileSeasonPlace[];
}

/** A commander the player picks most, with the record they hold with it. */
export interface ProfileCo {
  coId: number;
  /** The army they field it with most, which is the field its stamp stands on. */
  factionId: number;
  wins: number;
  losses: number;
  draws: number;
}

export interface ProfileSeat {
  slotIndex: number;
  /** Null for a seat the server plays, and for an opponent a pairing still withholds. */
  userId: string | null;
  userName: string;
  factionId: number;
  coId: number | null;
  isHidden: boolean;
}

/** A match the player is in now, as someone else may see it listed. */
export interface ProfileOngoingMatch {
  matchId: string;
  name: string;
  mapId: string;
  mapRevision: number;
  mapName: string;
  pool: RankedPool | null;
  clock: MatchClock;
  /** When the clock that matters runs out (the open turn, or a pairing's window). */
  deadlineAt: number | null;
  /** The player's own profile, and the match waits on them. */
  isOwed: boolean;
  /** A ranked pairing the player still has to confirm. Only on their own profile. */
  isOwedPairing: boolean;
  opponents: ProfileSeat[];
}

export interface PlayerProfile {
  userId: string;
  name: string;
  joinedAt: number;
  isViewer: boolean;
  /** The open season, or null between seasons. */
  currentSeason: number | null;
  /** From the season of the first rated match in any pool to the open season. */
  seasons: ProfileSeason[];
  pools: ProfilePool[];
  cos: ProfileCo[];
  ongoing: ProfileOngoingMatch[];
  loadedAt: number;
}
