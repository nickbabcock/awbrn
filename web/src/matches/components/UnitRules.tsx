/*
 * Unit bans and Lab units: the editor (the Restrictions dialog's Units tab), the
 * read-back rows, and the one mark of the Lab rule.
 */

import { Icon } from "@astryxdesign/core/Icon";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import {
  colorVars,
  focusVars,
  radiusVars,
  spacingVars,
  typeScaleVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useLayoutEffect, useState, type ReactNode } from "react";
import { getCoById } from "#/co_roster.ts";
import { CoStamp } from "#/components/CoStamp.tsx";
import { unitSpriteStyle } from "#/components/game_sprites.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { figureStyles } from "#/ui/Figure.tsx";
import { IconChevronDown, IconChevronUp, IconLock } from "#/ui/icons.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { onRadioGroupKeyDown, onTabListKeyDown } from "#/ui/radioGroup.ts";
import type { UnitKind } from "#/wasm/awbrn_wasm.js";
import {
  setUnitState,
  unitCatalog,
  unitGroups,
  unitName,
  unitState,
  type UnitRules,
} from "../unit_rules.ts";
import { scopeChipStyles } from "./SeatBans.tsx";

/** The rule's one phrase, everywhere it is said. */
export const LAB_RULE = "Needs your own Lab";

/** A unit sprite at 2x, in the reader's army. */
export function UnitSprite({
  factionCode,
  isLabeled = false,
  kind,
  xstyle,
}: {
  factionCode: string;
  /** Named for assistive tech and the pointer; otherwise decorative. */
  isLabeled?: boolean;
  kind: UnitKind;
  xstyle?: stylex.StyleXStyles;
}) {
  return (
    // Raw span: an inline atlas crop, which no Astryx component takes.
    <span
      aria-hidden={isLabeled ? undefined : "true"}
      aria-label={isLabeled ? unitName(kind) : undefined}
      role={isLabeled ? "img" : undefined}
      style={unitSpriteStyle(kind, factionCode, 2) ?? undefined}
      title={isLabeled ? unitName(kind) : undefined}
      {...stylex.props(styles.sprite, xstyle)}
    />
  );
}

/** The Lab rule's mark: `Lab`, or locked while you own no Lab. Its name is the rule. */
export function LabTag({
  id,
  isLocked = false,
  xstyle,
}: {
  id?: string;
  isLocked?: boolean;
  xstyle?: stylex.StyleXStyles;
}) {
  return (
    <span
      aria-label={LAB_RULE}
      id={id}
      role="img"
      title={LAB_RULE}
      {...stylex.props(styles.tag, isLocked && styles.tagLocked, xstyle)}
    >
      {isLocked ? <Icon icon={IconLock} size="xsm" xstyle={styles.tagLock} /> : null}
      Lab
    </span>
  );
}

/**
 * The Restrictions dialog's ink head: two tabs where the title was. Both
 * control the one panel `panelId` names, whose content follows the tab.
 */
export function RuleTabs({
  onChange,
  panelId,
  tab,
}: {
  onChange: (tab: "cos" | "units") => void;
  panelId: string;
  tab: "cos" | "units";
}) {
  return (
    <HStack
      aria-label="Restrictions"
      onKeyDown={onTabListKeyDown}
      role="tablist"
      xstyle={styles.tabs}
    >
      {(["cos", "units"] as const).map((id) => (
        <button
          aria-controls={panelId}
          aria-selected={tab === id}
          id={ruleTabId(panelId, id)}
          key={id}
          onClick={() => onChange(id)}
          role="tab"
          tabIndex={tab === id ? 0 : -1}
          type="button"
          {...stylex.props(styles.tab, tab === id && styles.tabOn)}
        >
          {id === "cos" ? "COs" : "Units"}
        </button>
      ))}
    </HStack>
  );
}

/** The id of a Restrictions tab, which its panel is labeled by. */
export function ruleTabId(panelId: string, tab: "cos" | "units"): string {
  return `${panelId}-${tab}`;
}

/**
 * The Units tab. A press sets a unit to the brush's state, and a second press
 * clears it; a group's key brushes the whole group. A map with no Lab has no
 * brush row, and a press bans.
 */
export function UnitEditor({
  factionCode,
  hasLab,
  initialBrush = "banned",
  onChange,
  rules,
}: {
  factionCode: string;
  hasLab: boolean;
  initialBrush?: "banned" | "lab";
  onChange: (rules: UnitRules) => void;
  rules: UnitRules;
}) {
  const [chosenBrush, setBrush] = useState(initialBrush);
  const brush = hasLab ? chosenBrush : "banned";
  const brushWord = brush === "banned" ? "Ban" : "Make Lab only";

  return (
    <VStack xstyle={styles.editor}>
      {hasLab ? (
        <HStack
          aria-label="Press to set"
          onKeyDown={onRadioGroupKeyDown}
          role="radiogroup"
          xstyle={scopeChipStyles.chips}
        >
          {(["banned", "lab"] as const).map((state) => (
            <button
              aria-checked={brush === state}
              key={state}
              onClick={() => setBrush(state)}
              role="radio"
              tabIndex={brush === state ? 0 : -1}
              type="button"
              {...stylex.props(scopeChipStyles.chip, brush === state && scopeChipStyles.chipOn)}
            >
              {state === "banned" ? "Ban" : "Lab only"}
            </button>
          ))}
        </HStack>
      ) : null}
      <VStack xstyle={styles.groups}>
        {unitGroups.map(({ group, name, facility }) => {
          const kinds = unitCatalog
            .filter((entry) => entry.group === group)
            .map((entry) => entry.kind);
          const isGroupSet = kinds.every((kind) => unitState(rules, kind) === brush);
          const groupLabel = `${isGroupSet ? "Clear" : brushWord} every ${name.toLowerCase()} unit`;
          return (
            <VStack
              aria-label={`${name}, ${facility}`}
              key={group}
              role="group"
              xstyle={styles.group}
            >
              <HStack as="span" xstyle={styles.groupHead}>
                <button
                  aria-label={groupLabel}
                  aria-pressed={isGroupSet}
                  onClick={() =>
                    onChange(setUnitState(rules, kinds, isGroupSet ? "buildable" : brush))
                  }
                  title={groupLabel}
                  type="button"
                  {...stylex.props(styles.key, styles.groupKey)}
                >
                  {name}
                </button>
                <Text color="inherit" type="inherit" aria-hidden="true" xstyle={styles.facility}>
                  {facility}
                </Text>
              </HStack>
              <HStack xstyle={styles.cells}>
                {kinds.map((kind) => {
                  const state = unitState(rules, kind);
                  return (
                    <button
                      aria-label={`${unitName(kind)}${state === "banned" ? ", banned" : state === "lab" ? ", Lab only" : ""}`}
                      aria-pressed={state !== "buildable"}
                      key={kind}
                      onClick={() =>
                        onChange(setUnitState(rules, [kind], state === brush ? "buildable" : brush))
                      }
                      type="button"
                      {...stylex.props(styles.key, styles.cell, state === "lab" && styles.cellLab)}
                    >
                      <span {...stylex.props(styles.art)}>
                        <UnitSprite
                          factionCode={factionCode}
                          kind={kind}
                          xstyle={state === "banned" && styles.spriteBanned}
                        />
                        {state === "banned" ? <CancelWaves /> : null}
                      </span>
                      <Text
                        color="inherit"
                        type="inherit"
                        xstyle={[styles.cellName, state === "banned" && styles.cellNameBanned]}
                      >
                        {unitName(kind)}
                      </Text>
                      {state === "lab" ? <LabTag xstyle={styles.cellTag} /> : null}
                    </button>
                  );
                })}
              </HStack>
            </VStack>
          );
        })}
      </VStack>
    </VStack>
  );
}

/** Two short waves: the sprite's outline stays readable through them. */
function CancelWaves() {
  const wave = "M0 6c6-4 9 4 15 0s9-4 15 0 9 4 14 0";
  return (
    <svg aria-hidden="true" viewBox="0 0 44 20" {...stylex.props(styles.waves)}>
      <path d={wave} />
      <path d={wave} transform="translate(0 8)" />
    </svg>
  );
}

/**
 * The read-back's well: a Paper grid whose rows share four columns (label,
 * items, the row's key, Edit), so two wells line up.
 */
export function RuleWell({ children, label }: { children: ReactNode; label?: string }) {
  if (label === undefined) return <VStack xstyle={styles.well}>{children}</VStack>;
  return (
    <VStack aria-label={label} role="group" xstyle={styles.field}>
      <Text color="inherit" type="inherit" xstyle={styles.fieldLabel}>
        {label}
      </Text>
      <VStack xstyle={styles.well}>{children}</VStack>
    </VStack>
  );
}

/** How many `size`px items, `gap` apart, the measured box holds; null until measured. */
function useFit(size: number, gap: number) {
  // Held in state, so the span a reopened row mounts is observed afresh.
  const [element, setElement] = useState<HTMLSpanElement | null>(null);
  const [fit, setFit] = useState<number | null>(null);
  useLayoutEffect(() => {
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => {
      if (entry) setFit(Math.max(1, Math.floor((entry.contentRect.width + gap) / (size + gap))));
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [element, size, gap]);
  return [setElement, fit] as const;
}

/**
 * One read-back row. It shows as many items as its width holds, then a `+N`
 * key (a chevron when all fit) that opens it in place: unit rows into a list
 * of names, the COs row into all its stamps.
 */
export function RuleRow({
  edit,
  itemSize,
  items,
  label,
  named,
  note,
  noteWhenClosed = false,
}: {
  edit?: ReactNode;
  itemSize: number;
  items: readonly ReactNode[];
  label?: string;
  /** The opened row's list of names; without one, the row opens only to wrap its items. */
  named?: ReactNode;
  /** Said on the opened row's key line. */
  note?: string;
  /** Also say the note under the closed row's items. */
  noteWhenClosed?: boolean;
}) {
  const [isOpen, setIsOpen] = useState(false);
  const gap = itemSize > 36 ? 4 : 6;
  const [itemsRef, fit] = useFit(itemSize, gap);
  const shown = fit === null ? items : items.slice(0, fit);
  const more = items.length - shown.length;
  const hasKey = more > 0 || named !== undefined;
  const isNamedOpen = isOpen && named !== undefined;
  const opener = isOpen ? (
    <button
      aria-expanded="true"
      aria-label="Show fewer"
      onClick={() => setIsOpen(false)}
      type="button"
      {...stylex.props(styles.rowKey)}
    >
      <Icon icon={IconChevronUp} size="sm" />
    </button>
  ) : hasKey ? (
    <button
      aria-expanded="false"
      aria-label={named === undefined ? `Show all ${items.length}` : "Show names"}
      onClick={() => setIsOpen(true)}
      type="button"
      {...stylex.props(styles.rowKey, more > 0 && figureStyles.figure)}
    >
      {more > 0 ? `+${more}` : <Icon icon={IconChevronDown} size="sm" />}
    </button>
  ) : null;

  return (
    <>
      <HStack
        xstyle={[styles.row, noteWhenClosed && !isOpen && note !== undefined && styles.rowNoted]}
      >
        <Text color="inherit" type="inherit" xstyle={styles.rowLabel}>
          {label}
        </Text>
        <VStack xstyle={styles.rowBody}>
          {isNamedOpen ? (
            note ? (
              <Text color="inherit" type="inherit" xstyle={styles.note}>
                {note}
              </Text>
            ) : null
          ) : (
            <span
              ref={itemsRef}
              {...stylex.props(
                styles.items,
                isOpen && styles.itemsOpen,
                gap === 4 && styles.itemsTight,
              )}
            >
              {isOpen ? items : shown}
            </span>
          )}
          {noteWhenClosed && !isOpen && note ? (
            <Text color="inherit" type="inherit" xstyle={styles.note}>
              {note}
            </Text>
          ) : null}
        </VStack>
        <HStack as="span" xstyle={styles.keyCell}>
          {opener}
        </HStack>
        <HStack as="span" xstyle={styles.editCell}>
          {edit}
        </HStack>
      </HStack>
      {isNamedOpen ? named : null}
    </>
  );
}

/** The read-back's COs row: the bans every seat shares. */
export function CoRow({ coIds, edit }: { coIds: readonly number[]; edit?: ReactNode }) {
  const cos = coIds.map((coId) => getCoById(coId)).filter((co) => co !== null);
  return (
    <RuleRow
      edit={edit}
      itemSize={40}
      items={
        cos.length === 0
          ? [
              <Text color="secondary" key="none">
                None
              </Text>,
            ]
          : cos.map((co) => <CoStamp coKey={co.key} key={co.key} xstyle={styles.sprite} />)
      }
      label="COs"
    />
  );
}

/** A unit row of the read-back, in the reader's army. */
export function UnitRow({
  edit,
  factionCode,
  isLab = false,
  kinds,
  label,
  noteWhenClosed,
}: {
  edit?: ReactNode;
  factionCode: string;
  /** Lab units say the rule. */
  isLab?: boolean;
  kinds: readonly UnitKind[];
  label?: string;
  noteWhenClosed?: boolean;
}) {
  return (
    <RuleRow
      edit={edit}
      itemSize={32}
      items={kinds.map((kind) => (
        <UnitSprite factionCode={factionCode} isLabeled key={kind} kind={kind} />
      ))}
      label={label}
      named={
        <ul {...stylex.props(styles.named)}>
          {kinds.map((kind) => (
            <li key={kind} {...stylex.props(styles.namedItem)}>
              <UnitSprite factionCode={factionCode} kind={kind} />
              <Text color="inherit" type="inherit" xstyle={styles.namedName}>
                {unitName(kind)}
              </Text>
            </li>
          ))}
        </ul>
      }
      note={isLab ? LAB_RULE : undefined}
      noteWhenClosed={noteWhenClosed}
    />
  );
}

/** What a Lab's readout says: the match's Lab units, and whether the reader owns any Lab. */
export interface LabReadout {
  units: readonly UnitKind[];
  ownsLab: boolean;
  factionCode: string;
}

/** Sprites the desktop readout shows before it counts the rest. */
const READOUT_SPRITES = 8;

/**
 * The Lab line of a Lab tile's readout. It follows whether the reader owns
 * any Lab, not who owns this one.
 */
export function LabLine({ lab, xstyle }: { lab: LabReadout; xstyle?: stylex.StyleXStyles }) {
  const isOver = lab.units.length > READOUT_SPRITES;
  const shown = isOver ? lab.units.slice(0, READOUT_SPRITES - 1) : lab.units;
  const cells = shown.length + (isOver ? 1 : 0);
  // Balanced rows of at most four: five is three over two.
  const columns = cells <= 4 ? cells : Math.ceil(cells / 2);
  return (
    <VStack xstyle={[styles.labLine, xstyle]}>
      <LabTag isLocked={!lab.ownsLab} />
      <span {...stylex.props(styles.labSprites, styles.labColumns(columns))}>
        {shown.map((kind) => (
          <UnitSprite
            factionCode={lab.factionCode}
            isLabeled
            key={kind}
            kind={kind}
            xstyle={!lab.ownsLab && styles.spriteLocked}
          />
        ))}
        {isOver ? (
          <Text color="inherit" type="inherit" xstyle={[figureStyles.figure, styles.labMore]}>
            +{lab.units.length - shown.length}
          </Text>
        ) : null}
      </span>
    </VStack>
  );
}

/** On a phone, the tile row's key to the Lab units sheet. */
export function LabKey({ lab, onOpen }: { lab: LabReadout; onOpen: () => void }) {
  const [first] = lab.units;
  return (
    <button
      aria-haspopup="dialog"
      aria-label={`Lab units${lab.ownsLab ? "" : `, ${LAB_RULE.toLowerCase()}`}`}
      onClick={onOpen}
      type="button"
      {...stylex.props(styles.labKey)}
    >
      <LabTag isLocked={!lab.ownsLab} />
      {first ? (
        <UnitSprite
          factionCode={lab.factionCode}
          kind={first}
          xstyle={!lab.ownsLab && styles.spriteLocked}
        />
      ) : null}
      {lab.units.length > 1 ? (
        <Text color="inherit" type="inherit" xstyle={[figureStyles.figure, styles.labMore]}>
          +{lab.units.length - 1}
        </Text>
      ) : null}
    </button>
  );
}

/** The Lab units sheet's body: a plain list to read, never a place to build. */
export function LabUnitsList({ lab }: { lab: LabReadout }) {
  return (
    <VStack xstyle={styles.labList}>
      {unitGroups.map(({ group, name }) => {
        const entries = unitCatalog.filter(
          (entry) => entry.group === group && lab.units.includes(entry.kind),
        );
        if (entries.length === 0) return null;
        return (
          <VStack aria-label={name} key={group} role="group">
            <Text color="inherit" type="inherit" xstyle={styles.labGroup}>
              {name}
            </Text>
            {entries.map((entry) => (
              <HStack
                key={entry.kind}
                xstyle={[styles.labItem, !lab.ownsLab && styles.labItemLocked]}
              >
                <UnitSprite
                  factionCode={lab.factionCode}
                  kind={entry.kind}
                  xstyle={!lab.ownsLab && styles.spriteLocked}
                />
                <Text color="inherit" type="inherit" xstyle={styles.labName}>
                  {entry.name}
                </Text>
                <Text color="inherit" type="inherit" xstyle={figureStyles.figure}>
                  {entry.cost.toLocaleString("en-US")}
                </Text>
              </HStack>
            ))}
          </VStack>
        );
      })}
    </VStack>
  );
}

const styles = stylex.create({
  labLine: {
    alignItems: "start",
    gap: "8px",
    paddingBlock: spacingVars["--spacing-2"],
    paddingInline: spacingVars["--spacing-2"],
  },
  labSprites: {
    display: "grid",
    alignItems: "center",
    justifyItems: "center",
    gap: "6px",
  },
  labColumns: (columns: number) => ({
    gridTemplateColumns: `repeat(${columns}, 32px)`,
  }),
  labMore: {
    color: colorVars["--color-text-secondary"],
    fontSize: awbrnVars.text13,
    fontWeight: 650,
  },
  spriteLocked: {
    filter: "grayscale(1) opacity(0.55)",
  },
  // A 36px box with a 44px target.
  labKey: {
    position: "relative",
    display: "inline-flex",
    alignItems: "center",
    gap: "6px",
    flexShrink: 0,
    blockSize: "36px",
    paddingInline: "6px",
    borderColor: awbrnVars.ruleStrong,
    borderRadius: "6px",
    borderStyle: "solid",
    borderWidth: "1px",
    backgroundColor: awbrnVars.sheet,
    color: colorVars["--color-text-primary"],
    font: "inherit",
    cursor: "pointer",
    overflow: "visible",
    "::before": {
      content: '""',
      position: "absolute",
      inset: "-5px -1px",
    },
  },
  labList: {
    display: "grid",
    gap: "8px",
    paddingBlock: "12px 20px",
    paddingInline: "16px",
    backgroundColor: awbrnVars.paper,
  },
  labGroup: {
    color: colorVars["--color-text-secondary"],
    fontSize: awbrnVars.text13,
    fontWeight: 700,
  },
  labItem: {
    alignItems: "center",
    gap: "10px",
    minBlockSize: "42px",
  },
  labItemLocked: {
    color: awbrnVars.textTertiary,
  },
  labName: {
    flexGrow: 1,
    fontWeight: 650,
  },
  sprite: {
    display: "block",
    flexShrink: 0,
  },
  tag: {
    display: "inline-flex",
    alignItems: "center",
    gap: "3px",
    flexShrink: 0,
    blockSize: "20px",
    paddingInline: "6px",
    borderRadius: "4px",
    backgroundColor: awbrnVars.sheet,
    boxShadow: `inset 0 0 0 1px ${awbrnVars.ruleStrong}`,
    color: colorVars["--color-text-primary"],
    fontSize: awbrnVars.text12,
    fontWeight: 750,
    lineHeight: 1,
    whiteSpace: "nowrap",
  },
  tagLocked: {
    paddingInlineStart: "5px",
    backgroundColor: awbrnVars.paperWell,
    boxShadow: "none",
    color: colorVars["--color-text-secondary"],
  },
  tagLock: {
    inlineSize: "12px",
    blockSize: "12px",
  },
  tabs: {
    alignSelf: "stretch",
    gap: "4px",
    marginBlock: `calc(${awbrnVars.spacing2_5} * -1)`,
    marginInlineStart: "-6px",
  },
  tab: {
    position: "relative",
    display: "flex",
    alignItems: "center",
    paddingInline: "12px",
    borderStyle: "none",
    backgroundColor: "transparent",
    color: awbrnVars.textOnInkSecondary,
    font: "inherit",
    fontSize: awbrnVars.text16,
    fontWeight: 650,
    cursor: "pointer",
    outline: {
      default: null,
      ":focus-visible": `2px solid ${focusVars["--focus-outline-color"]}`,
    },
    outlineOffset: { default: null, ":focus-visible": "-4px" },
  },
  tabOn: {
    color: awbrnVars.textOnInkStrong,
    "::after": {
      content: '""',
      position: "absolute",
      insetInline: "12px",
      insetBlockEnd: 0,
      blockSize: "3px",
      borderRadius: "2px 2px 0 0",
      backgroundColor: awbrnVars.textOnInkStrong,
    },
  },
  editor: {
    display: "grid",
  },
  groups: {
    display: "grid",
    paddingBlock: "4px 12px",
    paddingInline: {
      default: "20px",
      [pageLayout.phoneMedia]: "16px",
    },
  },
  group: {
    display: "grid",
    gridTemplateColumns: {
      default: "76px minmax(0, 1fr)",
      [pageLayout.phoneMedia]: "minmax(0, 1fr)",
    },
    alignItems: "start",
    gap: {
      default: spacingVars["--spacing-3"],
      [pageLayout.phoneMedia]: spacingVars["--spacing-1-5"],
    },
    paddingBlock: "12px",
    borderBlockStartColor: awbrnVars.rule,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: {
      default: "1px",
      ":first-child": 0,
    },
  },
  groupHead: {
    flexDirection: {
      default: "column",
      [pageLayout.phoneMedia]: "row",
    },
    alignItems: {
      default: "start",
      [pageLayout.phoneMedia]: "center",
    },
    gap: spacingVars["--spacing-1-5"],
    paddingBlockStart: {
      default: spacingVars["--spacing-3"],
      [pageLayout.phoneMedia]: 0,
    },
  },
  key: {
    borderStyle: "none",
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: {
      default: "transparent",
      ":hover": { default: null, [pageLayout.hoverMedia]: awbrnVars.paper },
      ":active": awbrnVars.paper,
    },
    color: colorVars["--color-text-primary"],
    font: "inherit",
    outline: {
      default: null,
      ":focus-visible": `2px solid ${focusVars["--focus-outline-color"]}`,
    },
    outlineOffset: { default: null, ":focus-visible": "2px" },
    cursor: "pointer",
  },
  // A hit area past its outline on a phone, where the box stays 34px.
  groupKey: {
    position: "relative",
    display: "inline-flex",
    alignItems: "center",
    minBlockSize: "34px",
    paddingBlock: "5px",
    paddingInline: "10px",
    borderRadius: "6px",
    boxShadow: `inset 0 0 0 1px ${awbrnVars.rule}`,
    backgroundColor: {
      default: "transparent",
      ":hover": { default: null, [pageLayout.hoverMedia]: awbrnVars.paperWell },
      ":active": awbrnVars.paperWell,
    },
    fontFamily: awbrnVars.fontUi,
    fontSize: "22px",
    fontWeight: 820,
    fontStretch: "62%",
    lineHeight: 1,
    "::before": {
      content: '""',
      position: "absolute",
      inset: {
        default: "0",
        [pageLayout.phoneMedia]: "-5px -4px",
      },
    },
  },
  facility: {
    color: colorVars["--color-text-secondary"],
    fontSize: typeScaleVars["--text-heading-5-size"],
    fontWeight: typeScaleVars["--text-heading-5-weight"],
  },
  cells: {
    display: {
      default: "flex",
      [pageLayout.phoneMedia]: "grid",
    },
    flexWrap: "wrap",
    gridTemplateColumns: {
      default: null,
      [pageLayout.phoneMedia]: "repeat(4, minmax(0, 1fr))",
    },
    gap: "4px",
    rowGap: "10px",
  },
  cell: {
    position: "relative",
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    gap: "4px",
    inlineSize: {
      default: "80px",
      [pageLayout.phoneMedia]: "auto",
    },
    minInlineSize: 0,
    paddingBlock: "12px 6px",
    paddingInline: 0,
    borderRadius: "6px",
  },
  cellLab: {
    backgroundColor: {
      default: awbrnVars.paperWell,
      ":hover": awbrnVars.paperWell,
    },
  },
  art: {
    position: "relative",
    display: "grid",
    placeItems: "center",
    inlineSize: "48px",
    blockSize: "40px",
  },
  spriteBanned: {
    filter: "grayscale(1)",
    opacity: 0.7,
  },
  waves: {
    position: "absolute",
    inset: 0,
    margin: "auto",
    inlineSize: "44px",
    blockSize: "20px",
    fill: "none",
    stroke: awbrnVars.violet,
    strokeWidth: 2,
    strokeLinecap: "round",
  },
  cellName: {
    maxInlineSize: "100%",
    overflow: "hidden",
    fontSize: awbrnVars.text12_5,
    fontWeight: 700,
    lineHeight: 1.3,
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  cellNameBanned: {
    color: awbrnVars.textTertiary,
    textDecoration: "line-through",
    textDecorationColor: awbrnVars.violet,
  },
  cellTag: {
    position: "absolute",
    insetBlockStart: "-7px",
    insetInlineEnd: "-3px",
    blockSize: "18px",
    fontSize: awbrnVars.text11_5,
    boxShadow: `inset 0 0 0 1px ${awbrnVars.ruleStrong}, 0 0 0 2px ${awbrnVars.sheet}`,
  },
  field: {
    display: "grid",
    gap: "6px",
  },
  fieldLabel: {
    fontSize: awbrnVars.text14,
    fontWeight: 650,
  },
  well: {
    display: "grid",
    gridTemplateColumns: {
      default: "44px minmax(0, 1fr) 40px auto",
      [pageLayout.phoneMedia]: "42px minmax(0, 1fr) 36px auto",
    },
    columnGap: "10px",
    rowGap: "4px",
    paddingBlock: "8px",
    paddingInline: "12px 10px",
    borderRadius: "6px",
    backgroundColor: awbrnVars.paper,
  },
  row: {
    display: "grid",
    gridColumn: "1 / -1",
    gridTemplateColumns: "subgrid",
    alignItems: "center",
    minBlockSize: "40px",
  },
  rowNoted: {
    paddingBlock: "4px",
  },
  rowLabel: {
    color: colorVars["--color-text-secondary"],
    fontSize: awbrnVars.text13_5,
    fontWeight: 650,
  },
  rowBody: {
    display: "grid",
    gap: "6px",
    minInlineSize: 0,
  },
  items: {
    display: "flex",
    alignItems: "center",
    gap: "6px",
    minInlineSize: 0,
    overflow: "hidden",
  },
  itemsTight: {
    gap: "4px",
  },
  itemsOpen: {
    flexWrap: "wrap",
  },
  note: {
    color: colorVars["--color-text-secondary"],
    fontSize: awbrnVars.text13_5,
    fontWeight: 650,
    textAlign: "center",
  },
  keyCell: {
    display: "flex",
    justifyContent: "flex-end",
  },
  editCell: {
    display: "flex",
    justifyContent: "flex-end",
  },
  // On a phone the box stays 36px and its hit area reaches 44px.
  rowKey: {
    position: "relative",
    display: "inline-grid",
    placeItems: "center",
    inlineSize: {
      default: "40px",
      [pageLayout.phoneMedia]: "36px",
    },
    blockSize: {
      default: "32px",
      [pageLayout.phoneMedia]: "36px",
    },
    padding: 0,
    borderColor: awbrnVars.ruleStrong,
    borderRadius: "6px",
    borderStyle: "solid",
    borderWidth: "1px",
    backgroundColor: awbrnVars.sheet,
    color: colorVars["--color-text-primary"],
    fontSize: awbrnVars.text13,
    fontWeight: 650,
    cursor: "pointer",
    "::before": {
      content: '""',
      position: "absolute",
      inset: {
        default: 0,
        [pageLayout.phoneMedia]: "-4px",
      },
    },
  },
  named: {
    display: "grid",
    gridColumn: "1 / -1",
    gridTemplateColumns: "repeat(auto-fill, minmax(124px, 1fr))",
    gap: "2px 10px",
    margin: 0,
    padding: 0,
    listStyle: "none",
  },
  namedItem: {
    display: "flex",
    alignItems: "center",
    gap: "8px",
    minBlockSize: "40px",
    minInlineSize: 0,
    fontSize: awbrnVars.text14,
    fontWeight: 650,
  },
  namedName: {
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
});
