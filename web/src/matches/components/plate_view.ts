import type { ArmyView } from "#/matches/components/Armies.tsx";

/**
 * The one plate under a phone's turn-order row. Live, it is yours while you
 * are in the match, otherwise the mover's. In review it is the army that moved
 * at that moment, as the roster read at that moment draws it.
 */
export function plateView(
  views: readonly ArmyView[],
  { isReviewing, moverSlot }: { isReviewing: boolean; moverSlot: number | null },
): ArmyView | null {
  const moment = views.find((view) => view.army.isActive);
  const mover = views.find((view) => view.army.entry.playerId === moverSlot);
  if (isReviewing) return moment ?? mover ?? views[0] ?? null;
  return (
    views.find((view) => view.isViewer && !view.army.entry.eliminated) ?? mover ?? views[0] ?? null
  );
}
