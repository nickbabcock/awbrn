import { create } from "zustand";
import type { PlayerRosterSnapshot, ProductionOptionsChanged } from "#/wasm/awbrn_wasm.js";

interface GameState {
  currentDay: number;
  playerRoster: PlayerRosterSnapshot | null;
  productionOptions: ProductionOptionsChanged | null;
}

interface GameActions {
  setCurrentDay: (day: number) => void;
  setPlayerRoster: (playerRoster: PlayerRosterSnapshot | null) => void;
  setProductionOptions: (productionOptions: ProductionOptionsChanged | null) => void;
  reset: () => void;
}

export const useGameStore = create<GameState & { actions: GameActions }>((set) => ({
  currentDay: 1,
  playerRoster: null,
  productionOptions: null,
  actions: {
    setCurrentDay: (day) => set({ currentDay: day }),
    setPlayerRoster: (playerRoster) => set({ playerRoster }),
    setProductionOptions: (productionOptions) => set({ productionOptions }),
    reset: () => set({ currentDay: 1, playerRoster: null, productionOptions: null }),
  },
}));

export const useGameActions = () => useGameStore((state) => state.actions);
