import { getFactionByCode } from "#/factions.ts";
import type { AttackForecast, DamageBracket } from "#/wasm/awbrn_wasm.js";

/**
 * How an engagement is worded.
 *
 * The numbers themselves are AWVM's and are never computed here. What this
 * decides is only how they read: what a bracket of one value looks like, and
 * how the whole prediction is said in a sentence for a reader who gets the row
 * rather than its layout.
 */

/** `65 – 75%`, or one number when no commander in the exchange grants luck. */
export function formatBracket(bracket: DamageBracket): string {
  return bracket.low === bracket.high ? `${bracket.low}%` : `${bracket.low} – ${bracket.high}%`;
}

/**
 * The whole engagement in one sentence.
 *
 * The visible row spends its width on numbers a player scans between; a screen
 * reader needs the same facts as prose, including the army spelled out rather
 * than left as the two letters whose colour the sprite is carrying.
 *
 * It carries the whole load now that the row itself is wordless: the visible
 * key says direction with a drawn arrow, which a reader who cannot see it needs
 * given as "back" or "first". A commander who answers first inverts the
 * exchange, and there is no other way to learn that from the row.
 */
export function engagementLabel(name: string, forecast: AttackForecast): string {
  const target = forecast.target;
  const who =
    target.type === "unit"
      ? `${getFactionByCode(target.factionCode)?.displayName ?? target.factionCode} ${target.name} at ${target.health} HP`
      : target.name;
  const outcome = forecast.destroys
    ? ", destroying it"
    : forecast.mayDestroy
      ? ", possibly destroying it"
      : "";
  // Not a counter of zero. Nothing answers at all, and a player spends a unit
  // differently knowing that. A destroyed target needs no such sentence: it is
  // already the reason nothing answers.
  const reply = forecast.counter
    ? `, taking ${formatBracket(forecast.counter)}${forecast.counterFirst ? " first" : " back"}`
    : forecast.destroys
      ? ""
      : ", with no reply";
  return `${name} ${who}, dealing ${formatBracket(forecast.damage)}${outcome}${reply}`;
}
