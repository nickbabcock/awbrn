import { customAlphabet } from "#/vendor/nanoid.ts";

const MAP_ID_ALPHABET = "0123456789abcdefghijklmnopqrstuvwxyz";
export const MAP_ID_LENGTH = 12;
const generateLowercaseMapId = customAlphabet(MAP_ID_ALPHABET, MAP_ID_LENGTH);

export function generateMapId(): string {
  return generateLowercaseMapId();
}
