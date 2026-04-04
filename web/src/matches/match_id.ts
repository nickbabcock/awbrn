import { customAlphabet } from "../vendor/nanoid";

const MATCH_ID_ALPHABET = "0123456789abcdefghijklmnopqrstuvwxyz";
export const MATCH_ID_LENGTH = 13;
const generateLowercaseMatchId = customAlphabet(MATCH_ID_ALPHABET, MATCH_ID_LENGTH);

export function generateMatchId(): string {
  return generateLowercaseMatchId();
}
