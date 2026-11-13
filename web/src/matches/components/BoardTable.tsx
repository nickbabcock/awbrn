import { HStack, Stack } from "@astryxdesign/core/Stack";
import { radiusVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import type { ReactNode, RefObject } from "react";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { matchLayout, matchType } from "#/matches/components/matchLayout.stylex.ts";
import {
  OPENING_SCALE,
  TILE_PX,
  useBoardView,
  type TableLayout,
} from "#/matches/components/BoardControls.tsx";

/**
 * The ink table the board sits on, and the canvas box on it.
 *
 * The DOM sizes the box (`useTableLayout`): `min(table, map at 2x)` per axis.
 * The engine fills the box and centers the map in it. On an axis the map fits,
 * a 2px ink frame sits on the map edge from the first tile row; on a cropped
 * axis the box runs edge to edge and fades into the table while more map lies
 * that way.
 *
 * Desktop modes place the docked columns: `dock` puts `side` left of the board
 * and `locator` right of it; `gather` puts both in one column against the rail;
 * `none` shows no column (the page moves them to the rail). Every column
 * starts level with the board's first tile row. A phone fills the table.
 *
 * `overlay` holds what is drawn over the board (the whole-map outline, a
 * calculator panel); it never takes a press from a tile.
 *
 * `waiting` names what the table is waiting on (the board, the archive) until
 * the engine reports its first view of the board. A phone says it in the tile
 * row instead.
 *
 * `notice` is a match notice for when the table has no mode yet, so no column
 * or View block to hold it (no board has come: a signed-out viewer, a refused
 * connection). It takes the board's place in the middle of the table, rather
 * than heading the rail. A phone says it in the tile row instead.
 */
export function BoardTable({
  canvasContainerRef,
  floating,
  layout,
  locator,
  map,
  notice = null,
  overlay,
  side,
  tableRef,
  waiting = null,
}: {
  canvasContainerRef: RefObject<HTMLDivElement | null>;
  /** A panel the player opened over the whole table (the calculator). */
  floating?: ReactNode;
  layout: TableLayout;
  locator?: ReactNode;
  map: { width: number; height: number } | null;
  notice?: ReactNode;
  overlay?: ReactNode;
  side?: ReactNode;
  tableRef: RefObject<HTMLDivElement | null>;
  waiting?: string | null;
}) {
  const { box, mode } = layout;
  const { bv } = useBoardView();
  const hasColumns = mode === "dock" || mode === "gather";

  return (
    <Stack
      ref={tableRef}
      xstyle={[
        styles.table,
        mode === "dock" && styles.dock,
        mode === "gather" && box && styles.gather(box.width, layout.columnWidth),
        mode === "dock" && box && styles.dockColumns(box.width),
      ]}
    >
      {hasColumns ? (
        <aside
          aria-label="View"
          {...stylex.props(
            styles.column,
            mode === "dock" ? styles.columnLeft : styles.columnGather,
            styles.columnTop(layout.columnTop),
          )}
        >
          {side}
          {mode === "gather" ? locator : null}
        </aside>
      ) : null}
      <Stack
        xstyle={[
          styles.boxWrap,
          box ? styles.boxSize(box.width, box.height) : styles.boxFill,
          mode === "gather" && styles.boxGather,
          mode === "dock" && styles.boxDock,
        ]}
      >
        <Stack xstyle={styles.box}>
          <Stack ref={canvasContainerRef} xstyle={styles.canvasHost} />
          <BoardFades box={box} map={map} />
          {overlay}
        </Stack>
        <BoardRing box={box} map={map} />
      </Stack>
      {mode === "dock" ? (
        <aside
          aria-label="Locator"
          {...stylex.props(styles.column, styles.columnRight, styles.columnTop(layout.columnTop))}
        >
          {locator}
        </aside>
      ) : null}
      {mode === "pending" && notice !== null ? (
        <Stack xstyle={styles.notice}>{notice}</Stack>
      ) : waiting !== null && bv === null ? (
        <HStack as="span" role="status" xstyle={styles.waiting}>
          <Stack as="span" aria-hidden="true" xstyle={styles.waitingDot} />
          {waiting}
        </HStack>
      ) : null}
      {floating}
    </Stack>
  );
}

/**
 * What the table is waiting on, as the phone's tile row says it: nothing is
 * drawn over a tile the player may tap. Shown until the engine reports its
 * first view of the board.
 */
export function WaitingLine({ label }: { label: string }) {
  const { bv } = useBoardView();
  if (bv !== null) return null;
  return (
    <HStack as="span" role="status" xstyle={styles.waitingLine}>
      <Stack as="span" aria-hidden="true" xstyle={styles.waitingDot} />
      {label}
    </HStack>
  );
}

/**
 * Where the map is drawn in the box, in box pixels, and the box itself.
 *
 * A phone's box fills the table and is never sized by the page (`box` is
 * null), so its size is read from the engine's report of the view instead.
 */
function useMapRect(
  measured: { width: number; height: number } | null,
  map: { width: number; height: number } | null,
) {
  const { bv } = useBoardView();
  if (bv !== null) {
    const tile = TILE_PX * bv.scale;
    return {
      box: measured ?? { width: bv.view.width * tile, height: bv.view.height * tile },
      left: -bv.view.x * tile,
      top: -bv.view.y * tile,
      width: bv.map.width * tile,
      height: bv.map.height * tile,
      view: bv.view,
      map: bv.map,
    };
  }
  if (measured === null || map === null) return null;
  const box = measured;
  // Before the engine reports, the map is where it opens: 2x, centered with
  // its overhang row.
  const tile = TILE_PX * OPENING_SCALE;
  const width = map.width * tile;
  const height = map.height * tile;
  const left = (box.width - width) / 2;
  const top = (box.height - height - tile) / 2 + tile;
  return {
    box,
    left,
    top,
    width,
    height,
    view: { x: -left / tile, y: -top / tile, width: box.width / tile, height: box.height / tile },
    map,
  };
}

function BoardRing({
  box: measured,
  map,
}: {
  box: { width: number; height: number } | null;
  map: { width: number; height: number } | null;
}) {
  const rect = useMapRect(measured, map);
  if (rect === null) return null;
  const { box } = rect;
  const fitsX = rect.left >= -0.5 && rect.left + rect.width <= box.width + 0.5;
  const fitsY = rect.top >= -0.5 && rect.top + rect.height <= box.height + 0.5;
  if (!fitsX && !fitsY) return null;

  return (
    <Stack
      as="span"
      aria-hidden="true"
      xstyle={[
        styles.ring,
        fitsX && fitsY ? styles.ringWhole : fitsX ? styles.ringSides : styles.ringEnds,
        styles.ringAt(
          fitsX ? rect.left : 0,
          fitsY ? rect.top : 0,
          fitsX ? rect.width : box.width,
          fitsY ? rect.height : box.height,
        ),
      ]}
    />
  );
}

function BoardFades({
  box,
  map,
}: {
  box: { width: number; height: number } | null;
  map: { width: number; height: number } | null;
}) {
  const rect = useMapRect(box, map);
  if (rect === null) return null;
  const { view } = rect;
  const more = {
    left: view.x > 0.05,
    right: view.x + view.width < rect.map.width - 0.05,
    top: view.y > 0.05,
    bottom: view.y + view.height < rect.map.height - 0.05,
  };

  return (
    <>
      {more.left ? <i aria-hidden="true" {...stylex.props(styles.fade, styles.fadeLeft)} /> : null}
      {more.right ? (
        <i aria-hidden="true" {...stylex.props(styles.fade, styles.fadeRight)} />
      ) : null}
      {more.top ? <i aria-hidden="true" {...stylex.props(styles.fade, styles.fadeTop)} /> : null}
      {more.bottom ? (
        <i aria-hidden="true" {...stylex.props(styles.fade, styles.fadeBottom)} />
      ) : null}
    </>
  );
}

const FADE = matchLayout.boardFade;

const waitingPulse = stylex.keyframes({
  "0%": { opacity: 1 },
  "50%": { opacity: 0.35 },
  "100%": { opacity: 1 },
});

const styles = stylex.create({
  table: {
    position: "relative",
    display: "grid",
    gridTemplateColumns: "minmax(0, 1fr)",
    gridTemplateRows: "minmax(0, 1fr)",
    alignItems: "center",
    justifyItems: "center",
    minBlockSize: 0,
    minInlineSize: 0,
    overflow: "hidden",
    backgroundColor: awbrnVars.ink950,
    backgroundImage: {
      default: `radial-gradient(ellipse at 50% 45%, ${matchLayout.tableGlow}, transparent 70%)`,
      [pageLayout.phoneMedia]: "none",
    },
  },
  dock: {
    columnGap: spacingVars["--spacing-4"],
  },
  dockColumns: (boxWidth: number) => ({
    gridTemplateColumns: `minmax(0, 1fr) ${boxWidth}px minmax(0, 1fr)`,
  }),
  gather: (boxWidth: number, columnWidth: number) => ({
    gridTemplateColumns: `12px ${boxWidth}px minmax(0, 1fr) ${columnWidth}px 12px`,
    gridTemplateAreas: '". box . side ."',
  }),
  column: {
    alignSelf: "start",
    display: "flex",
    flexDirection: "column",
    gap: spacingVars["--spacing-4"],
    minInlineSize: 0,
    maxBlockSize: "100%",
    overflow: "hidden",
    zIndex: 3,
  },
  columnTop: (top: number) => ({
    paddingBlockStart: `${top}px`,
    paddingBlockEnd: spacingVars["--spacing-3"],
  }),
  columnLeft: {
    justifySelf: "stretch",
    alignItems: "flex-end",
    paddingInlineStart: spacingVars["--spacing-3"],
  },
  columnRight: {
    justifySelf: "stretch",
    alignItems: "flex-start",
    paddingInlineEnd: spacingVars["--spacing-3"],
  },
  columnGather: {
    gridArea: "side",
    justifySelf: "stretch",
    alignItems: "stretch",
  },
  boxWrap: {
    position: "relative",
    flexShrink: 0,
  },
  boxSize: (width: number, height: number) => ({
    inlineSize: `${width}px`,
    blockSize: `${height}px`,
  }),
  boxFill: {
    position: "absolute",
    inset: 0,
  },
  boxGather: {
    gridArea: "box",
  },
  boxDock: {
    gridColumn: "2",
  },
  box: {
    position: "absolute",
    inset: 0,
    overflow: "hidden",
    backgroundColor: awbrnVars.ink950,
  },
  canvasHost: {
    position: "absolute",
    inset: 0,
  },
  ring: {
    position: "absolute",
    pointerEvents: "none",
    zIndex: 1,
  },
  ringWhole: {
    boxShadow: awbrnVars.shadowBoard,
  },
  ringSides: {
    borderInlineColor: awbrnVars.ink700,
    borderInlineStyle: "solid",
    borderInlineWidth: spacingVars["--spacing-0-5"],
    marginInline: `calc(-1 * ${spacingVars["--spacing-0-5"]})`,
  },
  ringEnds: {
    borderBlockColor: awbrnVars.ink700,
    borderBlockStyle: "solid",
    borderBlockWidth: spacingVars["--spacing-0-5"],
    marginBlock: `calc(-1 * ${spacingVars["--spacing-0-5"]})`,
  },
  ringAt: (left: number, top: number, width: number, height: number) => ({
    insetInlineStart: `${left}px`,
    insetBlockStart: `${top}px`,
    inlineSize: `${width}px`,
    blockSize: `${height}px`,
  }),
  // A phone says it in the tile row.
  notice: {
    position: "absolute",
    inset: 0,
    zIndex: 3,
    display: {
      default: "grid",
      [pageLayout.phoneMedia]: "none",
    },
    alignContent: "center",
    justifyContent: "center",
    gridTemplateColumns: `min(100%, ${matchLayout.railWidth})`,
    padding: spacingVars["--spacing-3"],
  },
  // A phone says it in the tile row: nothing is drawn over a tile.
  waiting: {
    position: "absolute",
    inset: 0,
    zIndex: 2,
    display: {
      default: "flex",
      [pageLayout.phoneMedia]: "none",
    },
    alignItems: "center",
    justifyContent: "center",
    gap: awbrnVars.spacing2_5,
    color: awbrnVars.textOnInkSecondary,
    fontSize: matchType.small,
    fontWeight: 600,
    pointerEvents: "none",
  },
  waitingLine: {
    display: "flex",
    alignItems: "center",
    gap: awbrnVars.spacing2_5,
    flexGrow: 1,
    minInlineSize: 0,
    blockSize: matchLayout.target,
    paddingInline: spacingVars["--spacing-3"],
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: awbrnVars.ink850,
    color: awbrnVars.textOnInkSecondary,
    fontSize: matchType.small,
    fontWeight: 600,
  },
  waitingDot: {
    inlineSize: matchLayout.dot,
    blockSize: matchLayout.dot,
    borderRadius: "50%",
    backgroundColor: awbrnVars.ink300,
    animationName: {
      default: waitingPulse,
      [pageLayout.reducedMotionMedia]: "none",
    },
    animationDuration: "1.2s",
    animationIterationCount: "infinite",
    animationTimingFunction: awbrnVars.easeStep,
  },
  fade: {
    position: "absolute",
    zIndex: 1,
    pointerEvents: "none",
  },
  fadeLeft: {
    insetBlock: 0,
    insetInlineStart: 0,
    inlineSize: matchLayout.fadeDepth,
    backgroundImage: `linear-gradient(90deg, ${FADE}, transparent)`,
  },
  fadeRight: {
    insetBlock: 0,
    insetInlineEnd: 0,
    inlineSize: matchLayout.fadeDepth,
    backgroundImage: `linear-gradient(-90deg, ${FADE}, transparent)`,
  },
  fadeTop: {
    insetInline: 0,
    insetBlockStart: 0,
    blockSize: matchLayout.fadeDepth,
    backgroundImage: `linear-gradient(180deg, ${FADE}, transparent)`,
  },
  fadeBottom: {
    insetInline: 0,
    insetBlockEnd: 0,
    blockSize: matchLayout.fadeDepth,
    backgroundImage: `linear-gradient(0deg, ${FADE}, transparent)`,
  },
});
