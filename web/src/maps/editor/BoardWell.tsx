import { Icon } from "@astryxdesign/core/Icon";
import { Kbd } from "@astryxdesign/core/Kbd";
import { HStack, Stack } from "@astryxdesign/core/Stack";
import { colorVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useState, type Ref } from "react";
import { terrainTileStyle, unitSpriteStyle } from "#/components/game_sprites.ts";
import type { GameRunner } from "#/engine/game_runner.ts";
import { useGameStore } from "#/engine/store.ts";
import { resizeKept, tileList } from "#/maps/map_editor.ts";
import {
  ArmiesStepper,
  SizeKey,
  SymmetryKey,
  type ResizeDraft,
} from "#/maps/editor/SetupMenus.tsx";
import { DefenseStars } from "#/maps/editor/DefenseStars.tsx";
import { BoardKeys, TILE_PX } from "#/matches/components/BoardControls.tsx";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { figureStyles } from "#/ui/Figure.tsx";
import { IconEyedropper } from "#/ui/icons.tsx";
import type {
  Brush,
  EditorStateChanged,
  PaletteCell,
  ResizeAnchor,
  Symmetry,
} from "#/wasm/awbrn_wasm.js";

export function BoardWell({
  canvasContainerRef,
  fillCells,
  onFill,
  onMenuDone,
  onResize,
  onRoster,
  onSelect,
  onSymmetry,
  runner,
  state,
  surfaceRef,
}: {
  canvasContainerRef: Ref<HTMLDivElement>;
  fillCells: readonly PaletteCell[];
  onFill: (terrain: number) => void;
  /** A setup menu closed by a click. */
  onMenuDone: () => void;
  onResize: (width: number, height: number, anchor: ResizeAnchor) => void;
  onRoster: (roster: string[]) => void;
  onSelect: (brush: Brush) => void;
  onSymmetry: (symmetry: Symmetry) => void;
  runner: GameRunner;
  state: EditorStateChanged | null;
  surfaceRef: Ref<HTMLElement>;
}) {
  const [draft, setDraft] = useState<ResizeDraft | null>(null);

  return (
    <section aria-label="Board" {...stylex.props(styles.well)}>
      <HStack align="center" gap={2} xstyle={styles.bar}>
        {state ? (
          <>
            <SymmetryKey
              available={state.availableSymmetries}
              onDone={onMenuDone}
              onChange={onSymmetry}
              symmetry={state.symmetry}
              unevenGroups={state.unevenGroups.length}
            />
            <ArmiesStepper onChange={onRoster} roster={state.roster} />
            <SizeKey
              fillCells={fillCells}
              onDone={onMenuDone}
              height={state.height}
              loadedBrush={state.brush}
              onDraft={setDraft}
              onFill={onFill}
              onResize={onResize}
              onSelect={onSelect}
              symmetry={state.symmetry}
              units={state.units}
              width={state.width}
            />
          </>
        ) : null}
        <Stack xstyle={styles.spacer} />
        <BoardKeys runner={runner} />
      </HStack>
      <Stack xstyle={styles.table}>
        <Stack ref={surfaceRef} xstyle={styles.surface}>
          <Stack ref={canvasContainerRef} xstyle={styles.canvasHost} />
          {state ? <BoardFrame state={state} /> : null}
          {state && draft ? <ResizePreview draft={draft} state={state} /> : null}
        </Stack>
      </Stack>
      <TileLine state={state} />
    </section>
  );
}

function useTileRect() {
  const bv = useGameStore((store) => store.boardView);
  if (bv === null) return null;
  const tile = TILE_PX * bv.scale;
  return (x0: number, y0: number, x1: number, y1: number) => ({
    left: (x0 - bv.view.x) * tile,
    top: (y0 - bv.view.y) * tile,
    width: (x1 - x0) * tile,
    height: (y1 - y0) * tile,
  });
}

/** The Board edge, from the first tile row. */
function BoardFrame({ state }: { state: EditorStateChanged }) {
  const rect = useTileRect();
  if (rect === null) return null;
  return (
    <span
      aria-hidden="true"
      style={rect(0, 0, state.width, state.height)}
      {...stylex.props(styles.frame)}
    />
  );
}

/** What a resize drops, dimmed, and what it keeps, dashed. */
function ResizePreview({ draft, state }: { draft: ResizeDraft; state: EditorStateChanged }) {
  const rect = useTileRect();
  if (rect === null) return null;
  const kept = resizeKept(state, draft, draft.anchor);
  const { width, height } = state;
  const dropped = [
    rect(0, 0, width, kept.y0),
    rect(0, kept.y1, width, height),
    rect(0, kept.y0, kept.x0, kept.y1),
    rect(kept.x1, kept.y0, width, kept.y1),
  ].filter((part) => part.width > 0 && part.height > 0);

  return (
    <>
      {dropped.map((part) => (
        <span
          aria-hidden="true"
          key={`${part.left}:${part.top}`}
          style={part}
          {...stylex.props(styles.dropped)}
        />
      ))}
      <span
        aria-hidden="true"
        style={rect(kept.x0, kept.y0, kept.x1, kept.y1)}
        {...stylex.props(styles.kept)}
      />
    </>
  );
}

/** The hovered tile, and what it is uneven against. */
function TileLine({ state }: { state: EditorStateChanged | null }) {
  const hovered = useGameStore((store) => store.hoveredTile);
  const group =
    hovered === null
      ? undefined
      : state?.unevenGroups.find((tiles) =>
          tiles.some((t) => t.x === hovered.x && t.y === hovered.y),
        );
  const others = group?.filter((t) => t.x !== hovered?.x || t.y !== hovered?.y) ?? [];

  return (
    <HStack align="center" gap={2} xstyle={styles.line}>
      {hovered ? (
        <>
          <span
            aria-hidden="true"
            style={terrainTileStyle(hovered.terrainSpriteIndex)}
            {...stylex.props(styles.art)}
          />
          <span>{hovered.terrainName}</span>
          <DefenseStars count={hovered.defenseStars} />
          <span {...stylex.props(figureStyles.figure, styles.figure)}>
            {hovered.x}, {hovered.y}
          </span>
        </>
      ) : null}
      {hovered?.unit ? (
        <>
          <span aria-hidden="true" {...stylex.props(styles.rule)} />
          <span
            aria-hidden="true"
            style={unitSpriteStyle(hovered.unit.unit, hovered.unit.factionCode) ?? undefined}
          />
          <span>{hovered.unit.name}</span>
          <span {...stylex.props(figureStyles.figure, styles.figure)}>
            {hovered.unit.health ?? 10}
          </span>
        </>
      ) : null}
      {others.length > 0 ? (
        <>
          <span aria-hidden="true" {...stylex.props(styles.rule)} />
          <HStack as="span" align="center" gap={2} xstyle={styles.uneven}>
            Uneven
            <span {...stylex.props(figureStyles.figure, styles.figure)}>vs {tileList(others)}</span>
          </HStack>
        </>
      ) : null}
      <HStack as="span" align="center" gap={2} xstyle={styles.pick}>
        <Icon icon={IconEyedropper} size="sm" />
        Pick up
        <Kbd keys="alt" xstyle={styles.keycap} />
      </HStack>
    </HStack>
  );
}

const styles = stylex.create({
  well: {
    display: "grid",
    gridTemplateRows: "56px minmax(0, 1fr) 44px",
    minInlineSize: 0,
    minBlockSize: 0,
    overflow: "hidden",
    borderRadius: "10px",
    backgroundColor: awbrnVars.ink950,
  },
  bar: {
    paddingInline: "8px",
    borderBlockEndColor: awbrnVars.ink800,
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: "1px",
    backgroundColor: awbrnVars.ink900,
  },
  spacer: {
    flexGrow: 1,
  },
  table: {
    minBlockSize: 0,
    padding: "16px",
  },
  // Clipped two pixels out, so the Board edge shows on a fitted board.
  surface: {
    position: "relative",
    flexGrow: 1,
    minBlockSize: 0,
    clipPath: "inset(-2px)",
  },
  canvasHost: {
    inlineSize: "100%",
    blockSize: "100%",
  },
  frame: {
    position: "absolute",
    boxShadow: `0 0 0 2px ${awbrnVars.ink700}`,
    pointerEvents: "none",
  },
  dropped: {
    position: "absolute",
    backgroundColor: `color-mix(in srgb, ${awbrnVars.ink950} 84%, transparent)`,
    pointerEvents: "none",
  },
  kept: {
    position: "absolute",
    outline: "2px dashed #ffffff",
    outlineOffset: "-1px",
    pointerEvents: "none",
  },
  line: {
    minInlineSize: 0,
    paddingInline: "12px",
    overflow: "hidden",
    borderBlockStartColor: awbrnVars.ink800,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: "1px",
    backgroundColor: awbrnVars.ink900,
    color: awbrnVars.textOnInk,
    fontSize: awbrnVars.text14,
    fontWeight: 650,
    whiteSpace: "nowrap",
  },
  art: {
    flexShrink: 0,
  },
  figure: {
    color: awbrnVars.textOnInkSecondary,
  },
  rule: {
    inlineSize: "1px",
    blockSize: "22px",
    backgroundColor: awbrnVars.ink700,
  },
  uneven: {
    color: awbrnVars.warnFill,
  },
  keycap: {
    [colorVars["--color-neutral"]]: awbrnVars.ink850,
    [colorVars["--color-border-emphasized"]]: awbrnVars.ink600,
    [colorVars["--color-text-primary"]]: awbrnVars.textOnInkSecondary,
  },
  pick: {
    marginInlineStart: "auto",
    color: awbrnVars.textOnInkSecondary,
    fontWeight: 600,
  },
});
