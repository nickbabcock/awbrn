import { env } from "cloudflare:workers";
import { matchSettingsSchema } from "./schemas";
import type {
  MatchCreateRequest,
  MatchCreateResponse,
  MatchMutationRequest,
  MatchMutationResponse,
  MatchParticipantSnapshot,
  MatchPhase,
  MatchSettings,
  MatchSnapshot,
} from "./schemas";
import { fetchAwbwMapData } from "../awbw/awbw.server";
import type { AwbwMapData } from "../awbw/schemas";
import { err, ok, type MatchResult } from "./match_protocol";
import { generateMatchId } from "./match_id";
import { getMatchStub } from "./match_service";

const PUBLIC_MATCH_PHASE: MatchPhase = "lobby";
const STARTING_MATCH_PHASE: MatchPhase = "starting";
const ACTIVE_MATCH_PHASE: MatchPhase = "active";

interface MatchRow {
  id: string;
  name: string;
  phase: MatchPhase;
  creatorUserId: string;
  creatorName: string;
  mapId: number;
  maxPlayers: number;
  isPrivate: number;
  joinSlug: string | null;
  settings: string;
  createdAt: number;
  updatedAt: number;
  startedAt: number | null;
  completedAt: number | null;
}

interface MatchParticipantRow {
  matchId: string;
  userId: string;
  userName: string;
  slotIndex: number;
  factionId: number;
  coId: number | null;
  ready: number;
  joinedAt: number;
  updatedAt: number;
}

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
      const now = Date.now();

      const result = await env.DB.prepare(
        `INSERT INTO matches (
          id,
          name,
          phase,
          creatorUserId,
          mapId,
          maxPlayers,
          isPrivate,
          joinSlug,
          settings,
          createdAt,
          updatedAt
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      )
        .bind(
          matchId,
          input.name,
          PUBLIC_MATCH_PHASE,
          creator.id,
          input.mapId,
          maxPlayers,
          input.isPrivate ? 1 : 0,
          joinSlug,
          JSON.stringify(input.settings),
          now,
          now,
        )
        .run();

      if (result.success) {
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
  const now = Date.now();
  const result = await env.DB.prepare(
    `INSERT OR IGNORE INTO match_participants (
      matchId,
      userId,
      slotIndex,
      factionId,
      coId,
      ready,
      joinedAt,
      updatedAt
    )
    SELECT
      m.id,
      ?,
      ?,
      ?,
      NULL,
      0,
      ?,
      ?
    FROM matches m
    WHERE m.id = ?
      AND m.phase = ?
      AND ? >= 0
      AND ? < m.maxPlayers
      AND (m.isPrivate = 0 OR m.joinSlug = ?)`,
  )
    .bind(
      viewer.id,
      slotIndex,
      factionId,
      now,
      now,
      matchId,
      PUBLIC_MATCH_PHASE,
      slotIndex,
      slotIndex,
      joinSlug,
    )
    .run();

  if (result.meta.changes === 1) {
    return ok(undefined);
  }

  const diagnostics = await diagnoseJoinFailure(matchId, viewer.id, slotIndex, joinSlug);
  return joinFailureFromDiagnostics(diagnostics);
}

async function removeParticipant(matchId: string, userId: string): Promise<MatchResult<void>> {
  const result = await env.DB.prepare(
    `DELETE FROM match_participants
    WHERE matchId = ?
      AND userId = ?
      AND EXISTS (
        SELECT 1
        FROM matches
        WHERE id = ?
          AND phase = ?
      )`,
  )
    .bind(matchId, userId, matchId, PUBLIC_MATCH_PHASE)
    .run();

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

  const result = await env.DB.prepare(
    `UPDATE match_participants
    SET factionId = ?,
        coId = ?,
        ready = ?,
        updatedAt = ?
    WHERE matchId = ?
      AND userId = ?
      AND EXISTS (
        SELECT 1
        FROM matches
        WHERE id = ?
          AND phase = ?
      )`,
  )
    .bind(
      nextFactionId,
      nextCoId,
      nextReady ? 1 : 0,
      Date.now(),
      matchId,
      userId,
      matchId,
      PUBLIC_MATCH_PHASE,
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
  if (row.isPrivate === 1 && row.joinSlug !== joinSlug) {
    return "privateJoinRequired";
  }

  const existingUser = await env.DB.prepare(
    `SELECT 1 AS value
    FROM match_participants
    WHERE matchId = ?
      AND userId = ?
    LIMIT 1`,
  )
    .bind(matchId, userId)
    .first<{ value: number }>();

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
  await env.DB.prepare(
    `UPDATE matches
    SET phase = ?,
        updatedAt = ?
    WHERE id = ?
      AND phase = ?
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
      ) = maxPlayers`,
  )
    .bind(STARTING_MATCH_PHASE, Date.now(), matchId, PUBLIC_MATCH_PHASE)
    .run();

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

  await env.DB.prepare(
    `UPDATE matches
    SET phase = ?,
        startedAt = COALESCE(startedAt, ?),
        updatedAt = ?
    WHERE id = ?
      AND phase = ?`,
  )
    .bind(ACTIVE_MATCH_PHASE, Date.now(), Date.now(), matchId, STARTING_MATCH_PHASE)
    .run();

  return ok(undefined);
}

async function buildMatchSetup(row: MatchRow): Promise<MatchResult<MatchSetup>> {
  const participantRows = await queryParticipantRows(row.id);
  if (participantRows.length !== row.maxPlayers) {
    return err("matchStartBlocked", "match lobby is not full", 409);
  }

  const settings = parseMatchSettingsValue(row.settings);
  if (!settings.ok) {
    return settings;
  }

  const sortedPlayers = [...participantRows].sort(
    (left, right) => left.slotIndex - right.slotIndex,
  );
  for (const participant of sortedPlayers) {
    if (participant.coId === null || participant.ready !== 1) {
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
    players: sortedPlayers.map((participant) => ({
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
    isPrivate: row.isPrivate === 1,
    joinSlug: row.joinSlug,
    settings: settings.value,
    createdAt: toIsoTimestamp(row.createdAt),
    updatedAt: toIsoTimestamp(row.updatedAt),
    startedAt: row.startedAt === null ? null : toIsoTimestamp(row.startedAt),
    completedAt: row.completedAt === null ? null : toIsoTimestamp(row.completedAt),
    participants: participantRows.map(toParticipantSnapshot),
  });
}

async function queryMatchRow(matchId: string): Promise<MatchRow | null> {
  return env.DB.prepare(
    `SELECT
      m.id,
      m.name,
      m.phase,
      m.creatorUserId,
      creator.name AS creatorName,
      m.mapId,
      m.maxPlayers,
      m.isPrivate,
      m.joinSlug,
      m.settings,
      m.createdAt,
      m.updatedAt,
      m.startedAt,
      m.completedAt
    FROM matches m
    INNER JOIN user creator
      ON creator.id = m.creatorUserId
    WHERE m.id = ?
    LIMIT 1`,
  )
    .bind(matchId)
    .first<MatchRow>();
}

async function queryParticipantRows(matchId: string): Promise<MatchParticipantRow[]> {
  const results = await env.DB.prepare(
    `SELECT
      p.matchId,
      p.userId,
      u.name AS userName,
      p.slotIndex,
      p.factionId,
      p.coId,
      p.ready,
      p.joinedAt,
      p.updatedAt
    FROM match_participants p
    INNER JOIN user u
      ON u.id = p.userId
    WHERE p.matchId = ?
    ORDER BY p.slotIndex ASC`,
  )
    .bind(matchId)
    .all<MatchParticipantRow>();

  return results.results ?? [];
}

function parseMatchSettingsValue(value: string): MatchResult<MatchSettings> {
  try {
    const result = matchSettingsSchema.safeParse(JSON.parse(value));
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
    ready: row.ready === 1,
    joinedAt: toIsoTimestamp(row.joinedAt),
    updatedAt: toIsoTimestamp(row.updatedAt),
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

function toIsoTimestamp(timestamp: number): string {
  return new Date(timestamp).toISOString();
}
