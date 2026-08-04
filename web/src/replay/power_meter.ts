import type { PlayerRosterEntry } from "#/wasm/awbrn_wasm.js";

/**
 * A CO power meter reduced to what a bar has to draw.
 *
 * Every value arrives from AWVM already scaled for the number of powers the CO
 * has used, so nothing here restates the ruleset. One star is worth
 * `starCharge`; a level costs a whole number of those stars.
 */
export interface PowerMeterReading {
  /** Charge the CO holds now. */
  charge: number;
  /** Charge one star is worth at this CO's current use count. */
  starCharge: number;
  /** Stars the CO holds now, including the fraction of the star in progress. */
  charged: number;
  cop: PowerLevelCost | null;
  scop: PowerLevelCost | null;
  /** The longer of the two powers, which is the length of the bar. */
  totalStars: number;
  level: PowerLevel;
}

/** One power's price, in both the units a bar draws and the ones AWVM counts. */
export interface PowerLevelCost {
  stars: number;
  charge: number;
  /** Charge still to earn before the power can be used. */
  remaining: number;
}

/** Which power the current charge pays for. */
export type PowerLevel = "charging" | "cop" | "scop";

/**
 * A meter is worth drawing only when the CO has a power and the star value is
 * known; a CO without either has nothing to charge toward.
 */
export function readPowerMeter(player: PlayerRosterEntry): PowerMeterReading | null {
  const starCharge = player.powerStarCharge;
  if (!starCharge) return null;

  const charge = player.powerCharge ?? 0;
  const cop = levelCost(player.copCost, charge, starCharge);
  const scop = levelCost(player.scopCost, charge, starCharge);
  const totalStars = Math.max(cop?.stars ?? 0, scop?.stars ?? 0);
  if (totalStars === 0) return null;

  return {
    charge,
    starCharge,
    // A full meter stays at its maximum rather than overflowing the bar, and an
    // eliminated army keeps whatever it died holding.
    charged: Math.min(charge / starCharge, totalStars),
    cop,
    scop,
    totalStars,
    level: powerLevel(charge, player.copCost, player.scopCost),
  };
}

/**
 * The strongest power the charge pays for. Super outranks normal, because a CO
 * holding enough for both is choosing between them rather than limited to one.
 */
function powerLevel(
  charge: number,
  copCost: number | undefined,
  scopCost: number | undefined,
): PowerLevel {
  if (scopCost !== undefined && charge >= scopCost) return "scop";
  if (copCost !== undefined && charge >= copCost) return "cop";
  return "charging";
}

function levelCost(
  cost: number | undefined,
  charge: number,
  starCharge: number,
): PowerLevelCost | null {
  if (cost === undefined) return null;
  return {
    stars: Math.round(cost / starCharge),
    charge: cost,
    remaining: Math.max(cost - charge, 0),
  };
}
