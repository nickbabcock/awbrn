/**
 * Sides in a team match.
 *
 * A match's teams are one number per seat, by slot index: 1 is side A, 2 is
 * side B, and so on. An empty list is a free-for-all. Turn order never changes
 * with a side, so these are only ever read beside the seats, never sorted by.
 */

/** At most eight sides, A to H. */
export const MAX_SIDES = 8;

/** The letter a side is shown as: 1 is A. */
export function teamLetter(team: number): string {
  return String.fromCharCode(64 + team);
}

/** The match's sides, in letter order. */
export function sidesOf(teams: readonly number[]): number[] {
  return [...new Set(teams)].sort((left, right) => left - right);
}

/** How many sides the match has. */
export function sideCount(teams: readonly number[]): number {
  return new Set(teams).size;
}

/**
 * The match's shape as a figure, sides in letter order: `4v1`, `2v2v1`.
 */
export function teamShape(teams: readonly number[]): string {
  const counts = new Map<number, number>();
  for (const team of teams) counts.set(team, (counts.get(team) ?? 0) + 1);
  return [...counts]
    .sort(([left], [right]) => left - right)
    .map(([, count]) => count)
    .join("v");
}

/**
 * Renumber the sides 1..k in the order their letters had, so a side that
 * empties takes no letter with it.
 */
export function packTeams(teams: readonly number[]): number[] {
  const letters = sidesOf(teams);
  return teams.map((team) => letters.indexOf(team) + 1);
}

/** Turning Teams on: the first half of the seats, in turn order, on A, the rest on B. */
export function splitTeams(seatCount: number): number[] {
  const firstHalf = Math.ceil(seatCount / 2);
  return Array.from({ length: seatCount }, (_, slotIndex) => (slotIndex < firstHalf ? 1 : 2));
}

/**
 * What is wrong with a match's teams, or null when they hold.
 *
 * Every seat is on a side or none is, there are two sides at least (a match
 * of one side has nobody to play), some side holds two seats (every seat alone
 * is a free-for-all, which the in-match roster draws without tags), and the
 * letters run from A with no gap.
 */
export function teamsProblem(teams: readonly number[], seatCount: number): string | null {
  if (teams.length === 0) return null;
  if (teams.length !== seatCount) return "every seat needs a team";
  if (teams.some((team) => !Number.isInteger(team) || team < 1 || team > MAX_SIDES)) {
    return "a team is outside A to H";
  }
  const sides = sideCount(teams);
  if (sides < 2) return "a team match needs two teams at least";
  if (sides === teams.length) return "a team match needs two seats on one team";
  if (teams.some((team) => team > sides)) return "teams must be lettered from A without a gap";
  return null;
}

/** One side a seat can be moved to, as its key's menu lists it. */
export interface TeamChoice {
  team: number;
  /** The seats on that side now. */
  slots: number[];
  isCurrent: boolean;
  /** Taking it would leave a single side. */
  isDisabled: boolean;
  /** A side nobody is on yet. */
  isNew: boolean;
}

/**
 * The sides a seat may move to: every side, then a new one.
 *
 * A new side is offered only while there is room for one, while it would
 * still leave two seats sharing a side (every seat alone is a free-for-all by
 * another name), and to a seat that is not already alone, because a lone seat
 * on a new side packs straight back to where it was.
 */
export function teamChoices(teams: readonly number[], slotIndex: number): TeamChoice[] {
  const current = teams[slotIndex]!;
  const sides = sideCount(teams);
  const isAlone = teams.filter((team) => team === current).length === 1;
  const choices: TeamChoice[] = Array.from({ length: sides }, (_, index) => {
    const team = index + 1;
    return {
      team,
      slots: slotsOn(teams, team),
      isCurrent: team === current,
      // A lone seat joining another side empties its own: with two sides,
      // that leaves one.
      isDisabled: team !== current && isAlone && sides === 2,
      isNew: false,
    };
  });
  if (!isAlone && sides < Math.min(MAX_SIDES, teams.length - 1)) {
    choices.push({ team: sides + 1, slots: [], isCurrent: false, isDisabled: false, isNew: true });
  }
  return choices;
}

/** The teams once a seat moves, packed; null when the move is not one the menu offers. */
export function moveSeat(
  teams: readonly number[],
  slotIndex: number,
  team: number,
): number[] | null {
  if (slotIndex < 0 || slotIndex >= teams.length) return null;
  const choice = teamChoices(teams, slotIndex).find((entry) => entry.team === team);
  if (!choice || choice.isDisabled) return null;
  const next = [...teams];
  next[slotIndex] = team;
  return packTeams(next);
}

/** The seats on one side, in turn order. */
export function slotsOn(teams: readonly number[], team: number): number[] {
  return teams.flatMap((entry, slotIndex) => (entry === team ? [slotIndex] : []));
}

/**
 * The side an engine or replay team id names: `team-2` (a match's side, A to
 * H) or an AWBW letter (`B`, any letter a replay uses). Null for an id that is
 * one army alone (`player-3`).
 */
export function teamFromId(id: string | null | undefined): number | null {
  const side = id ? /^team-(\d+)$/.exec(id)?.[1] : undefined;
  if (side !== undefined) {
    const team = Number(side);
    return team >= 1 && team <= MAX_SIDES ? team : null;
  }
  return id && /^[A-Z]$/.test(id) ? id.charCodeAt(0) - 64 : null;
}
