import { getCoPortraitByAwbwId } from "#/components/co_portraits.ts";
import { getFactionById } from "#/factions.ts";
import type { MatchParticipantSnapshot } from "#/matches/schemas.ts";
import type { PlayerRosterEntry, PlayerRosterSnapshot } from "#/wasm/awbrn_wasm.js";

/**
 * A seat, and whatever the engine currently knows about it.
 *
 * The seats are known from the match record before the board is running, so the
 * armies list is built from the record and the engine's statistics are merged in
 * as they arrive. The list therefore never starts empty and never reflows.
 *
 * A finished match reads the same way as a live one: the record still names the
 * seats and the engine still reports the statistics, so both pages describe
 * their armies from here rather than each from a copy of its own.
 */
export interface MatchArmy {
  entry: PlayerRosterEntry;
  hasLiveStats: boolean;
  isActive: boolean;
  name: string;
}

export function buildArmies(
  participants: MatchParticipantSnapshot[],
  playerRoster: PlayerRosterSnapshot | null,
): MatchArmy[] {
  const liveEntries = new Map(
    (playerRoster?.players ?? []).map((player) => [player.playerId, player]),
  );

  return participants.map((participant, index) => {
    const liveEntry = liveEntries.get(participant.slotIndex);

    return {
      entry: liveEntry ?? seatEntry(participant, index),
      hasLiveStats: liveEntry !== undefined,
      isActive: playerRoster?.activePlayerId === participant.slotIndex,
      name: participant.userName,
    };
  });
}

/**
 * A seat as the match record describes it, with every statistic still unknown.
 * The readouts render their own "--" until the engine reports real values.
 */
function seatEntry(participant: MatchParticipantSnapshot, index: number): PlayerRosterEntry {
  const faction = getFactionById(participant.factionId);
  const factionCode = faction?.code ?? "os";
  const factionName = faction?.displayName ?? "Orange Star";
  const portrait = getCoPortraitByAwbwId(participant.coId);

  return {
    playerId: participant.slotIndex,
    userId: 0,
    turnOrder: index,
    team: undefined,
    eliminated: false,
    actualFactionCode: factionCode,
    actualFactionName: factionName,
    displayFactionCode: factionCode,
    displayFactionName: factionName,
    factionCode,
    factionName,
    coKey: portrait?.key,
    coName: portrait?.displayName,
    tagCoKey: undefined,
    tagCoName: undefined,
    powerCharge: undefined,
    copCost: undefined,
    scopCost: undefined,
    powerStarCharge: undefined,
    activePower: undefined,
    stats: {
      funds: undefined,
      income: undefined,
      unitCount: undefined,
      unitValue: undefined,
      properties: undefined,
      comTowers: undefined,
    },
  };
}
