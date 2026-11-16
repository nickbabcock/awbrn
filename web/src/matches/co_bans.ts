/**
 * CO bans, per seat.
 *
 * A match keeps one list for every seat (All, `bannedCoIds`) and each seat may
 * add its own (`seatBannedCoIds`, by slot index). A seat's bans are the union,
 * so nothing overrides anything. A seat's own list never repeats All, and each
 * seat always keeps at least one CO.
 */

import { coRoster, isKnownCoId } from "#/co_roster.ts";
import { slotsOn } from "./teams.ts";

/** All, and each seat's own additions by slot index. */
export interface SeatBanLists {
  bannedCoIds: readonly number[];
  seatBannedCoIds?: readonly (readonly number[])[];
}

const ascending = (left: number, right: number) => left - right;

/** One seat's own list as it is stored: known, once each, ascending, and apart from All. */
export function ownBans(coIds: Iterable<number>, all: Iterable<number>): number[] {
  const inAll = new Set(all);
  return [...new Set(coIds)]
    .filter((coId) => isKnownCoId(coId) && !inAll.has(coId))
    .sort(ascending);
}

/** Every CO this seat may not take. */
export function seatBans(lists: SeatBanLists, slotIndex: number): Set<number> {
  return new Set([...lists.bannedCoIds, ...(lists.seatBannedCoIds?.[slotIndex] ?? [])]);
}

/** Whether a seat with these bans still has a CO to choose. */
export function leavesACo(banned: ReadonlySet<number>): boolean {
  return coRoster.some((co) => !banned.has(co.awbwId));
}

/**
 * The seats' own lists as the match stores them, or nothing when no seat adds
 * a ban (every 1v1 that bans only for both sides).
 */
export function seatListsFor(
  seats: readonly (readonly number[])[],
  seatCount: number,
): number[][] | undefined {
  const lists = Array.from({ length: seatCount }, (_, slotIndex) => [...(seats[slotIndex] ?? [])]);
  return lists.some((list) => list.length > 0) ? lists : undefined;
}

/**
 * The bans as a new match stores them: one setting, one representation.
 *
 * A CO that every seat bans is moved into All. This happens only as the match
 * is made: once the lobby opens, All is fixed and edits reach only seat lists.
 */
export function canonicalBans(
  lists: SeatBanLists,
  seatCount: number,
): { bannedCoIds: number[]; seatBannedCoIds: number[][] | undefined } {
  const all = new Set(lists.bannedCoIds.filter(isKnownCoId));
  const seats = Array.from({ length: seatCount }, (_, slotIndex) =>
    ownBans(lists.seatBannedCoIds?.[slotIndex] ?? [], all),
  );
  for (const coId of seats[0] ?? []) {
    if (seats.every((list) => list.includes(coId))) all.add(coId);
  }
  return {
    bannedCoIds: [...all].sort(ascending),
    seatBannedCoIds: seatListsFor(
      seats.map((list) => list.filter((coId) => !all.has(coId))),
      seatCount,
    ),
  };
}

/** Whether every seat has the same bans, so All alone says them. */
export function seatsAlike(lists: SeatBanLists, seatCount: number): boolean {
  const first = [...seatBans(lists, 0)].sort(ascending).join();
  return Array.from({ length: seatCount }, (_, slotIndex) => slotIndex).every(
    (slotIndex) => [...seatBans(lists, slotIndex)].sort(ascending).join() === first,
  );
}

/** What a ban editor holds: All and every seat's own list, one per seat. */
export interface BanDraft {
  all: number[];
  seats: number[][];
}

export function banDraft(lists: SeatBanLists, seatCount: number): BanDraft {
  return {
    all: [...lists.bannedCoIds],
    seats: Array.from({ length: seatCount }, (_, slotIndex) => [
      ...(lists.seatBannedCoIds?.[slotIndex] ?? []),
    ]),
  };
}

/**
 * Ban or lift COs, for All (`"all"`) or for some seats.
 *
 * A ban lands one CO at a time, and only on a seat that keeps a CO after it:
 * a press never empties a seat, it just leaves the last stamp unpressed. A ban
 * for All needs every seat to keep one, and takes the CO out of their own
 * lists so the two never overlap.
 */
export function setBans(
  draft: BanDraft,
  target: "all" | readonly number[],
  coIds: readonly number[],
  isBanned: boolean,
): BanDraft {
  const all = new Set(draft.all);
  const seats = draft.seats.map((list) => new Set(list));
  const union = (slotIndex: number) => new Set([...all, ...seats[slotIndex]!]);
  for (const coId of coIds) {
    if (target === "all") {
      if (!isBanned) {
        all.delete(coId);
        continue;
      }
      const keepsOne = seats.every((_, slotIndex) => leavesACo(union(slotIndex).add(coId)));
      if (!keepsOne) continue;
      all.add(coId);
      for (const list of seats) list.delete(coId);
      continue;
    }
    for (const slotIndex of target) {
      const list = seats[slotIndex]!;
      if (!isBanned) list.delete(coId);
      else if (!all.has(coId) && leavesACo(union(slotIndex).add(coId))) list.add(coId);
    }
  }
  return {
    all: [...all].sort(ascending),
    seats: seats.map((list) => [...list].sort(ascending)),
  };
}

/** One line of the read-back: seats that share a side and a list. */
export interface BanLine {
  /** The side, or null in a free-for-all. */
  team: number | null;
  slots: number[];
  /** Own bans first, then those the whole side shares, then All. */
  coIds: number[];
  /** Where the All group starts in `coIds`. */
  allStart: number;
}

/**
 * The read-back: one line per distinct list (per side and list in a team
 * match), in turn order of each line's first seat. Each line starts with what
 * sets it apart from the rest.
 */
export function banLines(
  lists: SeatBanLists,
  teams: readonly number[],
  seatCount: number,
  order: (coId: number) => number,
): BanLine[] {
  const byOrder = (coIds: Iterable<number>) =>
    [...coIds].sort((left, right) => order(left) - order(right));
  const all = new Set(lists.bannedCoIds);
  const own = (slotIndex: number) => lists.seatBannedCoIds?.[slotIndex] ?? [];
  const lines: (BanLine & { key: string })[] = [];
  for (let slotIndex = 0; slotIndex < seatCount; slotIndex += 1) {
    const team = teams[slotIndex] ?? null;
    const key = `${team}:${[...own(slotIndex)].sort(ascending).join()}`;
    const line = lines.find((entry) => entry.key === key);
    if (line) {
      line.slots.push(slotIndex);
      continue;
    }
    lines.push({ key, team, slots: [slotIndex], coIds: [], allStart: 0 });
  }
  return lines.map(({ team, slots }) => {
    const side = team === null ? [] : slotsOn(teams, team);
    const isSideWide = (coId: number) =>
      side.length > 1 && side.every((slot) => own(slot).includes(coId));
    const mine = own(slots[0]!);
    const apart = byOrder(mine.filter((coId) => !isSideWide(coId)));
    const shared = byOrder(mine.filter(isSideWide));
    const coIds = [...apart, ...shared, ...byOrder(all)];
    return { team, slots, coIds, allStart: apart.length + shared.length };
  });
}
