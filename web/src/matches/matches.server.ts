import { env } from "cloudflare:workers";
import { coDisplayName } from "#/co_roster.ts";
import { matchSettingsSchema } from "./schemas";
import type {
  MatchBrowseRequest,
  MatchBrowseResponse,
  MatchBrowseSummary,
  MatchCreateRequest,
  MatchCreateResponse,
  MatchMutationRequest,
  MatchMutationResponse,
  MatchParticipantSnapshot,
  MatchPhase,
  MatchSettings,
  MatchSetup,
  MatchSnapshot,
  MatchHistoryEntry,
  MatchHistoryRequest,
  MatchHistoryResponse,
  MatchHistorySeat,
  MyMatchesResponse,
  MyMatchSummary,
} from "./schemas";
import { groupMyMatchRows, ONGOING_MATCH_PHASES } from "./my_matches";
import {
  COMPLETED_MATCH_PHASE,
  decodeMatchHistoryCursor,
  encodeMatchHistoryCursor,
  MATCH_HISTORY_PAGE_SIZE,
} from "./match_history";
import { matchReplaysExist } from "./replay_archive";
import {
  MATCH_BROWSE_PAGE_SIZE,
  decodeMatchBrowseCursor,
  encodeMatchBrowseCursor,
} from "./match_browse";
import type { AwbrnMapDocument } from "#/maps/map_document.ts";
import { findCatalogEntry, loadMapRevision } from "#/maps/maps.server.ts";
import { err, ok, type MatchResult } from "./match_protocol";
import { generateMatchId } from "./match_id";
import { getMatchStub } from "./match_service";
import { drizzle } from "drizzle-orm/d1";
import {
  and,
  asc,
  count,
  desc,
  eq,
  exists,
  gt,
  inArray,
  isNotNull,
  lt,
  or,
  sql,
} from "drizzle-orm";
import {
  matches,
  matchParticipants,
  matchResults,
  matchVoids,
  mapSources,
  moderationActions,
  user,
} from "#/db/global.ts";
import type { Actor } from "#/auth/actor.ts";
import { moderationEntry } from "#/moderation/moderation.server.ts";
import { matchViewAnyGrant } from "./match_authz";

const db = drizzle(env.DB, {
  schema: {
    matches,
    matchParticipants,
    matchResults,
    matchVoids,
    mapSources,
    moderationActions,
    user,
  },
});

const PUBLIC_MATCH_PHASE: MatchPhase = "lobby";
const STARTING_MATCH_PHASE: MatchPhase = "starting";
const ACTIVE_MATCH_PHASE: MatchPhase = "active";

interface MatchViewer {
  id: string;
  name: string;
}

type MatchActionDiagnostics =
  | "notFound"
  | "notLobby"
  | "invalidSlot"
  | "privateJoinRequired"
  | "slotTaken"
  | "hotseatDisabled";

type MatchRow = Awaited<ReturnType<typeof queryMatchRow>>;
type MatchParticipantRow = Awaited<ReturnType<typeof queryParticipantRows>>[number];
type MatchBrowseRow = Awaited<ReturnType<typeof queryBrowseRows>>[number];
type MyMatchRow = Awaited<ReturnType<typeof queryMyMatchRows>>[number];
type MatchHistoryRow = Awaited<ReturnType<typeof queryMatchHistoryRows>>[number];

export async function createMatch(
  input: MatchCreateRequest,
  creator: MatchViewer,
): Promise<MatchResult<MatchCreateResponse>> {
  // The map must already be in the catalog. A player puts one there by
  // importing it, which is a separate step with its own report of what went
  // wrong, so a match never waits on a fetch to another site.
  const mapRef = input.map;
  const catalogEntry = await findCatalogEntry(mapRef);
  if (!catalogEntry) {
    return err("invalidMap", "the selected map is not in the catalog", 400);
  }

  try {
    const mapDocument = await loadMapRevision(mapRef);
    const maxPlayers = mapDocument.metadata.player_count;

    if (!Number.isSafeInteger(maxPlayers) || maxPlayers <= 0) {
      return err("invalidMap", "selected map has an invalid player count", 400);
    }

    for (let attempt = 0; attempt < 3; attempt += 1) {
      const matchId = generateMatchId();
      const joinSlug = input.isPrivate ? generateOpaqueToken(18) : null;
      const now = new Date();

      const result = await db
        .insert(matches)
        .values({
          id: matchId,
          name: input.name,
          phase: PUBLIC_MATCH_PHASE,
          creatorUserId: creator.id,
          mapId: mapRef.mapId,
          mapRevision: mapRef.revision,
          maxPlayers,
          isPrivate: input.isPrivate,
          joinSlug,
          settings: input.settings,
          createdAt: now,
          updatedAt: now,
        })
        .run();

      if (result.meta.changes === 1) {
        return ok({ matchId, joinSlug });
      }
    }

    return err("matchCreateFailed", "failed to allocate a unique match id", 500);
  } catch (error) {
    return err(
      "matchCreateFailed",
      error instanceof Error ? error.message : "failed to create match",
      502,
    );
  }
}

export async function getMatchSnapshot(
  matchId: string,
  viewerUserId: string | null,
  joinSlug: string | null,
  viewer: Actor | null = null,
): Promise<MatchResult<MatchSnapshot>> {
  const finalized = await finalizeStartingMatchIfNeeded(matchId);
  if (!finalized.ok) {
    return finalized;
  }

  const snapshot = await loadMatchSnapshot(matchId);
  if (!snapshot.ok) {
    return snapshot;
  }

  if (!canViewMatch(snapshot.value, viewerUserId, joinSlug, viewer)) {
    return err("matchNotFound", "match not found", 404);
  }

  return ok(applyViewerVisibility(snapshot.value, viewerUserId));
}

export interface VoidMatchInput {
  matchId: string;
  /** What the players are told. Kept on the match. */
  publicReason: string;
  /** Why the moderator acted. Kept in the log and never shown to them. */
  reason: string;
  actor: Actor;
}

/**
 * Whether the write lost the race to void a match that another call voided.
 *
 * SQLite names the table and the column it refused, which keeps this from
 * reading a different broken key as a second void.
 */
function isDuplicateVoid(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  return /UNIQUE constraint failed: match_voids\.matchId/i.test(message);
}

/**
 * Mark a match as not counting, without changing the result it recorded.
 *
 * The seats keep the outcome they earned, because a void is a statement
 * about the match and not a rewrite of what happened in it. What a void
 * changes is whether the rating reads the result, which `isRatedResult`
 * decides.
 *
 * The state and its record are written in one batch, so the log cannot end
 * up missing an act that landed.
 *
 * `match_voids` holds one row for each match, so a second void that comes in
 * while the first is still in flight breaks on the key rather than writing a
 * second record. The read before the write is there for the message it gives
 * and not for the rule, which the key holds.
 */
export async function voidMatch(input: VoidMatchInput): Promise<MatchResult<{ voidedAt: string }>> {
  const row = await db
    .select({ id: matches.id, startedAt: matches.startedAt })
    .from(matches)
    .where(eq(matches.id, input.matchId))
    .get();
  if (!row) {
    return err("matchNotFound", "match not found", 404);
  }
  if (row.startedAt === null) {
    return err("matchNotVoidable", "a match that never started cannot be voided", 409);
  }

  const existing = await db
    .select({ matchId: matchVoids.matchId })
    .from(matchVoids)
    .where(eq(matchVoids.matchId, input.matchId))
    .get();
  if (existing) {
    return err("matchAlreadyVoided", "this match is already voided", 409);
  }

  const now = new Date();
  try {
    await db.batch([
      db.insert(matchVoids).values({
        matchId: input.matchId,
        publicReason: input.publicReason,
        voidedAt: now,
      }),
      db.insert(moderationActions).values(
        moderationEntry({
          actor: input.actor,
          action: "match.void",
          subjectType: "match",
          subjectId: input.matchId,
          reason: input.reason,
          details: { publicReason: input.publicReason },
          now,
        }),
      ),
    ]);
  } catch (error) {
    if (isDuplicateVoid(error)) {
      return err("matchAlreadyVoided", "this match is already voided", 409);
    }
    throw error;
  }

  return ok({ voidedAt: now.toISOString() });
}

export async function listMatches(
  input: MatchBrowseRequest,
): Promise<MatchResult<MatchBrowseResponse>> {
  const cursor = decodeMatchBrowseCursor(input.cursor);
  const rows = await queryBrowseRows(cursor);
  const hasNextPage = rows.length > MATCH_BROWSE_PAGE_SIZE;
  const visibleRows = hasNextPage ? rows.slice(0, MATCH_BROWSE_PAGE_SIZE) : rows;
  const participantRows = await queryBrowseParticipantRows(rows.map((row) => row.matchId));
  const participantNamesByMatchId = new Map<string, string[]>();

  for (const participant of participantRows) {
    const current = participantNamesByMatchId.get(participant.matchId);
    if (current) {
      current.push(participant.userName);
    } else {
      participantNamesByMatchId.set(participant.matchId, [participant.userName]);
    }
  }
  const browseMatches: MatchBrowseSummary[] = [];

  for (const row of visibleRows) {
    const settings = parseMatchSettingsValue(row.settings);
    if (!settings.ok) {
      return settings;
    }
    browseMatches.push(
      toMatchBrowseSummary(row, settings.value, participantNamesByMatchId.get(row.matchId) ?? []),
    );
  }

  const lastVisibleRow = visibleRows[visibleRows.length - 1] ?? null;

  return ok({
    matches: browseMatches,
    pageSize: MATCH_BROWSE_PAGE_SIZE,
    hasNextPage,
    nextCursor:
      hasNextPage && lastVisibleRow
        ? encodeMatchBrowseCursor({
            createdAt: lastVisibleRow.createdAt.toISOString(),
            matchId: lastVisibleRow.matchId,
          })
        : null,
  });
}

export async function listMyMatches(viewerUserId: string): Promise<MatchResult<MyMatchesResponse>> {
  const rows = await queryMyMatchRows(viewerUserId);
  const myMatches: MyMatchSummary[] = [];

  for (const groupedRows of groupMyMatchRows(rows)) {
    const settings = parseMatchSettingsValue(groupedRows[0]!.settings);
    if (!settings.ok) {
      return settings;
    }
    myMatches.push(toMyMatchSummary(groupedRows, settings.value));
  }

  return ok({ matches: myMatches });
}

/**
 * The viewer's finished matches, newest first.
 *
 * Two queries rather than one: the page is a count of matches, not of seats,
 * so the ids are picked first and every seat of those matches is read second.
 */
export async function listMyCompletedMatches(
  viewerUserId: string,
  input: MatchHistoryRequest,
): Promise<MatchResult<MatchHistoryResponse>> {
  const cursor = decodeMatchHistoryCursor(input.cursor);
  const cursorCompletedAt = cursor ? new Date(cursor.completedAt) : null;
  const cursorPredicate =
    cursor && cursorCompletedAt && !Number.isNaN(cursorCompletedAt.getTime())
      ? or(
          lt(matches.completedAt, cursorCompletedAt),
          and(eq(matches.completedAt, cursorCompletedAt), lt(matches.id, cursor.matchId)),
        )
      : undefined;
  const recent = await db
    .select({ matchId: matches.id, completedAt: matches.completedAt })
    .from(matches)
    .innerJoin(
      matchParticipants,
      and(eq(matchParticipants.matchId, matches.id), eq(matchParticipants.userId, viewerUserId)),
    )
    .where(
      and(
        eq(matches.phase, COMPLETED_MATCH_PHASE),
        isNotNull(matches.completedAt),
        cursorPredicate,
      ),
    )
    .groupBy(matches.id)
    .orderBy(desc(matches.completedAt), desc(matches.id))
    .limit(MATCH_HISTORY_PAGE_SIZE + 1)
    .all();

  const hasNextPage = recent.length > MATCH_HISTORY_PAGE_SIZE;
  const visibleRows = hasNextPage ? recent.slice(0, MATCH_HISTORY_PAGE_SIZE) : recent;
  const matchIds = visibleRows.map((row) => row.matchId);
  if (matchIds.length === 0) {
    return ok({
      matches: [],
      pageSize: MATCH_HISTORY_PAGE_SIZE,
      hasNextPage: false,
      nextCursor: null,
    });
  }

  const [rows, storedReplays] = await Promise.all([
    queryMatchHistoryRows(matchIds),
    matchReplaysExist(env.CONTENT, matchIds),
  ]);

  const rowsByMatchId = new Map<string, MatchHistoryRow[]>();
  for (const row of rows) {
    const current = rowsByMatchId.get(row.matchId);
    if (current) current.push(row);
    else rowsByMatchId.set(row.matchId, [row]);
  }

  const history: MatchHistoryEntry[] = [];
  for (const matchId of matchIds) {
    const matchRows = rowsByMatchId.get(matchId);
    if (!matchRows) continue;

    const settings = parseMatchSettingsValue(matchRows[0]!.settings);
    if (!settings.ok) {
      return settings;
    }

    history.push(
      toMatchHistoryEntry(matchRows, settings.value, viewerUserId, storedReplays.has(matchId)),
    );
  }

  const lastVisibleRow = visibleRows[visibleRows.length - 1] ?? null;
  return ok({
    matches: history,
    pageSize: MATCH_HISTORY_PAGE_SIZE,
    hasNextPage,
    nextCursor:
      hasNextPage && lastVisibleRow
        ? encodeMatchHistoryCursor({
            completedAt: lastVisibleRow.completedAt!.toISOString(),
            matchId: lastVisibleRow.matchId,
          })
        : null,
  });
}

export async function mutateMatch(
  matchId: string,
  viewer: MatchViewer,
  action: MatchMutationRequest,
): Promise<MatchResult<MatchMutationResponse>> {
  const finalized = await finalizeStartingMatchIfNeeded(matchId);
  if (!finalized.ok) {
    return finalized;
  }

  switch (action.action) {
    case "join": {
      const joinResult = await insertParticipant(
        matchId,
        viewer,
        action.slotIndex,
        action.factionId,
        action.joinSlug ?? null,
      );
      if (!joinResult.ok) {
        return joinResult;
      }
      break;
    }
    case "leave": {
      const leaveResult = await removeParticipant(matchId, viewer.id, action.slotIndex);
      if (!leaveResult.ok) {
        return leaveResult;
      }
      break;
    }
    case "updateParticipant": {
      const updateResult = await updateParticipant(matchId, viewer.id, action.slotIndex, action);
      if (!updateResult.ok) {
        return updateResult;
      }
      break;
    }
  }

  const startResult = await tryStartMatch(matchId);
  if (!startResult.ok) {
    return startResult;
  }

  const snapshot = await getMatchSnapshot(matchId, viewer.id, mutationJoinSlug(action));
  if (!snapshot.ok) {
    return snapshot;
  }

  return ok({ match: snapshot.value });
}

async function insertParticipant(
  matchId: string,
  viewer: MatchViewer,
  slotIndex: number,
  factionId: number,
  joinSlug: string | null,
): Promise<MatchResult<void>> {
  const now = new Date();
  const result = await db
    .insert(matchParticipants)
    .select(
      db
        .select({
          matchId: matches.id,
          userId: sql<string>`${viewer.id}`.as("userId"),
          slotIndex: sql<number>`${slotIndex}`.as("slotIndex"),
          factionId: sql<number>`${factionId}`.as("factionId"),
          coId: sql<null>`NULL`.as("coId"),
          ready: sql<boolean>`0`.as("ready"),
          joinedAt: sql<Date>`${sql.param(now, matchParticipants.joinedAt)}`.as("joinedAt"),
          updatedAt: sql<Date>`${sql.param(now, matchParticipants.updatedAt)}`.as("updatedAt"),
        })
        .from(matches)
        .where(
          and(
            eq(matches.id, matchId),
            eq(matches.phase, PUBLIC_MATCH_PHASE),
            sql`${slotIndex} >= 0`,
            sql`${slotIndex} < ${matches.maxPlayers}`,
            or(eq(matches.isPrivate, false), sql`${matches.joinSlug} = ${joinSlug}`),
            sql`(
              COALESCE(json_extract(${matches.settings}, '$.hotseatEnabled'), 0) = 1
              OR NOT EXISTS (
                SELECT 1 FROM match_participants owned
                WHERE owned.matchId = ${matches.id}
                  AND owned.userId = ${viewer.id}
              )
            )`,
          ),
        ),
    )
    .onConflictDoNothing()
    .run();

  if (result.meta.changes === 1) {
    return ok(undefined);
  }

  const diagnostics = await diagnoseJoinFailure(matchId, viewer.id, slotIndex, joinSlug);
  return joinFailureFromDiagnostics(diagnostics);
}

async function removeParticipant(
  matchId: string,
  userId: string,
  slotIndex: number,
): Promise<MatchResult<void>> {
  const result = await db.run(sql`
    DELETE FROM match_participants
    WHERE matchId = ${matchId}
      AND userId = ${userId}
      AND slotIndex = ${slotIndex}
      AND EXISTS (
        SELECT 1
        FROM matches
        WHERE id = ${matchId}
          AND phase = ${PUBLIC_MATCH_PHASE}
      )
  `);

  if (result.meta.changes === 1) {
    return ok(undefined);
  }

  return err("notParticipant", "you are not currently in this match lobby", 409);
}

async function updateParticipant(
  matchId: string,
  userId: string,
  slotIndex: number,
  action: Extract<MatchMutationRequest, { action: "updateParticipant" }>,
): Promise<MatchResult<void>> {
  const snapshot = await loadMatchSnapshot(matchId);
  if (!snapshot.ok) {
    return snapshot;
  }

  const match = snapshot.value;
  if (match.phase !== PUBLIC_MATCH_PHASE) {
    return err("matchNotLobby", "match is no longer in lobby", 409);
  }

  const participant = match.participants.find(
    (entry) => entry.userId === userId && entry.slotIndex === slotIndex,
  );
  if (!participant) {
    return err("notParticipant", "you are not currently in this match lobby", 409);
  }

  const nextFactionId = action.factionId ?? participant.factionId;
  const nextCoId = "coId" in action ? (action.coId ?? null) : participant.coId;

  let nextReady = action.ready ?? participant.ready;
  if (
    ("factionId" in action && action.factionId !== participant.factionId) ||
    ("coId" in action && action.coId !== participant.coId)
  ) {
    nextReady = false;
  }

  if (nextCoId !== null && match.settings.bannedCoIds.includes(nextCoId)) {
    return err("participantInvalid", `${coDisplayName(nextCoId)} is banned in this match`, 409);
  }

  if (nextReady && nextCoId === null) {
    return err("participantInvalid", "select a CO before readying up", 409);
  }

  const result = await db
    .update(matchParticipants)
    .set({
      factionId: nextFactionId,
      coId: nextCoId,
      ready: nextReady,
      updatedAt: new Date(),
    })
    .where(
      and(
        eq(matchParticipants.matchId, matchId),
        eq(matchParticipants.userId, userId),
        eq(matchParticipants.slotIndex, slotIndex),
        exists(
          db
            .select({ _: sql`1` })
            .from(matches)
            .where(and(eq(matches.id, matchId), eq(matches.phase, PUBLIC_MATCH_PHASE))),
        ),
      ),
    )
    .run();

  if (result.meta.changes === 1) {
    return ok(undefined);
  }

  return err("notParticipant", "you are not currently in this match lobby", 409);
}

async function diagnoseJoinFailure(
  matchId: string,
  userId: string,
  slotIndex: number,
  joinSlug: string | null,
): Promise<MatchActionDiagnostics> {
  const row = await queryMatchRow(matchId);
  if (!row) {
    return "notFound";
  }
  if (row.phase !== PUBLIC_MATCH_PHASE) {
    return "notLobby";
  }
  if (slotIndex < 0 || slotIndex >= row.maxPlayers) {
    return "invalidSlot";
  }
  if (row.isPrivate && row.joinSlug !== joinSlug) {
    return "privateJoinRequired";
  }

  const settings = parseMatchSettingsValue(row.settings);
  if (!settings.ok || !settings.value.hotseatEnabled) {
    const existingUser = await db
      .select({ value: sql<number>`1` })
      .from(matchParticipants)
      .where(and(eq(matchParticipants.matchId, matchId), eq(matchParticipants.userId, userId)))
      .get();

    if (existingUser) {
      return "hotseatDisabled";
    }
  }

  return "slotTaken";
}

function joinFailureFromDiagnostics(diagnostics: MatchActionDiagnostics): MatchResult<void> {
  switch (diagnostics) {
    case "notFound":
      return err("matchNotFound", "match not found", 404);
    case "notLobby":
      return err("matchNotLobby", "match is no longer in lobby", 409);
    case "invalidSlot":
      return err("invalidSlot", "selected slot is outside the map's player count", 409);
    case "privateJoinRequired":
      return err("privateJoinRequired", "private match access was denied", 403);
    case "hotseatDisabled":
      return err("alreadyJoined", "you have already claimed a slot in this lobby", 409);
    case "slotTaken":
      return err("slotTaken", "that lobby slot has already been claimed", 409);
  }
}

async function tryStartMatch(matchId: string): Promise<MatchResult<void>> {
  await db.run(sql`
    UPDATE matches
    SET phase = ${STARTING_MATCH_PHASE},
        updatedAt = ${sql.param(new Date(), matches.updatedAt)}
    WHERE id = ${matchId}
      AND phase = ${PUBLIC_MATCH_PHASE}
      AND (
        SELECT COUNT(*)
        FROM match_participants p
        WHERE p.matchId = matches.id
      ) = maxPlayers
      AND (
        SELECT COUNT(*)
        FROM match_participants p
        WHERE p.matchId = matches.id
          AND p.ready = 1
          AND p.coId IS NOT NULL
      ) = maxPlayers
  `);

  return finalizeStartingMatchIfNeeded(matchId);
}

export async function finalizeStartingMatchIfNeeded(matchId: string): Promise<MatchResult<void>> {
  const row = await queryMatchRow(matchId);
  if (!row) {
    return err("matchNotFound", "match not found", 404);
  }

  if (row.phase !== STARTING_MATCH_PHASE) {
    return ok(undefined);
  }

  const setup = await buildMatchSetup(row);
  if (!setup.ok) {
    return setup;
  }

  const stub = getMatchStub(matchId);
  const initializeResult = (await stub.initializeMatch(
    setup.value,
  )) as MatchResult<MatchCreateResponse>;
  if (!initializeResult.ok) {
    return {
      ok: false,
      error: initializeResult.error,
    };
  }

  await db
    .update(matches)
    .set({
      phase: ACTIVE_MATCH_PHASE,
      startedAt: sql`COALESCE(${matches.startedAt}, ${sql.param(new Date(), matches.startedAt)})`,
      updatedAt: new Date(),
    })
    .where(and(eq(matches.id, matchId), eq(matches.phase, STARTING_MATCH_PHASE)))
    .run();

  return ok(undefined);
}

async function buildMatchSetup(row: NonNullable<MatchRow>): Promise<MatchResult<MatchSetup>> {
  const participantRows = await queryParticipantRows(row.id);
  if (participantRows.length !== row.maxPlayers) {
    return err("matchStartBlocked", "match lobby is not full", 409);
  }

  const settings = parseMatchSettingsValue(row.settings);
  if (!settings.ok) {
    return settings;
  }

  for (const participant of participantRows) {
    if (participant.coId === null || !participant.ready) {
      return err("matchStartBlocked", "all players must choose a CO and ready up", 409);
    }
  }

  let map: AwbrnMapDocument;
  try {
    map = await loadMapRevision({ mapId: row.mapId, revision: row.mapRevision });
  } catch (error) {
    return err(
      "matchStartBlocked",
      error instanceof Error ? error.message : "failed to load map data",
      500,
    );
  }

  return ok({
    matchId: row.id,
    mapId: row.mapId,
    revision: row.mapRevision,
    map,
    fogEnabled: settings.value.fogEnabled,
    startingFunds: settings.value.startingFunds,
    creatorUserId: row.creatorUserId,
    pool: row.pool,
    season: row.season,
    clock: settings.value.clock,
    players: participantRows.map((participant) => ({
      userId: participant.userId,
      factionId: participant.factionId,
      team: null,
      startingFunds: settings.value.startingFunds,
      coId: participant.coId!,
    })),
  });
}

async function loadMatchSnapshot(matchId: string): Promise<MatchResult<MatchSnapshot>> {
  const row = await queryMatchRow(matchId);
  if (!row) {
    return err("matchNotFound", "match not found", 404);
  }

  const settings = parseMatchSettingsValue(row.settings);
  if (!settings.ok) {
    return settings;
  }

  const [participantRows, voidRow] = await Promise.all([
    queryParticipantRows(matchId),
    db
      .select({ publicReason: matchVoids.publicReason, voidedAt: matchVoids.voidedAt })
      .from(matchVoids)
      .where(eq(matchVoids.matchId, matchId))
      .get(),
  ]);
  return ok({
    matchId: row.id,
    name: row.name,
    phase: row.phase,
    creatorUserId: row.creatorUserId,
    creatorName: row.creatorName,
    mapId: row.mapId,
    mapRevision: row.mapRevision,
    maxPlayers: row.maxPlayers,
    isPrivate: row.isPrivate,
    joinSlug: row.joinSlug ?? null,
    settings: settings.value,
    createdAt: row.createdAt.toISOString(),
    updatedAt: row.updatedAt.toISOString(),
    startedAt: row.startedAt === null ? null : row.startedAt.toISOString(),
    completedAt: row.completedAt === null ? null : row.completedAt.toISOString(),
    participants: participantRows.map(toParticipantSnapshot),
    void: voidRow
      ? { publicReason: voidRow.publicReason, voidedAt: voidRow.voidedAt.toISOString() }
      : null,
  });
}

async function queryMatchRow(matchId: string) {
  return db
    .select({
      id: matches.id,
      name: matches.name,
      phase: matches.phase,
      creatorUserId: matches.creatorUserId,
      creatorName: user.name,
      mapId: matches.mapId,
      mapRevision: matches.mapRevision,
      maxPlayers: matches.maxPlayers,
      isPrivate: matches.isPrivate,
      joinSlug: matches.joinSlug,
      settings: matches.settings,
      createdAt: matches.createdAt,
      updatedAt: matches.updatedAt,
      startedAt: matches.startedAt,
      completedAt: matches.completedAt,
      pool: matches.pool,
      season: matches.season,
    })
    .from(matches)
    .innerJoin(user, eq(user.id, matches.creatorUserId))
    .where(eq(matches.id, matchId))
    .get();
}

async function queryParticipantRows(matchId: string) {
  return db
    .select({
      matchId: matchParticipants.matchId,
      userId: matchParticipants.userId,
      userName: user.name,
      slotIndex: matchParticipants.slotIndex,
      factionId: matchParticipants.factionId,
      coId: matchParticipants.coId,
      ready: matchParticipants.ready,
      joinedAt: matchParticipants.joinedAt,
      updatedAt: matchParticipants.updatedAt,
    })
    .from(matchParticipants)
    .innerJoin(user, eq(user.id, matchParticipants.userId))
    .where(eq(matchParticipants.matchId, matchId))
    .orderBy(asc(matchParticipants.slotIndex))
    .all();
}

async function queryBrowseRows(cursor: { createdAt: string; matchId: string } | null) {
  const cursorCreatedAt = cursor ? new Date(cursor.createdAt) : null;
  const cursorPredicate =
    cursor && cursorCreatedAt && !Number.isNaN(cursorCreatedAt.getTime())
      ? or(
          lt(matches.createdAt, cursorCreatedAt),
          and(eq(matches.createdAt, cursorCreatedAt), lt(matches.id, cursor.matchId)),
        )
      : undefined;
  const whereClause = cursorPredicate
    ? and(eq(matches.isPrivate, false), eq(matches.phase, PUBLIC_MATCH_PHASE), cursorPredicate)
    : and(eq(matches.isPrivate, false), eq(matches.phase, PUBLIC_MATCH_PHASE));

  return db
    .select({
      matchId: matches.id,
      name: matches.name,
      creatorName: user.name,
      mapId: matches.mapId,
      mapRevision: matches.mapRevision,
      maxPlayers: matches.maxPlayers,
      participantCount: count(matchParticipants.userId),
      settings: matches.settings,
      createdAt: matches.createdAt,
    })
    .from(matches)
    .innerJoin(user, eq(user.id, matches.creatorUserId))
    .leftJoin(matchParticipants, eq(matchParticipants.matchId, matches.id))
    .where(whereClause)
    .groupBy(
      matches.id,
      matches.name,
      user.name,
      matches.mapId,
      matches.mapRevision,
      matches.maxPlayers,
      matches.settings,
      matches.createdAt,
    )
    .having(gt(matches.maxPlayers, count(matchParticipants.userId)))
    .orderBy(desc(matches.createdAt), desc(matches.id))
    .limit(MATCH_BROWSE_PAGE_SIZE + 1)
    .all();
}

async function queryBrowseParticipantRows(matchIds: readonly string[]) {
  if (matchIds.length === 0) {
    return [];
  }

  return db
    .select({
      matchId: matchParticipants.matchId,
      userName: user.name,
      slotIndex: matchParticipants.slotIndex,
    })
    .from(matchParticipants)
    .innerJoin(user, eq(user.id, matchParticipants.userId))
    .where(inArray(matchParticipants.matchId, matchIds))
    .orderBy(asc(matchParticipants.matchId), asc(matchParticipants.slotIndex))
    .all();
}

async function queryMyMatchRows(viewerUserId: string) {
  return db
    .select({
      matchId: matches.id,
      name: matches.name,
      phase: matches.phase,
      creatorName: user.name,
      mapId: matches.mapId,
      mapRevision: matches.mapRevision,
      maxPlayers: matches.maxPlayers,
      participantCount: sql<number>`(
        SELECT COUNT(*)
        FROM match_participants p
        WHERE p.matchId = ${matches.id}
      )`.as("participantCount"),
      isPrivate: matches.isPrivate,
      settings: matches.settings,
      createdAt: matches.createdAt,
      updatedAt: matches.updatedAt,
      startedAt: matches.startedAt,
      viewerSlotIndex: matchParticipants.slotIndex,
      viewerFactionId: matchParticipants.factionId,
      viewerCoId: matchParticipants.coId,
      viewerReady: matchParticipants.ready,
      viewerJoinedAt: matchParticipants.joinedAt,
      viewerUpdatedAt: matchParticipants.updatedAt,
    })
    .from(matches)
    .innerJoin(user, eq(user.id, matches.creatorUserId))
    .innerJoin(
      matchParticipants,
      and(eq(matchParticipants.matchId, matches.id), eq(matchParticipants.userId, viewerUserId)),
    )
    .where(inArray(matches.phase, ONGOING_MATCH_PHASES))
    .orderBy(
      sql`CASE ${matches.phase}
        WHEN 'active' THEN 0
        WHEN 'starting' THEN 1
        WHEN 'lobby' THEN 2
        WHEN 'draft' THEN 3
        ELSE 4
      END`,
      desc(matches.updatedAt),
      desc(matches.id),
    )
    .all();
}

/** Every seat of the named matches, with the result recorded for each. */
async function queryMatchHistoryRows(matchIds: string[]) {
  return db
    .select({
      matchId: matches.id,
      name: matches.name,
      mapId: matches.mapId,
      mapRevision: matches.mapRevision,
      awbwMapId: mapSources.sourceMapId,
      isPrivate: matches.isPrivate,
      settings: matches.settings,
      startedAt: matches.startedAt,
      completedAt: matches.completedAt,
      seatSlotIndex: matchParticipants.slotIndex,
      seatUserId: matchParticipants.userId,
      seatUserName: user.name,
      seatFactionId: matchParticipants.factionId,
      seatCoId: matchParticipants.coId,
      seatOutcome: matchResults.outcome,
      seatPlacement: matchResults.placement,
      seatReason: matchResults.reason,
    })
    .from(matches)
    .innerJoin(matchParticipants, eq(matchParticipants.matchId, matches.id))
    .innerJoin(user, eq(user.id, matchParticipants.userId))
    .leftJoin(mapSources, and(eq(mapSources.mapId, matches.mapId), eq(mapSources.source, "awbw")))
    .leftJoin(
      matchResults,
      and(
        eq(matchResults.matchId, matchParticipants.matchId),
        eq(matchResults.slotIndex, matchParticipants.slotIndex),
      ),
    )
    .where(inArray(matches.id, matchIds))
    .orderBy(asc(matchParticipants.slotIndex))
    .all();
}

function parseMatchSettingsValue(value: unknown): MatchResult<MatchSettings> {
  try {
    const result = matchSettingsSchema.safeParse(value);
    if (!result.success) {
      const issue = result.error.issues[0];
      return err("matchInvalid", issue?.message ?? "match settings were invalid", 500);
    }
    return ok(result.data);
  } catch (error) {
    return err("matchInvalid", "match settings were invalid", 500, {
      reason: error instanceof Error ? error.message : String(error),
    });
  }
}

function toParticipantSnapshot(row: MatchParticipantRow): MatchParticipantSnapshot {
  return {
    userId: row.userId,
    userName: row.userName,
    slotIndex: row.slotIndex,
    factionId: row.factionId,
    coId: row.coId,
    ready: row.ready,
    joinedAt: row.joinedAt.toISOString(),
    updatedAt: row.updatedAt.toISOString(),
  };
}

function toMatchBrowseSummary(
  row: MatchBrowseRow,
  settings: MatchSettings,
  joinedPlayerNames: string[],
): MatchBrowseSummary {
  const participantCount = Number(row.participantCount);

  return {
    matchId: row.matchId,
    name: row.name,
    creatorName: row.creatorName,
    mapId: row.mapId,
    mapRevision: row.mapRevision,
    maxPlayers: row.maxPlayers,
    participantCount,
    openSlotCount: Math.max(0, row.maxPlayers - participantCount),
    joinedPlayerNames,
    settings,
    createdAt: row.createdAt.toISOString(),
  };
}

function toMyMatchSummary(rows: MyMatchRow[], settings: MatchSettings): MyMatchSummary {
  const row = rows[0]!;
  const participantCount = Number(row.participantCount);

  return {
    matchId: row.matchId,
    name: row.name,
    phase: row.phase,
    creatorName: row.creatorName,
    mapId: row.mapId,
    mapRevision: row.mapRevision,
    maxPlayers: row.maxPlayers,
    participantCount,
    openSlotCount: Math.max(0, row.maxPlayers - participantCount),
    isPrivate: row.isPrivate,
    settings,
    createdAt: row.createdAt.toISOString(),
    updatedAt: row.updatedAt.toISOString(),
    startedAt: row.startedAt === null ? null : row.startedAt.toISOString(),
    viewerParticipants: rows
      .map((viewerRow) => ({
        slotIndex: viewerRow.viewerSlotIndex,
        factionId: viewerRow.viewerFactionId,
        coId: viewerRow.viewerCoId,
        ready: viewerRow.viewerReady,
        joinedAt: viewerRow.viewerJoinedAt.toISOString(),
        updatedAt: viewerRow.viewerUpdatedAt.toISOString(),
      }))
      .sort((a, b) => a.slotIndex - b.slotIndex),
  };
}

/** One finished match, as the report the viewer reads. */
function toMatchHistoryEntry(
  rows: MatchHistoryRow[],
  settings: MatchSettings,
  viewerUserId: string,
  hasReplay: boolean,
): MatchHistoryEntry {
  const row = rows[0]!;
  const seats: MatchHistorySeat[] = rows.map((seatRow) => ({
    slotIndex: seatRow.seatSlotIndex,
    userId: seatRow.seatUserId,
    userName: seatRow.seatUserName,
    factionId: seatRow.seatFactionId,
    coId: seatRow.seatCoId,
    outcome: seatRow.seatOutcome,
    placement: seatRow.seatPlacement,
    reason: seatRow.seatReason,
  }));

  return {
    matchId: row.matchId,
    name: row.name,
    mapId: row.mapId,
    mapRevision: row.mapRevision,
    awbwMapId: row.awbwMapId,
    isPrivate: row.isPrivate,
    settings,
    startedAt: row.startedAt === null ? null : row.startedAt.toISOString(),
    // Only matches with a completion time are selected, so this is never null.
    completedAt: (row.completedAt ?? row.startedAt ?? new Date(0)).toISOString(),
    viewerSlotIndexes: seats
      .filter((seat) => seat.userId === viewerUserId)
      .map((seat) => seat.slotIndex),
    seats,
    hasReplay,
  };
}

function canViewMatch(
  snapshot: MatchSnapshot,
  viewerUserId: string | null,
  joinSlug: string | null,
  viewer: Actor | null,
): boolean {
  if (!snapshot.isPrivate) {
    return true;
  }
  // Taking part is checked below. This is the grant that reaches past it.
  if (matchViewAnyGrant(viewer) !== null) {
    return true;
  }
  if (viewerUserId !== null && snapshot.creatorUserId === viewerUserId) {
    return true;
  }
  if (
    viewerUserId !== null &&
    snapshot.participants.some((participant) => participant.userId === viewerUserId)
  ) {
    return true;
  }
  return snapshot.joinSlug !== null && snapshot.joinSlug === joinSlug;
}

function applyViewerVisibility(
  snapshot: MatchSnapshot,
  viewerUserId: string | null,
): MatchSnapshot {
  const hideRankedCommanders =
    snapshot.phase === "pending" &&
    !snapshot.participants.every((participant) => participant.ready);
  return {
    ...snapshot,
    joinSlug: viewerUserId === snapshot.creatorUserId ? snapshot.joinSlug : null,
    participants: hideRankedCommanders
      ? snapshot.participants.map((participant) =>
          participant.userId === viewerUserId ? participant : { ...participant, coId: null },
        )
      : snapshot.participants,
  };
}

function mutationJoinSlug(action: MatchMutationRequest): string | null {
  switch (action.action) {
    case "join":
      return action.joinSlug ?? null;
    case "updateParticipant":
      return action.joinSlug ?? null;
    case "leave":
      return null;
  }
}

function generateOpaqueToken(byteLength: number): string {
  const bytes = crypto.getRandomValues(new Uint8Array(byteLength));
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}
