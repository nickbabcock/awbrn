import { create } from "zustand";
import type {
  PlayerRosterSnapshot,
  ProductionOptionsChanged,
  UnitActionsChanged,
} from "#/wasm/awbrn_wasm.js";

interface GameState {
  currentDay: number;
  playerRoster: PlayerRosterSnapshot | null;
  productionOptions: ProductionOptionsChanged | null;
  /** The orders offered at a proposed destination, or null when none is. */
  unitActions: UnitActionsChanged | null;
}

interface GameActions {
  setCurrentDay: (day: number) => void;
  setPlayerRoster: (playerRoster: PlayerRosterSnapshot | null) => void;
  setProductionOptions: (productionOptions: ProductionOptionsChanged | null) => void;
  setUnitActions: (unitActions: UnitActionsChanged | null) => void;
  reset: () => void;
}

export const useGameStore = create<GameState & { actions: GameActions }>((set) => ({
  currentDay: 1,
  playerRoster: null,
  productionOptions: null,
  unitActions: null,
  actions: {
    setCurrentDay: (day) => set({ currentDay: day }),
    setPlayerRoster: (playerRoster) => set({ playerRoster }),
    setProductionOptions: (productionOptions) => set({ productionOptions }),
    setUnitActions: (unitActions) => set({ unitActions }),
    reset: () =>
      set({ currentDay: 1, playerRoster: null, productionOptions: null, unitActions: null }),
  },
}));

export const useGameActions = () => useGameStore((state) => state.actions);
