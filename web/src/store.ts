import { create } from "zustand";

interface GameState {
  currentDay: number;
}

interface GameActions {
  setCurrentDay: (day: number) => void;
}

export const useGameStore = create<GameState & { actions: GameActions }>(
  (set) => ({
    currentDay: 1,
    actions: {
      setCurrentDay: (day) => set({ currentDay: day }),
    },
  }),
);

export const useGameActions = () => useGameStore((state) => state.actions);
