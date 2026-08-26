import { customAlphabet } from "#/vendor/nanoid.ts";

const MODERATION_ID_ALPHABET = "0123456789abcdefghijklmnopqrstuvwxyz";
export const MODERATION_ID_LENGTH = 16;
const generateLowercaseModerationId = customAlphabet(MODERATION_ID_ALPHABET, MODERATION_ID_LENGTH);

export function generateModerationId(): string {
  return generateLowercaseModerationId();
}
