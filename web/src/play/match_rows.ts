import { myMatchPhaseRank, needsViewerAction } from "#/matches/my_matches.ts";
import type { MyMatchSummary } from "#/matches/schemas.ts";

/** One of the viewer's matches, with what its row needs to say. */
export interface MatchRowData {
  match: MyMatchSummary;
  /** The match waits on the viewer. */
  isOwed: boolean;
  /** Ranked matches are titled by their map. */
  isRanked: boolean;
  /** The map a ranked match is titled by, as the ranked pools name it. */
  mapName: string | null;
  /**
   * When the clock that matters runs out: the open turn (whoever holds it),
   * or a pairing's confirmation window. Null when nothing is running.
   */
  deadlineAt: number | null;
}

/**
 * The viewer's matches, owed first and soonest deadline first, then the rest
 * in the order the match list ranks phases.
 */
export function orderMatchRows(
  matches: readonly MyMatchSummary[],
  confirmDeadlines: ReadonlyMap<string, number>,
  rankedMaps: ReadonlyMap<string, string>,
): MatchRowData[] {
  return matches
    .map((match): MatchRowData => {
      const isOwed = needsViewerAction(match);
      const deadlineAt =
        match.phase === "active" && match.turnDeadlineAt !== null
          ? Date.parse(match.turnDeadlineAt)
          : match.phase === "pending"
            ? (confirmDeadlines.get(match.matchId) ?? null)
            : null;
      const mapName = rankedMaps.get(match.matchId) ?? null;
      return { match, isOwed, isRanked: mapName !== null, mapName, deadlineAt };
    })
    .sort((left, right) => {
      if (left.isOwed !== right.isOwed) return left.isOwed ? -1 : 1;
      if (left.deadlineAt !== null && right.deadlineAt !== null) {
        return left.deadlineAt - right.deadlineAt;
      }
      if (left.deadlineAt !== null) return -1;
      if (right.deadlineAt !== null) return 1;
      return myMatchPhaseRank(left.match.phase) - myMatchPhaseRank(right.match.phase);
    });
}

/** The one key a match row carries. */
export function matchKeyLabel(row: Pick<MatchRowData, "match" | "isOwed">): string {
  switch (row.match.phase) {
    case "active":
      return row.isOwed ? "Play turn" : "Watch";
    case "pending":
      return row.isOwed ? "Confirm pairing" : "Open";
    default:
      return "Open";
  }
}
