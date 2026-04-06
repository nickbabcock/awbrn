import { env } from "cloudflare:workers";
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
  MatchSnapshot,
} from "./schemas";
import {
  MATCH_BROWSE_PAGE_SIZE,
  decodeMatchBrowseCursor,
  encodeMatchBrowseCursor,
} from "./match_browse";
import { fetchAwbwMapData } from "#/awbw/awbw.server.ts";
import type { AwbwMapData } from "#/awbw/schemas.ts";
import { err, ok, type MatchResult } from "./match_protocol";
import { generateMatchId } from "./match_id";
import { getMatchStub } from "./match_service";
import { drizzle } from "drizzle-orm/d1";
import { and, asc, count, desc, eq, exists, gt, inArray, lt, or, sql } from "drizzle-orm";
import { matches, matchParticipants, user } from "#/db/global.ts";

const db = drizzle(env.DB, { schema: { matches, matchParticipants, user } });

const PUBLIC_MATCH_PHASE: MatchPhase = "lobby";
const STARTING_MATCH_PHASE: MatchPhase = "starting";
const ACTIVE_MATCH_PHASE: MatchPhase = "active";

interface MatchViewer {
  id: string;
  name: string;
}

interface MatchSetupPlayer {
  factionId: number;
  team: null;
  startingFunds: number;
  coId: number;
}

interface MatchSetup {
  matchId: string;
  mapId: number;
  map: AwbwMapData;
  players: MatchSetupPlayer[];
  fogEnabled: boolean;
  startingFunds: number;
}

type MatchActionDiagnostics =
  | "notFound"
  | "notLobby"
  | "invalidSlot"
  | "privateJoinRequired"
  | "slotTaken"
  | "alreadyJoined";

type MatchRow = Awaited<ReturnType<typeof queryMatchRow>>;
type MatchParticipantRow = Awaited<ReturnType<typeof queryParticipantRows>>[number];
type MatchBrowseRow = Awaited<ReturnType<typeof queryBrowseRows>>[number];

export async function createMatch(
  input: MatchCreateRequest,
  creator: MatchViewer,
): Promise<MatchResult<MatchCreateResponse>> {
  try {
    const awbwMap = await fetchAwbwMapData(input.mapId);
    const maxPlayers = awbwMap["Player Count"];

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
          mapId: input.mapId,
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
): Promise<MatchResult<MatchSnapshot>> {
  const finalized = await finalizeStartingMatchIfNeeded(matchId);
  if (!finalized.ok) {
    return finalized;
  }

  const snapshot = await loadMatchSnapshot(matchId);
  if (!snapshot.ok) {
    return snapshot;
  }

  if (!canViewMatch(snapshot.value, viewerUserId, joinSlug)) {
    return err("matchNotFound", "match not found", 404);
  }

  return ok(applyViewerVisibility(snapshot.value, viewerUserId));
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
      const leaveResult = await removeParticipant(matchId, viewer.id);
      if (!leaveResult.ok) {
        return leaveResult;
      }
      break;
    }
    case "updateParticipant": {
      const updateResult = await updateParticipant(matchId, viewer.id, action);
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

async function removeParticipant(matchId: string, userId: string): Promise<MatchResult<void>> {
  const result = await db.run(sql`
    DELETE FROM match_participants
    WHERE matchId = ${matchId}
      AND userId = ${userId}
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

  const participant = match.participants.find((entry) => entry.userId === userId);
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

  const existingUser = await db
    .select({ value: sql<number>`1` })
    .from(matchParticipants)
    .where(and(eq(matchParticipants.matchId, matchId), eq(matchParticipants.userId, userId)))
    .get();

  if (existingUser) {
    return "alreadyJoined";
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
    case "alreadyJoined":
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

async function finalizeStartingMatchIfNeeded(matchId: string): Promise<MatchResult<void>> {
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

  let map: AwbwMapData;
  try {
    map = await fetchAwbwMapData(row.mapId);
  } catch (error) {
    return err(
      "matchStartBlocked",
      error instanceof Error ? error.message : "failed to fetch map data",
      502,
    );
  }

  return ok({
    matchId: row.id,
    mapId: row.mapId,
    map,
    fogEnabled: settings.value.fogEnabled,
    startingFunds: settings.value.startingFunds,
    players: participantRows.map((participant) => ({
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

  const participantRows = await queryParticipantRows(matchId);
  return ok({
    matchId: row.id,
    name: row.name,
    phase: row.phase,
    creatorUserId: row.creatorUserId,
    creatorName: row.creatorName,
    mapId: row.mapId,
    maxPlayers: row.maxPlayers,
    isPrivate: row.isPrivate,
    joinSlug: row.joinSlug ?? null,
    settings: settings.value,
    createdAt: row.createdAt.toISOString(),
    updatedAt: row.updatedAt.toISOString(),
    startedAt: row.startedAt === null ? null : row.startedAt.toISOString(),
    completedAt: row.completedAt === null ? null : row.completedAt.toISOString(),
    participants: participantRows.map(toParticipantSnapshot),
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
      maxPlayers: matches.maxPlayers,
      isPrivate: matches.isPrivate,
      joinSlug: matches.joinSlug,
      settings: matches.settings,
      createdAt: matches.createdAt,
      updatedAt: matches.updatedAt,
      startedAt: matches.startedAt,
      completedAt: matches.completedAt,
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
    maxPlayers: row.maxPlayers,
    participantCount,
    openSlotCount: Math.max(0, row.maxPlayers - participantCount),
    joinedPlayerNames,
    settings,
    createdAt: row.createdAt.toISOString(),
  };
}

function canViewMatch(
  snapshot: MatchSnapshot,
  viewerUserId: string | null,
  joinSlug: string | null,
): boolean {
  if (!snapshot.isPrivate) {
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
  return {
    ...snapshot,
    joinSlug: viewerUserId === snapshot.creatorUserId ? snapshot.joinSlug : null,
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
