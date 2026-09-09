/**
 * What the brush is loaded with.
 *
 * The rail is the game's own art, not a list of names. An Advance Wars player
 * knows the mountain cell and the base roof before they read anything, and a
 * map is drawn from fifty of these: a dropdown would make every stroke start
 * with a sentence. The name stays under the tile, because the difference
 * between an airport and a port is one that art alone loses at this size.
 *
 * Fifty keys with a heading over every group is a rail longer than the screen,
 * and a map maker who has to scroll to reach the sea has lost the board while
 * they do it. So the rail has three drawers — the land, the buildings, the
 * units — and opens the one that is being drawn from. Inside a drawer the order
 * still runs ground, water, ways, which is the grouping the headings used to
 * carry.
 *
 * A drawer is shut to buy height, so a rail with height to spare shuts nothing.
 * The rail measures itself and opens every other drawer that fits under the one
 * in use: on a tall window the land and the buildings are both under the hand,
 * and on a short one the rail is what it always was. The drawers never change
 * places while this happens — a rail whose drawers move is a rail nobody learns
 * — and the one in use keeps its mark, because it is the one the bracket keys
 * step along.
 *
 * The army selector sits at the top of the rail rather than beside the board,
 * because it changes what the buildings and the units below it are: choosing
 * Blue Moon redraws half the rail.
 *
 * The rail is driven from outside it, because the keys reach it as well as the
 * pointer does: 1, 2 and 3 open a drawer, the bracket keys step along one, and
 * a tile picked up off the board opens the drawer it was filed in. See
 * `useEditorShortcuts`.
 */

import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import {
  borderVars,
  colorVars,
  radiusVars,
  spacingVars,
  textSizeVars,
  typographyVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { getFactionByCode } from "#/factions.ts";
import { terrainSpriteStyle, unitSpriteStyle } from "#/components/game_sprites.ts";
import {
  brushKey,
  drawerOfBrush,
  keyName,
  type PaletteDrawer,
  type PaletteSection,
} from "#/maps/map_editor.ts";
import type { Brush, PaletteCell } from "#/wasm/awbrn_wasm.js";

export function PaletteRail({
  armies,
  drawer,
  factionCode,
  loadedBrush,
  onDrawerChange,
  onFactionChange,
  onSelect,
  sections,
}: {
  /** The armies the fold shares an edit out to, in seat order. */
  armies: readonly string[];
  /** The open drawer. The keys reach it too, so the screen holds it. */
  drawer: PaletteDrawer;
  factionCode: string;
  loadedBrush: Brush | null;
  onDrawerChange: (drawer: PaletteDrawer) => void;
  onFactionChange: (factionCode: string) => void;
  onSelect: (brush: Brush) => void;
  sections: readonly PaletteSection[];
}) {
  const loaded = loadedBrush === null ? null : brushKey(loadedBrush);

  // A brush can be loaded from somewhere other than the rail — an undo, a
  // board that opens with one, or a tile picked up off the board — so the rail
  // opens the drawer it came from. Picking a unit off the map therefore lands
  // on the units drawer with that unit lit, which is where a map maker who
  // just pointed at one expects to be.
  //
  // The brush arrives deserialized, so it is a new object on every update the
  // board sends, and following the object itself would reopen the drawer after
  // every stroke and undo a drawer the map maker had just chosen by hand. The
  // key is what actually changed, so the drawer follows that and reads the
  // brush off a ref.
  const latestBrush = useRef(loadedBrush);
  latestBrush.current = loadedBrush;
  useEffect(() => {
    const brush = latestBrush.current;
    if (loaded !== null && brush) onDrawerChange(drawerOfBrush(brush));
  }, [loaded, onDrawerChange]);

  const inUse =
    sections.find((section) => section.drawer === drawer)?.drawer ?? sections[0]?.drawer;
  const opened = useOpenDrawers({ inUse, sections });
  // The open set is a new Set on every render, so the reach watches what it
  // holds rather than the set itself.
  const edges = useDrawerReach([inUse, [...opened.drawers].join()]);

  return (
    <VStack gap={3} ref={opened.railRef} xstyle={styles.rail}>
      <VStack gap={1.5} ref={opened.headRef}>
        <Text color="secondary" type="label">
          Painting as
        </Text>
        <HStack gap={1} wrap="wrap">
          {armies.map((code) => (
            <ArmyKey
              code={code}
              isSelected={code === factionCode}
              key={code}
              onSelect={onFactionChange}
            />
          ))}
        </HStack>
      </VStack>

      {sections.map((section) => {
        const isOpen = opened.drawers.has(section.drawer);
        return (
          <VStack
            gap={1.5}
            key={section.drawer}
            xstyle={section.drawer === inUse ? styles.sectionInUse : styles.section}
          >
            <button
              aria-expanded={isOpen}
              aria-pressed={section.drawer === inUse}
              onClick={() => onDrawerChange(section.drawer)}
              ref={opened.measureTab}
              type="button"
              {...stylex.props(styles.tab, section.drawer === inUse && styles.tabInUse)}
            >
              {section.label}
            </button>

            <div
              onScroll={section.drawer === inUse ? edges.onScroll : undefined}
              ref={section.drawer === inUse ? edges.boxRef : undefined}
              {...stylex.props(
                styles.drawer,
                section.drawer === inUse && styles.drawerInUse,
                section.drawer === inUse && FADES[edges.reach],
                !isOpen && styles.drawerShut,
              )}
            >
              <HStack
                as="ul"
                gap={1}
                ref={opened.measure(section.drawer)}
                wrap="wrap"
                xstyle={styles.grid}
              >
                {section.cells.map((cell) => {
                  const key = brushKey(cell.brush);
                  return (
                    <VStack as="li" gap={0} key={key} xstyle={styles.gridItem}>
                      <PaletteKey
                        cell={cell}
                        factionCode={factionCode}
                        isSelected={key === loaded}
                        onSelect={onSelect}
                      />
                    </VStack>
                  );
                })}
              </HStack>
            </div>
          </VStack>
        );
      })}
    </VStack>
  );
}

/** The space between two drawers, and between the head of the rail and the first. */
const DRAWER_GAP = 12;

/** The space between a drawer's name and the keys under it. */
const TAB_GAP = 6;

/**
 * The room a drawer must have over its own height before it opens.
 *
 * A drawer that opens into the last pixel of the rail gives the rail a
 * scrollbar, and a scrollbar narrows the rail, and a narrower rail rewraps the
 * keys into a taller drawer that no longer fits. This is the margin that keeps
 * that argument from ever starting.
 */
const SLACK = 8;

/**
 * Which drawers the rail has the height to hold open.
 *
 * The drawer in use is always one of them. The rest are opened in the order
 * they are filed in, while what is left of the rail can hold them whole: a
 * drawer that would have to scroll to be read is a drawer that is better shut,
 * because the map maker would lose the board reaching into it.
 *
 * The heights are read off the rail rather than worked out from the number of
 * keys. A shut drawer keeps its layout — it is folded to nothing and made
 * invisible rather than removed — so every drawer can be measured whether it is
 * open or not, and the answer follows a window that is resized.
 */
function useOpenDrawers({
  inUse,
  sections,
}: {
  inUse: PaletteDrawer | undefined;
  sections: readonly PaletteSection[];
}) {
  const railRef = useRef<HTMLDivElement | null>(null);
  const headRef = useRef<HTMLDivElement | null>(null);
  const tabRef = useRef<HTMLButtonElement | null>(null);
  const keyRefs = useRef(new Map<PaletteDrawer, HTMLElement>());
  const [room, setRoom] = useState({ head: 0, rail: 0, tab: 0 });
  const [keyHeights, setKeyHeights] = useState<ReadonlyMap<PaletteDrawer, number>>(new Map());

  const read = useCallback(() => {
    const rail = railRef.current;
    if (!rail) return;

    setRoom((was) => {
      const now = {
        head: headRef.current?.offsetHeight ?? 0,
        rail: rail.clientHeight,
        // Every drawer is named the same way, so one name is measured and all
        // three are charged for it.
        tab: tabRef.current?.offsetHeight ?? 0,
      };
      return was.head === now.head && was.rail === now.rail && was.tab === now.tab ? was : now;
    });

    setKeyHeights((was) => {
      const now = new Map<PaletteDrawer, number>();
      for (const [drawer, element] of keyRefs.current) now.set(drawer, element.offsetHeight);
      const same = was.size === now.size && [...now].every(([key, at]) => was.get(key) === at);
      return same ? was : now;
    });
  }, []);

  useLayoutEffect(() => {
    read();
    if (typeof ResizeObserver === "undefined") return;

    const observer = new ResizeObserver(read);
    if (railRef.current) observer.observe(railRef.current);
    if (headRef.current) observer.observe(headRef.current);
    for (const element of keyRefs.current.values()) observer.observe(element);
    return () => observer.disconnect();
  }, [read, sections]);

  // What is left for keys once the rail has paid for its head and for the three
  // drawer names, which are printed whether the drawer under them is open or
  // shut. Nothing is measured on the first pass, so the rail opens the drawer
  // in use and nothing else until it has read itself.
  const drawers = new Set<PaletteDrawer>();
  let left = room.rail - room.head - SLACK - sections.length * (DRAWER_GAP + room.tab + TAB_GAP);

  if (inUse !== undefined) {
    drawers.add(inUse);
    left -= keyHeights.get(inUse) ?? 0;
  }

  // The rest open in the order they are filed in, and only while what is left
  // of the rail can hold one whole. A drawer that would have to scroll to be
  // read is better shut: the map maker would lose the board reaching into it.
  for (const section of sections) {
    if (drawers.has(section.drawer)) continue;
    const keys = keyHeights.get(section.drawer);
    if (keys === undefined || keys > left) continue;
    left -= keys;
    drawers.add(section.drawer);
  }

  return {
    drawers,
    headRef,
    measure: (drawer: PaletteDrawer) => (element: HTMLElement | null) => {
      if (element) keyRefs.current.set(drawer, element);
      else keyRefs.current.delete(drawer);
    },
    measureTab: (element: HTMLButtonElement | null) => {
      if (element) tabRef.current ??= element;
    },
    railRef,
  };
}

/** How much of a drawer is out of sight, and on which side. */
type DrawerReach = "whole" | "below" | "above" | "both";

/** How deep the fade at a drawer's edge runs. Two key rows of the art. */
const FADE = "2rem";

/**
 * Which of a drawer's edges say there is more behind them.
 *
 * The affordance is drawn rather than borrowed. A platform with overlay
 * scrollbars, or a browser that ignores `scrollbar-color`, leaves the drawer
 * looking sliced off at the bottom edge with nothing to say it continues, and
 * that reads as broken rather than as deep. A fade is platform-independent,
 * costs no layout, and goes away at the end of the scroll, which a scrollbar
 * held open in a gutter cannot do.
 *
 * It is read on scroll and on resize rather than animated by a scroll
 * timeline, because the four answers are the whole vocabulary and every
 * browser can be told them.
 */
function useDrawerReach(watch: readonly unknown[]) {
  const boxRef = useRef<HTMLDivElement | null>(null);
  const [reach, setReach] = useState<DrawerReach>("whole");

  const read = useCallback(() => {
    const box = boxRef.current;
    if (!box) return;
    // A pixel of slack: a fractional layout can leave a scroll box one
    // hundredth of a pixel short of its own end, and a fade that never quite
    // clears is worse than no fade at all.
    const above = box.scrollTop > 1;
    const below = box.scrollTop + box.clientHeight < box.scrollHeight - 1;
    const now: DrawerReach = above ? (below ? "both" : "above") : below ? "below" : "whole";
    setReach((was) => (was === now ? was : now));
  }, []);

  useLayoutEffect(() => {
    read();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(read);
    const box = boxRef.current;
    if (box) {
      observer.observe(box);
      // The keys are what grows, and a drawer that fills without changing its
      // own height would otherwise never be re-read.
      for (const child of box.children) observer.observe(child);
    }
    return () => observer.disconnect();
    // biome-ignore lint/correctness/useExhaustiveDependencies: the drawer in
    // use and the set of open drawers both change what there is to measure.
  }, [read, ...watch]);

  return { boxRef, onScroll: read, reach };
}

/** One army the rail can paint as. The crest is the colour and the letters. */
function ArmyKey({
  code,
  isSelected,
  onSelect,
}: {
  code: string;
  isSelected: boolean;
  onSelect: (code: string) => void;
}) {
  const faction = getFactionByCode(code);

  return (
    <button
      aria-pressed={isSelected}
      onClick={() => onSelect(code)}
      title={faction?.displayName ?? code}
      type="button"
      {...stylex.props(styles.armyKey, isSelected && styles.armyKeySelected)}
      style={{ "--army": `var(--color-faction-${code}-accent)` } as React.CSSProperties}
    >
      <span aria-hidden="true" {...stylex.props(styles.armyBar)} />
      {code.toUpperCase()}
    </button>
  );
}

function PaletteKey({
  cell,
  factionCode,
  isSelected,
  onSelect,
}: {
  cell: PaletteCell;
  factionCode: string;
  isSelected: boolean;
  onSelect: (brush: Brush) => void;
}) {
  const isUnit = cell.brush.kind === "unit" || cell.brush.kind === "erase-unit";

  return (
    <button
      aria-pressed={isSelected}
      onClick={() => onSelect(cell.brush)}
      type="button"
      {...stylex.props(styles.key, isSelected && styles.keySelected)}
    >
      <HStack
        as="span"
        gap={0}
        justify="center"
        xstyle={[styles.keyArt, isUnit && styles.keyArtShort]}
      >
        <CellArt cell={cell} factionCode={factionCode} />
      </HStack>
      <VStack as="span" gap={0} xstyle={styles.keyLabel}>
        {keyName(cell.name)}
      </VStack>
    </button>
  );
}

/**
 * The art on one key.
 *
 * Terrain is drawn from the atlas cell the board would draw, overhang and all,
 * so a mountain on the rail is the mountain that lands on the map. An eraser
 * has no sprite of its own and wears a cut-out square instead.
 */
function CellArt({ cell, factionCode }: { cell: PaletteCell; factionCode: string }) {
  if (cell.brush.kind === "unit") {
    const sprite = unitSpriteStyle(cell.brush.unit, factionCode, 2);
    return sprite ? <span aria-hidden="true" style={sprite} {...stylex.props(styles.art)} /> : null;
  }

  if (cell.spriteIndex !== undefined) {
    return (
      <span
        aria-hidden="true"
        style={terrainSpriteStyle(cell.spriteIndex, 2)}
        {...stylex.props(styles.art)}
      />
    );
  }

  return <span aria-hidden="true" {...stylex.props(styles.eraserArt)} />;
}

const styles = stylex.create({
  rail: {
    // The rail itself never scrolls. The names of the three drawers are the way
    // between them, so they have to stay on the screen whatever is open: it is
    // the drawer in use that gives up height and keeps its own scrollbar on a
    // window too short to hold it whole.
    blockSize: "100%",
    overflow: "hidden",
  },
  section: {
    flexShrink: 0,
  },
  sectionInUse: {
    minBlockSize: 0,
  },
  // A drawer that is shut is folded to nothing rather than taken out of the
  // rail, so its keys keep their layout and the rail can go on measuring what
  // it would cost to open. `visibility` is what takes it off the screen, and it
  // takes it out of the reading order and off the tab ring with it.
  drawer: {
    display: "block",
  },
  // The drawer being drawn from is the one that gives, because it is the one
  // the map maker is looking at and the only one that was opened without first
  // being measured against the room left for it.
  //
  // It says so. A drawer cut off at the bottom edge with nothing in the margin
  // reads as a drawer that is broken rather than one that is deep, and the
  // browser draws no scrollbar of its own where the system hides them. This one
  // is drawn in the panel's own ink, and its gutter is held open whether it is
  // scrolling or not, so the keys never rewrap as a drawer fills.
  drawerInUse: {
    minBlockSize: 0,
    overflowY: "auto",
    overscrollBehavior: "contain",
    scrollbarColor: `${colorVars["--color-border-emphasized"]} transparent`,
    scrollbarGutter: "stable",
    scrollbarWidth: "thin",
    // A drawer that fills stops on whole rows rather than through the middle
    // of one. A half-height row of keys at the edge is what makes a deep
    // drawer read as a broken one. `proximity` rather than `mandatory`: this
    // is a correction to where a scroll lands, not a rail that takes the
    // scroll over.
    scrollSnapType: "y proximity",
  },
  // The fade is only ever on an edge there is something behind. See
  // `useDrawerReach`.
  reachWhole: {},
  reachBelow: {
    maskImage: `linear-gradient(to bottom, #000 calc(100% - ${FADE}), transparent 100%)`,
  },
  reachAbove: {
    maskImage: `linear-gradient(to bottom, transparent 0, #000 ${FADE})`,
  },
  reachBoth: {
    maskImage: `linear-gradient(to bottom, transparent 0, #000 ${FADE}, #000 calc(100% - ${FADE}), transparent 100%)`,
  },
  drawerShut: {
    maxBlockSize: 0,
    overflow: "hidden",
    visibility: "hidden",
  },
  // The name of a drawer is the control that opens it. The one in use wears the
  // orange rule, because it is the drawer the bracket keys are stepping along —
  // a distinction that only matters once more than one of them is open.
  tab: {
    borderWidth: 0,
    borderBlockEndWidth: "2px",
    borderBlockEndStyle: "solid",
    borderBlockEndColor: {
      default: colorVars["--color-border-emphasized"],
      ":hover": { "@media (hover: hover)": colorVars["--color-text-primary"] },
    },
    backgroundColor: "transparent",
    color: {
      default: colorVars["--color-text-secondary"],
      ":hover": { "@media (hover: hover)": colorVars["--color-text-primary"] },
    },
    cursor: "pointer",
    flexShrink: 0,
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-xs"],
    letterSpacing: "0.06em",
    paddingBlock: spacingVars["--spacing-1"],
    paddingInline: 0,
    textAlign: "start",
    textTransform: "uppercase",
    outline: {
      default: null,
      ":focus-visible": `2px solid ${colorVars["--color-accent"]}`,
    },
    outlineOffset: { default: null, ":focus-visible": "2px" },
  },
  tabInUse: {
    borderBlockEndColor: {
      default: colorVars["--color-accent"],
      ":hover": { "@media (hover: hover)": colorVars["--color-accent"] },
    },
    color: colorVars["--color-text-primary"],
  },
  grid: {
    listStyle: "none",
    margin: 0,
    padding: 0,
  },
  gridItem: {
    display: "block",
    // Every key is a snap point, so a drawer stops with a row whole at the top
    // edge whichever row that is.
    scrollSnapAlign: "start",
  },
  key: {
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    justifyContent: "flex-end",
    gap: spacingVars["--spacing-0-5"],
    // Five keys to a row, counted against the rail with its scroll gutter held
    // open, so a drawer that fills does not rewrap the drawer above it. The
    // terrain art is 32px wide at the scale the board draws it, so the key was
    // carrying half its own width in air: this is two rows off the land and
    // three off the units, which is what puts a second drawer under the hand
    // on a window that could not hold one before.
    inlineSize: "3.5rem",
    paddingBlock: spacingVars["--spacing-0-5"],
    paddingInline: spacingVars["--spacing-0-5"],
    borderWidth: borderVars["--border-width"],
    borderStyle: "solid",
    borderColor: colorVars["--color-border-emphasized"],
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: {
      default: colorVars["--color-background-surface"],
      ":hover": { "@media (hover: hover)": colorVars["--color-background-muted"] },
    },
    color: colorVars["--color-text-primary"],
    cursor: "pointer",
    outline: {
      default: null,
      ":focus-visible": `2px solid ${colorVars["--color-accent"]}`,
    },
    outlineOffset: { default: null, ":focus-visible": "2px" },
  },
  // The game's own cursor: an orange fill sitting flush on the chrome.
  keySelected: {
    backgroundColor: {
      default: colorVars["--color-accent"],
      ":hover": { "@media (hover: hover)": colorVars["--color-accent"] },
    },
    color: colorVars["--color-on-accent"],
  },
  keyArt: {
    // A terrain cell is 16 by 32 with its overhang; one height for every key in
    // the drawer keeps the names on one line across the grid.
    blockSize: "4rem",
    alignItems: "flex-end",
  },
  // A unit is 16 by 16 and needs none of that height. The units drawer is the
  // longest one on the rail, and this is what keeps it on the screen.
  keyArtShort: {
    blockSize: "2rem",
  },
  keyLabel: {
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-xs"],
    letterSpacing: "0.02em",
    lineHeight: 1.15,
    // A name longer than the key it sits under breaks rather than running over
    // its own outline — at the soft hyphen `keyName` marks where there is one,
    // and anywhere at all where there is not. Every key holds the two lines
    // whether it needs them or not. A rail of keys that are each as tall as their own name is a rail
    // with a ragged floor, and the eye reads the ragged edge as the tiles being
    // different sizes rather than the words being different lengths.
    blockSize: "2.3em",
    justifyContent: "center",
    overflowWrap: "anywhere",
    textAlign: "center",
    textTransform: "uppercase",
  },
  art: {
    display: "block",
    imageRendering: "pixelated",
  },
  // An eraser draws nothing, so its key shows the empty tile it leaves.
  eraserArt: {
    blockSize: "2rem",
    inlineSize: "2rem",
    borderWidth: borderVars["--border-width"],
    borderStyle: "dashed",
    borderColor: colorVars["--color-border-emphasized"],
    borderRadius: radiusVars["--radius-element"],
  },
  armyKey: {
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    gap: spacingVars["--spacing-0-5"],
    inlineSize: "2.5rem",
    paddingBlockEnd: spacingVars["--spacing-0-5"],
    borderWidth: borderVars["--border-width"],
    borderStyle: "solid",
    borderColor: colorVars["--color-border-emphasized"],
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: {
      default: colorVars["--color-background-surface"],
      ":hover": { "@media (hover: hover)": colorVars["--color-background-muted"] },
    },
    color: colorVars["--color-text-primary"],
    cursor: "pointer",
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-xs"],
    letterSpacing: "0.06em",
    overflow: "hidden",
    outline: {
      default: null,
      ":focus-visible": `2px solid ${colorVars["--color-accent"]}`,
    },
    outlineOffset: { default: null, ":focus-visible": "2px" },
  },
  armyKeySelected: {
    backgroundColor: {
      default: colorVars["--color-accent"],
      ":hover": { "@media (hover: hover)": colorVars["--color-accent"] },
    },
    color: colorVars["--color-on-accent"],
  },
  // The army wears its colour as a bar across the top, the way a faction panel
  // does, and still says which army it is in letters underneath.
  armyBar: {
    backgroundColor: "var(--army)",
    blockSize: spacingVars["--spacing-1-5"],
    inlineSize: "100%",
    marginBlockEnd: spacingVars["--spacing-0-5"],
  },
});

/** The fade each answer wears. */
const FADES: Record<DrawerReach, ReturnType<typeof stylex.create>[string]> = {
  whole: styles.reachWhole,
  below: styles.reachBelow,
  above: styles.reachAbove,
  both: styles.reachBoth,
};
