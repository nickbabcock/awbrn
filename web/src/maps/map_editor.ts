import type {
  Brush,
  EditorPalette,
  EditorStateChanged,
  EditorTile,
  PaletteCell,
  ResizeAnchor,
  Symmetry,
} from "#/wasm/awbrn_wasm.js";
import { factions } from "#/factions.ts";
import { unitCatalog, type UnitGroup } from "#/matches/unit_rules.ts";
import { MAP_SAVE_REFUSALS } from "#/maps/schemas.ts";

/** Every mode, in the order the menu offers them. */
export const SYMMETRY_MODES: readonly Symmetry[] = [
  "none",
  "mirror-left-right",
  "mirror-top-bottom",
  "rotate180",
  "rotate90",
  "mirror-diagonal",
  "mirror-anti-diagonal",
  "quad-mirror",
];

export const SYMMETRY_LABELS: Record<Symmetry, string> = {
  none: "Free",
  "mirror-left-right": "Left–right",
  "mirror-top-bottom": "Top–bottom",
  rotate180: "Half turn",
  rotate90: "Quarter turn",
  "mirror-diagonal": "Diagonal",
  "mirror-anti-diagonal": "Anti-diagonal",
  "quad-mirror": "Quarters",
};

/** The modes that swap the board's sides; adjacent in `SYMMETRY_MODES`. */
export const SQUARE_ONLY: ReadonlySet<Symmetry> = new Set([
  "rotate90",
  "mirror-diagonal",
  "mirror-anti-diagonal",
]);

export type BrushTab = "terrain" | "buildings" | "units";

export const BRUSH_TABS: readonly { tab: BrushTab; label: string; key: string }[] = [
  { tab: "terrain", label: "Terrain", key: "1" },
  { tab: "buildings", label: "Buildings", key: "2" },
  { tab: "units", label: "Units", key: "3" },
];

export const ERASE_CELL: PaletteCell = {
  brush: { kind: "erase" },
  name: "Erase",
  group: "ground",
  defense: 0,
};

export const LIFT_CELL: PaletteCell = {
  brush: { kind: "erase-unit" },
  name: "Lift unit",
  group: "unit",
  defense: 0,
};

const TRAY_COLUMNS = 7;

/**
 * A tab's cells as the tray lays them out, `null` for an empty slot: each
 * group starts a row, and the eraser takes the last row's last slot.
 */
export function trayCells(
  tab: BrushTab,
  army: EditorPalette | null,
  neutral: EditorPalette | null,
): (PaletteCell | null)[] {
  const terrain = (army ?? neutral)?.terrain ?? [];
  switch (tab) {
    case "terrain":
      return laidOut([terrain.filter((cell) => cell.group !== "property")], ERASE_CELL);
    case "buildings":
      return laidOut(
        [
          (army?.terrain ?? []).filter((cell) => cell.group === "property"),
          (neutral?.terrain ?? []).filter((cell) => cell.group === "property"),
        ],
        ERASE_CELL,
      );
    case "units": {
      const units = army?.units ?? [];
      const groups: UnitGroup[] = ["ground", "air", "sea"];
      return laidOut(
        groups.map((group) =>
          unitCatalog
            .filter((entry) => entry.group === group)
            .flatMap((entry) =>
              units.filter((cell) => cell.brush.kind === "unit" && cell.brush.unit === entry.kind),
            ),
        ),
        LIFT_CELL,
      );
    }
  }
}

function laidOut(groups: PaletteCell[][], eraser: PaletteCell): (PaletteCell | null)[] {
  const filled = groups.filter((group) => group.length > 0);
  if (filled.length === 0) return [];
  const cells: (PaletteCell | null)[] = [];
  filled.forEach((group, index) => {
    cells.push(...group);
    const end = index === filled.length - 1 ? TRAY_COLUMNS - 1 : 0;
    while (cells.length % TRAY_COLUMNS !== end) cells.push(null);
  });
  cells.push(eraser);
  return cells;
}

export function tabOfBrush(brush: Brush): BrushTab {
  switch (brush.kind) {
    case "property":
      return "buildings";
    case "unit":
    case "erase-unit":
      return "units";
    default:
      return "terrain";
  }
}

/** What an eraser leaves, which its tooltip and the brush well say. */
export const ERASER_NOTES = { erase: "Plain, no unit", "erase-unit": "Keeps the ground" } as const;

export function eraserOfTab(tab: BrushTab): Brush {
  return tab === "units" ? LIFT_CELL.brush : ERASE_CELL.brush;
}

/** Two brushes are the same brush when they would paint the same thing. */
export function brushKey(brush: Brush): string {
  switch (brush.kind) {
    case "terrain":
      return `terrain:${brush.terrain}`;
    case "connecting":
      return `connecting:${brush.connection}`;
    case "property":
      return `property:${brush.property}:${brush.faction ?? "neutral"}`;
    case "unit":
      return `unit:${brush.unit}:${brush.faction}`;
    case "erase":
      return "erase";
    case "erase-unit":
      return "erase-unit";
  }
}

/** One step along `cells`, wrapping; a brush not among them steps to the first. */
export function stepBrush(
  cells: readonly PaletteCell[],
  loaded: Brush | null,
  step: 1 | -1,
): Brush | null {
  if (cells.length === 0) return null;
  const key = loaded === null ? null : brushKey(loaded);
  const at = cells.findIndex((cell) => brushKey(cell.brush) === key);
  if (at === -1) return cells[0]?.brush ?? null;
  return cells[(at + step + cells.length) % cells.length]?.brush ?? null;
}

/** The brush recolored for `faction`; brushes no army holds pass through. */
export function brushForArmy(brush: Brush, faction: string): Brush {
  if (brush.kind === "unit") return { ...brush, faction };
  if (brush.kind === "property" && brush.faction !== undefined) return { ...brush, faction };
  return brush;
}

export const MIN_ROSTER = 2;
export const MAX_ROSTER = 8;

/** The roster at a new size: seats it holds stay, new seats follow game order. */
export function rosterOfSize(roster: readonly string[], size: number): string[] {
  const wanted = Math.min(Math.max(size, MIN_ROSTER), MAX_ROSTER);
  const seats = roster.slice(0, wanted);
  for (const faction of factions) {
    if (seats.length >= wanted) break;
    if (!seats.includes(faction.code)) seats.push(faction.code);
  }
  return seats;
}

/** The part of the old board a resize keeps, in its tiles: `[x0, x1) × [y0, y1)`. */
export function resizeKept(
  from: { width: number; height: number },
  to: { width: number; height: number },
  anchor: ResizeAnchor,
): { x0: number; y0: number; x1: number; y1: number } {
  // `resize_offset` in the map crate; Rust's division truncates toward zero.
  const dx = to.width - from.width;
  const dy = to.height - from.height;
  const [ox, oy] =
    anchor === "top-left"
      ? [0, 0]
      : anchor === "center"
        ? [Math.trunc(dx / 2), Math.trunc(dy / 2)]
        : [dx, dy];
  return {
    x0: Math.max(0, -ox),
    y0: Math.max(0, -oy),
    x1: Math.min(from.width, to.width - ox),
    y1: Math.min(from.height, to.height - oy),
  };
}

export type EditorProblem =
  | { kind: "uneven"; tiles: EditorTile[] }
  | { kind: "extra-hq"; faction: string; tiles: EditorTile[] }
  | { kind: "no-hq"; faction: string }
  | { kind: "no-build"; faction: string }
  | { kind: "empty"; factions: string[] };

/** Roster seats whose army holds nothing. */
export function emptySeats(state: Pick<EditorStateChanged, "armies" | "roster">): string[] {
  const held = new Set(state.armies.map((army) => army.faction));
  return state.roster.filter((faction) => !held.has(faction));
}

/** Every problem, in the order `N` steps through them: tile problems, then army problems. */
export function problems(state: EditorStateChanged, isPristine: boolean): EditorProblem[] {
  if (isPristine) return [];
  const list: EditorProblem[] = state.unevenGroups.map((tiles) => ({ kind: "uneven", tiles }));
  for (const army of state.armies) {
    if (army.hqAt.length > 1) {
      list.push({ kind: "extra-hq", faction: army.faction, tiles: army.hqAt });
    }
  }
  for (const army of state.armies) {
    if (army.hqAt.length === 0 && army.labs === 0) {
      list.push({ kind: "no-hq", faction: army.faction });
    }
  }
  for (const army of state.armies) {
    if (army.production === 0) list.push({ kind: "no-build", faction: army.faction });
  }
  const empty = emptySeats(state);
  if (empty.length > 0) list.push({ kind: "empty", factions: empty });
  return list;
}

export function problemTiles(problem: EditorProblem | null): EditorTile[] {
  return problem !== null && "tiles" in problem ? problem.tiles : [];
}

/** The army a problem is about, for the ledger to keep in view. */
export function problemArmy(problem: EditorProblem | null): string | null {
  if (problem === null || problem.kind === "uneven") return null;
  return problem.kind === "empty" ? (problem.factions[0] ?? null) : problem.faction;
}

/** `5, 7 · 16, 8` */
export function tileList(tiles: readonly EditorTile[]): string {
  return tiles.map((tile) => `${tile.x}, ${tile.y}`).join(" · ");
}

export type SaveIntent = "create" | "revise" | "fork";

/** Unsaved: the history moved from where it stood at open or last save, or the name changed. */
export function isDirty(
  state: Pick<EditorStateChanged, "historyId">,
  savedAt: { historyId: number; name: string },
  name: string,
): boolean {
  return state.historyId !== savedAt.historyId || name !== savedAt.name;
}

/** Red only while something is unsaved; inert where a save would write nothing. */
export function saveKey(
  intent: SaveIntent,
  isUnsaved: boolean,
): { label: string; isRed: boolean; isInert: boolean } {
  const label =
    intent === "fork"
      ? "Save as new map"
      : intent === "create"
        ? "Save map"
        : isUnsaved
          ? "Save changes"
          : "No changes";
  return { label, isRed: isUnsaved, isInert: !isUnsaved && intent !== "fork" };
}

export type SaveFailure =
  | { kind: "changed" | "missing" | "forbidden" | "busy" | "failed" }
  | { kind: "refused"; reason: string };

export function saveFailure(error: unknown): SaveFailure {
  const message = error instanceof Error ? error.message : String(error);
  if (message === MAP_SAVE_REFUSALS.changed) return { kind: "changed" };
  if (message === MAP_SAVE_REFUSALS.missing) return { kind: "missing" };
  if (message === MAP_SAVE_REFUSALS.forbidden) return { kind: "forbidden" };
  if (message.startsWith(MAP_SAVE_REFUSALS.board)) {
    return { kind: "refused", reason: message.slice(MAP_SAVE_REFUSALS.board.length) };
  }
  // `requireFnRateLimit`'s refusal.
  if (/too many requests/i.test(message)) return { kind: "busy" };
  const issues = validationIssues(message);
  if (issues !== null) return { kind: "refused", reason: issues };
  // A dropped connection, an expired session, a server fault: worth another try.
  return { kind: "failed" };
}

/** The request schema's refusal, which arrives as its issues in JSON. */
function validationIssues(message: string): string | null {
  try {
    const issues: unknown = JSON.parse(message);
    if (!Array.isArray(issues) || issues.length === 0) return null;
    const messages = issues.map((issue: { message?: unknown }) => issue?.message);
    return messages.every((text) => typeof text === "string") ? messages.join("; ") : null;
  } catch {
    return null;
  }
}

export const SAVE_FAILURE_TITLES: Record<SaveFailure["kind"], string> = {
  changed: "Changed since opened",
  missing: "Map no longer exists",
  forbidden: "No longer yours to revise",
  busy: "Too many saves just now",
  refused: "Board not accepted",
  failed: "Not saved",
};

/** The revision can't be written any more, but the draft can still become a map of its own. */
export function savesAsNewMap(failure: SaveFailure): boolean {
  return failure.kind === "changed" || failure.kind === "missing" || failure.kind === "forbidden";
}

export function nameProblem(name: string, max: number): string | null {
  const trimmed = name.trim();
  if (trimmed.length === 0) return "Name required";
  if (trimmed.length > max) return `${max} characters at most`;
  return null;
}

export function funds(amount: number): string {
  return amount.toLocaleString("en-US");
}
