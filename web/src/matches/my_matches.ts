import type { MatchPhase, MyMatchSummary } from "./schemas";

export const ONGOING_MATCH_PHASES = [
  "draft",
  "lobby",
  "pending",
  "starting",
  "active",
] as const satisfies readonly MatchPhase[];

const phaseRank: Record<MatchPhase, number> = {
  active: 0,
  starting: 1,
  lobby: 2,
  pending: 3,
  draft: 4,
  completed: 5,
  cancelled: 6,
};

export function myMatchPhaseRank(phase: MatchPhase): number {
  return phaseRank[phase];
}

export function formatMyMatchPhaseLabel(phase: MatchPhase): string {
  switch (phase) {
    case "active":
      return "Active";
    case "starting":
      return "Starting";
    case "lobby":
      return "Lobby";
    case "pending":
      return "Awaiting Confirmation";
    case "draft":
      return "Draft";
    case "completed":
      return "Complete";
    case "cancelled":
      return "Cancelled";
  }
}

/**
 * True when the match is waiting on the viewer rather than on anyone else.
 *
 * The nav badge counts these matches and the page marks them, and both read
 * the same rule from here so a player never reads a count of one and a page
 * with nothing on it.
 */
export function needsViewerAction(match: MyMatchSummary): boolean {
  switch (match.phase) {
    case "active":
      return (
        match.activeSlotIndex !== null &&
        match.viewerParticipants.some(
          (participant) => participant.slotIndex === match.activeSlotIndex,
        )
      );
    case "pending":
      return match.viewerParticipants.some((participant) => !participant.ready);
    case "lobby":
      return match.viewerParticipants.some(
        (participant) => !participant.ready || participant.coId === null,
      );
    default:
      return false;
  }
}

export function myMatchActionLabel(phase: MatchPhase): string {
  switch (phase) {
    case "active":
      return "Open Match";
    case "starting":
      return "View Starting Match";
    case "lobby":
    case "draft":
      return "Open Lobby";
    case "pending":
      return "Confirm Match";
    case "completed":
      return "View Match";
    case "cancelled":
      return "View Match";
  }
}

export function groupMyMatchRows<T extends { matchId: string }>(rows: T[]): T[][] {
  const grouped = new Map<string, T[]>();
  for (const row of rows) {
    const current = grouped.get(row.matchId);
    if (current) current.push(row);
    else grouped.set(row.matchId, [row]);
  }
  return [...grouped.values()];
}
