import { Text } from "@astryxdesign/core/Text";
import { HStack, Stack } from "@astryxdesign/core/Stack";
import { Icon } from "@astryxdesign/core/Icon";
import { VisuallyHidden } from "@astryxdesign/core/VisuallyHidden";
import { borderVars, radiusVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
  type RefObject,
} from "react";
import { create } from "zustand";
import type { GameRunner } from "#/engine/game_runner.ts";
import { useGameStore } from "#/engine/store.ts";
import { matchLayout, matchType } from "#/matches/components/matchLayout.stylex.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { figureStyles } from "#/ui/Figure.tsx";
import { IconBack, IconMinus, IconPlus, IconWholeMap } from "#/ui/icons.tsx";
import type { BoardView, TileRect } from "#/wasm/awbrn_wasm.js";
import {
  followBoardView,
  openWholeMap,
  SCALE_EPSILON,
  WHOLE_MAP_CLOSED,
  type WholeMapState,
} from "#/matches/components/whole_map.ts";

/** A tile is 16 map pixels; the camera opens at 2x, so 32 CSS px. */
export const TILE_PX = 16;
export const OPENING_SCALE = 2;
/** Each zoom key steps by the same factor the wheel and pinch use. */
const ZOOM_STEP = 1.25;

/** How much of the board the camera shows, read from the engine's `BoardView`. */
export interface BoardViewState {
  bv: BoardView | null;
  canZoomIn: boolean;
  canZoomOut: boolean;
  /** True while every tile of the map is inside the canvas. */
  mapFits: boolean;
}

export function readBoardView(bv: BoardView | null): BoardViewState {
  if (bv === null) {
    return { bv, canZoomIn: true, canZoomOut: true, mapFits: false };
  }
  return {
    bv,
    canZoomIn: bv.scale < bv.maxScale - SCALE_EPSILON,
    canZoomOut: bv.scale > bv.fitScale + SCALE_EPSILON,
    mapFits: viewHoldsMap(bv),
  };
}

export function useBoardView(): BoardViewState {
  const bv = useGameStore((state) => state.boardView);
  return readBoardView(bv);
}

function viewHoldsMap(bv: BoardView): boolean {
  const { map, view } = bv;
  // The overhang row above the first tile row is not map; half a tile of slack
  // keeps a snapped camera from reading as cropped.
  return (
    view.x <= 0.5 &&
    view.y <= 0.5 &&
    view.x + view.width >= map.width - 0.5 &&
    view.y + view.height >= map.height - 0.5
  );
}

/**
 * The whole-map view a player opened, and the view they left to open it. It is
 * shared by the desktop keys and the phone view key, so the two never disagree
 * about which way the toggle goes.
 */
interface WholeMapStore extends WholeMapState {
  set: (next: WholeMapState) => void;
  follow: (bv: BoardView | null) => void;
}

const useWholeMapStore = create<WholeMapStore>((set) => ({
  ...WHOLE_MAP_CLOSED,
  set: (next) => set(next),
  follow: (bv) =>
    set((state) => {
      const current = { left: state.left, awaitingFit: state.awaitingFit };
      const next = followBoardView(current, bv);
      return next === current ? state : next;
    }),
}));

/**
 * Whole map, and back. `fitBoard` saves the view the engine had, `restoreView`
 * goes back to it; a zoom by any other means leaves the whole map without a
 * way back, so the saved view is dropped then (see `followBoardView`).
 */
export function useWholeMap(runner: GameRunner) {
  const left = useWholeMapStore((state) => state.left);
  const setWhole = useWholeMapStore((state) => state.set);
  const follow = useWholeMapStore((state) => state.follow);
  const view = useBoardView();
  const bv = view.bv;

  useEffect(() => {
    follow(bv);
  }, [bv, follow]);

  const isWhole = left !== null;
  const open = useCallback(() => {
    if (bv === null) return;
    setWhole(openWholeMap(bv.view));
    void runner.fitBoard().catch((error: unknown) => {
      console.error("Error showing the whole map:", error);
      setWhole(WHOLE_MAP_CLOSED);
    });
  }, [bv, runner, setWhole]);
  const back = useCallback(() => {
    setWhole(WHOLE_MAP_CLOSED);
    void runner.restoreView().catch((error: unknown) => {
      console.error("Error returning from the whole map:", error);
    });
  }, [runner, setWhole]);

  return {
    back,
    /** The view outline to draw over the whole map. */
    left,
    isWhole,
    /** Nothing to open while the whole map is already in view. */
    isAvailable: isWhole || (bv !== null && !view.mapFits),
    open,
    toggle: isWhole ? back : open,
  };
}

/** `2×`, `2.5×`, `0.84×`: the scale as the player reads it. */
export function formatScale(scale: number): string {
  const rounded = Math.round(scale * 100) / 100;
  return `${Number.isInteger(rounded) ? rounded : rounded.toFixed(2).replace(/0$/, "")}×`;
}

/**
 * The board keys: zoom out, the scale, zoom in, then the whole map. Zoom steps
 * 1.25x about the middle of the canvas and stops at its limit; whole map goes
 * to the fit scale and back, and rests when the map already fits.
 */
export function BoardKeys({
  isFloating = false,
  runner,
}: {
  /** Over the board (full screen, no column to dock into). */
  isFloating?: boolean;
  runner: GameRunner;
}) {
  const view = useBoardView();
  const whole = useWholeMap(runner);
  const scale = view.bv?.scale ?? OPENING_SCALE;
  const zoom = (factor: number) => {
    void runner.zoomBy(factor).catch((error: unknown) => {
      console.error("Error zooming the board:", error);
    });
  };

  return (
    <HStack as="span" aria-label="Board view" role="group" xstyle={styles.keys}>
      <HStack as="span" xstyle={[styles.zoomGroup, isFloating && styles.floating]}>
        <Button
          icon={<Icon icon={IconMinus} />}
          isDisabled={!view.canZoomOut}
          isIconOnly
          label="Zoom out"
          onClick={() => zoom(1 / ZOOM_STEP)}
          tooltip="Zoom out (-)"
          variant="ghost-on-ink"
          xstyle={styles.key}
        />
        <HStack as="span" aria-hidden="true" xstyle={[figureStyles.figure, styles.readout]}>
          {formatScale(scale)}
        </HStack>
        <SettledScale scale={scale} />
        <Button
          icon={<Icon icon={IconPlus} />}
          isDisabled={!view.canZoomIn}
          isIconOnly
          label="Zoom in"
          onClick={() => zoom(ZOOM_STEP)}
          tooltip="Zoom in (=)"
          variant="ghost-on-ink"
          xstyle={styles.key}
        />
      </HStack>
      <Button
        aria-pressed={whole.isWhole}
        icon={<Icon icon={IconWholeMap} />}
        isDisabled={!whole.isAvailable}
        isIconOnly
        label={whole.isWhole ? "Back to the view you left" : "Whole map"}
        onClick={whole.toggle}
        tooltip={
          whole.isAvailable
            ? whole.isWhole
              ? "Back"
              : "Whole map"
            : "The whole map is on the table"
        }
        variant="ghost-on-ink"
        xstyle={[styles.wholeKey, isFloating && styles.floating, whole.isWhole && styles.wholeOn]}
      />
    </HStack>
  );
}

/** How long the scale holds still before it is announced. */
const SCALE_SETTLE_MS = 500;

/**
 * The scale for assistive tech, announced once a zoom has settled. The
 * readout itself redraws every frame of a pinch; a live region on it would
 * read out every frame.
 */
function SettledScale({ scale }: { scale: number }) {
  const [settled, setSettled] = useState(scale);
  useEffect(() => {
    const timer = window.setTimeout(() => setSettled(scale), SCALE_SETTLE_MS);
    return () => window.clearTimeout(timer);
  }, [scale]);
  return (
    <VisuallyHidden>
      <Text color="inherit" type="inherit" role="status">{`Scale ${formatScale(settled)}`}</Text>
    </VisuallyHidden>
  );
}

/**
 * The locator: the map's small picture (4px a tile) with the camera's view
 * drawn on it. Press or drag to move the view there; with the keyboard, the
 * arrow keys move the view a quarter of itself (Shift: a whole view). It is
 * shown only while the map is cropped.
 */
export function Locator({
  mapName,
  runner,
  src,
}: {
  mapName: string;
  runner: GameRunner;
  src: string;
}) {
  const view = useBoardView();
  const pictureRef = useRef<HTMLButtonElement>(null);
  const dragRef = useRef(false);
  const bv = view.bv;
  if (bv === null || view.mapFits) return null;

  const { map } = bv;
  const rect = clampRect(bv.view, map.width, map.height);
  const centerOn = (x: number, y: number, ease: boolean) => {
    const tile = {
      x: Math.min(Math.max(Math.floor(x), 0), map.width - 1),
      y: Math.min(Math.max(Math.floor(y), 0), map.height - 1),
    };
    void runner.centerOn(tile, ease).catch((error: unknown) => {
      console.error("Error moving the board view:", error);
    });
  };
  const move = (event: PointerEvent<HTMLButtonElement>, ease: boolean) => {
    const bounds = pictureRef.current?.getBoundingClientRect();
    if (!bounds || bounds.width === 0) return;
    centerOn(
      ((event.clientX - bounds.left) / bounds.width) * map.width,
      ((event.clientY - bounds.top) / bounds.height) * map.height,
      ease,
    );
  };
  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    const step = LOCATOR_KEY_STEPS[event.key];
    if (step === undefined) return;
    event.preventDefault();
    const share = event.shiftKey ? 1 : 0.25;
    const dx = Math.max(Math.round(bv.view.width * share), 1) * step.x;
    const dy = Math.max(Math.round(bv.view.height * share), 1) * step.y;
    centerOn(rect.x + rect.width / 2 + dx, rect.y + rect.height / 2 + dy, true);
  };
  const first = { x: Math.floor(rect.x) + 1, y: Math.floor(rect.y) + 1 };
  const last = {
    x: Math.min(Math.ceil(rect.x + rect.width), map.width),
    y: Math.min(Math.ceil(rect.y + rect.height), map.height),
  };

  return (
    <figure {...stylex.props(styles.locator)}>
      <button
        aria-description={`Showing columns ${first.x} to ${last.x}, rows ${first.y} to ${last.y}. Arrow keys move the view.`}
        aria-keyshortcuts="ArrowUp ArrowDown ArrowLeft ArrowRight"
        aria-label={`${mapName} locator`}
        onKeyDown={onKeyDown}
        onPointerDown={(event) => {
          event.currentTarget.setPointerCapture(event.pointerId);
          dragRef.current = true;
          move(event, true);
        }}
        onPointerMove={(event) => {
          if (dragRef.current) move(event, false);
        }}
        onPointerUp={() => {
          dragRef.current = false;
        }}
        onPointerCancel={() => {
          dragRef.current = false;
        }}
        ref={pictureRef}
        title="Move the view"
        type="button"
        {...stylex.props(styles.locatorMap, styles.locatorSize(map.width * 4, map.height * 4))}
      >
        <img alt="" draggable={false} src={src} {...stylex.props(styles.locatorImage)} />
        <Stack
          as="span"
          aria-hidden="true"
          xstyle={[
            styles.locatorView,
            styles.locatorRect(
              (rect.x / map.width) * 100,
              (rect.y / map.height) * 100,
              (rect.width / map.width) * 100,
              (rect.height / map.height) * 100,
            ),
          ]}
        />
      </button>
    </figure>
  );
}

/** Which way each arrow key moves the locator's view. */
const LOCATOR_KEY_STEPS: Record<string, { x: number; y: number }> = {
  ArrowUp: { x: 0, y: -1 },
  ArrowDown: { x: 0, y: 1 },
  ArrowLeft: { x: -1, y: 0 },
  ArrowRight: { x: 1, y: 0 },
};

function clampRect(rect: TileRect, width: number, height: number): TileRect {
  const x = Math.max(rect.x, 0);
  const y = Math.max(rect.y, 0);
  return {
    x,
    y,
    width: Math.max(Math.min(rect.x + rect.width, width) - x, 0),
    height: Math.max(Math.min(rect.y + rect.height, height) - y, 0),
  };
}

/**
 * The view the player left, outlined over the whole map (2px white, ink edge),
 * so they can see where Back returns them.
 */
export function WholeMapOutline({ runner }: { runner: GameRunner }) {
  const whole = useWholeMap(runner);
  const bv = useBoardView().bv;
  if (!whole.isWhole || whole.left === null || bv === null) return null;
  const tile = TILE_PX * bv.scale;
  const left = (whole.left.x - bv.view.x) * tile;
  const top = (whole.left.y - bv.view.y) * tile;

  return (
    <Stack
      as="span"
      aria-hidden="true"
      xstyle={[
        styles.outline,
        styles.outlineAt(left, top, whole.left.width * tile, whole.left.height * tile),
      ]}
    />
  );
}

/**
 * The phone's one board key, at the end of the tile row: Whole map, then Back.
 */
export function ViewKey({ runner }: { runner: GameRunner }) {
  const whole = useWholeMap(runner);

  return (
    <Button
      aria-pressed={whole.isWhole}
      isDisabled={!whole.isAvailable}
      label={whole.isWhole ? "Back to the view you left" : "Whole map"}
      onClick={whole.toggle}
      variant="on-ink"
      xstyle={[styles.viewKey, whole.isWhole && styles.viewKeyOn]}
    >
      <Stack as="span" xstyle={styles.viewKeyBody}>
        <Icon icon={whole.isWhole ? IconBack : IconWholeMap} size="md" />
        <Text color="inherit" type="inherit" xstyle={styles.viewKeyLabel}>
          {whole.isWhole ? "Back" : "Whole map"}
        </Text>
      </Stack>
    </Button>
  );
}

const FIRST_TOUCH_KEY = "awbrn.board.firstTouchSeen";

/**
 * Once per device, a phone's tile row says how the board moves: "Drag to pan ·
 * pinch to zoom" with OK. Until then it holds `children` back.
 */
export function FirstTouchHint({ children }: { children: ReactNode }) {
  const [isShowing, setIsShowing] = useState(false);
  useEffect(() => {
    try {
      setIsShowing(window.localStorage.getItem(FIRST_TOUCH_KEY) === null);
    } catch {
      setIsShowing(false);
    }
  }, []);
  if (!isShowing) return <>{children}</>;
  return (
    <HStack as="span" role="status" xstyle={styles.hint}>
      <HStack as="span" xstyle={styles.hintText}>
        Drag to pan · pinch to zoom
      </HStack>
      <Button
        label="OK"
        onClick={() => {
          setIsShowing(false);
          try {
            window.localStorage.setItem(FIRST_TOUCH_KEY, "1");
          } catch {
            // A browser that keeps nothing shows the hint again next time.
          }
        }}
        variant="on-ink"
        xstyle={styles.hintKey}
      />
    </HStack>
  );
}

/** The canvas box size and the table's mode, measured from the table. */
export type TableMode = "pending" | "dock" | "gather" | "none" | "phone";

export interface TableLayout {
  mode: TableMode;
  /** The canvas box, or null to fill the table. */
  box: { width: number; height: number } | null;
  /** The column width in gather mode. */
  columnWidth: number;
  /** Where the docked columns start, level with the board's first tile row. */
  columnTop: number;
}

/** A margin narrower than this holds no column. */
const COLUMN_MIN = 212;
const COLUMN_MAX = 240;
const TABLE_INSET = 12;

/**
 * The canvas box is `min(table, map at 2x)` per axis, measured at load and on
 * resize, never on zoom. The spare width picks the table's mode: a column on
 * each side (dock), one column against the rail (gather), or none.
 */
export function useTableLayout(
  tableRef: RefObject<HTMLElement | null>,
  map: { width: number; height: number } | null,
  isPhone: boolean,
): TableLayout {
  const [size, setSize] = useState<{ width: number; height: number } | null>(null);

  useEffect(() => {
    const table = tableRef.current;
    if (!table) return;
    const observer = new ResizeObserver(([entry]) => {
      if (!entry) return;
      const { width, height } = entry.contentRect;
      setSize((previous) =>
        previous && previous.width === width && previous.height === height
          ? previous
          : { width, height },
      );
    });
    observer.observe(table);
    return () => observer.disconnect();
  }, [tableRef]);

  if (isPhone) return { mode: "phone", box: null, columnWidth: 0, columnTop: TABLE_INSET };
  if (size === null || map === null) {
    return { mode: "pending", box: null, columnWidth: 0, columnTop: TABLE_INSET };
  }

  const tile = TILE_PX * OPENING_SCALE;
  const mapWidth = map.width * tile;
  // The overhang row above the first tile row is drawn, so it is boxed.
  const mapHeight = (map.height + 1) * tile;
  const width = Math.floor(Math.min(size.width, mapWidth));
  const height = Math.floor(Math.min(size.height, mapHeight));
  const spare = size.width - width;
  const fitsHeight = mapHeight <= size.height;
  const columnTop = fitsHeight ? Math.round((size.height - height) / 2 + tile) : TABLE_INSET;
  const mode: TableMode =
    spare / 2 >= COLUMN_MIN ? "dock" : spare >= COLUMN_MIN ? "gather" : "none";

  return {
    mode,
    box: { width, height },
    columnWidth: Math.min(spare - 40, COLUMN_MAX),
    columnTop,
  };
}

const styles = stylex.create({
  keys: {
    display: "flex",
    alignItems: "center",
    flexShrink: 0,
    gap: spacingVars["--spacing-1-5"],
  },
  zoomGroup: {
    display: "inline-flex",
    alignItems: "center",
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: awbrnVars.ink900,
    boxShadow: `inset 0 0 0 1px ${awbrnVars.ink700}`,
  },
  floating: {
    backgroundColor: matchLayout.inkFloat,
  },
  key: {
    inlineSize: matchLayout.target,
    blockSize: matchLayout.target,
    minInlineSize: matchLayout.target,
    paddingInline: 0,
    color: awbrnVars.textOnInkStrong,
  },
  readout: {
    display: "inline-grid",
    placeItems: "center",
    inlineSize: matchLayout.scaleReadout,
    blockSize: matchLayout.target,
    borderInlineColor: awbrnVars.ink700,
    borderInlineStyle: "solid",
    borderInlineWidth: borderVars["--border-width"],
    color: awbrnVars.textOnInkStrong,
    fontSize: matchType.figure,
    fontWeight: 600,
    letterSpacing: "-0.02em",
  },
  wholeKey: {
    inlineSize: matchLayout.target,
    blockSize: matchLayout.target,
    minInlineSize: matchLayout.target,
    paddingInline: 0,
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: awbrnVars.ink900,
    boxShadow: `inset 0 0 0 1px ${awbrnVars.ink700}`,
    color: awbrnVars.textOnInkStrong,
  },
  wholeOn: {
    backgroundColor: awbrnVars.ink100,
    color: awbrnVars.ink950,
  },
  // The well is the minimap and its padding, never the column's width.
  locator: {
    display: "grid",
    flexShrink: 0,
    justifyItems: "start",
    justifySelf: "start",
    alignSelf: "start",
    inlineSize: "fit-content",
    margin: 0,
    padding: spacingVars["--spacing-1-5"],
    maxInlineSize: "100%",
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: awbrnVars.ink900,
    boxShadow: `inset 0 0 0 1px ${awbrnVars.ink700}`,
  },
  locatorMap: {
    position: "relative",
    display: "block",
    maxInlineSize: "100%",
    margin: 0,
    padding: 0,
    overflow: "hidden",
    borderStyle: "none",
    borderRadius: `calc(${radiusVars["--radius-inner"]} / 2)`,
    backgroundColor: "transparent",
    cursor: "pointer",
    touchAction: "none",
    outlineColor: awbrnVars.blueOnInk,
    outlineOffset: spacingVars["--spacing-0-5"],
    outlineStyle: {
      default: "none",
      ":focus-visible": "solid",
    },
    outlineWidth: spacingVars["--spacing-0-5"],
  },
  locatorSize: (width: number, height: number) => ({
    inlineSize: `${width}px`,
    aspectRatio: `${width} / ${height}`,
  }),
  locatorImage: {
    display: "block",
    inlineSize: "100%",
    blockSize: "100%",
    imageRendering: "pixelated",
    userSelect: "none",
  },
  locatorView: {
    position: "absolute",
    borderRadius: borderVars["--border-width"],
    boxShadow: `0 0 0 calc(1.5 * ${borderVars["--border-width"]}) ${awbrnVars.textOnInkStrong}, 0 0 0 100vmax ${matchLayout.locatorDim}`,
    pointerEvents: "none",
  },
  locatorRect: (x: number, y: number, width: number, height: number) => ({
    insetInlineStart: `${x}%`,
    insetBlockStart: `${y}%`,
    inlineSize: `${width}%`,
    blockSize: `${height}%`,
  }),
  outline: {
    position: "absolute",
    zIndex: 2,
    borderRadius: borderVars["--border-width"],
    boxShadow: `0 0 0 ${borderVars["--border-width"]} ${awbrnVars.ink950}, inset 0 0 0 ${spacingVars["--spacing-0-5"]} ${awbrnVars.textOnInkStrong}, inset 0 0 0 calc(3 * ${borderVars["--border-width"]}) ${awbrnVars.ink950}`,
    pointerEvents: "none",
  },
  outlineAt: (left: number, top: number, width: number, height: number) => ({
    insetInlineStart: `${left}px`,
    insetBlockStart: `${top}px`,
    inlineSize: `${width}px`,
    blockSize: `${height}px`,
  }),
  hint: {
    display: "flex",
    alignItems: "center",
    gap: spacingVars["--spacing-2"],
    flexGrow: 1,
    minInlineSize: 0,
  },
  hintText: {
    display: "flex",
    alignItems: "center",
    flexGrow: 1,
    blockSize: matchLayout.target,
    paddingInline: spacingVars["--spacing-3"],
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: awbrnVars.ink850,
    color: awbrnVars.textOnInk,
    fontSize: matchType.small,
  },
  hintKey: {
    blockSize: matchLayout.target,
    minInlineSize: matchLayout.dockKeyWidth,
  },
  viewKey: {
    flexShrink: 0,
    marginInlineStart: "auto",
    inlineSize: matchLayout.rowHeight,
    minInlineSize: matchLayout.dockKeyWidth,
    blockSize: matchLayout.target,
    paddingInline: 0,
  },
  viewKeyOn: {
    backgroundColor: awbrnVars.sheet,
    color: awbrnVars.ink950,
  },
  viewKeyBody: {
    display: "grid",
    justifyItems: "center",
    gap: spacingVars["--spacing-0-5"],
  },
  viewKeyLabel: {
    fontSize: matchType.micro,
    fontWeight: 650,
    lineHeight: 1,
    whiteSpace: "nowrap",
  },
});
