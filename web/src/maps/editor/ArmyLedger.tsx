import { HStack, Stack } from "@astryxdesign/core/Stack";
import { colorVars, radiusVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useLayoutEffect, useRef, useState, type RefObject } from "react";
import { terrainTileStyle } from "#/components/game_sprites.ts";
import { FactionLogo } from "#/components/FactionLogo.tsx";
import { getFactionByCode } from "#/factions.ts";
import { emptySeats, funds, type EditorProblem } from "#/maps/map_editor.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { factionColorVar } from "#/ui/faction_colors.ts";
import { figureStyles } from "#/ui/Figure.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import type { EditorArmy, EditorStateChanged } from "#/wasm/awbrn_wasm.js";

const ROW = 40;
const MIN_ROWS = 2;

export function ArmyLedger({
  chosen,
  focused,
  focusedArmy,
  isPristine,
  neutralCity,
  onChoose,
  onLoadBase,
  onLoadHq,
  onStepHqs,
  railRef,
  state,
}: {
  chosen: string;
  focused: EditorProblem | null;
  /** The army kept in view before the chosen one. */
  focusedArmy: string | null;
  isPristine: boolean;
  /** The neutral city's sprite index. */
  neutralCity: number | undefined;
  onChoose: (faction: string) => void;
  onLoadBase: (faction: string) => void;
  onLoadHq: (faction: string) => void;
  onStepHqs: (faction: string) => void;
  railRef: RefObject<HTMLElement | null>;
  state: EditorStateChanged;
}) {
  const empty = isPristine ? [] : emptySeats(state);
  const armies: (EditorArmy | string)[] = isPristine ? [...state.roster] : [...state.armies];
  const rowCount = armies.length + (empty.length > 0 ? 1 : 0);
  const sheetRef = useRef<HTMLElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const rows = useWholeRows(railRef, sheetRef, scrollRef, rowCount);
  const seat = (faction: string) => state.roster.indexOf(faction) + 1;
  const neutralIncome =
    state.boardIncome - state.armies.reduce((sum, army) => sum + army.income, 0);

  // In view, as far as the rows allow: the focused problem's army, then the
  // folded empty seats, then the chosen army.
  useLayoutEffect(() => {
    const scroller = scrollRef.current;
    if (!scroller || rows === null) return;
    const rowOf = (faction: string | null) => {
      if (faction === null) return -1;
      const at = armies.findIndex(
        (army) => (typeof army === "string" ? army : army.faction) === faction,
      );
      return at !== -1 ? at : empty.includes(faction) ? armies.length : -1;
    };
    const wanted = [rowOf(focusedArmy), empty.length > 0 ? armies.length : -1, rowOf(chosen)];
    let first = -1;
    let last = -1;
    for (const row of wanted) {
      if (row === -1) continue;
      const low = first === -1 ? row : Math.min(first, row);
      const high = Math.max(last, row);
      if (high - low >= rows) continue;
      first = low;
      last = high;
    }
    if (first !== -1) scroller.scrollTop = Math.max(0, last - rows + 1) * ROW;
    // Only a new army to keep in view, or a new height, moves the list.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chosen, focusedArmy, rows, rowCount]);

  const acted = (faction: string, kinds: readonly EditorProblem["kind"][]) =>
    focused !== null &&
    kinds.includes(focused.kind) &&
    "faction" in focused &&
    focused.faction === faction;

  return (
    <section aria-label="Armies" ref={sheetRef} {...stylex.props(styles.sheet)}>
      <Stack xstyle={[styles.grid, styles.head]}>
        <span />
        <span />
        <span>HQ</span>
        <span>BUILD</span>
        <span>UNITS</span>
        <span>INCOME</span>
      </Stack>
      <div
        ref={scrollRef}
        {...stylex.props(styles.scroll, rows !== null && styles.rows(rows * ROW))}
      >
        {armies.map((army) => {
          const faction = typeof army === "string" ? army : army.faction;
          const name = getFactionByCode(faction)?.displayName ?? faction;
          const n = seat(faction);
          return (
            <div
              key={faction}
              onClick={() => onChoose(faction)}
              {...stylex.props(styles.grid, styles.row, faction === chosen && styles.rowOn)}
              style={{ backgroundColor: factionColorVar(faction, "wash") }}
            >
              <span
                {...stylex.props(
                  figureStyles.figure,
                  styles.seat,
                  faction === chosen && styles.seatOn,
                )}
              >
                {n > 0 ? n : ""}
              </span>
              <button
                aria-pressed={faction === chosen}
                title={n > 0 ? `Paint as ${name} · Shift+${n}` : `Paint as ${name}`}
                type="button"
                {...stylex.props(styles.who)}
              >
                <FactionLogo factionCode={faction} isLabelHidden size={14} />
                <span {...stylex.props(styles.ellipsis)}>{name}</span>
              </button>
              {typeof army === "string" ? (
                <>
                  <Dash />
                  <Dash />
                  <Dash />
                  <Dash />
                </>
              ) : (
                <>
                  {army.hqAt.length === 1 ? (
                    <Figure value={1} />
                  ) : (
                    <WarnCell
                      isActed={acted(faction, ["no-hq", "extra-hq"])}
                      isDashed={army.hqAt.length === 0}
                      label={
                        army.hqAt.length === 0
                          ? `${name}: no HQ. Load the HQ`
                          : `${name}: ${army.hqAt.length} HQs. Step through them`
                      }
                      onPress={() =>
                        army.hqAt.length === 0 ? onLoadHq(faction) : onStepHqs(faction)
                      }
                      value={army.hqAt.length}
                    />
                  )}
                  {army.production > 0 ? (
                    <Figure value={army.production} />
                  ) : (
                    <WarnCell
                      isActed={acted(faction, ["no-build"])}
                      isDashed
                      label={`${name} can't build. Load the Base`}
                      onPress={() => onLoadBase(faction)}
                      value={0}
                    />
                  )}
                  <Figure value={army.units} />
                  <span {...stylex.props(figureStyles.figure, styles.figure, styles.income)}>
                    {funds(army.income)}
                  </span>
                </>
              )}
            </div>
          );
        })}
        {empty.length > 0 ? (
          <Stack xstyle={[styles.grid, styles.row]}>
            <span />
            <HStack align="center" gap={1} xstyle={styles.emptySeats}>
              {empty.map((faction) => {
                const name = getFactionByCode(faction)?.displayName ?? faction;
                return (
                  <button
                    aria-label={`Paint as ${name}`}
                    aria-pressed={faction === chosen}
                    key={faction}
                    onClick={() => onChoose(faction)}
                    title={`Paint as ${name} · Shift+${seat(faction)}`}
                    type="button"
                    {...stylex.props(styles.crest, faction === chosen && styles.crestOn)}
                  >
                    <FactionLogo factionCode={faction} isLabelHidden size={14} />
                  </button>
                );
              })}
              <span>{empty.length} empty</span>
            </HStack>
            <Dash />
            <Dash />
            <Dash />
            <Dash />
          </Stack>
        ) : null}
      </div>
      <Stack xstyle={[styles.grid, styles.neutral]}>
        <span />
        <HStack align="center" gap={2}>
          {neutralCity === undefined ? null : (
            <span aria-hidden="true" style={terrainTileStyle(neutralCity)} />
          )}
          Neutral
          {!isPristine && state.neutralProperties > 0 ? (
            <span {...stylex.props(figureStyles.figure, styles.neutralCount)}>
              {state.neutralProperties}
            </span>
          ) : null}
        </HStack>
        <span />
        <span />
        <span />
        {isPristine || state.neutralProperties === 0 ? (
          <Dash />
        ) : (
          <span {...stylex.props(figureStyles.figure, styles.figure, styles.income)}>
            {funds(neutralIncome)}
          </span>
        )}
      </Stack>
    </section>
  );
}

/**
 * How many whole rows the ledger shows: what the rail leaves once its other
 * sheets take their natural height, at least two.
 */
function useWholeRows(
  railRef: RefObject<HTMLElement | null>,
  sheetRef: RefObject<HTMLElement | null>,
  scrollRef: RefObject<HTMLElement | null>,
  rowCount: number,
): number | null {
  const [rows, setRows] = useState<number | null>(null);
  useLayoutEffect(() => {
    const rail = railRef.current;
    const sheet = sheetRef.current;
    const scroller = scrollRef.current;
    if (!rail || !sheet || !scroller) return;
    const measure = () => {
      const gap = Number.parseFloat(getComputedStyle(rail).rowGap) || 0;
      const siblings = [...rail.children].filter(
        (child): child is HTMLElement => child instanceof HTMLElement && child !== sheet,
      );
      const taken = siblings.reduce(
        (sum, child) => sum + (child.dataset.spacer === undefined ? naturalHeight(child) : 0),
        gap * siblings.length,
      );
      const chrome = sheet.offsetHeight - scroller.offsetHeight;
      const fit = Math.floor((rail.clientHeight - taken - chrome) / ROW);
      const next = Math.min(rowCount, Math.max(MIN_ROWS, fit));
      setRows(next >= rowCount ? null : next);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(rail);
    for (const child of rail.children) observer.observe(child);
    return () => observer.disconnect();
  }, [railRef, sheetRef, scrollRef, rowCount]);
  return rows;
}

/** A sheet's height before any part of it scrolls. */
function naturalHeight(element: HTMLElement): number {
  let height = element.offsetHeight;
  for (const child of element.children) height += child.scrollHeight - child.clientHeight;
  return height;
}

function Figure({ value }: { value: number }) {
  return <span {...stylex.props(figureStyles.figure, styles.figure)}>{value}</span>;
}

function Dash() {
  return <span {...stylex.props(figureStyles.figure, styles.figure, styles.dim)}>–</span>;
}

function WarnCell({
  isActed,
  isDashed,
  label,
  onPress,
  value,
}: {
  isActed: boolean;
  isDashed: boolean;
  label: string;
  onPress: () => void;
  value: number;
}) {
  return (
    <button
      aria-label={label}
      onClick={(event) => {
        event.stopPropagation();
        onPress();
      }}
      title={label}
      type="button"
      {...stylex.props(
        figureStyles.figure,
        styles.figure,
        styles.warn,
        isDashed && styles.warnDashed,
        isActed && styles.warnActed,
      )}
    >
      {value}
    </button>
  );
}

const styles = stylex.create({
  sheet: {
    display: "flex",
    flexDirection: "column",
    flexShrink: 0,
    paddingBlock: "4px 2px",
    overflow: "hidden",
    borderRadius: "10px",
    backgroundColor: awbrnVars.sheet,
    boxShadow: `${awbrnVars.shadowSheet}, 0 0 0 1px ${awbrnVars.rule}`,
  },
  grid: {
    display: "grid",
    gridTemplateColumns: "14px minmax(0, 1fr) 28px 40px 38px 50px",
    alignItems: "center",
    columnGap: "6px",
    marginInline: "6px",
    paddingInline: "6px 8px",
  },
  head: {
    blockSize: "26px",
    color: awbrnVars.textTertiary,
    fontSize: awbrnVars.textMicro,
    fontWeight: 650,
    letterSpacing: "0.1em",
    lineHeight: 1,
    textAlign: "end",
  },
  // The list snaps to whole rows and keeps what matters in view, so it
  // scrolls by wheel without a bar.
  scroll: {
    flexShrink: 0,
    overflowY: "auto",
    scrollbarWidth: "none",
  },
  rows: (height: number) => ({
    blockSize: `${height}px`,
    boxShadow: `inset 0 -1px 0 ${awbrnVars.rule}`,
  }),
  row: {
    position: "relative",
    blockSize: "40px",
    borderRadius: radiusVars["--radius-element"],
    cursor: "pointer",
  },
  rowOn: {
    boxShadow: `inset 0 0 0 2px ${awbrnVars.ink900}`,
  },
  seat: {
    color: awbrnVars.textTertiary,
    fontSize: awbrnVars.text12,
    textAlign: "end",
  },
  seatOn: {
    color: colorVars["--color-text-primary"],
    fontWeight: 700,
  },
  who: {
    display: "flex",
    alignItems: "center",
    gap: "8px",
    minInlineSize: 0,
    padding: 0,
    borderStyle: "none",
    backgroundColor: "transparent",
    color: colorVars["--color-text-primary"],
    cursor: "pointer",
    fontFamily: awbrnVars.fontUi,
    fontSize: awbrnVars.text15,
    fontWeight: 650,
    textAlign: "start",
  },
  ellipsis: {
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  figure: {
    justifySelf: "end",
    color: colorVars["--color-text-primary"],
    fontSize: awbrnVars.text13_5,
    fontWeight: 600,
    textAlign: "end",
  },
  income: {
    color: colorVars["--color-text-secondary"],
  },
  dim: {
    color: awbrnVars.textTertiary,
  },
  warn: {
    paddingBlock: "3px",
    paddingInline: "6px",
    borderStyle: "none",
    borderRadius: "4px",
    backgroundColor: "transparent",
    boxShadow: `inset 0 0 0 1.5px ${awbrnVars.warnFill}`,
    color: awbrnVars.warn,
    cursor: "pointer",
  },
  warnDashed: {
    boxShadow: "none",
    outline: `1.5px dashed ${awbrnVars.warn}`,
    outlineOffset: "-1.5px",
  },
  warnActed: {
    outline: `2px solid ${awbrnVars.blue}`,
    outlineOffset: "2px",
  },
  emptySeats: {
    color: colorVars["--color-text-secondary"],
    fontSize: awbrnVars.text15,
    fontWeight: 650,
  },
  crest: {
    display: "grid",
    placeItems: "center",
    inlineSize: "24px",
    blockSize: "24px",
    padding: 0,
    borderStyle: "none",
    borderRadius: "4px",
    backgroundColor: {
      default: "transparent",
      ":hover": { default: null, [pageLayout.hoverMedia]: awbrnVars.paperWell },
    },
    cursor: "pointer",
  },
  crestOn: {
    backgroundColor: awbrnVars.sheet,
    boxShadow: `inset 0 0 0 2px ${awbrnVars.ink900}`,
  },
  neutral: {
    blockSize: "32px",
    color: colorVars["--color-text-secondary"],
    fontSize: awbrnVars.text14,
    fontWeight: 600,
  },
  neutralCount: {
    color: awbrnVars.textTertiary,
    fontSize: awbrnVars.text12_5,
  },
});
