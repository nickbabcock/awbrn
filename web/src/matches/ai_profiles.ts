/**
 * The opponents a match may seat, as a screen shows them.
 *
 * The roster itself is the engine's: `aiProfiles()` in the match wasm names
 * every profile, its tier and how it plays. This module is that roster written
 * out in plain TypeScript so a screen, a zod schema and a check constraint can
 * all read it without loading a wasm module to draw a menu.
 *
 * `ai_profiles.test.ts` holds the two to each other. Adding or retuning a
 * profile is a change in the engine first and here second, and the test is
 * what says so.
 */

import { aiProfileIds, type AiProfileId } from "./schemas.ts";

/** How hard an opponent is, as a player reads it. */
export type AiTier = "easy" | "standard" | "hard";

export interface AiProfileDisplay {
  id: AiProfileId;
  tier: AiTier;
  /** The tier's name. What a picker shows. */
  label: string;
  /** One line on how this opponent plays. */
  blurb: string;
}

export const aiProfileDisplays: readonly AiProfileDisplay[] = [
  {
    id: "ai-easy-v1",
    tier: "easy",
    label: "Easy",
    blurb: "Moves at random. It will take a property it stumbles onto and little else.",
  },
  {
    id: "ai-standard-v1",
    tier: "standard",
    label: "Standard",
    blurb: "Scores every play and takes the best one. It captures, builds, and trades.",
  },
  {
    id: "ai-hard-v1",
    tier: "hard",
    label: "Hard",
    blurb: "Scores the promoted weighting and punishes a thin front.",
  },
];

const displayById = new Map(aiProfileDisplays.map((profile) => [profile.id, profile]));

/** The opponent with this identifier, or null when no profile has it. */
export function aiProfileDisplay(id: string | null | undefined): AiProfileDisplay | null {
  return id === null || id === undefined ? null : (displayById.get(id as AiProfileId) ?? null);
}

/**
 * What a seat the server plays is called.
 *
 * It stands where a person's name stands, so it reads like one rather than
 * like a setting.
 */
export function aiSeatName(id: AiProfileId): string {
  return `${aiProfileDisplay(id)?.label ?? "CPU"} CPU`;
}

/** The default opponent a host is offered. */
export const DEFAULT_AI_PROFILE_ID: AiProfileId = "ai-standard-v1";

/** Every identifier, in the order a picker offers them. */
export const orderedAiProfileIds: readonly AiProfileId[] = aiProfileIds;
