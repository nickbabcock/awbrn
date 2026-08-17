import type {
  BattleBracket,
  BattleFighter,
  BattleImpossible,
  BattleResult,
  BattleSide,
  CatalogDomain,
  CatalogTerrain,
  CatalogUnit,
  CommanderKind,
  FundsBracket,
  NetFundsBracket,
  PlayerRosterEntry,
  PlayerRosterSnapshot,
  Terrain,
  UnitKind,
} from "#/wasm/awbrn_wasm.js";

/** Calculator context that can show values the board has hidden. */
export type CalculatorSide = Omit<BattleSide, "funds" | "properties" | "comTowers"> & {
  funds: number | undefined;
  properties: number | undefined;
  comTowers: number | undefined;
};

/**
 * What the calculator is, apart from how it looks.
 *
 * Every number the panel prints comes back from AWVM. What lives here is the
 * shape of the question — which two armies are being compared, what a blank
 * calculator starts as, and how a figure is worded — none of which is a rule
 * and all of which is worth testing without a browser.
 */

/** Health as the board draws it: ten bars, of ten points each. */
export const FULL_HEALTH_POINTS = 100;
export const HEALTH_STEP = 10;

/** A whole unit, in the bars the board draws it in. */
export const HEALTH_BARS_MAX = 10;

/** What a bar is worth in the points the reducer counts. */
export function barsToPoints(bars: number): number {
  return bars * HEALTH_STEP;
}

/**
 * The bar a health lands on, rounded up.
 *
 * A unit seeded off a real board can hold any exact value, and 61 points is a
 * unit the board draws as seven bars. Rounding down would show a unit as
 * weaker than the map does.
 */
export function pointsToBars(points: number): number {
  return Math.max(1, Math.min(10, Math.ceil(points / HEALTH_STEP)));
}

/** Which power a side is running. `null` is day-to-day. */
export type PowerChoice = "cop" | "scop" | null;

/**
 * The ground a unit is put on when nothing has said otherwise.
 *
 * Chosen per domain because a picker that opened every unit on a plain would
 * open a Battleship on dry land, and the first number a player read would be
 * one they had to correct before it meant anything.
 */
export function defaultTerrain(domain: CatalogDomain): Terrain {
  switch (domain) {
    case "sea":
      return "sea";
    // Air units are not on the ground at all. The plain is the neutral choice
    // for them the way the road is for nothing else: it is what the map is
    // mostly made of.
    case "air":
      return "plain";
    default:
      return "plain";
  }
}

/** A fresh fighter of this kind, at full strength on ground that suits it. */
export function newFighter(unit: CatalogUnit): BattleFighter {
  return {
    unit: unit.unit,
    health: FULL_HEALTH_POINTS,
    ammo: unit.maxAmmo > 0 ? unit.maxAmmo : undefined,
    terrain: defaultTerrain(unit.domain),
  };
}

/**
 * Keep a fighter's condition while changing what it is.
 *
 * Swapping a Tank for a Neo Tank is a question about the same engagement, so
 * the health and the ground stay. The magazine cannot: the two units carry
 * different amounts, and a Neo Tank inheriting a Tank's nine shells would be
 * carrying more than it has.
 */
export function retypeFighter(fighter: BattleFighter, unit: CatalogUnit): BattleFighter {
  return {
    unit: unit.unit,
    health: fighter.health,
    ammo: unit.maxAmmo > 0 ? unit.maxAmmo : undefined,
    terrain: fighter.terrain,
  };
}

/** A side with no commander, no money and no ground held. */
export function emptySide(): CalculatorSide {
  return { commander: undefined, power: undefined, funds: 0, properties: 0, comTowers: 0 };
}

/**
 * The two armies the panel opens on, read off the board.
 *
 * The attacker is the army whose turn it is to act, because that is the seat
 * the player is sitting in and the seat every figure is reported from. The
 * defender is the first army that is not on the attacker's team — an ally is
 * not who a player is weighing an attack against, and in a team game the
 * neighbouring seat often is one.
 */
export function seatsFrom(
  roster: PlayerRosterSnapshot | null,
  viewerSlotIndex: number | null,
): { attacker: PlayerRosterEntry | null; defender: PlayerRosterEntry | null } {
  const players = roster?.players ?? [];
  if (players.length === 0) return { attacker: null, defender: null };

  const attacker =
    players.find((player) => player.playerId === viewerSlotIndex) ??
    players.find((player) => player.playerId === roster?.activePlayerId) ??
    players[0] ??
    null;
  const defender =
    players.find(
      (player) =>
        player.playerId !== attacker?.playerId &&
        (player.team === undefined || player.team !== attacker?.team),
    ) ??
    players.find((player) => player.playerId !== attacker?.playerId) ??
    null;

  return { attacker, defender };
}

/**
 * One army's combat context, as the board currently has it.
 *
 * Everything here is a value a commander rule reads. Funds and property counts
 * are withheld from a player under fog. A withheld figure stays empty until
 * the player enters a value. A reported zero stays zero.
 */
export function sideFrom(entry: PlayerRosterEntry | null): CalculatorSide {
  if (!entry) return emptySide();

  return {
    commander: commanderFrom(entry.coKey),
    power: entry.activePower ?? undefined,
    funds: entry.stats.funds,
    properties: entry.stats.properties,
    comTowers: entry.stats.comTowers,
  };
}

/**
 * The commander a roster entry names.
 *
 * AWVM and the portrait sheet share one kebab vocabulary, except for the
 * no-commander placeholder the sheet calls `no-co`, which is the absence this
 * returns rather than a commander of that name.
 */
export function commanderFrom(coKey: string | undefined): CommanderKind | undefined {
  if (!coKey || coKey === "no-co" || coKey === "neutral") return undefined;
  return coKey as CommanderKind;
}

/** `65 – 75%`, or one figure when no commander in the exchange grants luck. */
export function formatDamage(bracket: BattleBracket): string {
  return bracket.low === bracket.high ? `${bracket.low}%` : `${bracket.low} – ${bracket.high}%`;
}

/** Funds, grouped the way money is read. */
export function formatFunds(value: number): string {
  return value.toLocaleString("en-US");
}

/** A bracket of funds, collapsed to one figure when both ends agree. */
export function formatFundsBracket(bracket: FundsBracket): string {
  return bracket.low === bracket.high
    ? formatFunds(bracket.low)
    : `${formatFunds(bracket.low)} – ${formatFunds(bracket.high)}`;
}

/**
 * What the exchange moves, always signed.
 *
 * The sign is the whole point of the figure and is never implied by position or
 * colour: a trade that loses money says so with a minus, on both ends.
 */
export function formatNet(net: NetFundsBracket): string {
  return net.low === net.high ? signed(net.low) : `${signed(net.low)} – ${signed(net.high)}`;
}

function signed(value: number): string {
  const sign = value < 0 ? "−" : "+";
  return `${sign}${formatFunds(Math.abs(value))}`;
}

/** Why a pairing has no numbers, said the way a player would say it. */
export function impossibleLabel(reason: BattleImpossible): string {
  return reason === "unarmed"
    ? "Cannot reach this target"
    : "Attacker has no weapon entry for this target";
}

/**
 * The whole engagement in one sentence.
 *
 * The visible row spends its width on figures a player scans between, in
 * columns that only mean something to someone who can see them lined up. This
 * is the same prediction as prose, and it carries the pairing the columns
 * cannot: the good outcome is the top of what is dealt with the bottom of what
 * is taken.
 */
export function engagementLabel(
  attackerName: string,
  targetName: string,
  target: BattleFighter,
  result: BattleResult | undefined,
  reason: BattleImpossible | undefined,
): string {
  const who = `${targetName} at ${pointsToBars(target.health)} HP`;
  if (!result) {
    return `${attackerName} against ${who}: ${(reason && impossibleLabel(reason)) ?? "no forecast"}`;
  }

  const outcome = result.destroys
    ? ", destroying it"
    : result.mayDestroy
      ? ", possibly destroying it"
      : "";
  const reply = result.counter
    ? `, taking ${formatDamage(result.counter)}${result.counterFirst ? " first" : " back"} and losing ${formatFundsBracket(result.valueTaken ?? { low: 0, high: 0 })} funds`
    : result.destroys
      ? ""
      : ", with no reply";

  return `${attackerName} against ${who}: dealing ${formatDamage(result.damage)}${outcome}, worth ${formatFundsBracket(result.valueDealt)} funds${reply}. Net ${formatNet(result.net)} funds.`;
}

/** Whether a unit carries a magazine worth asking about. */
export function hasMagazine(catalog: CatalogUnit[], unit: UnitKind): boolean {
  return (catalog.find((entry) => entry.unit === unit)?.maxAmmo ?? 0) > 0;
}

/** The catalog entry for a kind, which every picker needs and none may guess. */
export function unitEntry(catalog: CatalogUnit[], unit: UnitKind): CatalogUnit | undefined {
  return catalog.find((entry) => entry.unit === unit);
}

/**
 * The catalog entry for one ground, which carries the tile the board draws.
 *
 * The sprite index is the ruleset's answer rather than a table this file could
 * hold: a picker that guessed which cell of the sheet a com tower lives in
 * would draw the wrong building the first time the sheet was regenerated.
 */
export function terrainEntry(
  catalog: CatalogTerrain[],
  terrain: Terrain,
): CatalogTerrain | undefined {
  return catalog.find((entry) => entry.terrain === terrain);
}
