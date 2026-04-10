import { create } from "zustand";
import type { PlayerRosterSnapshot } from "#/wasm/awbrn_wasm.js";

interface GameState {
  currentDay: number;
  playerRoster: PlayerRosterSnapshot | null;
}

interface GameActions {
  setCurrentDay: (day: number) => void;
  setPlayerRoster: (playerRoster: PlayerRosterSnapshot | null) => void;
  reset: () => void;
}

export const useGameStore = create<GameState & { actions: GameActions }>((set) => ({
  currentDay: 1,
  playerRoster: null,
  actions: {
    setCurrentDay: (day) => set({ currentDay: day }),
    setPlayerRoster: (playerRoster) => set({ playerRoster }),
    reset: () => set({ currentDay: 1, playerRoster: null }),
  },
}));

export const useGameActions = () => useGameStore((state) => state.actions);
