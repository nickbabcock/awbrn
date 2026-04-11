import type { MatchPhase } from "./schemas";

export const ONGOING_MATCH_PHASES = [
  "draft",
  "lobby",
  "starting",
  "active",
] as const satisfies readonly MatchPhase[];

const phaseRank: Record<MatchPhase, number> = {
  active: 0,
  starting: 1,
  lobby: 2,
  draft: 3,
  completed: 4,
  cancelled: 5,
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
    case "draft":
      return "Draft";
    case "completed":
      return "Complete";
    case "cancelled":
      return "Cancelled";
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
    case "completed":
      return "View Match";
    case "cancelled":
      return "View Match";
  }
}
