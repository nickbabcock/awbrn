/**
 * The keys the drafting table answers to.
 *
 * A map maker who has drawn a board before does not reach across the screen
 * fifty times to change a tile. They keep one hand on the board and the other
 * on the rail, which means the rail has to be reachable without the pointer.
 * That is the whole of this file: it is what turns a palette a beginner can
 * click into an instrument somebody can play.
 *
 * Two things stay out of here on purpose. Undo lives in the engine, because
 * the board owns its own history. The eyedropper lives in the engine too,
 * because only the board knows what is on a tile. What is left is what the
 * rail knows and nothing else: which drawer is open, which brush is next in
 * it, and which army is being painted as.
 */

import { useEffect } from "react";
import {
  eraserOfDrawer,
  stepBrush,
  type PaletteDrawer,
  type PaletteSection,
} from "#/maps/map_editor.ts";
import type { Brush } from "#/wasm/awbrn_wasm.js";

/** The drawers, in the order the number keys reach them. */
const DRAWER_KEYS: readonly PaletteDrawer[] = ["terrain", "property", "unit"];

/**
 * Whether the keystroke belongs to something the visitor is typing into.
 *
 * The map's name is a text field on this same screen, and a map maker naming a
 * board "1v1 Estuary" must not have the rail change under them three times
 * while they do it.
 */
function isTyping(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  return ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName);
}

export function useEditorShortcuts({
  armies,
  drawer,
  isEnabled,
  loadedBrush,
  onArmyChange,
  onDrawerChange,
  onSelect,
  sections,
}: {
  /** The armies the fold seats, in seat order. */
  armies: readonly string[];
  drawer: PaletteDrawer;
  /** False while the board is still opening, when there is nothing to load. */
  isEnabled: boolean;
  loadedBrush: Brush | null;
  onArmyChange: (factionCode: string) => void;
  onDrawerChange: (drawer: PaletteDrawer) => void;
  onSelect: (brush: Brush) => void;
  sections: readonly PaletteSection[];
}): void {
  useEffect(() => {
    if (!isEnabled) return;

    function onKeyDown(event: KeyboardEvent) {
      // Alt belongs to the board, where it is the eyedropper, and ctrl and the
      // command key belong to the browser and to undo. What is left is the
      // bare key, which is the only thing the rail claims.
      if (event.ctrlKey || event.metaKey || event.altKey) return;
      if (isTyping(event.target)) return;

      // A seat is shift and its number, which reads as the shifted form of the
      // number on most layouts, so the seat is taken off `event.code`.
      if (event.shiftKey) {
        const seat = /^Digit([1-8])$/.exec(event.code);
        if (!seat?.[1]) return;
        const army = armies[Number(seat[1]) - 1];
        if (army === undefined) return;
        event.preventDefault();
        onArmyChange(army);
        return;
      }

      const open = sections.find((section) => section.drawer === drawer) ?? sections[0];

      switch (event.key) {
        case "1":
        case "2":
        case "3": {
          const wanted = DRAWER_KEYS[Number(event.key) - 1];
          if (wanted === undefined) return;
          if (!sections.some((section) => section.drawer === wanted)) return;
          event.preventDefault();
          onDrawerChange(wanted);
          return;
        }
        case "[":
        case "]": {
          if (!open) return;
          const next = stepBrush(open.cells, loadedBrush, event.key === "]" ? 1 : -1);
          if (next === null) return;
          event.preventDefault();
          onSelect(next);
          return;
        }
        case "e":
        case "E": {
          event.preventDefault();
          onSelect(eraserOfDrawer(open?.drawer ?? drawer));
          return;
        }
        default:
          return;
      }
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [armies, drawer, isEnabled, loadedBrush, onArmyChange, onDrawerChange, onSelect, sections]);
}
