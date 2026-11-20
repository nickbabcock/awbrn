import { useEffect, useEffectEvent } from "react";
import { BRUSH_TABS, eraserOfTab, stepBrush, type BrushTab } from "#/maps/map_editor.ts";
import type { Brush, PaletteCell } from "#/wasm/awbrn_wasm.js";

export function isField(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName);
}

/** Keys for a field or an open menu belong to it, not to the editor. */
export function isKeyedElsewhere(target: EventTarget | null): boolean {
  return (
    isField(target) || (target instanceof HTMLElement && target.closest('[role="dialog"]') !== null)
  );
}

/**
 * The rail's keys. Undo, pick up and zoom are the board's own, so they are
 * forwarded to the canvas when focus sits on a control a keyboard user
 * reached. Space stays with that control; a click hands focus back to the
 * board, where Space pans.
 */
export function useEditorShortcuts({
  armies,
  canvas,
  cells,
  isEnabled,
  isKeysOpen,
  loadedBrush,
  onArmyChange,
  onKeysToggle,
  onSelect,
  onStepProblem,
  onTabChange,
  tab,
}: {
  /** Roster seats, in seat order. */
  armies: readonly string[];
  canvas: () => HTMLCanvasElement | null;
  /** The open tab's brushes, in tray order. */
  cells: readonly PaletteCell[];
  isEnabled: boolean;
  isKeysOpen: boolean;
  loadedBrush: Brush | null;
  onArmyChange: (factionCode: string) => void;
  onKeysToggle: (isOpen: boolean) => void;
  onSelect: (brush: Brush) => void;
  onStepProblem: (step: 1 | -1) => void;
  onTabChange: (tab: BrushTab) => void;
  tab: BrushTab;
}): void {
  const onKeyDown = useEffectEvent((event: KeyboardEvent) => {
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    if (isKeyedElsewhere(event.target)) return;

    if (event.key === "?" || (event.key === "Escape" && isKeysOpen)) {
      event.preventDefault();
      onKeysToggle(!isKeysOpen);
      return;
    }
    if (event.key === "N" || event.key === "n") {
      event.preventDefault();
      onStepProblem(event.shiftKey ? -1 : 1);
      return;
    }

    if (event.shiftKey) {
      // Shift turns the digit into a symbol on most layouts, so read the key's place.
      const seat = /^Digit([1-8])$/.exec(event.code);
      const army = seat?.[1] === undefined ? undefined : armies[Number(seat[1]) - 1];
      if (army === undefined) return;
      event.preventDefault();
      onArmyChange(army);
      return;
    }

    const wanted = BRUSH_TABS.find((entry) => entry.key === event.key);
    if (wanted) {
      event.preventDefault();
      onTabChange(wanted.tab);
      return;
    }
    if (event.key === "[" || event.key === "]") {
      const next = stepBrush(cells, loadedBrush, event.key === "]" ? 1 : -1);
      if (next === null) return;
      event.preventDefault();
      onSelect(next);
      return;
    }
    if (event.key === "e" || event.key === "E") {
      event.preventDefault();
      onSelect(eraserOfTab(tab));
    }
  });

  const forward = useEffectEvent((event: KeyboardEvent) => {
    const board = canvas();
    if (board === null || event.target === board || event.key === " ") return;
    // A modifier released in a field would otherwise stay held on the board.
    const isModifierUp = event.type === "keyup" && MODIFIER_KEYS.has(event.key);
    if (isKeyedElsewhere(event.target) && !isModifierUp) return;
    board.dispatchEvent(new KeyboardEvent(event.type, { ...keyInit(event), cancelable: true }));
  });

  // The editor's board has no use for Tab, so Tab leaves it for the rail. It is
  // stopped before the canvas, which keeps Tab for the match board.
  const releaseTab = useEffectEvent((event: KeyboardEvent) => {
    if (event.key === "Tab" && !event.shiftKey && event.target === canvas()) {
      event.stopPropagation();
    }
  });

  useEffect(() => {
    if (!isEnabled) return;
    window.addEventListener("keydown", releaseTab, { capture: true });
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keydown", forward);
    window.addEventListener("keyup", forward);
    return () => {
      window.removeEventListener("keydown", releaseTab, { capture: true });
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keydown", forward);
      window.removeEventListener("keyup", forward);
    };
  }, [isEnabled]);
}

const MODIFIER_KEYS: ReadonlySet<string> = new Set(["Control", "Alt", "Shift", "Meta"]);

function keyInit(event: KeyboardEvent): KeyboardEventInit {
  const { altKey, code, ctrlKey, key, metaKey, repeat, shiftKey } = event;
  return { altKey, code, ctrlKey, key, metaKey, repeat, shiftKey };
}
