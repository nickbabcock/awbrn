/**
 * Every commanding officer AWBRN knows, by name and by AWBW id.
 *
 * This is the roster on its own, without the sprite sheet the portraits are
 * cut from, so a server that has to decide whether a CO id is real can read it
 * without pulling a texture into its bundle. `components/co_portraits.ts` adds
 * the picture on top of these same entries.
 */

import coPortraitAtlas from "../../assets/data/co_portraits.json";

/** The entry that stands for a seat which has chosen no CO yet. */
export const DEFAULT_CO_PORTRAIT_KEY = "no-co";

export interface CoRosterEntry {
  key: string;
  displayName: string;
  awbwId: number;
}

/**
 * The COs a player can be, in the order they are read.
 *
 * Alphabetical rather than by AWBW id: a picker is scanned for a name, and the
 * id order is an accident of the order AWBW added them.
 */
export const coRoster: readonly CoRosterEntry[] = coPortraitAtlas.portraits
  .filter((portrait) => portrait.key !== DEFAULT_CO_PORTRAIT_KEY)
  .map(({ awbwId, displayName, key }) => ({ awbwId, displayName, key }))
  .sort((left, right) => left.displayName.localeCompare(right.displayName));

const coById = new Map(coRoster.map((co) => [co.awbwId, co]));

/** True while `coId` names a CO this build knows. */
export function isKnownCoId(coId: number): boolean {
  return coById.has(coId);
}

/** The CO with this id, or null when no CO has it. */
export function getCoById(coId: number | null | undefined): CoRosterEntry | null {
  return coId === null || coId === undefined ? null : (coById.get(coId) ?? null);
}

/** What a seat's CO is called, including the seat that has not chosen one. */
export function coDisplayName(coId: number | null | undefined): string {
  return getCoById(coId)?.displayName ?? "No CO";
}
