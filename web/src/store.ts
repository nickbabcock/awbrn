import { create } from "zustand";
import type { ReplayLoaded } from "./wasm/awbrn_wasm";

interface GameState {
  currentDay: number;
  replayRoster: ReplayLoaded | null;
}

interface GameActions {
  setCurrentDay: (day: number) => void;
  setReplayRoster: (replayRoster: ReplayLoaded | null) => void;
}

export const useGameStore = create<GameState & { actions: GameActions }>((set) => ({
  currentDay: 1,
  replayRoster: null,
  actions: {
    setCurrentDay: (day) => set({ currentDay: day }),
    setReplayRoster: (replayRoster) => set({ replayRoster }),
  },
}));

export const useGameActions = () => useGameStore((state) => state.actions);
