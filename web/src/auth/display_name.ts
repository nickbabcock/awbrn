import { z } from "zod";

export const DISPLAY_NAME_MIN_LENGTH = 3;
export const DISPLAY_NAME_MAX_LENGTH = 20;

/** How long a player waits between renaming themselves. */
export const DISPLAY_NAME_COOLDOWN_MS = 30 * 24 * 60 * 60 * 1000;

const ALLOWED = /^[\p{L}\p{M}\p{N} _.-]+$/u;
const HAS_LETTER_OR_DIGIT = /[\p{L}\p{N}]/u;
const SEPARATORS = /[ _.-]/g;

/** Names that read as the site speaking, or as staff. Compared by key. */
const RESERVED_KEYS = new Set(
  ["admin", "administrator", "mod", "moderator", "staff", "support", "system", "awbrn"].map(
    displayNameKey,
  ),
);

/** The name as it is stored and shown: NFKC, trimmed, one space between words. */
export function normalizeDisplayName(raw: string): string {
  return raw.normalize("NFKC").trim().replace(/\s+/g, " ");
}

/**
 * What uniqueness is checked on.
 *
 * Case and separators are dropped, so "Max.Strike" and "max strike" are one
 * name: either one reads as the other player.
 */
export function displayNameKey(name: string): string {
  return normalizeDisplayName(name).toLowerCase().normalize("NFKC").replace(SEPARATORS, "");
}

/** CPU seats are labeled "Hard CPU" and the like, so no player may end in "CPU". */
function isReserved(name: string): boolean {
  const words = name.toLowerCase().split(SEPARATORS);
  return RESERVED_KEYS.has(displayNameKey(name)) || words.at(-1) === "cpu";
}

export const displayNameSchema = z
  .string()
  .transform(normalizeDisplayName)
  .pipe(
    z
      .string()
      .min(DISPLAY_NAME_MIN_LENGTH, `Use at least ${DISPLAY_NAME_MIN_LENGTH} characters`)
      .max(DISPLAY_NAME_MAX_LENGTH, `Use at most ${DISPLAY_NAME_MAX_LENGTH} characters`)
      .regex(ALLOWED, "Use letters, numbers, spaces, and _ . -")
      .regex(HAS_LETTER_OR_DIGIT, "Use letters, numbers, spaces, and _ . -")
      .refine((name) => !isReserved(name), "That name is reserved"),
  );
