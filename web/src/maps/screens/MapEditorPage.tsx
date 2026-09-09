/*
 * THE DRAFTING TABLE
 *
 * THESIS: a competitive map is not drawn tile by tile, it is drawn once and
 *   folded. The fold is therefore not a setting on this screen, it is the
 *   screen: the board is the engine's own board, the rail is the game's own
 *   art, and the panel on the right is the only thing here that is not in the
 *   game at all — the readout that says whether what has been drawn is a
 *   match yet.
 * OWN-WORLD: one cream drafting table on the open sky, divided by the same
 *   black rule every panel in this system wears. The brush cursor and the
 *   chosen fold are the game's own orange cursor, sitting flush.
 * STORY: a map maker opens a blank board or forks one they like, sets the
 *   fold, draws half a map, watches the other half draw itself, and keeps it.
 * FIRST VIEWPORT: the board, the rail beside it, the fold beside that.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { Banner } from "@astryxdesign/core/Banner";
import { Card } from "@astryxdesign/core/Card";
import { Divider } from "@astryxdesign/core/Divider";
import { Heading } from "@astryxdesign/core/Heading";
import { Kbd } from "@astryxdesign/core/Kbd";
import { Layout, LayoutPanel } from "@astryxdesign/core/Layout";
import { Section } from "@astryxdesign/core/Section";
import { Skeleton } from "@astryxdesign/core/Skeleton";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { TextInput } from "@astryxdesign/core/TextInput";
import {
  borderVars,
  colorVars,
  spacingVars,
  textSizeVars,
  typographyVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { ArrowLeft as ArrowLeftIcon } from "pixelarticons/react/ArrowLeft";
import { useEffect, useMemo, useState } from "react";
import { useCanvasCourierSurface } from "#/canvas_courier/index.ts";
import { TileInfoBar } from "#/components/TileInfoBar.tsx";
import { useActor } from "#/auth/useActor.ts";
import { useMapEditorRunner } from "#/engine/runtime_context.tsx";
import { mapEditGrant } from "#/maps/map_authz.ts";
import {
  EDITOR_SHORTCUTS,
  funds,
  paletteSections,
  rosterOfSize,
  saveLabel,
  type PaletteDrawer,
  type SaveIntent,
} from "#/maps/map_editor.ts";
import { mapKeys } from "#/maps/maps.keys.ts";
import { saveMapFn } from "#/maps/maps.functions.ts";
import { mapQueryOptions, mapRevisionQueryOptions } from "#/maps/maps.queries.ts";
import { MAP_EDITOR_DEFAULT_SIZE } from "#/maps/schemas.ts";
import { BoardShapePanel } from "#/maps/editor/BoardShapePanel.tsx";
import { MusterPanel } from "#/maps/editor/MusterPanel.tsx";
import { PaletteRail } from "#/maps/editor/PaletteRail.tsx";
import { SymmetryPanel } from "#/maps/editor/SymmetryPanel.tsx";
import { useEditorShortcuts } from "#/maps/editor/useEditorShortcuts.ts";
import { useMapEditor, type MapEditorSource } from "#/maps/editor/useMapEditor.ts";
import { Button } from "#/ui/Button.tsx";
import { RouterTextLink } from "#/ui/astryx-links.tsx";

export function MapEditorPage({
  mapId,
  startFrom,
}: {
  /** The map being edited, or absent for a board of its own. */
  mapId?: string;
  /** The map a new board is forked from. */
  startFrom?: string;
}) {
  const actor = useActor();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const runner = useMapEditorRunner();
  const { canvasRef, surfaceRef } = useCanvasCourierSurface({ controller: runner });

  const sourceMapId = mapId ?? startFrom ?? null;
  const sourceMap = useQuery({
    ...mapQueryOptions(sourceMapId ?? ""),
    enabled: sourceMapId !== null,
  });
  const entry = sourceMap.data ?? null;
  const sourceRevision = useQuery({
    ...mapRevisionQueryOptions(entry?.mapId ?? "", entry?.revision ?? 0),
    enabled: entry !== null,
  });

  const resolvedSource = useMemo<MapEditorSource | null>(() => {
    if (sourceMapId === null) {
      return { document: null, revision: null, ...MAP_EDITOR_DEFAULT_SIZE };
    }
    if (!sourceRevision.data) return null;
    return {
      document: sourceRevision.data,
      revision: entry?.revision ?? null,
      ...MAP_EDITOR_DEFAULT_SIZE,
    };
  }, [entry?.revision, sourceMapId, sourceRevision.data]);

  const [opened, setOpened] = useState<{
    mapId: string | null;
    source: MapEditorSource;
    entry: typeof entry;
  } | null>(null);
  useEffect(() => {
    if (!resolvedSource) return;
    setOpened((current) =>
      current?.mapId === sourceMapId
        ? current
        : { mapId: sourceMapId, source: resolvedSource, entry },
    );
  }, [entry, resolvedSource, sourceMapId]);

  const openedEditor = opened?.mapId === sourceMapId ? opened : null;
  const source = sourceMapId === null ? resolvedSource : (openedEditor?.source ?? null);
  const editorEntry = openedEditor?.entry ?? null;

  const [factionCode, setFactionCode] = useState("os");
  const editor = useMapEditor({ factionCode, runner, source });
  const state = editor.state;

  const [name, setName] = useState("");
  useEffect(() => {
    if (!editorEntry) return;
    setName(mapId === undefined ? `${editorEntry.name} (fork)` : editorEntry.name);
  }, [editorEntry, mapId]);

  const grant = editorEntry ? mapEditGrant(editorEntry, actor) : null;
  const intent: SaveIntent =
    mapId !== undefined && grant !== null ? "revise" : sourceMapId === null ? "create" : "fork";

  const save = useMutation({
    mutationFn: async () => {
      const trimmed = name.trim();
      const document = await editor.readDocument(trimmed, editorEntry?.author ?? "");
      if (intent === "revise") {
        if (
          mapId === undefined ||
          source === null ||
          source.revision === null ||
          editorEntry === null
        ) {
          throw new Error("The revision this board opened is not available.");
        }
        return saveMapFn({
          data: {
            mapId,
            expectedRevision: source.revision,
            expectedName: editorEntry.name,
            name: trimmed,
            document,
          },
        });
      }
      return saveMapFn({ data: { name: trimmed, document } });
    },
    onSuccess: async (result) => {
      await queryClient.invalidateQueries({ queryKey: mapKeys.all });
      await navigate({ params: { mapId: result.mapId }, to: "/maps/$mapId" });
    },
  });

  const roster = state?.roster ?? rosterOfSize([], 2);
  // The rail paints as an army the fold knows about. An army that leaves the
  // roster leaves the brush pointing at nobody, so the rail follows it back.
  useEffect(() => {
    if (roster.length > 0 && !roster.includes(factionCode)) {
      setFactionCode(roster[0] ?? "os");
    }
  }, [factionCode, roster]);

  // The open drawer belongs to the screen rather than to the rail, because two
  // things reach it: the tabs on the rail, and the number keys. A brush loaded
  // from the board — an undo, or a tile picked up with alt — opens its drawer
  // through the same one piece of state.
  const [drawer, setDrawer] = useState<PaletteDrawer>("terrain");
  const sections = useMemo(
    () => paletteSections([...(editor.palette?.terrain ?? []), ...(editor.palette?.units ?? [])]),
    [editor.palette],
  );

  useEditorShortcuts({
    armies: roster,
    drawer,
    isEnabled: state !== null && sections.length > 0,
    loadedBrush: state?.brush ?? null,
    onArmyChange: setFactionCode,
    onDrawerChange: setDrawer,
    onSelect: editor.setBrush,
    sections,
  });

  const isSignedIn = actor !== null;
  const canSave =
    isSignedIn && name.trim().length > 0 && state !== null && editor.sourceReady && !save.isPending;

  return (
    <Section padding={5} variant="transparent">
      <VStack gap={3}>
        <HStack align="center" gap={1}>
          <ArrowLeftIcon aria-hidden height={14} width={14} />
          <RouterTextLink to="/maps">All maps</RouterTextLink>
        </HStack>

        <HStack align="end" gap={4} justify="between" wrap="wrap">
          <VStack gap={0.5}>
            <Heading level={1} xstyle={styles.title}>
              {sourceMapId === null ? "New map" : (editorEntry?.name ?? "Map")}
            </Heading>
            <Text color="secondary" type="label">
              {intent === "revise"
                ? `Editing revision ${source?.revision ?? 1}`
                : intent === "fork"
                  ? `Forked from ${editorEntry?.name ?? "a map"} by ${editorEntry?.author ?? "unknown"}`
                  : "A board of your own"}
            </Text>
          </VStack>

          {/* Keeping the map is one act, so it is one cluster: the name it is
            kept under, and the key that keeps it. The board's own two keys sit
            with it because a map maker reaches for undo as often as for save. */}
          <HStack align="end" gap={2} wrap="wrap">
            <Button
              clickAction={editor.undo}
              isDisabled={!state?.canUndo}
              label="Undo"
              size="sm"
              variant="secondary"
            />
            <Button
              clickAction={editor.redo}
              isDisabled={!state?.canRedo}
              label="Redo"
              size="sm"
              variant="secondary"
            />
            <VStack gap={0} xstyle={styles.nameField}>
              <TextInput
                label="Map name"
                onChange={setName}
                placeholder="Name this battlefield"
                size="sm"
                value={name}
              />
            </VStack>
            <Button
              clickAction={() => save.mutate()}
              isDisabled={!canSave}
              isLoading={save.isPending}
              label={saveLabel(intent)}
              size="sm"
              variant="primary"
            />
          </HStack>
        </HStack>

        {isSignedIn ? null : (
          <Text color="secondary" type="supporting">
            Sign in to keep a map in the catalog. The board works either way.
          </Text>
        )}

        {editor.error ? (
          <Banner description={editor.error} status="error" title="The board reported a problem" />
        ) : null}
        {save.isError ? (
          <Banner
            description={
              save.error instanceof Error
                ? save.error.message
                : "The catalog did not say why. Try again in a moment."
            }
            status="error"
            title="The map was not kept"
          />
        ) : null}

        <Card padding={0} xstyle={styles.table}>
          <Layout
            end={
              <LayoutPanel
                hasDivider
                label="Map settings"
                padding={3}
                width={340}
                xstyle={styles.settings}
              >
                <VStack gap={4}>
                  <VStack gap={2}>
                    <PanelHeading>The fold</PanelHeading>
                    {state ? (
                      <SymmetryPanel
                        available={state.availableSymmetries}
                        onRosterChange={editor.setRoster}
                        onSymmetryChange={editor.setSymmetry}
                        roster={state.roster}
                        symmetry={state.symmetry}
                      />
                    ) : (
                      <Skeleton height={220} radius="none" />
                    )}
                  </VStack>

                  <Divider />

                  <BoardShapePanel
                    height={state?.height ?? MAP_EDITOR_DEFAULT_SIZE.height}
                    loadedBrush={state?.brush ?? null}
                    onFill={editor.fill}
                    onResize={editor.resize}
                    width={state?.width ?? MAP_EDITOR_DEFAULT_SIZE.width}
                  />

                  <Divider />

                  <KeyLegend />
                </VStack>
              </LayoutPanel>
            }
            height="fill"
            // The rail is wide enough for five keys, the drawer's scroll
            // gutter and the slack that keeps a full drawer from rewrapping:
            // 24px of board buys a whole column of brushes and takes a row off
            // every drawer. See `styles.key` in `PaletteRail`.
            start={
              <LayoutPanel hasDivider label="Brushes" padding={3} width={344}>
                <PaletteRail
                  armies={roster}
                  drawer={drawer}
                  factionCode={factionCode}
                  loadedBrush={state?.brush ?? null}
                  onDrawerChange={setDrawer}
                  onFactionChange={setFactionCode}
                  onSelect={editor.setBrush}
                  sections={sections}
                />
              </LayoutPanel>
            }
          >
            <VStack gap={0} xstyle={styles.board}>
              <VStack gap={0} ref={surfaceRef} xstyle={styles.surface}>
                <canvas
                  height={640}
                  ref={canvasRef}
                  tabIndex={0}
                  width={960}
                  {...stylex.props(styles.canvas)}
                />
                <TileInfoBar />
              </VStack>

              <HStack gap={4} justify="between" wrap="wrap" xstyle={styles.hud}>
                <VStack as="span" gap={0} xstyle={styles.hudLine}>
                  Drag to draw · Alt-click to pick up a tile · Space and drag to move · Wheel to
                  zoom
                </VStack>
                {/* The other half of the strip is what the board is, in
                    figures that belong to the whole board rather than to a
                    seat: how big it is, how much of it is still unclaimed, and
                    what it pays. The share is the number a map maker is
                    drawing towards — what a half of this map is worth when the
                    halves are equal. */}
                <VStack as="span" gap={0} xstyle={styles.hudLine}>
                  {state
                    ? `${state.width} × ${state.height} · ${state.neutralProperties} neutral · ${funds(state.boardIncome)} / turn · ${funds(state.incomePerArmy)} an army`
                    : ""}
                </VStack>
              </HStack>

              {state ? <MusterPanel state={state} /> : null}
            </VStack>
          </Layout>
        </Card>
      </VStack>
    </Section>
  );
}

/**
 * The one heading inside the settings panel.
 *
 * It keeps its rank in the document and gives up the display voice: a stack of
 * signage headings down one panel is a poster, not a set of controls. The other
 * three headings this panel used to carry are gone with the sections they
 * named — a labelled control does not need a heading over it as well.
 */
function PanelHeading({ children }: { children: string }) {
  return (
    <Heading level={2} xstyle={styles.panelHeading}>
      {children}
    </Heading>
  );
}

/**
 * The keys, printed where the hand can see them.
 *
 * This is the part of the screen that is for somebody on their tenth map
 * rather than their first. A map maker who has drawn a board before does not
 * click fifty times across a rail; they keep one hand on the board and step
 * the other along the drawer. The legend stays on the screen rather than
 * hiding behind a key of its own, because a tool somebody sits at for an hour
 * should teach its own instrument without interrupting the board — and because
 * this column has the height to spend on a tall window, which is where the
 * space the muster cannot use goes.
 *
 * The rows carry the keys the screen owns and the two the board owns, since a
 * map maker does not care which side of the bridge a key is answered on.
 */
function KeyLegend() {
  return (
    <VStack gap={2}>
      <PanelHeading>The keys</PanelHeading>
      <VStack as="dl" gap={1.5} xstyle={styles.legend}>
        {EDITOR_SHORTCUTS.map((shortcut) => (
          <HStack align="start" gap={2} key={shortcut.does} xstyle={styles.legendRow}>
            <HStack as="dt" gap={1} wrap="wrap" xstyle={styles.legendKeys}>
              {shortcut.keys.map((keys) => (
                <Kbd key={keys} keys={keys} />
              ))}
            </HStack>
            <VStack as="dd" gap={0} xstyle={styles.legendDoes}>
              {shortcut.does}
            </VStack>
          </HStack>
        ))}
      </VStack>
    </VStack>
  );
}

const styles = stylex.create({
  // The drafting table is one screen, not a page that scrolls. The height it
  // is given is the window less the shell, the deck above it and the page
  // inset, so the rail, the board and the settings are all in reach of the
  // hand that is drawing. Each region keeps its own scrollbar for a window too
  // short to hold it, which is a fallback and not the arrangement.
  table: {
    blockSize: "calc(100svh - 15rem)",
    minBlockSize: "30rem",
    overflow: "hidden",
  },
  // The map's own name is the title of this screen and it is also a field, so
  // the heading takes the body face at title size rather than the signage face:
  // one line of lettering over a text input reads as a label for it.
  title: {
    fontFamily: typographyVars["--font-family-body"],
    fontSize: textSizeVars["--font-size-2xl"],
    lineHeight: 1.2,
  },
  nameField: {
    inlineSize: "13rem",
  },
  // The settings column reads down and never across. A control that asks for
  // more width than the column has is a defect in that control, and a
  // sideways scrollbar is the slowest possible way to hear about it: the
  // column clips on the inline axis so the fault shows where it is made,
  // while the block axis still scrolls on a window too short to hold it.
  settings: {
    overflowX: "clip",
  },
  board: {
    blockSize: "100%",
    minBlockSize: 0,
  },
  // The board takes the height the table has left after the strips under it,
  // and the engine is told the size it ends up with.
  surface: {
    position: "relative",
    flexGrow: 1,
    minBlockSize: 0,
    backgroundColor: colorVars["--color-background-inverted"],
  },
  canvas: {
    display: "block",
    blockSize: "100%",
    inlineSize: "100%",
    imageRendering: "pixelated",
    outline: "none",
  },
  // The strip under the board says what the pointer can do, in the HUD voice.
  // It is under the board rather than in a tooltip because a map maker needs
  // it on the first stroke and never again.
  hud: {
    alignItems: "center",
    borderBlockStartWidth: borderVars["--border-width"],
    borderBlockStartStyle: "solid",
    borderBlockStartColor: colorVars["--color-border-emphasized"],
    backgroundColor: colorVars["--color-background-surface"],
    color: colorVars["--color-text-secondary"],
    paddingBlock: spacingVars["--spacing-1-5"],
    paddingInline: spacingVars["--spacing-3"],
  },
  // The one heading left in the settings panel takes the HUD voice rather than
  // the signage one it would inherit from its level.
  panelHeading: {
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    fontWeight: 400,
    letterSpacing: "0.06em",
    textTransform: "uppercase",
    color: colorVars["--color-text-secondary"],
  },
  hudLine: {
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-xs"],
    letterSpacing: "0.04em",
    textTransform: "uppercase",
  },
  legend: {
    margin: 0,
  },
  // The keys hold their column so the sentences beside them start on one line
  // down the legend. A key that is one glyph and a key that is three still
  // leave the reading edge where the eye left it.
  legendRow: {
    alignItems: "baseline",
  },
  legendKeys: {
    flexShrink: 0,
    inlineSize: "5.5rem",
  },
  // The row says what the key does, so it is a sentence and takes the body
  // voice. The key beside it is the readout, and wears the one the system
  // gives a key.
  legendDoes: {
    color: colorVars["--color-text-secondary"],
    fontSize: textSizeVars["--font-size-xs"],
    lineHeight: 1.35,
    margin: 0,
    minInlineSize: 0,
  },
});
