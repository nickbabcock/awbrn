import { Icon } from "@astryxdesign/core/Icon";
import { NumberInput } from "@astryxdesign/core/NumberInput";
import { Popover } from "@astryxdesign/core/Popover";
import { HStack, Stack, VStack } from "@astryxdesign/core/Stack";
import { colorVars, radiusVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { terrainTileStyle } from "#/components/game_sprites.ts";
import {
  MAX_ROSTER,
  MIN_ROSTER,
  rosterOfSize,
  SQUARE_ONLY,
  SYMMETRY_LABELS,
  SYMMETRY_MODES,
} from "#/maps/map_editor.ts";
import { MAP_EDITOR_MAX_SIZE, MAP_EDITOR_MIN_SIZE } from "#/maps/schemas.ts";
import { SymmetryGlyph } from "#/maps/editor/SymmetryGlyph.tsx";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { figureStyles } from "#/ui/Figure.tsx";
import { IconChevronDown, IconMinus, IconPlus, IconWarn } from "#/ui/icons.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import type { Brush, PaletteCell, ResizeAnchor, Symmetry } from "#/wasm/awbrn_wasm.js";

export interface ResizeDraft {
  width: number;
  height: number;
  anchor: ResizeAnchor;
}

/** A key on the well's top bar that opens a paper menu with an ink head. */
function MenuKey({
  children,
  label,
  menu,
  onDone,
  title,
  width,
}: {
  children: ReactNode;
  label: string;
  menu: (close: () => void) => ReactNode;
  /** A menu closed by a click hands the keyboard back to the board. */
  onDone: () => void;
  title: string;
  width: number;
}) {
  const [isOpen, setIsOpen] = useState(false);
  const isPointer = useRef(false);
  const handsBack = useRef(false);
  return (
    <Popover
      alignment="start"
      content={
        isOpen ? (
          <div
            onClickCapture={(event) => {
              isPointer.current = event.detail > 0;
            }}
          >
            <Stack xstyle={styles.menuHead}>{title}</Stack>
            {menu(() => {
              handsBack.current = isPointer.current;
              setIsOpen(false);
            })}
          </div>
        ) : null
      }
      isOpen={isOpen}
      label={title}
      onOpenChange={setIsOpen}
      padding={0}
      placement="below"
      width={width}
      xstyle={styles.menu}
    >
      {(trigger) => (
        <button
          {...trigger}
          aria-label={label}
          // The closing menu returns focus here; a click's close passes it on.
          onFocus={() => {
            if (!handsBack.current) return;
            handsBack.current = false;
            onDone();
          }}
          type="button"
          {...stylex.props(styles.key, isOpen && styles.keyOpen)}
        >
          {children}
          <Icon icon={IconChevronDown} size="sm" xstyle={styles.chevron} />
        </button>
      )}
    </Popover>
  );
}

export function SymmetryKey({
  available,
  onChange,
  onDone,
  symmetry,
  unevenGroups,
}: {
  available: readonly Symmetry[];
  onChange: (symmetry: Symmetry) => void;
  onDone: () => void;
  symmetry: Symmetry;
  unevenGroups: number;
}) {
  const isSquare = [...SQUARE_ONLY].every((mode) => available.includes(mode));
  const row = (mode: Symmetry, close: () => void) => {
    const isChosen = mode === symmetry;
    const isOut = !available.includes(mode);
    return (
      <button
        aria-disabled={isOut || undefined}
        aria-pressed={isChosen}
        key={mode}
        onClick={() => {
          if (isOut) return;
          onChange(mode);
          close();
        }}
        type="button"
        {...stylex.props(styles.row, isChosen && styles.rowChosen, isOut && styles.rowOut)}
      >
        {isChosen ? <Caret /> : <span />}
        <SymmetryGlyph mode={mode} />
        <span>{SYMMETRY_LABELS[mode]}</span>
      </button>
    );
  };
  const square = SYMMETRY_MODES.filter((mode) => SQUARE_ONLY.has(mode));

  return (
    <MenuKey
      label={`Symmetry: ${SYMMETRY_LABELS[symmetry]}`}
      onDone={onDone}
      menu={(close) => (
        <VStack gap={0} xstyle={styles.menuBody}>
          {SYMMETRY_MODES.slice(0, 4).map((mode) => row(mode, close))}
          {isSquare ? (
            square.map((mode) => row(mode, close))
          ) : (
            <Stack xstyle={styles.squareOnly}>
              {square.map((mode) => row(mode, close))}
              <span {...stylex.props(styles.squareOnlyNote)}>Square only</span>
            </Stack>
          )}
          {row("quad-mirror", close)}
        </VStack>
      )}
      title="Symmetry"
      width={288}
    >
      <SymmetryGlyph mode={symmetry} size={22} />
      {SYMMETRY_LABELS[symmetry]}
      {unevenGroups > 0 ? (
        <span title="Uneven" {...stylex.props(figureStyles.figure, styles.badge)}>
          {unevenGroups}
        </span>
      ) : null}
    </MenuKey>
  );
}

function Caret() {
  return (
    <svg aria-hidden="true" viewBox="0 0 16 16" {...stylex.props(styles.caret)}>
      <path d="M5 3l6 5-6 5z" fill="currentColor" />
    </svg>
  );
}

export function ArmiesStepper({
  onChange,
  roster,
}: {
  onChange: (roster: string[]) => void;
  roster: readonly string[];
}) {
  const size = roster.length;
  return (
    <HStack as="span" aria-label="Armies" role="group" xstyle={styles.group}>
      <Button
        icon={<Icon icon={IconMinus} />}
        isDisabled={size <= MIN_ROSTER}
        isIconOnly
        label="Fewer armies"
        onClick={() => onChange(rosterOfSize(roster, size - 1))}
        variant="ghost-on-ink"
        xstyle={styles.groupKey}
      />
      <span {...stylex.props(figureStyles.figure, styles.groupFigure)}>{size}P</span>
      <Button
        icon={<Icon icon={IconPlus} />}
        isDisabled={size >= MAX_ROSTER}
        isIconOnly
        label="More armies"
        onClick={() => onChange(rosterOfSize(roster, size + 1))}
        variant="ghost-on-ink"
        xstyle={styles.groupKey}
      />
    </HStack>
  );
}

const ANCHORS: readonly { anchor: ResizeAnchor; label: string; at: number }[] = [
  { anchor: "top-left", label: "Keep top left", at: 3 },
  { anchor: "center", label: "Keep center", at: 5.5 },
  { anchor: "bottom-right", label: "Keep bottom right", at: 8 },
];

export function SizeKey({
  fillCells,
  height,
  loadedBrush,
  onDone,
  onDraft,
  onFill,
  onResize,
  onSelect,
  symmetry,
  units,
  width,
}: {
  /** The brushes `fill` takes, as the palette draws them. */
  fillCells: readonly PaletteCell[];
  height: number;
  loadedBrush: Brush | null;
  /** The size the menu would resize to, for the board's preview; null when unchanged. */
  onDone: () => void;
  onDraft: (draft: ResizeDraft | null) => void;
  onFill: (terrain: number) => void;
  onResize: (width: number, height: number, anchor: ResizeAnchor) => void;
  onSelect: (brush: Brush) => void;
  symmetry: Symmetry;
  units: number;
  width: number;
}) {
  return (
    <MenuKey
      label="Size and fill"
      onDone={onDone}
      menu={(close) => (
        <SizeMenu
          close={close}
          fillCells={fillCells}
          height={height}
          loadedBrush={loadedBrush}
          onDraft={onDraft}
          onFill={onFill}
          onResize={onResize}
          onSelect={onSelect}
          symmetry={symmetry}
          units={units}
          width={width}
        />
      )}
      title="Size and fill"
      width={300}
    >
      <span {...stylex.props(figureStyles.figure, styles.size)}>
        {width} × {height}
      </span>
    </MenuKey>
  );
}

function SizeMenu({
  close,
  fillCells,
  height,
  loadedBrush,
  onDraft,
  onFill,
  onResize,
  onSelect,
  symmetry,
  units,
  width,
}: Omit<Parameters<typeof SizeKey>[0], "onDone"> & { close: () => void }) {
  const [draft, setDraft] = useState<ResizeDraft>({ width, height, anchor: "top-left" });
  const isChanged = draft.width !== width || draft.height !== height;
  const losesSymmetry = isChanged && SQUARE_ONLY.has(symmetry) && draft.width !== draft.height;
  const fill = fillCells.find(
    (cell) =>
      loadedBrush?.kind === "terrain" &&
      cell.brush.kind === "terrain" &&
      cell.brush.terrain === loadedBrush.terrain,
  );

  const change = (next: ResizeDraft) => {
    const clamped = {
      ...next,
      width: Math.min(Math.max(next.width, MAP_EDITOR_MIN_SIZE), MAP_EDITOR_MAX_SIZE),
      height: Math.min(Math.max(next.height, MAP_EDITOR_MIN_SIZE), MAP_EDITOR_MAX_SIZE),
    };
    setDraft(clamped);
    onDraft(clamped.width !== width || clamped.height !== height ? clamped : null);
  };
  // However the menu closes, the preview goes with it.
  useEffect(() => () => onDraft(null), [onDraft]);
  const limits = `${MAP_EDITOR_MIN_SIZE}–${MAP_EDITOR_MAX_SIZE}`;

  return (
    <VStack gap={0}>
      <VStack gap={3} xstyle={styles.menuSection}>
        <Stack xstyle={styles.dimensions}>
          <NumberInput
            hasNumberSteppers={false}
            isIntegerOnly
            label="Width"
            max={MAP_EDITOR_MAX_SIZE}
            min={MAP_EDITOR_MIN_SIZE}
            onChange={(value) => change({ ...draft, width: value })}
            units={limits}
            value={draft.width}
          />
          <span aria-hidden="true" {...stylex.props(styles.times)}>
            ×
          </span>
          <NumberInput
            hasNumberSteppers={false}
            isIntegerOnly
            label="Height"
            max={MAP_EDITOR_MAX_SIZE}
            min={MAP_EDITOR_MIN_SIZE}
            onChange={(value) => change({ ...draft, height: value })}
            units={limits}
            value={draft.height}
          />
        </Stack>
        {isChanged ? (
          <VStack gap={1.5}>
            <span {...stylex.props(styles.fieldLabel)}>Keep</span>
            <HStack as="span" role="group" aria-label="Keep" xstyle={styles.anchors}>
              {ANCHORS.map(({ anchor, label, at }) => (
                <button
                  aria-label={label}
                  aria-pressed={draft.anchor === anchor}
                  key={anchor}
                  onClick={() => change({ ...draft, anchor })}
                  title={label}
                  type="button"
                  {...stylex.props(styles.anchor, draft.anchor === anchor && styles.anchorOn)}
                >
                  <svg aria-hidden="true" height="22" viewBox="0 0 22 22" width="22">
                    <rect
                      fill="none"
                      height="20"
                      rx="2"
                      stroke="currentColor"
                      strokeDasharray="2.5 2"
                      strokeWidth="1.5"
                      width="20"
                      x="1"
                      y="1"
                    />
                    <rect fill="currentColor" height="11" rx="1.5" width="11" x={at} y={at} />
                  </svg>
                </button>
              ))}
            </HStack>
          </VStack>
        ) : null}
        {losesSymmetry ? (
          <HStack align="center" gap={2} xstyle={styles.note}>
            <SymmetryGlyph mode={symmetry} size={18} />
            {SYMMETRY_LABELS[symmetry]} becomes Free
          </HStack>
        ) : null}
        <Button
          isDisabled={!isChanged}
          label={`Resize to ${draft.width} × ${draft.height}`}
          onClick={() => {
            onResize(draft.width, draft.height, draft.anchor);
            close();
          }}
          variant="secondary"
          width="100%"
        />
      </VStack>
      <VStack gap={2} xstyle={[styles.menuSection, styles.fillSection]}>
        {fill ? (
          <>
            <Button
              label={`Fill board with ${fill.name}`}
              onClick={() => {
                if (fill.brush.kind === "terrain") onFill(fill.brush.terrain);
                close();
              }}
              variant="secondary"
              width="100%"
            >
              <HStack align="center" as="span" gap={2}>
                Fill board with
                <TileSwatch cell={fill} size={16} />
                {fill.name}
              </HStack>
            </Button>
            {units > 0 ? (
              <HStack align="center" gap={2} xstyle={styles.note}>
                <Icon icon={IconWarn} size="sm" />
                Clears all units
              </HStack>
            ) : null}
          </>
        ) : (
          <>
            <Button isDisabled label="Fill board" variant="secondary" width="100%" />
            <HStack gap={1.5}>
              {fillCells.map((cell) => (
                <button
                  aria-label={`Load ${cell.name}`}
                  key={cell.name}
                  onClick={() => onSelect(cell.brush)}
                  title={cell.name}
                  type="button"
                  {...stylex.props(styles.fillKey)}
                >
                  <TileSwatch cell={cell} size={24} />
                </button>
              ))}
            </HStack>
          </>
        )}
      </VStack>
    </VStack>
  );
}

/** A palette cell's tile square, without the art that rises above it. */
export function TileSwatch({ cell, size }: { cell: PaletteCell; size: 16 | 24 }) {
  if (cell.spriteIndex === undefined) return null;
  return (
    <span
      aria-hidden="true"
      style={terrainTileStyle(cell.spriteIndex, size / 16)}
      {...stylex.props(styles.swatch)}
    />
  );
}

const styles = stylex.create({
  key: {
    display: "inline-flex",
    alignItems: "center",
    flexShrink: 0,
    gap: "8px",
    blockSize: "44px",
    paddingInline: "10px",
    borderRadius: radiusVars["--radius-element"],
    borderStyle: "none",
    backgroundColor: {
      default: awbrnVars.ink900,
      ":hover": { default: null, [pageLayout.hoverMedia]: awbrnVars.ink850 },
    },
    boxShadow: `inset 0 0 0 1px ${awbrnVars.ink700}`,
    color: awbrnVars.textOnInkStrong,
    cursor: "pointer",
    fontFamily: awbrnVars.fontUi,
    fontSize: awbrnVars.text14_5,
    fontWeight: 650,
    whiteSpace: "nowrap",
  },
  keyOpen: {
    backgroundColor: awbrnVars.ink800,
    boxShadow: `inset 0 0 0 1.5px ${awbrnVars.textOnInkSecondary}`,
  },
  chevron: {
    color: awbrnVars.textOnInkSecondary,
  },
  size: {
    fontSize: awbrnVars.text13_5,
    fontWeight: 600,
  },
  badge: {
    display: "inline-grid",
    placeItems: "center",
    minInlineSize: "22px",
    blockSize: "20px",
    paddingInline: "6px",
    borderRadius: "999px",
    backgroundColor: awbrnVars.warnFill,
    color: awbrnVars.ink950,
    fontSize: awbrnVars.text11_5,
    fontWeight: 600,
  },
  group: {
    display: "inline-flex",
    alignItems: "center",
    flexShrink: 0,
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: awbrnVars.ink900,
    boxShadow: `inset 0 0 0 1px ${awbrnVars.ink700}`,
  },
  groupKey: {
    inlineSize: "40px",
    minInlineSize: "40px",
    blockSize: "44px",
    paddingInline: 0,
    color: awbrnVars.textOnInkStrong,
  },
  groupFigure: {
    display: "inline-grid",
    placeItems: "center",
    minInlineSize: "46px",
    blockSize: "44px",
    borderInlineColor: awbrnVars.ink700,
    borderInlineStyle: "solid",
    borderInlineWidth: "1px",
    color: awbrnVars.textOnInkStrong,
    fontSize: awbrnVars.text13,
    fontWeight: 600,
  },
  menu: {
    overflow: "hidden",
  },
  menuHead: {
    paddingBlock: "10px",
    paddingInline: "14px",
    backgroundColor: awbrnVars.ink900,
    color: awbrnVars.textOnInkStrong,
    fontSize: awbrnVars.text15,
    fontWeight: 700,
  },
  menuBody: {
    padding: "6px",
  },
  row: {
    display: "grid",
    gridTemplateColumns: "16px 26px minmax(0, 1fr)",
    alignItems: "center",
    gap: "10px",
    blockSize: "44px",
    paddingInline: "6px 10px",
    borderStyle: "none",
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: {
      default: "transparent",
      ":hover": { default: null, [pageLayout.hoverMedia]: awbrnVars.paperWell },
    },
    color: colorVars["--color-text-primary"],
    cursor: "pointer",
    fontFamily: awbrnVars.fontUi,
    fontSize: awbrnVars.text15,
    fontWeight: 600,
    textAlign: "start",
  },
  rowChosen: {
    backgroundColor: awbrnVars.redWash,
  },
  rowOut: {
    color: awbrnVars.textTertiary,
    cursor: "not-allowed",
    backgroundColor: "transparent",
  },
  caret: {
    inlineSize: "14px",
    blockSize: "14px",
    color: awbrnVars.red,
  },
  squareOnly: {
    position: "relative",
  },
  squareOnlyNote: {
    position: "absolute",
    insetInlineEnd: "8px",
    insetBlock: "8px",
    display: "flex",
    alignItems: "center",
    gap: "8px",
    color: awbrnVars.textTertiary,
    fontSize: awbrnVars.text12_5,
    fontWeight: 650,
    pointerEvents: "none",
    "::before": {
      content: '""',
      alignSelf: "stretch",
      inlineSize: "7px",
      borderColor: awbrnVars.ruleStrong,
      borderStyle: "solid",
      borderWidth: "1.5px 1.5px 1.5px 0",
      borderRadius: "0 4px 4px 0",
    },
  },
  menuSection: {
    padding: "14px",
  },
  fillSection: {
    borderBlockStartColor: awbrnVars.rule,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: "1px",
  },
  dimensions: {
    display: "grid",
    gridTemplateColumns: "1fr 14px 1fr",
    alignItems: "end",
    gap: "6px",
  },
  times: {
    paddingBlockEnd: "12px",
    color: awbrnVars.textTertiary,
    textAlign: "center",
  },
  fieldLabel: {
    color: colorVars["--color-text-secondary"],
    fontSize: awbrnVars.text13,
    fontWeight: 650,
  },
  anchors: {
    display: "inline-flex",
    alignSelf: "flex-start",
    gap: "2px",
    padding: "3px",
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: awbrnVars.paperWell,
  },
  anchor: {
    display: "grid",
    placeItems: "center",
    inlineSize: "44px",
    blockSize: "38px",
    borderStyle: "none",
    borderRadius: "4px",
    backgroundColor: "transparent",
    color: colorVars["--color-text-secondary"],
    cursor: "pointer",
  },
  anchorOn: {
    backgroundColor: awbrnVars.ink900,
    color: awbrnVars.textOnInkStrong,
  },
  note: {
    color: awbrnVars.warn,
    fontSize: awbrnVars.text13_5,
    fontWeight: 650,
  },
  fillKey: {
    inlineSize: "24px",
    blockSize: "24px",
    padding: 0,
    overflow: "hidden",
    borderStyle: "none",
    borderRadius: "4px",
    boxShadow: `0 0 0 1px ${awbrnVars.rule}`,
    cursor: "pointer",
  },
  swatch: {
    display: "block",
    flexShrink: 0,
  },
});
