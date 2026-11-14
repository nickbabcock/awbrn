import { useQuery } from "@tanstack/react-query";
import { coRoster, getCoById } from "#/co_roster.ts";
import { myMatchesQueryOptions } from "#/matches/matches.queries.ts";
import type { MyMatchSummary } from "#/matches/schemas.ts";

/** Andy, the game's own first CO, for a player with no commander on record. */
export const FIRST_CO_ID = 1;

/**
 * The commander a seat opens on, so a briefing leads with a stamp rather than
 * an empty box: the one the player last chose in another match, else Andy,
 * else the first CO the match allows. Null only when every CO is banned.
 */
export function openingCoId(
  matches: readonly MyMatchSummary[],
  bannedCoIds: ReadonlySet<number>,
  currentMatchId: string,
): number | null {
  const isAllowed = (coId: number | null): coId is number =>
    coId !== null && getCoById(coId) !== null && !bannedCoIds.has(coId);
  const last = matches
    .filter((match) => match.matchId !== currentMatchId)
    .flatMap((match) => match.viewerParticipants)
    .filter((seat) => isAllowed(seat.coId))
    .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt))[0];
  if (last?.coId != null) return last.coId;
  if (isAllowed(FIRST_CO_ID)) return FIRST_CO_ID;
  return coRoster.find((co) => isAllowed(co.awbwId))?.awbwId ?? null;
}

/**
 * `openingCoId` from the viewer's matches, once they are read. Undefined while
 * the list is on its way, so a caller does not settle on Andy a moment before
 * the player's own last pick arrives.
 */
export function useOpeningCo(
  bannedCoIds: ReadonlySet<number>,
  currentMatchId: string,
  { isEnabled }: { isEnabled: boolean },
): number | null | undefined {
  const { data, isError } = useQuery({ ...myMatchesQueryOptions(), enabled: isEnabled });
  if (!isEnabled) return undefined;
  if (data) return openingCoId(data.matches, bannedCoIds, currentMatchId);
  return isError ? openingCoId([], bannedCoIds, currentMatchId) : undefined;
}
