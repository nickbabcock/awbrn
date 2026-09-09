/**
 * What the editor screen knows without asking the engine.
 *
 * The rules of an edit live in the map crate and the board is the engine's
 * own, so this file holds only the two things a screen has to decide for
 * itself: what each control is called, and what the readout says about the
 * board it is looking at.
 */

import type { EditorStateChanged, PaletteCell, PaletteGroup, Symmetry } from "#/wasm/awbrn_wasm.js";
import { factions, getFactionByCode } from "#/factions.ts";

/** Every mode, in the order the picker offers them. */
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

/**
 * What each mode is called, and what it does to an edit.
 *
 * The name says the fold and the line says what a stroke costs, because those
 * are two different questions: a map maker choosing a mode is asking the
 * first, and a map maker who has just painted four headquarters is asking the
 * second.
 */
export const SYMMETRY_LABELS: Record<Symmetry, string> = {
  none: "Free",
  "mirror-left-right": "Left and right",
  "mirror-top-bottom": "Top and bottom",
  rotate180: "Half turn",
  rotate90: "Quarter turn",
  "mirror-diagonal": "Diagonal",
  "mirror-anti-diagonal": "Anti-diagonal",
  "quad-mirror": "Quarters",
};

export const SYMMETRY_BLURBS: Record<Symmetry, string> = {
  none: "One stroke, one tile. Nothing is repeated.",
  "mirror-left-right": "The left half faces the right. Two armies.",
  "mirror-top-bottom": "The top half faces the bottom. Two armies.",
  rotate180: "Turned about the middle. Two armies, facing corners.",
  rotate90: "Turned a quarter at a time. Four armies, one corner each.",
  "mirror-diagonal": "Folded on the diagonal from the top left. Two armies.",
  "mirror-anti-diagonal": "Folded on the diagonal from the top right. Two armies.",
  "quad-mirror": "Mirrored on both axes. Four armies, one quarter each.",
};

/** Why a mode is out of reach. There is only ever one reason. */
export const SQUARE_ONLY_REASON =
  "This fold exchanges the sides of the board, so it needs a square one.";

/**
 * The three drawers the rail files its brushes in.
 *
 * The palette arrives in five groups, which is five headings down a rail that
 * has to fit on the screen beside the board. Ground, water and ways are one
 * job — drawing the land — and they keep their order inside the drawer, so the
 * grouping survives without a heading for each of them.
 */
export type PaletteDrawer = "terrain" | "property" | "unit";

const PALETTE_DRAWERS: readonly { drawer: PaletteDrawer; label: string; groups: PaletteGroup[] }[] =
  [
    { drawer: "terrain", label: "Land", groups: ["ground", "water", "ways"] },
    { drawer: "property", label: "Buildings", groups: ["property"] },
    { drawer: "unit", label: "Units", groups: ["unit"] },
  ];

export interface PaletteSection {
  drawer: PaletteDrawer;
  label: string;
  cells: PaletteCell[];
}

/**
 * The two brushes that take something away rather than putting one down.
 *
 * They have no sprite of their own, so the engine's palette does not carry
 * them. Each is filed last in the drawer it empties: a key that takes
 * something away sits at the end of the tray, not among the tiles.
 */
export const ERASERS: readonly PaletteCell[] = [
  { brush: { kind: "erase" }, name: "Erase", group: "ground", defense: 0 },
  { brush: { kind: "erase-unit" }, name: "Lift unit", group: "unit", defense: 0 },
];

/**
 * The palette, one drawer at a time.
 *
 * The order is fixed rather than read off the cells, so a drawer keeps its
 * place on the rail when an army with no headquarters removes one cell from
 * it. A rail whose drawers move is a rail nobody learns.
 *
 * This is the one order in the editor. The rail draws it and the keys step
 * through it, so `[` moves to the tile that sits to the left on the screen
 * rather than to whichever cell a second list happened to put there.
 */
export function paletteSections(cells: readonly PaletteCell[]): PaletteSection[] {
  return PALETTE_DRAWERS.map(({ drawer, label, groups }) => ({
    drawer,
    label,
    cells: [
      ...groups.flatMap((group) => cells.filter((cell) => cell.group === group)),
      ...ERASERS.filter((eraser) => drawerOfBrush(eraser.brush) === drawer),
    ],
  })).filter((section) => section.cells.some((cell) => !isEraser(cell.brush)));
}

/** Whether a brush takes something off the board rather than putting it down. */
export function isEraser(brush: PaletteCell["brush"]): boolean {
  return brush.kind === "erase" || brush.kind === "erase-unit";
}

/**
 * The brush one step along the open drawer from the one that is loaded.
 *
 * Stepping is what makes a rail of fifty keys usable without the mouse: a map
 * maker who is on Plain and wants Wood presses one key rather than crossing
 * the screen and back. The drawer wraps, because a rail that stops at the end
 * makes the last key the hardest one to reach.
 *
 * A brush that is not in the open drawer — the board just handed one over, or
 * the army changed under it — steps to the front rather than nowhere.
 */
export function stepBrush(
  cells: readonly PaletteCell[],
  loaded: PaletteCell["brush"] | null,
  step: 1 | -1,
): PaletteCell["brush"] | null {
  if (cells.length === 0) return null;

  const loadedKey = loaded === null ? null : brushKey(loaded);
  const at = cells.findIndex((cell) => brushKey(cell.brush) === loadedKey);
  if (at === -1) return cells[0]?.brush ?? null;

  const next = (at + step + cells.length) % cells.length;
  return cells[next]?.brush ?? null;
}

/** The eraser that empties `drawer`: the ground in a drawer of ground, the unit in a drawer of units. */
export function eraserOfDrawer(drawer: PaletteDrawer): PaletteCell["brush"] {
  return drawer === "unit" ? { kind: "erase-unit" } : { kind: "erase" };
}

/** Which drawer a brush is filed in, so the rail can open it. */
export function drawerOfBrush(brush: PaletteCell["brush"]): PaletteDrawer {
  switch (brush.kind) {
    case "property":
      return "property";
    case "unit":
    case "erase-unit":
      return "unit";
    default:
      return "terrain";
  }
}

/**
 * A key that tells one brush from another.
 *
 * The engine reports the loaded brush back as a value rather than as an index,
 * so the rail needs to recognise its own cell in that answer. Two brushes are
 * the same brush when they would paint the same thing.
 */
export function brushKey(brush: PaletteCell["brush"]): string {
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

/**
 * A key's name, with the point it may break at marked.
 *
 * A key is five to a rail and holds about six letters on a line, so a long
 * name takes two. Left alone the browser breaks it wherever the line runs out
 * — "SUBMARIN E" — because these are single words with nothing in them to
 * break on. A soft hyphen is a break the browser prefers over that, so the
 * name reads as one word split rather than two words misspelled.
 *
 * Only names that cannot fit are listed. Automatic hyphenation is not used
 * because it needs a dictionary the browser may not have, and a rail that
 * reads differently on two machines is worse than one short list here.
 */
const BREAKS: Record<string, string> = {
  Artillery: "Artil\u00ADlery",
  Battleship: "Battle\u00ADship",
  Infantry: "Infan\u00ADtry",
  Mountain: "Moun\u00ADtain",
  Piperunner: "Pipe\u00ADrunner",
  Submarine: "Sub\u00ADmarine",
  Teleport: "Tele\u00ADport",
};

export function keyName(name: string): string {
  return BREAKS[name] ?? name;
}

/** The fewest and the most armies a fold can share an edit out to. */
export const MIN_ROSTER = 2;
export const MAX_ROSTER = 8;

/**
 * The roster at a new size, keeping the armies it already seats.
 *
 * A map that comes out of the catalog is seated by the armies drawn on it,
 * which are not always the first the game lists: a board of Red Fire and Brown
 * Desert is a board of Red Fire and Brown Desert. Growing the roster adds the
 * next armies the game lists; shrinking it drops from the end. Neither
 * rewrites the seats the board arrived with.
 */
export function rosterOfSize(roster: readonly string[], size: number): string[] {
  const wanted = Math.min(Math.max(size, MIN_ROSTER), MAX_ROSTER);
  const seats = roster.slice(0, wanted);
  for (const faction of factions) {
    if (seats.length >= wanted) break;
    if (!seats.includes(faction.code)) seats.push(faction.code);
  }
  return seats;
}

/** How serious a note about the board is. */
export type BoardNoteTone = "ready" | "warning";

export interface BoardNote {
  tone: BoardNoteTone;
  message: string;
}

/**
 * What the board still needs before it is a match.
 *
 * Every note here is something a board cannot be played without: an army with
 * no headquarters cannot be beaten, an army with no base cannot build, and
 * ground that does not fold was drawn with the fold turned off.
 *
 * What is deliberately absent is any note about the armies holding different
 * things. A competitive map answers the first-turn advantage with buildings
 * and units that are meant to be uneven — a base the second army takes on turn
 * one, a city one side reaches first. Counting those and calling them a
 * problem is the editor arguing with the map maker about a decision that is
 * theirs. The muster still prints what each army holds, because the counts are
 * worth seeing; it just does not have an opinion about them.
 *
 * Nothing here blocks a save. A map maker part way through a board knows it is
 * unfinished; the readout is there to say what is left, not to argue.
 */
export function boardNotes(state: EditorStateChanged): BoardNote[] {
  const notes: BoardNote[] = [];

  if (state.armies.length === 0) {
    return [{ tone: "warning", message: "No army holds anything yet. Place a headquarters." }];
  }

  if (state.armies.length === 1) {
    notes.push({ tone: "warning", message: "One army holds the board. A match needs two." });
  }

  const withoutHq = state.armies.filter((army) => army.headquarters === 0);
  if (withoutHq.length > 0) {
    notes.push({
      tone: "warning",
      message: `${armyNames(withoutHq.map((army) => army.faction))} ${
        withoutHq.length === 1 ? "holds" : "hold"
      } no headquarters.`,
    });
  }

  const doubledHq = state.armies.filter((army) => army.headquarters > 1);
  if (doubledHq.length > 0) {
    notes.push({
      tone: "warning",
      message: `${armyNames(doubledHq.map((army) => army.faction))} ${
        doubledHq.length === 1 ? "holds" : "hold"
      } more than one headquarters.`,
    });
  }

  const withoutProduction = state.armies.filter((army) => army.production === 0);
  if (withoutProduction.length > 0) {
    notes.push({
      tone: "warning",
      message: `${armyNames(withoutProduction.map((army) => army.faction))} cannot build: no base, airport or port.`,
    });
  }

  if (state.symmetry !== "none" && !state.symmetric) {
    notes.push({ tone: "warning", message: unevenNote(state) });
  }

  if (notes.length === 0) {
    notes.push({ tone: "ready", message: "Every army is seated and can build." });
  }

  return notes;
}

/**
 * The tiles that do not fold, named rather than counted.
 *
 * "The board does not read the same under half turn" tells a map maker that
 * something is wrong and nothing about where, which on a 19 by 19 board is 361
 * places to look. The count says how much work is left and the first tile says
 * where to start; the rest are found the same way once that one is fixed.
 */
function unevenNote(state: EditorStateChanged): string {
  const fold = SYMMETRY_LABELS[state.symmetry].toLowerCase();
  const first = state.firstUnevenTile;
  if (!first) {
    return `The board does not read the same under ${fold}.`;
  }
  const rest = state.unevenTiles - 1;
  const where = rest > 0 ? `(${first.x}, ${first.y}) and ${rest} more` : `(${first.x}, ${first.y})`;
  return `${state.unevenTiles} ${state.unevenTiles === 1 ? "tile does" : "tiles do"} not fold under ${fold}: ${where}.`;
}

/**
 * Funds per turn, written the way the HUD writes them.
 *
 * Thousands are where a map maker's eye goes — the difference between 14,000
 * and 41,000 is the whole question — so the separator is worth the character
 * it costs.
 */
export function funds(amount: number): string {
  return `${amount.toLocaleString("en-US")}`;
}

/** Armies named the way a sentence names them. */
function armyNames(codes: readonly string[]): string {
  const names = codes.map((code) => getFactionByCode(code)?.displayName ?? code);
  if (names.length <= 1) return names[0] ?? "Nobody";
  return `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}

/** The armies a board seats, which is what the catalog files it under. */
export function seatedArmies(state: EditorStateChanged | null): number {
  return state?.armies.length ?? 0;
}

/**
 * What the save button does, which is not the same thing on every board.
 *
 * A map that is being drawn for the first time is kept; a map that is being
 * edited by its author gets another revision; a map that is being edited by
 * anybody else becomes a map of their own. The button says which.
 */
export type SaveIntent = "create" | "revise" | "fork";

export function saveLabel(intent: SaveIntent): string {
  switch (intent) {
    case "create":
      return "Keep this map";
    case "revise":
      return "Save a new revision";
    case "fork":
      return "Save as my own map";
  }
}

export function saveBlurb(intent: SaveIntent, revision: number | null): string {
  switch (intent) {
    case "create":
      return "The map goes into the catalog under your name.";
    case "revise":
      return `Revision ${(revision ?? 1) + 1} takes the place of the one the board lists. The rank starts again.`;
    case "fork":
      return "The map you started from is left as it is.";
  }
}

/**
 * One row of the key legend.
 *
 * The legend is on the screen rather than behind a `?` because this is a tool
 * somebody sits at for an hour, not a page they visit: the keys are part of
 * the instrument and a map maker learning them should not have to interrupt
 * the board to read one. `keys` is spelled the way `Kbd` reads it.
 *
 * A row is a fragment rather than a sentence. An expert legend is a table of
 * keys, and a column of seven sentences beside the board is prose the map
 * maker has to read past to reach the thing they came for. The pointer
 * gestures are not here at all: they are printed under the board, where the
 * hand that makes them is, and the split gives each list one voice.
 */
export interface EditorShortcut {
  keys: readonly string[];
  does: string;
}

export const EDITOR_SHORTCUTS: readonly EditorShortcut[] = [
  { keys: ["1", "2", "3"], does: "Open a drawer" },
  { keys: ["[", "]"], does: "Step the brush" },
  { keys: ["e"], does: "Erase" },
  { keys: ["shift+1"], does: "Paint as seat 1 to 8" },
  { keys: ["mod+z"], does: "Undo · shift to redo" },
];
