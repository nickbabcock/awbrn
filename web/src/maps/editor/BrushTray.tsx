import { Icon } from "@astryxdesign/core/Icon";
import { Kbd } from "@astryxdesign/core/Kbd";
import { HStack, Stack } from "@astryxdesign/core/Stack";
import {
  colorVars,
  radiusVars,
  spacingVars,
  typeScaleVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { Fragment, type ReactNode } from "react";
import { terrainSpriteStyle, unitSpriteStyle } from "#/components/game_sprites.ts";
import { FactionLogo } from "#/components/FactionLogo.tsx";
import { DefenseStars } from "#/maps/editor/DefenseStars.tsx";
import { getFactionByCode } from "#/factions.ts";
import { BRUSH_TABS, brushKey, ERASER_NOTES, type BrushTab } from "#/maps/map_editor.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { IconErase, IconKeyboard, IconLift } from "#/ui/icons.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import type { Brush, PaletteCell } from "#/wasm/awbrn_wasm.js";

export function BrushTray({
  cells,
  isKeysOpen,
  loaded,
  onKeysToggle,
  onSelect,
  onStep,
  onTabChange,
  tab,
}: {
  /** The tab's cells, `null` for an empty slot. */
  cells: readonly (PaletteCell | null)[];
  isKeysOpen: boolean;
  /** The loaded brush, with its cell when the palette has one. */
  loaded: { brush: Brush; cell: PaletteCell | null } | null;
  onKeysToggle: (isOpen: boolean) => void;
  onSelect: (brush: Brush) => void;
  onStep: (step: 1 | -1) => void;
  onTabChange: (tab: BrushTab) => void;
  tab: BrushTab;
}) {
  const loadedKey = loaded === null ? null : brushKey(loaded.brush);

  return (
    <section aria-label="Brushes" {...stylex.props(styles.sheet)}>
      <HStack xstyle={styles.tabs}>
        <HStack aria-label="Brush tab" role="group" xstyle={styles.tabList}>
          {BRUSH_TABS.map((entry) => {
            const isOn = !isKeysOpen && entry.tab === tab;
            return (
              <button
                aria-pressed={isOn}
                key={entry.tab}
                onClick={() => {
                  onKeysToggle(false);
                  onTabChange(entry.tab);
                }}
                type="button"
                {...stylex.props(styles.tab, isOn && styles.tabOn)}
              >
                {entry.label}
                <Kbd keys={entry.key} />
              </button>
            );
          })}
        </HStack>
        <button
          aria-label="Keys"
          aria-pressed={isKeysOpen}
          onClick={() => onKeysToggle(!isKeysOpen)}
          title="Keys · ?"
          type="button"
          {...stylex.props(styles.tab, styles.keysTab, isKeysOpen && styles.tabOn)}
        >
          <Icon icon={IconKeyboard} size="sm" />
          <Kbd keys="?" />
        </button>
      </HStack>
      {isKeysOpen ? (
        <KeysTable />
      ) : (
        <>
          <BrushWell loaded={loaded} onStep={onStep} />
          <div {...stylex.props(styles.grid)}>
            {cells.map((cell, index) =>
              cell === null ? (
                <span key={`gap-${index}`} />
              ) : (
                <BrushCell
                  cell={cell}
                  isOn={brushKey(cell.brush) === loadedKey}
                  key={brushKey(cell.brush)}
                  onSelect={onSelect}
                />
              ),
            )}
          </div>
        </>
      )}
    </section>
  );
}

function BrushCell({
  cell,
  isOn,
  onSelect,
}: {
  cell: PaletteCell;
  isOn: boolean;
  onSelect: (brush: Brush) => void;
}) {
  const kind = cell.brush.kind;
  const isEraser = kind === "erase" || kind === "erase-unit";
  const label = isEraser ? `${cell.name}: ${ERASER_NOTES[kind]}` : cell.name;
  return (
    <button
      aria-label={label}
      aria-pressed={isOn}
      onClick={() => onSelect(cell.brush)}
      title={isEraser ? `${label} · E` : label}
      type="button"
      {...stylex.props(styles.cell, isOn && styles.cellOn)}
    >
      <BrushArt brush={cell.brush} cell={cell} isOnInk={isOn} />
      {isEraser ? <Kbd keys="e" xstyle={[styles.cellCap, isOn && styles.cellCapOnInk]} /> : null}
    </button>
  );
}

function BrushArt({
  brush,
  cell,
  isOnInk,
}: {
  brush: Brush;
  cell: PaletteCell | null;
  isOnInk: boolean;
}) {
  if (brush.kind === "unit") {
    const sprite = unitSpriteStyle(brush.unit, brush.faction, 2);
    return sprite ? <span aria-hidden="true" style={sprite} /> : null;
  }
  if (brush.kind === "erase" || brush.kind === "erase-unit") {
    return (
      <Icon
        icon={brush.kind === "erase" ? IconErase : IconLift}
        size="md"
        xstyle={[styles.eraser, isOnInk && styles.eraserOnInk]}
      />
    );
  }
  if (cell?.spriteIndex === undefined) return null;
  return (
    <span aria-hidden="true" {...stylex.props(styles.tile)}>
      <span style={terrainSpriteStyle(cell.spriteIndex, 2)} {...stylex.props(styles.tileCell)} />
    </span>
  );
}

function BrushWell({
  loaded,
  onStep,
}: {
  loaded: { brush: Brush; cell: PaletteCell | null } | null;
  onStep: (step: 1 | -1) => void;
}) {
  const brush = loaded?.brush ?? null;
  const faction =
    brush?.kind === "unit" || brush?.kind === "property" ? (brush.faction ?? null) : null;
  let note: ReactNode = null;
  if (brush?.kind === "erase" || brush?.kind === "erase-unit") note = ERASER_NOTES[brush.kind];
  else if (faction !== null) {
    note = (
      <>
        <FactionLogo factionCode={faction} isLabelHidden size={14} />
        {getFactionByCode(faction)?.displayName ?? faction}
      </>
    );
  } else if (brush?.kind === "property") note = "Neutral";
  else if (loaded?.cell) note = <DefenseStars count={loaded.cell.defense} />;

  return (
    <Stack xstyle={styles.well}>
      <button
        aria-label="Previous brush"
        onClick={() => onStep(-1)}
        title="Previous brush · ["
        type="button"
        {...stylex.props(styles.step)}
      >
        <Kbd keys="[" />
      </button>
      <span {...stylex.props(styles.wellArt)}>
        {brush ? <BrushArt brush={brush} cell={loaded?.cell ?? null} isOnInk /> : null}
      </span>
      <Stack xstyle={styles.wellName}>
        <span {...stylex.props(styles.wellTitle)}>{loaded?.cell?.name ?? ""}</span>
        <HStack align="center" as="span" gap={1.5} xstyle={styles.wellNote}>
          {note}
        </HStack>
      </Stack>
      <button
        aria-label="Next brush"
        onClick={() => onStep(1)}
        title="Next brush · ]"
        type="button"
        {...stylex.props(styles.step)}
      >
        <Kbd keys="]" />
      </button>
    </Stack>
  );
}

/** One binding per row; caps joined with `+`, mouse inputs as words, alternatives with `or`. */
type KeyRow = readonly [readonly (string | readonly string[])[], string];

const KEY_GROUPS: readonly (readonly KeyRow[])[] = [
  [
    [[["1"], ["2"], ["3"]], "Brush tab"],
    [[["["], ["]"]], "Step brush"],
    [[["e"]], "Eraser"],
    [[["shift", "1"], "–", ["8"]], "Army"],
  ],
  [
    [[["alt"], "+ click"], "Pick up"],
    [[["alt", "shift"], "+ click"], "Pick up ground"],
    [[["space"], "+ drag"], "Pan"],
    [[["="], ["−"], "or wheel"], "Zoom in, out"],
    [[["n"]], "Next to fix"],
    [[["shift", "n"]], "Previous to fix"],
  ],
  [
    [[["mod", "z"]], "Undo"],
    [[["mod", "shift", "z"], "or", ["mod", "y"]], "Redo"],
    [[["?"], "or Esc"], "Keys, back"],
  ],
];

function KeysTable() {
  return (
    <dl aria-label="Keys" {...stylex.props(styles.keys)}>
      {KEY_GROUPS.map((group, index) => (
        <div key={group[0]?.[1]} {...stylex.props(styles.keysGroup, index > 0 && styles.keysRule)}>
          {group.map(([keys, does]) => (
            <Fragment key={does}>
              <dt {...stylex.props(styles.keysKeys)}>
                {keys.map((part, at) =>
                  typeof part === "string" ? (
                    <span key={at}>{part}</span>
                  ) : (
                    <Combo key={at} keys={part} />
                  ),
                )}
              </dt>
              <dd {...stylex.props(styles.keysDoes)}>{does}</dd>
            </Fragment>
          ))}
        </div>
      ))}
    </dl>
  );
}

function Combo({ keys }: { keys: readonly string[] }) {
  return (
    <HStack as="span" align="center" gap={0.5}>
      {keys.map((key, index) => (
        <Fragment key={key}>
          {index > 0 ? <span>+</span> : null}
          {/* Kbd capitalizes a key it has no name for; Space reads like Shift and Ctrl. */}
          {key === "space" ? (
            <kbd {...stylex.props(styles.spaceCap)}>Space</kbd>
          ) : (
            <Kbd keys={key} />
          )}
        </Fragment>
      ))}
    </HStack>
  );
}

const styles = stylex.create({
  sheet: {
    display: "grid",
    gridTemplateRows: "auto auto minmax(0, 1fr)",
    flexShrink: 0.01,
    minBlockSize: 0,
    overflow: "hidden",
    borderRadius: "10px",
    backgroundColor: awbrnVars.sheet,
    boxShadow: `${awbrnVars.shadowSheet}, 0 0 0 1px ${awbrnVars.rule}`,
  },
  tabs: {
    gap: "2px",
    paddingInline: "8px",
    borderBlockEndColor: awbrnVars.rule,
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: "1px",
  },
  tab: {
    position: "relative",
    display: "inline-flex",
    alignItems: "center",
    gap: "8px",
    blockSize: { default: "44px", [pageLayout.shortMedia]: "40px" },
    paddingInline: "8px",
    borderStyle: "none",
    backgroundColor: "transparent",
    color: {
      default: colorVars["--color-text-secondary"],
      ":hover": { default: null, [pageLayout.hoverMedia]: colorVars["--color-text-primary"] },
    },
    cursor: "pointer",
    fontFamily: awbrnVars.fontUi,
    fontSize: awbrnVars.text15,
    fontWeight: 600,
  },
  tabOn: {
    color: colorVars["--color-text-primary"],
    fontWeight: 700,
    "::after": {
      content: '""',
      position: "absolute",
      insetInline: "6px",
      insetBlockEnd: "-1px",
      blockSize: "3px",
      borderRadius: "2px 2px 0 0",
      backgroundColor: awbrnVars.ink900,
    },
  },
  tabList: {
    gap: "2px",
  },
  keysTab: {
    marginInlineStart: "auto",
    gap: "6px",
  },
  well: {
    display: "grid",
    gridTemplateColumns: {
      default: "30px 52px minmax(0, 1fr) 30px",
      [pageLayout.shortMedia]: "30px 44px minmax(0, 1fr) 30px",
    },
    alignItems: "center",
    gap: "10px",
    paddingBlock: { default: "8px 6px", [pageLayout.shortMedia]: "6px 4px" },
    paddingInline: "12px",
  },
  step: {
    display: "grid",
    placeItems: "center",
    inlineSize: "30px",
    blockSize: "44px",
    padding: 0,
    borderStyle: "none",
    backgroundColor: "transparent",
    cursor: "pointer",
  },
  wellArt: {
    display: "grid",
    placeItems: "end center",
    inlineSize: { default: "52px", [pageLayout.shortMedia]: "44px" },
    blockSize: { default: "56px", [pageLayout.shortMedia]: "44px" },
    paddingBlockEnd: { default: "6px", [pageLayout.shortMedia]: "2px" },
    overflow: "hidden",
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: awbrnVars.ink950,
  },
  wellName: {
    minInlineSize: 0,
  },
  wellTitle: {
    color: colorVars["--color-text-primary"],
    fontSize: { default: awbrnVars.text17, [pageLayout.shortMedia]: awbrnVars.text16 },
    fontWeight: 700,
    lineHeight: 1.2,
  },
  wellNote: {
    minBlockSize: "16px",
    marginBlockStart: "4px",
    color: colorVars["--color-text-secondary"],
    fontSize: awbrnVars.text13,
    fontWeight: 600,
  },
  grid: {
    display: "grid",
    gridTemplateColumns: "repeat(7, 42px)",
    alignContent: "start",
    gap: { default: "5px 6px", [pageLayout.shortMedia]: "4px 6px" },
    paddingBlock: { default: "2px 12px", [pageLayout.shortMedia]: "2px 8px" },
    paddingInline: "12px",
    overflowY: "auto",
    scrollbarWidth: "thin",
    scrollbarColor: `${awbrnVars.ruleStrong} transparent`,
  },
  cell: {
    position: "relative",
    display: "grid",
    placeItems: "end center",
    inlineSize: "42px",
    blockSize: { default: "47px", [pageLayout.shortMedia]: "40px" },
    padding: 0,
    paddingBlockEnd: { default: "4px", [pageLayout.shortMedia]: "2px" },
    overflow: "hidden",
    borderStyle: "none",
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: {
      default: awbrnVars.paperWell,
      ":hover": { default: null, [pageLayout.hoverMedia]: awbrnVars.ink100 },
    },
    cursor: "pointer",
  },
  cellOn: {
    overflow: "visible",
    backgroundColor: {
      default: awbrnVars.ink900,
      ":hover": awbrnVars.ink900,
    },
    boxShadow: `0 0 0 2px ${awbrnVars.sheet}, 0 0 0 4px ${awbrnVars.ink900}`,
  },
  cellCap: {
    position: "absolute",
    insetBlockStart: "3px",
    insetInlineEnd: "3px",
    [spacingVars["--spacing-5"]]: "16px",
    [typeScaleVars["--text-supporting-size"]]: awbrnVars.textMicro,
  },
  tile: {
    display: "flex",
    alignItems: "flex-end",
    inlineSize: "32px",
    blockSize: "40px",
    overflow: "hidden",
  },
  tileCell: {
    flexShrink: 0,
  },
  eraser: {
    marginBlockEnd: { default: "6px", [pageLayout.shortMedia]: 0 },
    color: colorVars["--color-text-primary"],
  },
  eraserOnInk: {
    color: awbrnVars.textOnInkStrong,
  },
  keys: {
    display: "grid",
    gridTemplateColumns: "auto 1fr",
    alignContent: "start",
    columnGap: "14px",
    margin: 0,
    paddingBlock: "10px 12px",
    paddingInline: "16px",
    overflowY: "auto",
  },
  keysKeys: {
    display: "flex",
    alignItems: "center",
    justifyContent: "flex-end",
    gap: "3px",
    blockSize: { default: "22px", [pageLayout.shortMedia]: "21px" },
    color: colorVars["--color-text-secondary"],
    fontSize: awbrnVars.text12_5,
    fontWeight: 600,
  },
  keysDoes: {
    display: "flex",
    alignItems: "center",
    margin: 0,
    color: colorVars["--color-text-primary"],
    fontSize: awbrnVars.text14,
    fontWeight: 600,
  },
  keysGroup: {
    display: "grid",
    gridColumn: "1 / -1",
    gridTemplateColumns: "subgrid",
  },
  keysRule: {
    marginBlockStart: "4px",
    paddingBlockStart: "4px",
    borderBlockStartColor: awbrnVars.rule,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: "1px",
  },
  // Kbd's own cap, for the one key it can't name.
  spaceCap: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    minInlineSize: spacingVars["--spacing-5"],
    blockSize: spacingVars["--spacing-5"],
    paddingInline: spacingVars["--spacing-1"],
    borderRadius: radiusVars["--radius-inner"],
    backgroundColor: colorVars["--color-neutral"],
    borderBlockEndColor: colorVars["--color-border-emphasized"],
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: "2px",
    color: colorVars["--color-text-primary"],
    fontFamily: awbrnVars.fontUi,
    fontSize: typeScaleVars["--text-supporting-size"],
    fontWeight: 500,
  },
  cellCapOnInk: {
    [colorVars["--color-neutral"]]: awbrnVars.ink850,
    [colorVars["--color-border-emphasized"]]: awbrnVars.ink600,
    [colorVars["--color-text-primary"]]: awbrnVars.textOnInkSecondary,
  },
});
