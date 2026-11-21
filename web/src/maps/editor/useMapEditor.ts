import { useCallback, useEffect, useRef, useState } from "react";
import type { GameRunner } from "#/engine/game_runner.ts";
import type { AwbrnMapDocument } from "#/maps/map_document.ts";
import type {
  Brush,
  EditorCommand,
  EditorPalette,
  EditorStateChanged,
  EditorTile,
  ResizeAnchor,
  Symmetry,
} from "#/wasm/awbrn_wasm.js";

export interface MapEditorSource {
  /** The map to open, or null for a blank board. */
  document: AwbrnMapDocument | null;
  width: number;
  height: number;
}

export interface MapEditorHandle {
  state: EditorStateChanged | null;
  /** Whether `source` has opened on the board. */
  sourceReady: boolean;
  /** The chosen army's buildings and units. */
  palette: EditorPalette | null;
  /** The neutral buildings. */
  neutralPalette: EditorPalette | null;
  error: string | null;
  setBrush: (brush: Brush) => void;
  setSymmetry: (symmetry: Symmetry) => void;
  setRoster: (factionCodes: string[]) => void;
  resize: (width: number, height: number, anchor: ResizeAnchor) => void;
  fill: (terrain: number) => void;
  undo: () => void;
  redo: () => void;
  /** Rings `tiles`; `center` also brings the first into view. */
  focusTiles: (tiles: EditorTile[], center: boolean) => void;
  readDocument: (name: string, author: string) => Promise<AwbrnMapDocument>;
}

function message(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

export function useMapEditor({
  factionCode,
  runner,
  source,
}: {
  factionCode: string;
  runner: GameRunner;
  source: MapEditorSource | null;
}): MapEditorHandle {
  const [state, setState] = useState<EditorStateChanged | null>(null);
  const [palette, setPalette] = useState<EditorPalette | null>(null);
  const [neutralPalette, setNeutralPalette] = useState<EditorPalette | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [openedSource, setOpenedSource] = useState<MapEditorSource | null>(null);

  useEffect(() => {
    runner.setEditorStateHandler((next) => setState(next));
    return () => runner.setEditorStateHandler(undefined);
  }, [runner]);

  useEffect(() => {
    if (!source) return;
    let canceled = false;
    // Deferred, so a mount that is torn down at once (Strict Mode) opens nothing.
    void Promise.resolve()
      .then(async () => {
        if (canceled) return;
        if (source.document) await runner.openEditor(source.document);
        else await runner.openBlankEditor(source.width, source.height);
        if (!canceled) setOpenedSource(source);
      })
      .catch((cause: unknown) => {
        if (!canceled) setError(message(cause));
      });
    return () => {
      canceled = true;
    };
  }, [runner, source]);

  useEffect(() => {
    let canceled = false;
    Promise.all([runner.loadEditorPalette(factionCode), runner.loadEditorPalette(null)]).then(
      ([army, neutral]) => {
        if (canceled) return;
        setPalette(army);
        setNeutralPalette(neutral);
      },
      (cause: unknown) => {
        if (!canceled) setError(message(cause));
      },
    );
    return () => {
      canceled = true;
    };
  }, [factionCode, runner]);

  // Commands answer out of order; only the newest may set or clear the error.
  const sent = useRef(0);
  const send = useCallback(
    (command: EditorCommand) => {
      sent.current += 1;
      const mine = sent.current;
      runner.sendEditorCommand(command).then(
        () => {
          if (mine === sent.current) setError(null);
        },
        (cause: unknown) => {
          if (mine === sent.current) setError(message(cause));
        },
      );
    },
    [runner],
  );

  return {
    state,
    sourceReady: source !== null && openedSource === source,
    palette,
    neutralPalette,
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
    focusTiles: useCallback(
      (tiles: EditorTile[], center: boolean) => send({ type: "focusTiles", tiles, center }),
      [send],
    ),
    readDocument: useCallback(
      (name: string, author: string) => runner.readEditorDocument(name, author),
      [runner],
    ),
  };
}
