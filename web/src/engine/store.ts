import { create } from "zustand";
import type { ActionMenuEvent, PlayerRosterSnapshot } from "#/wasm/awbrn_wasm.js";

interface GameState {
  actionMenu: ActionMenuEvent | null;
  currentDay: number;
  playerRoster: PlayerRosterSnapshot | null;
}

interface GameActions {
  setActionMenu: (actionMenu: ActionMenuEvent | null) => void;
  setCurrentDay: (day: number) => void;
  setPlayerRoster: (playerRoster: PlayerRosterSnapshot | null) => void;
  reset: () => void;
}

export const useGameStore = create<GameState & { actions: GameActions }>((set) => ({
  actionMenu: null,
  currentDay: 1,
  playerRoster: null,
  actions: {
    setActionMenu: (actionMenu) => set({ actionMenu }),
    setCurrentDay: (day) => set({ currentDay: day }),
    setPlayerRoster: (playerRoster) => set({ playerRoster }),
    reset: () => set({ actionMenu: null, currentDay: 1, playerRoster: null }),
  },
}));

export const useGameActions = () => useGameStore((state) => state.actions);
