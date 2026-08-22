import { customAlphabet } from "#/vendor/nanoid.ts";
import { z } from "zod";

const MATCH_ID_ALPHABET = "0123456789abcdefghijklmnopqrstuvwxyz";
export const MATCH_ID_LENGTH = 13;
export const matchIdSchema = z
  .string()
  .length(MATCH_ID_LENGTH)
  .regex(/^[0-9a-z]+$/);
const generateLowercaseMatchId = customAlphabet(MATCH_ID_ALPHABET, MATCH_ID_LENGTH);

export function generateMatchId(): string {
  return generateLowercaseMatchId();
}
