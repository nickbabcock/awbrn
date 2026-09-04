import { create } from "zustand";
import type {
  AttackPreviewChanged,
  ReplayPositionChanged,
  ReplayViewpointChanged,
  PlayerRosterSnapshot,
  ProductionOptionsChanged,
  HoveredTile,
  InspectedUnitReadout,
  TurnReadinessChanged,
  UnitActionsChanged,
} from "#/wasm/awbrn_wasm.js";

interface GameState {
  currentDay: number;
  playerRoster: PlayerRosterSnapshot | null;
  productionOptions: ProductionOptionsChanged | null;
  hoveredTile: HoveredTile | null;
  /** The orders offered at a proposed destination, or null when none is. */
  turnReadiness: TurnReadinessChanged | undefined;
  unitActions: UnitActionsChanged | null;
  /** What the pointer is aimed at costs both sides, or null when it aims at nothing. */
  attackPreview: AttackPreviewChanged | null;
  /** What the unit being read reaches, or null when none is being read. */
  inspectedUnit: InspectedUnitReadout | null;
  /** Where the viewer is standing in a loaded archive, or null without one. */
  replayPosition: ReplayPositionChanged | null;
  /**
   * Whose eyes a loaded archive is being watched through, or null before the
   * engine has said. The engine owns this: the board's own keys change the
   * viewpoint too, so a page that kept its own copy would disagree with the
   * board the moment one was pressed.
   */
  replayViewpoint: ReplayViewpointChanged | null;
}

interface GameActions {
  setCurrentDay: (day: number) => void;
  setPlayerRoster: (playerRoster: PlayerRosterSnapshot | null) => void;
  setProductionOptions: (productionOptions: ProductionOptionsChanged | null) => void;
  setHoveredTile: (hoveredTile: HoveredTile | null) => void;
  setTurnReadiness: (turnReadiness: TurnReadinessChanged | undefined) => void;
  setUnitActions: (unitActions: UnitActionsChanged | null) => void;
  setAttackPreview: (attackPreview: AttackPreviewChanged | null) => void;
  setInspectedUnit: (inspectedUnit: InspectedUnitReadout | null) => void;
  setReplayPosition: (replayPosition: ReplayPositionChanged | null) => void;
  setReplayViewpoint: (replayViewpoint: ReplayViewpointChanged | null) => void;
  reset: () => void;
}

export const useGameStore = create<GameState & { actions: GameActions }>((set) => ({
  currentDay: 1,
  playerRoster: null,
  productionOptions: null,
  hoveredTile: null,
  turnReadiness: undefined,
  unitActions: null,
  attackPreview: null,
  inspectedUnit: null,
  replayPosition: null,
  replayViewpoint: null,
  actions: {
    setCurrentDay: (day) => set({ currentDay: day }),
    setPlayerRoster: (playerRoster) => set({ playerRoster }),
    setProductionOptions: (productionOptions) => set({ productionOptions }),
    setHoveredTile: (hoveredTile) => set({ hoveredTile }),
    setTurnReadiness: (turnReadiness) => set({ turnReadiness }),
    setUnitActions: (unitActions) => set({ unitActions }),
    setAttackPreview: (attackPreview) => set({ attackPreview }),
    setInspectedUnit: (inspectedUnit) => set({ inspectedUnit }),
    setReplayPosition: (replayPosition) => set({ replayPosition }),
    setReplayViewpoint: (replayViewpoint) => set({ replayViewpoint }),
    reset: () =>
      set({
        currentDay: 1,
        playerRoster: null,
        productionOptions: null,
        hoveredTile: null,
        turnReadiness: undefined,
        unitActions: null,
        attackPreview: null,
        inspectedUnit: null,
        replayPosition: null,
        replayViewpoint: null,
      }),
  },
}));

export const useGameActions = () => useGameStore((state) => state.actions);
