import type { MatchSetup } from "./schemas.ts";

export function ownedSlotIndices(setup: MatchSetup, userId: string): number[] {
  return setup.players
    .map((player, slotIndex) => ({ player, slotIndex }))
    .filter(({ player }) => player.userId === userId)
    .map(({ slotIndex }) => slotIndex);
}

export function lastActingOwnedSlot(events: unknown[], ownedSlots: number[]): number | null {
  const owned = new Set(ownedSlots);
  for (let index = events.length - 1; index >= 0; index -= 1) {
    const event = events[index];
    if (
      typeof event === "object" &&
      event !== null &&
      "player" in event &&
      typeof event.player === "number" &&
      owned.has(event.player)
    ) {
      return event.player;
    }
  }
  return null;
}

export function selectOwnedPerspectiveSlot(
  ownedSlots: number[],
  activePlayerSlot: number,
  events: unknown[],
): number | null {
  const lowestOwnedSlot = ownedSlots[0] ?? null;
  if (lowestOwnedSlot === null) return null;
  if (ownedSlots.includes(activePlayerSlot)) return activePlayerSlot;
  return lastActingOwnedSlot(events, ownedSlots) ?? lowestOwnedSlot;
}
