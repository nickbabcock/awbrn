/**
 * The editor, as a screen holds it.
 *
 * The board keeps the map: every edit is applied inside the engine, where the
 * rules for a road and a mirror already are. This hook is the wire between
 * that and the panels around it. It sends what the map maker asked for and
 * holds the last report the board sent back.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import type { GameRunner } from "#/engine/game_runner.ts";
import type { AwbrnMapDocument } from "#/maps/map_document.ts";
import type {
  Brush,
  EditorCommand,
  EditorPalette,
  EditorStateChanged,
  ResizeAnchor,
  Symmetry,
} from "#/wasm/awbrn_wasm.js";

export interface MapEditorSource {
  /** Fork or edit this map, or open a blank board when it is absent. */
  document: AwbrnMapDocument | null;
  /** The size of a blank board. Ignored when a document is given. */
  width: number;
  height: number;
}

export interface MapEditorHandle {
  /** The last report the board sent, or null before the first one. */
  state: EditorStateChanged | null;
  /** Every brush the palette can offer, in the selected army's colours. */
  palette: EditorPalette | null;
  /** What went wrong, in the words it went wrong in. */
  error: string | null;
  setBrush: (brush: Brush) => void;
  setSymmetry: (symmetry: Symmetry) => void;
  setRoster: (factionCodes: string[]) => void;
  resize: (width: number, height: number, anchor: ResizeAnchor) => void;
  fill: (terrain: number) => void;
  undo: () => void;
  redo: () => void;
  /** The map as the board now stands. */
  readDocument: (name: string, author: string) => Promise<AwbrnMapDocument>;
}

export function useMapEditor({
  factionCode,
  runner,
  source,
}: {
  /** The army whose buildings and units the palette offers. */
  factionCode: string | null;
  runner: GameRunner;
  source: MapEditorSource | null;
}): MapEditorHandle {
  const [state, setState] = useState<EditorStateChanged | null>(null);
  const [palette, setPalette] = useState<EditorPalette | null>(null);
  const [error, setError] = useState<string | null>(null);

  // The board reports on its own schedule, so the handler is registered once
  // and reads through a ref rather than being replaced on every report.
  const stateRef = useRef<EditorStateChanged | null>(null);
  stateRef.current = state;

  useEffect(() => {
    runner.setEditorStateHandler((next) => setState(next));
    return () => runner.setEditorStateHandler(undefined);
  }, [runner]);

  useEffect(() => {
    if (!source) return;

    let cancelled = false;
    void Promise.resolve()
      .then(async () => {
        if (cancelled) return;
        if (source.document) {
          await runner.openEditor(source.document);
        } else {
          await runner.openBlankEditor(source.width, source.height);
        }
      })
      .catch((cause: unknown) => {
        if (cancelled) return;
        setError(cause instanceof Error ? cause.message : "The board did not open.");
      });

    return () => {
      cancelled = true;
    };
  }, [runner, source]);

  useEffect(() => {
    let cancelled = false;
    void runner
      .loadEditorPalette(factionCode)
      .then((next) => {
        if (!cancelled) setPalette(next);
      })
      .catch((cause: unknown) => {
        if (cancelled) return;
        setError(cause instanceof Error ? cause.message : "The palette could not be read.");
      });

    return () => {
      cancelled = true;
    };
  }, [factionCode, runner]);

  // Commands are answered out of order, so each one carries a number and only
  // the newest one may write the error line. Without it a refusal from a
  // command two strokes back could land on top of what the board just said,
  // and the map maker would read the wrong reason.
  const sent = useRef(0);

  const send = useCallback(
    (command: EditorCommand) => {
      sent.current += 1;
      const mine = sent.current;
      void runner.sendEditorCommand(command).then(
        () => {
          if (mine === sent.current) setError(null);
        },
        (cause: unknown) => {
          if (mine !== sent.current) return;
          setError(cause instanceof Error ? cause.message : "The board refused that.");
        },
      );
    },
    [runner],
  );

  return {
    state,
    palette,
    error,
    setBrush: useCallback((brush: Brush) => send({ type: "setBrush", brush }), [send]),
    setSymmetry: useCallback(
      (symmetry: Symmetry) => send({ type: "setSymmetry", symmetry }),
      [send],
    ),
    setRoster: useCallback((factions: string[]) => send({ type: "setRoster", factions }), [send]),
    resize: useCallback(
      (width: number, height: number, anchor: ResizeAnchor) =>
        send({ type: "resize", width, height, anchor }),
      [send],
    ),
    fill: useCallback((terrain: number) => send({ type: "fill", terrain }), [send]),
    undo: useCallback(() => send({ type: "undo" }), [send]),
    redo: useCallback(() => send({ type: "redo" }), [send]),
    readDocument: useCallback(
      (name: string, author: string) => runner.readEditorDocument(name, author),
      [runner],
    ),
  };
}
