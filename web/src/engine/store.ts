import { create } from "zustand";
import type {
  AttackPreviewChanged,
  PlayerRosterSnapshot,
  ProductionOptionsChanged,
  HoveredTile,
  InspectedUnitReadout,
  UnitActionsChanged,
} from "#/wasm/awbrn_wasm.js";

interface GameState {
  currentDay: number;
  playerRoster: PlayerRosterSnapshot | null;
  productionOptions: ProductionOptionsChanged | null;
  hoveredTile: HoveredTile | null;
  /** The orders offered at a proposed destination, or null when none is. */
  unitActions: UnitActionsChanged | null;
  /** What the pointer is aimed at costs both sides, or null when it aims at nothing. */
  attackPreview: AttackPreviewChanged | null;
  /** What the unit being read reaches, or null when none is being read. */
  inspectedUnit: InspectedUnitReadout | null;
}

interface GameActions {
  setCurrentDay: (day: number) => void;
  setPlayerRoster: (playerRoster: PlayerRosterSnapshot | null) => void;
  setProductionOptions: (productionOptions: ProductionOptionsChanged | null) => void;
  setHoveredTile: (hoveredTile: HoveredTile | null) => void;
  setUnitActions: (unitActions: UnitActionsChanged | null) => void;
  setAttackPreview: (attackPreview: AttackPreviewChanged | null) => void;
  setInspectedUnit: (inspectedUnit: InspectedUnitReadout | null) => void;
  reset: () => void;
}

export const useGameStore = create<GameState & { actions: GameActions }>((set) => ({
  currentDay: 1,
  playerRoster: null,
  productionOptions: null,
  hoveredTile: null,
  unitActions: null,
  attackPreview: null,
  inspectedUnit: null,
  actions: {
    setCurrentDay: (day) => set({ currentDay: day }),
    setPlayerRoster: (playerRoster) => set({ playerRoster }),
    setProductionOptions: (productionOptions) => set({ productionOptions }),
    setHoveredTile: (hoveredTile) => set({ hoveredTile }),
    setUnitActions: (unitActions) => set({ unitActions }),
    setAttackPreview: (attackPreview) => set({ attackPreview }),
    setInspectedUnit: (inspectedUnit) => set({ inspectedUnit }),
    reset: () =>
      set({
        currentDay: 1,
        playerRoster: null,
        productionOptions: null,
        hoveredTile: null,
        unitActions: null,
        attackPreview: null,
        inspectedUnit: null,
      }),
  },
}));

export const useGameActions = () => useGameStore((state) => state.actions);
