import { useEffect, useMemo, useSyncExternalStore } from "react";
import { SeatChanges, type SeatPatch } from "#/matches/seat_changes.ts";

export function useSeatChanges(
  identity: string,
  save: (slotIndex: number, patch: SeatPatch) => Promise<void>,
  refresh: () => Promise<unknown>,
) {
  // Each controller keeps the match and user that created it.
  const changes = useMemo(() => new SeatChanges(save, refresh), [identity]);
  useEffect(() => {
    changes.start();
    return () => changes.stop();
  }, [changes]);
  const state = useSyncExternalStore(changes.subscribe, changes.getSnapshot, changes.getSnapshot);
  return { changes, ...state };
}
