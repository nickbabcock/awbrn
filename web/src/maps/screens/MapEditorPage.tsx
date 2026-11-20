import { useMediaQuery } from "@astryxdesign/core/hooks";
import { Heading } from "@astryxdesign/core/Heading";
import { Icon } from "@astryxdesign/core/Icon";
import { Stack } from "@astryxdesign/core/Stack";
import { colorVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { useEffect, useEffectEvent, useMemo, useRef, useState } from "react";
import { useActor } from "#/auth/useActor.ts";
import { useCanvasCourierSurface } from "#/canvas_courier/index.ts";
import { useMapEditorRunner } from "#/engine/runtime_context.tsx";
import { ArmyLedger } from "#/maps/editor/ArmyLedger.tsx";
import { BoardWell } from "#/maps/editor/BoardWell.tsx";
import { BrushTray } from "#/maps/editor/BrushTray.tsx";
import { SendSheet } from "#/maps/editor/SendSheet.tsx";
import { isField, useEditorShortcuts } from "#/maps/editor/useEditorShortcuts.ts";
import { useMapEditor, type MapEditorSource } from "#/maps/editor/useMapEditor.ts";
import { MapPicture } from "#/maps/components/MapPicture.tsx";
import { MapRankMedal } from "#/maps/components/MapRankMedal.tsx";
import { mapEditGrant } from "#/maps/map_authz.ts";
import {
  brushForArmy,
  brushKey,
  ERASE_CELL,
  isDirty,
  LIFT_CELL,
  nameProblem,
  problemArmy,
  problems as listProblems,
  problemTiles,
  saveFailure,
  saveKey,
  savesAsNewMap,
  stepBrush,
  tabOfBrush,
  trayCells,
  type BrushTab,
  type EditorProblem,
  type SaveFailure,
  type SaveIntent,
} from "#/maps/map_editor.ts";
import { mapScreenshotSize } from "#/maps/map_screenshot.ts";
import { mapKeys } from "#/maps/maps.keys.ts";
import { saveMapFn } from "#/maps/maps.functions.ts";
import { mapQueryOptions, mapRevisionQueryOptions } from "#/maps/maps.queries.ts";
import { MAP_EDITOR_DEFAULT_SIZE, MAP_NAME_MAX, type MapCatalogEntry } from "#/maps/schemas.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { RouterButton } from "#/ui/astryx-links.tsx";
import { figureStyles } from "#/ui/Figure.tsx";
import { IconAlert, IconBack, IconFork } from "#/ui/icons.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { ShellMode } from "#/ui/ShellMode.tsx";
import type { Brush, PaletteCell } from "#/wasm/awbrn_wasm.js";

export function MapEditorPage({ mapId, startFrom }: { mapId?: string; startFrom?: string }) {
  const sourceMapId = mapId ?? startFrom ?? null;
  const sourceMap = useQuery({
    ...mapQueryOptions(sourceMapId ?? ""),
    enabled: sourceMapId !== null,
  });
  const isPhone = useMediaQuery(pageLayout.phoneMedia.replace("@media ", ""));

  if (isPhone) return <EditorPhoneFallback map={sourceMap.data ?? null} />;
  // Each source opens a fresh editor, so nothing of one map carries into the next.
  return (
    <MapEditor
      entry={sourceMap.data ?? null}
      key={`${mapId ?? ""}:${startFrom ?? ""}`}
      mapId={mapId}
      sourceMapId={sourceMapId}
    />
  );
}

function MapEditor({
  entry: liveEntry,
  mapId,
  sourceMapId,
}: {
  entry: MapCatalogEntry | null;
  mapId: string | undefined;
  sourceMapId: string | null;
}) {
  const actor = useActor();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const runner = useMapEditorRunner();
  const { canvasContainerRef, surfaceRef } = useCanvasCourierSurface({
    host: runner.host,
    canvasClassName: stylex.props(styles.canvas).className ?? "",
  });

  // The map as it was when the board opened; a later refetch never reopens it.
  const [entry, setEntry] = useState<MapCatalogEntry | null>(null);
  const [name, setName] = useState("");
  const [savedAt, setSavedAt] = useState({ historyId: 0, name: "" });
  useEffect(() => {
    if (liveEntry === null || entry !== null) return;
    setEntry(liveEntry);
    const isRevise = mapId !== undefined && mapEditGrant(liveEntry, actor) !== null;
    const opened = isRevise ? liveEntry.name : `${liveEntry.name} (fork)`;
    setName(opened);
    setSavedAt({ historyId: 0, name: opened });
  }, [actor, entry, liveEntry, mapId]);
  const revision = useQuery({
    ...mapRevisionQueryOptions(entry?.mapId ?? "", entry?.revision ?? 0),
    enabled: entry !== null,
  });
  const source = useMemo<MapEditorSource | null>(() => {
    if (sourceMapId === null) return { document: null, ...MAP_EDITOR_DEFAULT_SIZE };
    return revision.data ? { document: revision.data, ...MAP_EDITOR_DEFAULT_SIZE } : null;
  }, [revision.data, sourceMapId]);

  const intent: SaveIntent =
    sourceMapId === null
      ? "create"
      : mapId !== undefined && entry !== null && mapEditGrant(entry, actor) !== null
        ? "revise"
        : "fork";
  const [recovered, setRecovered] = useState(false);
  const saveIntent: SaveIntent = recovered ? "fork" : intent;

  const [factionCode, setFactionCode] = useState("os");
  const editor = useMapEditor({ factionCode, runner, source });
  const state = editor.sourceReady ? editor.state : null;

  const nameRef = useRef<HTMLInputElement>(null);
  const [nameError, setNameError] = useState<string | null>(null);
  const [failure, setFailure] = useState<SaveFailure | null>(null);

  const [tab, setTab] = useState<BrushTab>("terrain");
  const [isKeysOpen, setIsKeysOpen] = useState(false);
  const [step, setStep] = useState(0);
  const [hqTurn, setHqTurn] = useState(0);

  const roster = state?.roster ?? [];
  useEffect(() => {
    if (roster.length > 0 && !roster.includes(factionCode)) setFactionCode(roster[0] ?? "os");
  }, [factionCode, roster]);

  const cells = useMemo(
    () => trayCells(tab, editor.palette, editor.neutralPalette),
    [editor.neutralPalette, editor.palette, tab],
  );
  const brushes = useMemo(() => cells.filter((cell) => cell !== null), [cells]);
  const loaded = state?.brush ?? null;
  const loadedKey = loaded === null ? null : brushKey(loaded);

  // A brush loaded from the board brings its tab and its army with it.
  const followBrush = useEffectEvent(() => {
    if (loaded === null) return;
    if (!brushes.some((cell) => brushKey(cell.brush) === loadedKey)) setTab(tabOfBrush(loaded));
    const faction = "faction" in loaded ? loaded.faction : undefined;
    if (faction !== undefined && roster.includes(faction)) setFactionCode(faction);
  });
  useEffect(() => followBrush(), [loadedKey]);

  const isPristine = intent === "create" && state?.historyId === 0;
  const problems = useMemo(
    () => (state ? listProblems(state, isPristine) : []),
    [isPristine, state],
  );
  const focusedAt = problems.length === 0 ? 0 : Math.min(step, problems.length - 1);
  const focused: EditorProblem | null = problems[focusedAt] ?? null;
  const ringed = problemTiles(focused);
  const turn = ringed.length > 0 ? hqTurn % ringed.length : 0;
  // The view moves only on an explicit step; an edit that changes the ring leaves it be.
  const [jumps, setJumps] = useState(0);
  const jumped = useRef(0);
  const sendRing = useEffectEvent(() => {
    if (!editor.sourceReady) return;
    editor.focusTiles([...ringed.slice(turn), ...ringed.slice(0, turn)], jumps !== jumped.current);
    jumped.current = jumps;
  });
  const ringKey = JSON.stringify([editor.sourceReady, ringed, turn, jumps]);
  useEffect(() => sendRing(), [ringKey]);

  const stepProblem = (by: 1 | -1) => {
    if (problems.length === 0) return;
    setHqTurn(0);
    setStep((focusedAt + by + problems.length) % problems.length);
    setJumps((count) => count + 1);
  };
  const focusProblem = (match: (problem: EditorProblem) => boolean) => {
    const at = problems.findIndex(match);
    if (at === -1) return;
    setStep(at);
    setJumps((count) => count + 1);
  };

  const choose = (faction: string) => {
    setFactionCode(faction);
    if (loaded !== null) {
      const recolored = brushForArmy(loaded, faction);
      if (recolored !== loaded) editor.setBrush(recolored);
    }
  };
  const load = (faction: string, property: "hq" | "base", kind: EditorProblem["kind"]) => {
    setFactionCode(faction);
    editor.setBrush({ kind: "property", property, faction });
    setTab("buildings");
    setIsKeysOpen(false);
    focusProblem(
      (problem) => problem.kind === kind && "faction" in problem && problem.faction === faction,
    );
  };

  const boardCanvas = () => surfaceRef.current?.querySelector("canvas") ?? null;
  const focusBoard = () => boardCanvas()?.focus({ preventScroll: true });
  const toggleKeys = (isOpen: boolean) => {
    setIsKeysOpen(isOpen);
    if (!isOpen) focusBoard();
  };
  // A click on a key hands the keyboard back to the board. A key that opens a
  // menu, and the menu itself, leave it to the menu (see `MenuKey`).
  const onPageClick = (event: React.MouseEvent) => {
    const target = event.target;
    if (event.detail === 0 || isField(target)) return;
    if (target instanceof Element && target.closest('[role="dialog"], [aria-haspopup]')) return;
    focusBoard();
  };

  useEditorShortcuts({
    armies: roster,
    canvas: boardCanvas,
    cells: brushes,
    isEnabled: state !== null,
    isKeysOpen,
    loadedBrush: loaded,
    onArmyChange: choose,
    onKeysToggle: toggleKeys,
    onSelect: editor.setBrush,
    onStepProblem: stepProblem,
    onTabChange: (next) => {
      setIsKeysOpen(false);
      setTab(next);
    },
    tab,
  });

  const isUnsaved = state !== null && isDirty(state, savedAt, name);
  const save = useMutation({
    mutationFn: async () => {
      const trimmed = name.trim();
      const document = await editor.readDocument(trimmed, entry?.author ?? "");
      if (saveIntent === "revise" && mapId !== undefined && entry !== null) {
        return saveMapFn({
          data: {
            mapId,
            expectedRevision: entry.revision,
            expectedName: entry.name,
            name: trimmed,
            document,
          },
        });
      }
      return saveMapFn({ data: { name: trimmed, document } });
    },
    onMutate: () => setFailure(null),
    onSuccess: async (result) => {
      await queryClient.invalidateQueries({ queryKey: mapKeys.all });
      await navigate({ params: { mapId: result.mapId }, to: "/maps/$mapId" });
    },
    onError: (error) => {
      const next = saveFailure(error);
      setFailure(next);
      if (savesAsNewMap(next) && !recovered) {
        setRecovered(true);
        setName((current) => `${current.trim()} (fork)`);
      }
    },
  });
  const submit = () => {
    const problem = nameProblem(name, MAP_NAME_MAX);
    setNameError(problem);
    if (problem !== null) {
      nameRef.current?.focus();
      return;
    }
    save.mutate();
  };

  const findCell = (brush: Brush): PaletteCell | null => {
    const key = brushKey(brush);
    const all = [
      ...(editor.palette?.terrain ?? []),
      ...(editor.palette?.units ?? []),
      ...(editor.neutralPalette?.terrain ?? []),
      ERASE_CELL,
      LIFT_CELL,
    ];
    return all.find((cell) => brushKey(cell.brush) === key) ?? null;
  };
  const fillCells = (editor.neutralPalette?.terrain ?? []).filter(
    (cell) => cell.brush.kind === "terrain",
  );
  const neutralCity = editor.neutralPalette?.terrain.find(
    (cell) => cell.brush.kind === "property" && cell.brush.property === "city",
  )?.spriteIndex;
  const railRef = useRef<HTMLElement>(null);

  return (
    <>
      <ShellMode mode="tool" />
      <div onClick={onPageClick} {...stylex.props(styles.page)}>
        <BoardWell
          canvasContainerRef={canvasContainerRef}
          fillCells={fillCells}
          onFill={editor.fill}
          onMenuDone={focusBoard}
          onResize={editor.resize}
          onRoster={editor.setRoster}
          onSelect={editor.setBrush}
          onSymmetry={editor.setSymmetry}
          runner={runner}
          state={state}
          surfaceRef={surfaceRef}
        />
        <aside ref={railRef} {...stylex.props(styles.rail)}>
          <EditorHead
            entry={entry}
            intent={intent}
            name={name}
            nameError={nameError}
            nameRef={nameRef}
            onName={(next) => {
              setName(next);
              if (nameError !== null) setNameError(nameProblem(next, MAP_NAME_MAX));
            }}
          />
          {state ? (
            <ArmyLedger
              chosen={factionCode}
              focused={focused}
              focusedArmy={problemArmy(focused)}
              isPristine={isPristine}
              neutralCity={neutralCity}
              onChoose={choose}
              onLoadBase={(faction) => load(faction, "base", "no-build")}
              onLoadHq={(faction) => load(faction, "hq", "no-hq")}
              onStepHqs={(faction) => {
                const isFocused = focused?.kind === "extra-hq" && focused.faction === faction;
                if (isFocused) {
                  setHqTurn((current) => current + 1);
                  setJumps((count) => count + 1);
                } else {
                  setHqTurn(0);
                  focusProblem(
                    (problem) => problem.kind === "extra-hq" && problem.faction === faction,
                  );
                }
              }}
              railRef={railRef}
              state={state}
            />
          ) : null}
          {state ? (
            <BrushTray
              cells={cells}
              isKeysOpen={isKeysOpen}
              loaded={loaded === null ? null : { brush: loaded, cell: findCell(loaded) }}
              onKeysToggle={toggleKeys}
              onSelect={editor.setBrush}
              onStep={(by) => {
                const next = stepBrush(brushes, loaded, by);
                if (next !== null) editor.setBrush(next);
              }}
              onTabChange={setTab}
              tab={tab}
            />
          ) : null}
          <span data-spacer="" {...stylex.props(styles.spacer)} />
          <SendSheet
            canRedo={state?.canRedo ?? false}
            canUndo={state?.canUndo ?? false}
            failure={failure}
            focusedAt={focusedAt}
            isPristine={isPristine}
            isSaving={save.isPending}
            isUnsaved={isUnsaved}
            latestHref={mapId === undefined ? null : `/maps/${mapId}`}
            notice={editor.error}
            onRedo={editor.redo}
            onRetry={submit}
            onSave={submit}
            onStep={stepProblem}
            onUndo={editor.undo}
            problems={problems}
            saveKey={
              state === null
                ? { ...saveKey(saveIntent, false), isInert: true }
                : saveKey(saveIntent, isUnsaved)
            }
            unranks={
              saveIntent === "revise" && state?.boardChanged === true ? (entry?.rank ?? null) : null
            }
          />
        </aside>
      </div>
    </>
  );
}

function EditorHead({
  entry,
  intent,
  name,
  nameError,
  nameRef,
  onName,
}: {
  entry: MapCatalogEntry | null;
  intent: SaveIntent;
  name: string;
  nameError: string | null;
  nameRef: React.Ref<HTMLInputElement>;
  onName: (name: string) => void;
}) {
  return (
    <div {...stylex.props(styles.head)}>
      <Link aria-label="Maps" title="Maps" to="/maps" {...stylex.props(styles.back)}>
        <Icon icon={IconBack} size="md" />
      </Link>
      <Stack xstyle={styles.headMain}>
        <input
          aria-describedby={nameError === null ? undefined : "map-name-error"}
          aria-invalid={nameError !== null || undefined}
          aria-label="Map name"
          onChange={(event) => onName(event.target.value)}
          placeholder="Name"
          ref={nameRef}
          value={name}
          {...stylex.props(styles.name, nameError !== null && styles.nameInvalid)}
        />
        {nameError === null ? null : (
          <span id="map-name-error" {...stylex.props(styles.nameError)}>
            <Icon icon={IconAlert} size="sm" />
            {nameError}
          </span>
        )}
        {entry !== null && intent === "revise" ? (
          <p {...stylex.props(figureStyles.figure, styles.postage)}>
            Rev {entry.revision} · {entry.author}
          </p>
        ) : null}
        {entry !== null && intent === "fork" ? (
          <p {...stylex.props(figureStyles.figure, styles.postage)}>
            <Icon icon={IconFork} size="sm" xstyle={styles.postageIcon} />
            <Link
              params={{ mapId: entry.mapId }}
              rel="noopener"
              target="_blank"
              title="Opens in a new tab"
              to="/maps/$mapId"
              {...stylex.props(styles.source)}
            >
              {entry.name}
            </Link>
            · {entry.author}
          </p>
        ) : null}
      </Stack>
      {entry !== null && intent === "revise" ? <MapRankMedal rank={entry.rank} size="sm" /> : null}
    </div>
  );
}

function EditorPhoneFallback({ map }: { map: MapCatalogEntry | null }) {
  const picture = map === null ? null : mapScreenshotSize("small", map.width, map.height);
  return (
    <Stack xstyle={styles.phone}>
      <ShellMode mode="tool" />
      <Stack gap={5} xstyle={styles.phoneSheet}>
        {map === null || picture === null ? null : (
          <Stack xstyle={[styles.phoneWell, styles.phoneWellWidth(picture.width * 2)]}>
            <MapPicture
              alt=""
              maxScale={2}
              sourceHeight={picture.height}
              sourceWidth={picture.width}
              src={map.screenshot.small}
            />
          </Stack>
        )}
        <Heading level={1} xstyle={styles.phoneTitle}>
          Edit on a desktop
        </Heading>
        {map === null ? (
          <RouterButton
            icon={<Icon icon={IconBack} />}
            label="Maps"
            size="lg"
            to="/maps"
            variant="secondary"
          />
        ) : (
          <RouterButton
            icon={<Icon icon={IconBack} />}
            label={map.name}
            params={{ mapId: map.mapId }}
            size="lg"
            to="/maps/$mapId"
            variant="secondary"
          />
        )}
      </Stack>
    </Stack>
  );
}

const styles = stylex.create({
  page: {
    display: "grid",
    gridTemplateColumns: "minmax(0, 1fr) 384px",
    gap: "20px",
    boxSizing: "border-box",
    blockSize: `calc(100dvh - ${pageLayout.topBarHeight})`,
    minBlockSize: "640px",
    paddingBlock: "16px 20px",
    paddingInline: "24px",
  },
  canvas: {
    display: "block",
    inlineSize: "100%",
    blockSize: "100%",
    imageRendering: "pixelated",
    outline: "none",
  },
  rail: {
    display: "flex",
    flexDirection: "column",
    gap: { default: "12px", [pageLayout.shortMedia]: "10px" },
    minBlockSize: 0,
  },
  spacer: {
    flexGrow: 1,
    flexShrink: 1,
    flexBasis: 0,
  },
  head: {
    display: "grid",
    gridTemplateColumns: "28px minmax(0, 1fr) auto",
    alignItems: "start",
    columnGap: "8px",
    flexShrink: 0,
    paddingInline: "2px",
  },
  back: {
    display: "grid",
    placeItems: "center",
    inlineSize: "28px",
    blockSize: "36px",
    marginInlineStart: "-6px",
    borderRadius: "6px",
    color: awbrnVars.blue,
  },
  headMain: {
    minInlineSize: 0,
  },
  name: {
    boxSizing: "border-box",
    inlineSize: "100%",
    paddingBlock: "2px 4px",
    paddingInline: 0,
    borderStyle: "none",
    borderBlockEndColor: awbrnVars.ruleStrong,
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: "1px",
    backgroundColor: "transparent",
    color: colorVars["--color-text-primary"],
    fontFamily: awbrnVars.fontUi,
    fontSize: { default: "28px", [pageLayout.shortMedia]: "26px" },
    fontStretch: "68%",
    fontWeight: 800,
    lineHeight: 1.1,
    wordSpacing: "0.1em",
    outline: "none",
    "::placeholder": {
      color: awbrnVars.textTertiary,
    },
  },
  nameInvalid: {
    borderBlockEndColor: awbrnVars.danger,
    borderBlockEndWidth: "2px",
  },
  nameError: {
    display: "flex",
    alignItems: "center",
    gap: "6px",
    marginBlockStart: "6px",
    color: awbrnVars.danger,
    fontSize: awbrnVars.text13_5,
    fontWeight: 650,
  },
  postage: {
    display: "flex",
    alignItems: "center",
    gap: "6px",
    margin: 0,
    marginBlockStart: "8px",
    overflow: "hidden",
    color: colorVars["--color-text-secondary"],
  },
  postageIcon: {
    color: awbrnVars.textTertiary,
  },
  source: {
    color: awbrnVars.blue,
    fontFamily: awbrnVars.fontUi,
    fontSize: awbrnVars.text13_5,
    fontWeight: 650,
    textDecoration: "none",
  },
  phone: {
    paddingBlock: "20px",
    paddingInline: pageLayout.gutterPhone,
  },
  // The empty-state sheet, centered, with the title in the display face.
  phoneSheet: {
    alignItems: "center",
    padding: "28px",
    borderColor: awbrnVars.ruleStrong,
    borderRadius: "10px",
    borderStyle: "dashed",
    borderWidth: "1px",
    backgroundColor: awbrnVars.sheet,
    textAlign: "center",
  },
  phoneTitle: {
    fontSize: awbrnVars.text26,
  },
  phoneWell: {
    boxSizing: "content-box",
    maxInlineSize: "100%",
    padding: "16px",
    borderRadius: "10px",
    backgroundColor: awbrnVars.ink950,
  },
  phoneWellWidth: (width: number) => ({
    inlineSize: `${width}px`,
  }),
});
