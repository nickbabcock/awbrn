import type { AwbwGameArmy, AwbwGameSnapshot } from "#/awbw/schemas.ts";
import type { MovePlannerPowerOn, MovePlannerSeat } from "#/wasm/awbrn_wasm.js";

/** The AWBW id of the CO that has no abilities. */
export const NO_CO_ID = 31;

/** What the page asks for each army, as the fields hold it. */
export interface SeatInput {
  coId: number;
  funds: string;
  /** The power meter, in stars at `powerUses`. */
  powerStars: string;
  /** How many powers the CO used before, which raises the cost of a star. */
  powerUses: number;
  powerOn: MovePlannerPowerOn;
  /** The team from an AWBW game, which replaces the planner file's team. */
  team?: string;
  /**
   * The exact charge from an AWBW game. The stars round it, so it stays until
   * the player types new stars.
   */
  importedCharge?: number;
}

export const DEFAULT_SEAT: SeatInput = {
  coId: NO_CO_ID,
  funds: "0",
  powerStars: "0",
  powerUses: 0,
  powerOn: "none",
};

/** The seat that the board opens with, from what the fields hold. */
export function seatSetup(
  playerId: number,
  seat: SeatInput,
  starCharges: number[],
): MovePlannerSeat {
  return {
    playerId,
    coId: seat.coId,
    funds: Number(seat.funds),
    powerCharge:
      seat.importedCharge ??
      Math.round(Number(seat.powerStars) * starChargeAt(starCharges, seat.powerUses)),
    powerUses: seat.powerUses,
    powerOn: seat.powerOn,
    team: seat.team,
  };
}

/** The settings that an AWBW game gives the planner. */
export interface AwbwPlannerSetup {
  mapId: string | null;
  firstPlayerId: string | null;
  previewedPlayerId: string;
  seats: Record<number, SeatInput>;
}

/**
 * The planner settings from `game`, for the armies that the planner file
 * holds. Null when the game has none of them, because then it is another
 * game.
 *
 * Funds stay as they are when fog hides them.
 */
export function setupFromAwbwGame(
  game: AwbwGameSnapshot,
  plannerPlayerIds: number[],
  current: Record<number, SeatInput>,
  starCharges: number[],
  areaStrikeCoIds: number[],
): AwbwPlannerSetup | null {
  const armies = game.armies.filter((army) => plannerPlayerIds.includes(army.playerId));
  if (armies.length === 0) return null;

  const seats: Record<number, SeatInput> = { ...current };
  for (const army of armies) {
    const seat = current[army.playerId] ?? DEFAULT_SEAT;
    const powerUses = powerUsesOf(army, starCharges);
    // AWBW records ten times the charge that AWVM counts.
    const charge = Math.round(army.coPower / 10);
    seats[army.playerId] = {
      coId: army.coId,
      funds: army.funds === null ? seat.funds : String(army.funds),
      powerStars: formatStars(charge / starChargeAt(starCharges, powerUses)),
      powerUses,
      powerOn: army.powerOn,
      team: army.team,
      importedCharge: charge,
    };
  }

  const active = armies.find((army) => army.playerId === game.activePlayerId);
  return {
    mapId: game.mapId === null ? null : String(game.mapId),
    firstPlayerId: active ? String(active.playerId) : null,
    previewedPlayerId: String(previewedArmy(armies, active, areaStrikeCoIds)?.playerId ?? ""),
    seats,
  };
}

/**
 * How many powers the army's CO used, from how far AWBW raised its power
 * costs. Each use raises the cost by the same steps that raise a star.
 */
function powerUsesOf(army: AwbwGameArmy, starCharges: number[]): number {
  const [cost, base] =
    army.scopBaseCost > 0 ? [army.scopCost, army.scopBaseCost] : [army.copCost, army.copBaseCost];
  if (base <= 0 || starCharges.length === 0) return 0;

  // The nearest step, as AWBW rounds its costs.
  let best = 0;
  for (let uses = 1; uses < starCharges.length; uses += 1) {
    const error = Math.abs(cost * starCharges[0] - base * starCharges[uses]);
    const bestError = Math.abs(cost * starCharges[0] - base * starCharges[best]);
    if (error < bestError) best = uses;
  }
  return best;
}

/**
 * The army whose missiles to preview: an enemy of the army to move, if one
 * has missiles, because their missiles are the ones to plan around.
 */
function previewedArmy(
  armies: AwbwGameArmy[],
  active: AwbwGameArmy | undefined,
  areaStrikeCoIds: number[],
): AwbwGameArmy | undefined {
  const withMissiles = armies.filter(
    (army) => !army.eliminated && areaStrikeCoIds.includes(army.coId),
  );
  return (
    withMissiles.find((army) => active === undefined || army.team !== active.team) ??
    withMissiles[0]
  );
}

function starChargeAt(starCharges: number[], uses: number): number {
  return starCharges[Math.min(uses, starCharges.length - 1)] ?? 1;
}

/** Stars to two decimals, without trailing zeros. */
function formatStars(stars: number): string {
  return String(Math.round(stars * 100) / 100);
}
