/** A faction's color token, for styles that only know the army at runtime. Roles are in `awbrnTheme.ts`. */
export type FactionColorRole = "accent" | "soft" | "wash" | "field" | "text";

export function factionColorVar(code: string, role: FactionColorRole): string {
  return `var(--color-faction-${code}-${role}, var(--color-paper-well))`;
}
