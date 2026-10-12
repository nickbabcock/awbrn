/**
 * How strong each commanding officer plays, in bands a player can read at a
 * glance.
 *
 * A tier is a judgment by the players, not a rule, so it does not come from
 * AWVM. Edit this file to change a band; `co_tiers.test.ts` makes sure that
 * every CO stays in exactly one band of each list.
 *
 * A CO's strength depends on the match, so there is one list for each kind of
 * match. Fog changes who is strong, and so does a free-for-all, where a power
 * that hits every army at once counts for more. A match takes the list that
 * fits its settings.
 */

import { coRoster, type CoRosterEntry } from "#/co_roster.ts";

/** The kinds of match that have their own tier list. */
export type CoTierList = "standard" | "fog" | "ffa";

/** The bands, strongest first. A band's index is its tier number. */
export const coTierNames = ["Dominant", "Strong", "Solid", "Situational", "Underdog"] as const;

export type CoTier = 0 | 1 | 2 | 3 | 4;

const tierLists: Readonly<Record<CoTierList, readonly (readonly string[])[]>> = {
  standard: [
    ["colin", "grit", "hachi", "kanbei", "sensei", "sturm"],
    ["hawke", "javier", "nell", "sasha", "von-bolt"],
    ["eagle", "kindle", "max", "olaf", "sami"],
    ["andy", "drake", "lash", "rachel"],
    ["adder", "flak", "grimm", "jake", "jess", "jugger", "koal", "sonja"],
  ],
  fog: [
    ["colin", "hachi", "kanbei", "sensei"],
    ["grit", "hawke", "javier", "nell", "sasha", "sturm", "von-bolt"],
    ["eagle", "max", "olaf"],
    ["andy", "drake", "kindle", "lash", "rachel", "sonja"],
    ["adder", "flak", "grimm", "jake", "jess", "jugger", "koal", "sami"],
  ],
  ffa: [
    ["colin", "grit", "hachi", "kanbei", "sensei", "sturm"],
    ["hawke", "javier", "nell", "sasha", "von-bolt"],
    ["drake", "eagle", "kindle", "olaf", "rachel", "sami"],
    ["andy", "lash", "max"],
    ["adder", "flak", "grimm", "jake", "jess", "jugger", "koal", "sonja"],
  ],
};

/**
 * The list that fits a match.
 *
 * A match of more than two armies takes the free-for-all list, with or without
 * fog, because AWBW keeps no list for fog in a free-for-all and the number of
 * armies changes more than the fog does.
 */
export function coTierListFor({
  fogEnabled,
  playerCount,
}: {
  fogEnabled: boolean;
  playerCount: number;
}): CoTierList {
  if (playerCount > 2) return "ffa";
  return fogEnabled ? "fog" : "standard";
}

/** The CO's tier in a list, or null for a CO that no list knows yet. */
export function coTierOf(coKey: string, list: CoTierList): CoTier | null {
  const index = tierLists[list].findIndex((band) => band.includes(coKey));
  return index === -1 ? null : (index as CoTier);
}

export interface CoTierBand {
  tier: CoTier;
  name: (typeof coTierNames)[number];
  cos: CoRosterEntry[];
}

/**
 * The roster cut into bands, strongest first, alphabetical inside a band.
 *
 * A CO that no list knows yet goes into the last band, so a new CO is always
 * on the board even before somebody rates it.
 */
export function coTierBands(list: CoTierList): CoTierBand[] {
  const bands: CoTierBand[] = coTierNames.map((name, tier) => ({
    tier: tier as CoTier,
    name,
    cos: [],
  }));
  for (const co of coRoster) {
    const tier = coTierOf(co.key, list) ?? 4;
    bands[tier]!.cos.push(co);
  }
  return bands.filter((band) => band.cos.length > 0);
}
