import type { Observation, PlayerStatus } from "#/wasm/awbrn_server.js";

/**
 * How an army left the match, as its ledger row says it: `Out · Day 11` and
 * the reason. Either part is null when nothing this page has read can say it,
 * and the row then says less rather than guess.
 */
export interface ArmyFall {
  day: number | null;
  reason: string | null;
}

/** The reason a seat's status gives, in the words the match over sheet uses. */
function fallReason(status: PlayerStatus): string | null {
  switch (status) {
    case "active":
      return null;
    case "resigned":
      return "Resigned";
    case "timed-out":
      return "Clock ran out";
    case "eliminated":
      return "Defeated";
  }
}

/** Every seat an observation shows as out of the match, by slot. */
function observedFalls(observation: Observation): Map<number, PlayerStatus> {
  const falls = new Map<number, PlayerStatus>();
  for (const player of observation.players) {
    if (player.status === "active") continue;
    const slot = Number(player.id);
    if (Number.isInteger(slot)) falls.set(slot, player.status);
  }
  return falls;
}

/**
 * Record the seats an observation shows as fallen that `known` has not seen
 * fall yet. A fall first seen in an observation the page was handed as the
 * match moved happened on that observation's day; one already standing in
 * the opening observation happened on a day it does not say.
 */
export function recordFalls(
  known: ReadonlyMap<number, ArmyFall>,
  observation: Observation,
  isOpening: boolean,
): ReadonlyMap<number, ArmyFall> {
  let next: Map<number, ArmyFall> | null = null;
  for (const [slot, status] of observedFalls(observation)) {
    if (known.has(slot)) continue;
    next ??= new Map(known);
    next.set(slot, {
      day: isOpening ? null : observation.turn.day,
      reason: fallReason(status),
    });
  }
  return next ?? known;
}

/**
 * The day a seat fell, read from the match's outline when the outline alone
 * can say it.
 *
 * A seat falls somewhere between its last action and the turn it would have
 * taken next. When every action in that stretch happened on one day, that is
 * the day; when the stretch crosses midnight, the outline cannot say which
 * side the fall was on, and the answer is null.
 */
export function fallDayFromOutline(
  boundaries: readonly { day: number; actingSlot: number | null }[],
  slot: number,
  turnOrder: readonly number[],
): number | null {
  const place = turnOrder.indexOf(slot);
  if (place < 0) return null;
  let last = -1;
  for (let index = boundaries.length - 1; index >= 0; index -= 1) {
    if (boundaries[index]!.actingSlot === slot) {
      last = index;
      break;
    }
  }
  if (last < 1) return null;

  // An action is taken on the day before it: one that ends the last turn of
  // a day lands on the next.
  const takenOn = (index: number) => boundaries[index - 1]!.day;
  // The seat's next turn, had it stayed: the day after its last one, at its
  // own place in the order. The fall happened before anybody acted past it.
  const nextDay = takenOn(last) + 1;
  const days = new Set<number>();
  for (let index = last; index < boundaries.length; index += 1) {
    const acting = boundaries[index]!.actingSlot;
    const day = takenOn(index);
    if (index > last && acting !== null && acting !== slot) {
      const actingPlace = turnOrder.indexOf(acting);
      if (day > nextDay || (day === nextDay && actingPlace > place)) break;
    }
    days.add(day);
  }
  return days.size === 1 ? [...days][0]! : null;
}
