import { getCoAbilities } from "#/co_abilities.ts";
import { getCoPortraitByAwbwId } from "#/components/co_portraits.ts";
import type { ActionSummary, LogBoundary } from "#/matches/components/turn_log.ts";
import type { UnitKind } from "#/wasm/awbrn_wasm.js";

/**
 * An archive's outline, read in the page while the engine loads the same file:
 * one boundary per moment, the opening included, with the day, the seat that
 * acted and what it did. The scrubber's day ticks, the rail's log and the
 * phone's log key come from here, so they are there as soon as the file is.
 *
 * The engine replays the archive to find its turns and does not report them,
 * so this reads the archive's own record of them: an AWBW archive files every
 * action under the turn and day it was taken in, and an AWBRN archive names the
 * seat behind every action, in seat order. The screen checks the count against
 * the engine's and drops an outline that disagrees.
 *
 * Null when the archive cannot be read this way (an unknown shape, or a browser
 * without `DecompressionStream`); the screen then goes without ticks and log.
 */
export async function readArchiveOutline(bytes: Uint8Array): Promise<LogBoundary[] | null> {
  try {
    const first = bytes.find((byte) => !isWhitespace(byte));
    if (first === OPEN_BRACE) {
      return awbrnOutline(JSON.parse(new TextDecoder().decode(bytes)) as unknown);
    }
    const log = await readAwbwActionLog(bytes);
    return log === null ? null : awbwOutline(log);
  } catch (error) {
    console.warn("The replay's outline could not be read:", error);
    return null;
  }
}

/* ---------------------------------------------------------------- AWBRN */

interface AwbrnAction {
  player: number;
  command: { type: string; [field: string]: unknown };
}

/**
 * An AWBRN archive: the setup (its seats, in turn order) and every action with
 * the seat that took it. The turn passes when the seat holding it ends it or
 * resigns, and the day turns over when it passes back to an earlier seat.
 */
export function awbrnOutline(archive: unknown): LogBoundary[] | null {
  if (!isRecord(archive) || !isRecord(archive.setup) || !Array.isArray(archive.actions)) {
    return null;
  }
  const players: unknown[] = Array.isArray(archive.setup.players) ? archive.setup.players : [];
  const seats = players.length;
  const actions = archive.actions.filter(isAwbrnAction);
  if (seats === 0 || actions.length !== archive.actions.length) return null;

  const out = new Set<number>();
  // The seat holding the turn: whoever takes the next action that is not a
  // resignation, since any seat may resign while another is moving.
  const holderAfter = (index: number) =>
    actions.slice(index).find((action) => action.command.type !== "resign")?.player;
  const nextSeat = (seat: number) => {
    for (let step = 1; step <= seats; step += 1) {
      const candidate = (seat + step) % seats;
      if (!out.has(candidate)) return candidate;
    }
    return seat;
  };

  let day = 1;
  let holder = holderAfter(0) ?? 0;
  const boundaries: LogBoundary[] = [{ day, actingSlot: null }];
  actions.forEach((action, index) => {
    const type = action.command.type;
    if (type === "resign") out.add(action.player);
    const passes =
      action.player === holder && (type === "endTurn" || type === "resign" || type === "timeout");
    if (passes) {
      const next = holderAfter(index + 1) ?? nextSeat(holder);
      if (next <= holder) day += 1;
      holder = next;
    }
    const seat = players[action.player];
    const coId = isRecord(seat) && typeof seat.coId === "number" ? seat.coId : null;
    boundaries.push({ day, actingSlot: action.player, summary: awbrnSummary(action, coId) });
  });
  return boundaries;
}

function isAwbrnAction(value: unknown): value is AwbrnAction {
  return (
    isRecord(value) &&
    typeof value.player === "number" &&
    isRecord(value.command) &&
    typeof value.command.type === "string"
  );
}

const AWBRN_MOVE_VERBS: Record<string, string> = {
  attack: "Attacks",
  capture: "Captures",
  explode: "Explodes",
  hide: "Hides",
  join: "Joins",
  launch: "Launches",
  load: "Loads",
  repair: "Repairs",
  supply: "Supplies",
  unhide: "Unhides",
  unload: "Unloads",
  wait: "Moves",
};

function awbrnSummary(action: AwbrnAction, coId: number | null): ActionSummary | null {
  const command = action.command;
  switch (command.type) {
    case "build":
      return {
        unit: isUnitKind(command.unit_type) ? command.unit_type : null,
        verb: "Built",
        tile: readTile(command.position),
      };
    case "moveUnit": {
      const path = Array.isArray(command.path) ? command.path : [];
      const after = isRecord(command.action) ? command.action.type : "wait";
      // An attack from where the unit stands is a unit firing, not moving in.
      const verb =
        after === "attack" && path.length <= 1
          ? "Fires"
          : (AWBRN_MOVE_VERBS[String(after)] ?? "Moves");
      return { unit: null, verb, tile: readTile(path[path.length - 1]) };
    }
    case "unload":
      return { unit: null, verb: "Unloads", tile: readTile(command.position) };
    case "deleteUnit":
      return { unit: null, verb: "Deletes a unit", tile: null };
    case "activatePower": {
      const coKey = coId === null ? undefined : getCoPortraitByAwbwId(coId)?.key;
      const abilities = coKey ? getCoAbilities(coKey) : null;
      const isSuper = command.level === "scop";
      const power = isSuper ? abilities?.superPower : abilities?.power;
      return { unit: null, verb: power?.name ?? (isSuper ? "Super power" : "Power"), tile: null };
    }
    case "endTurn":
      return { unit: null, verb: "Ends turn", tile: null };
    case "resign":
      return { unit: null, verb: "Resigns", tile: null };
    case "timeout":
      return { unit: null, verb: "Runs out of time", tile: null };
    default:
      return null;
  }
}

/* ---------------------------------------------------------------- AWBW */

/**
 * An AWBW archive's action log: one line per turn,
 * `p:<player>;d:<day>;a:<PHP array of JSON actions>`. Every action in a line
 * was taken in that turn; the boundary after a turn's last action is already
 * the next turn's.
 */
export function awbwOutline(log: Uint8Array): LogBoundary[] | null {
  const turns = readAwbwTurns(log);
  if (turns.length === 0) return null;

  const boundaries: LogBoundary[] = [{ day: turns[0]!.day, actingSlot: null }];
  turns.forEach((turn, turnIndex) => {
    const nextDay = turns[turnIndex + 1]?.day ?? turn.day;
    turn.actions.forEach((action, actionIndex) => {
      const isLast = actionIndex === turn.actions.length - 1;
      boundaries.push({
        day: isLast ? nextDay : turn.day,
        actingSlot: turn.player,
        summary: awbwSummary(action),
      });
    });
  });
  return boundaries;
}

interface AwbwTurn {
  player: number;
  day: number;
  actions: unknown[];
}

const TURN_HEAD = /^p:(\d+);d:(\d+);/;

function readAwbwTurns(log: Uint8Array): AwbwTurn[] {
  const turns: AwbwTurn[] = [];
  const decoder = new TextDecoder();
  let start = 0;
  while (start < log.length) {
    let end = log.indexOf(NEWLINE, start);
    if (end === -1) end = log.length;
    const line = log.subarray(start, end);
    start = end + 1;
    const head = TURN_HEAD.exec(decoder.decode(line.subarray(0, 48)));
    if (!head) continue;
    turns.push({
      player: Number(head[1]),
      day: Number(head[2]),
      actions: readSerializedStrings(line, head[0].length).map((json) => {
        try {
          return JSON.parse(decoder.decode(json)) as unknown;
        } catch {
          return null;
        }
      }),
    });
  }
  return turns;
}

/**
 * Every PHP string (`s:<bytes>:"…";`) in a serialized value, in order. The
 * array that holds a turn's actions has no other strings in it, and each
 * string is skipped whole, so nothing inside an action's JSON is mistaken for
 * the next one.
 */
function readSerializedStrings(line: Uint8Array, from: number): Uint8Array[] {
  const strings: Uint8Array[] = [];
  let at = from;
  while (at < line.length) {
    const marker = findStringMarker(line, at);
    if (marker === -1) break;
    let cursor = marker + 2;
    let length = 0;
    while (cursor < line.length && isDigit(line[cursor]!)) {
      length = length * 10 + (line[cursor]! - DIGIT_ZERO);
      cursor += 1;
    }
    if (line[cursor] !== COLON || line[cursor + 1] !== QUOTE) {
      at = marker + 2;
      continue;
    }
    const begin = cursor + 2;
    strings.push(line.subarray(begin, begin + length));
    at = begin + length;
  }
  return strings;
}

function findStringMarker(line: Uint8Array, from: number): number {
  for (let index = from; index < line.length - 2; index += 1) {
    if (line[index] === LETTER_S && line[index + 1] === COLON && isDigit(line[index + 2]!)) {
      return index;
    }
  }
  return -1;
}

const AWBW_VERBS: Record<string, string> = {
  AttackSeam: "Attacks a pipe seam",
  Build: "Built",
  Capt: "Captures",
  Delete: "Deletes a unit",
  End: "Ends turn",
  Explode: "Explodes",
  Hide: "Hides",
  Join: "Joins",
  Launch: "Launches",
  Load: "Loads",
  Move: "Moves",
  Repair: "Repairs",
  Resign: "Resigns",
  Supply: "Supplies",
  Tag: "Tags",
  Unhide: "Unhides",
  Unload: "Unloads",
};

function awbwSummary(action: unknown): ActionSummary | null {
  if (!isRecord(action) || typeof action.action !== "string") return null;
  const kind = action.action;
  if (kind === "Power") {
    return {
      unit: null,
      verb: typeof action.powerName === "string" ? action.powerName : "Power",
      tile: null,
    };
  }
  // The unit that acted: a new unit, the unit an action moved first, or the
  // unit a plain move or an unload names.
  const unit =
    seenUnit(isRecord(action.newUnit) ? action.newUnit : null) ??
    seenUnit(isRecord(action.Move) && isRecord(action.Move.unit) ? action.Move.unit : null) ??
    seenUnit(isRecord(action.unit) ? action.unit : null);
  const verb = kind === "Fire" ? (unit === null ? "Fires" : "Attacks") : (AWBW_VERBS[kind] ?? kind);
  const tile = unit?.tile ?? (kind === "Fire" ? attackerTile(action) : null);
  return { unit: unit?.kind ?? null, verb, tile };
}

/** A unit as an AWBW action reports it, keyed by who can see it. */
function seenUnit(
  byViewer: Record<string, unknown> | null,
): { kind: UnitKind | null; tile: { x: number; y: number } | null } | null {
  if (byViewer === null) return null;
  const unit = [byViewer.global, ...Object.values(byViewer)].find(isRecord);
  if (!unit || typeof unit.units_name !== "string") return null;
  return {
    kind: awbwUnitKind(unit.units_name),
    tile: readTile({ x: unit.units_x, y: unit.units_y }),
  };
}

function attackerTile(action: Record<string, unknown>): { x: number; y: number } | null {
  const fire = isRecord(action.Fire) ? action.Fire : null;
  const vision = fire && isRecord(fire.combatInfoVision) ? fire.combatInfoVision : null;
  const seen = vision ? [vision.global, ...Object.values(vision)].find(isRecord) : undefined;
  const info = seen && isRecord(seen.combatInfo) ? seen.combatInfo : null;
  const attacker = info && isRecord(info.attacker) ? info.attacker : null;
  return attacker ? readTile({ x: attacker.units_x, y: attacker.units_y }) : null;
}

/** `Md.Tank` → `md-tank`, `Neotank` → `neo-tank`, `Black Boat` → `black-boat`. */
function awbwUnitKind(name: string): UnitKind | null {
  const slug = name
    .toLowerCase()
    .replace(/[.\s]+/g, "-")
    .replace(/^neotank$/, "neo-tank");
  return isUnitKind(slug) ? slug : null;
}

/**
 * The action log inside an AWBW .zip: the entry named `a<game id>`, itself
 * gzipped. Read with the browser's own `DecompressionStream`.
 */
async function readAwbwActionLog(zip: Uint8Array): Promise<Uint8Array | null> {
  if (typeof DecompressionStream === "undefined") return null;
  const view = new DataView(zip.buffer, zip.byteOffset, zip.byteLength);
  const directoryEnd = findDirectoryEnd(view);
  if (directoryEnd === -1) return null;
  const entries = view.getUint16(directoryEnd + 10, true);
  let at = view.getUint32(directoryEnd + 16, true);
  const decoder = new TextDecoder();
  for (let entry = 0; entry < entries; entry += 1) {
    if (at + 46 > zip.length || view.getUint32(at, true) !== CENTRAL_ENTRY) return null;
    const method = view.getUint16(at + 10, true);
    const size = view.getUint32(at + 20, true);
    const nameLength = view.getUint16(at + 28, true);
    const extraLength = view.getUint16(at + 30, true);
    const commentLength = view.getUint16(at + 32, true);
    const localAt = view.getUint32(at + 42, true);
    const name = decoder.decode(zip.subarray(at + 46, at + 46 + nameLength));
    at += 46 + nameLength + extraLength + commentLength;
    if (!/(^|\/)a\d+$/.test(name)) continue;
    if (view.getUint32(localAt, true) !== LOCAL_ENTRY) return null;
    const dataAt =
      localAt + 30 + view.getUint16(localAt + 26, true) + view.getUint16(localAt + 28, true);
    const stored = zip.subarray(dataAt, dataAt + size);
    const content =
      method === 0 ? stored : method === 8 ? await decompress(stored, "deflate-raw") : null;
    if (content === null) return null;
    return content[0] === 0x1f && content[1] === 0x8b ? gunzipMembers(content) : content;
  }
  return null;
}

/**
 * Gunzip a file of several gzip members, one after another (AWBW writes one
 * per turn). A browser's gzip stream stops at the end of the first member and
 * refuses what follows it, so each member is cut out and read on its own. The
 * members share their 10-byte header, which is what they are cut at.
 */
async function gunzipMembers(gzipped: Uint8Array): Promise<Uint8Array> {
  const header = gzipped.subarray(0, 10);
  const starts = [0];
  for (let at = 10; at <= gzipped.length - header.length; at += 1) {
    if (header.every((byte, index) => gzipped[at + index] === byte)) starts.push(at);
  }
  const members = await Promise.all(
    starts.map((start, index) =>
      decompress(gzipped.subarray(start, starts[index + 1] ?? gzipped.length), "gzip"),
    ),
  );
  const joined = new Uint8Array(members.reduce((total, member) => total + member.length, 0));
  let offset = 0;
  for (const member of members) {
    joined.set(member, offset);
    offset += member.length;
  }
  return joined;
}

function findDirectoryEnd(view: DataView): number {
  const lowest = Math.max(0, view.byteLength - 22 - 0xffff);
  for (let at = view.byteLength - 22; at >= lowest; at -= 1) {
    if (view.getUint32(at, true) === DIRECTORY_END) return at;
  }
  return -1;
}

async function decompress(bytes: Uint8Array, format: CompressionFormat): Promise<Uint8Array> {
  const stream = new Blob([bytes as BlobPart])
    .stream()
    .pipeThrough(new DecompressionStream(format));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

/* ---------------------------------------------------------------- shared */

const UNIT_KINDS: ReadonlySet<string> = new Set<UnitKind>([
  "anti-air",
  "apc",
  "artillery",
  "b-copter",
  "battleship",
  "black-boat",
  "black-bomb",
  "bomber",
  "carrier",
  "cruiser",
  "fighter",
  "infantry",
  "lander",
  "md-tank",
  "mech",
  "mega-tank",
  "missile",
  "neo-tank",
  "piperunner",
  "recon",
  "rocket",
  "stealth",
  "sub",
  "t-copter",
  "tank",
]);

function isUnitKind(value: unknown): value is UnitKind {
  return typeof value === "string" && UNIT_KINDS.has(value);
}

function readTile(value: unknown): { x: number; y: number } | null {
  if (!isRecord(value)) return null;
  const x = Number(value.x);
  const y = Number(value.y);
  return Number.isFinite(x) && Number.isFinite(y) ? { x, y } : null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isWhitespace(byte: number): boolean {
  return byte === 0x20 || byte === 0x09 || byte === 0x0a || byte === 0x0d;
}

function isDigit(byte: number): boolean {
  return byte >= DIGIT_ZERO && byte <= DIGIT_ZERO + 9;
}

const OPEN_BRACE = 0x7b;
const NEWLINE = 0x0a;
const LETTER_S = 0x73;
const COLON = 0x3a;
const QUOTE = 0x22;
const DIGIT_ZERO = 0x30;
const DIRECTORY_END = 0x06054b50;
const CENTRAL_ENTRY = 0x02014b50;
const LOCAL_ENTRY = 0x04034b50;
