/**
 * A faction's color token, by army code, for styles that only know the army at
 * runtime (a stamp's field, a card's wash).
 *
 * Usage: `style={{ backgroundColor: factionColorVar("os", "wash") }}`. Roles:
 * `accent` the identity color, `wash` an 8 to 22% tint for a card, plate, ledger
 * row or turn-order cell, `soft` a chip fill, `field` a stamp's ground, `text`
 * the darkened value that reads on `soft`. Hue never carries an army alone.
 */
export type FactionColorRole = "accent" | "soft" | "wash" | "field" | "text";

export function factionColorVar(code: string, role: FactionColorRole): string {
  return `var(--color-faction-${code}-${role}, var(--color-paper-well))`;
}
