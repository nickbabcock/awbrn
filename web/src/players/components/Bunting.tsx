import { HStack, Stack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { VisuallyHidden } from "@astryxdesign/core/VisuallyHidden";
import {
  borderVars,
  focusVars,
  radiusVars,
  spacingVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import {
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type PointerEvent,
  type Ref,
} from "react";
import { formatRating, rankedPoolCopy } from "#/matchmaking/ranked_display.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Figure, figureStyles } from "#/ui/Figure.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import {
  markSummary,
  nearestMark,
  type BuntingLayout,
  type BuntingMark,
  type BuntingRow,
  type FlagBand,
} from "../bunting.ts";
import { buntingSizes } from "./bunting.stylex.ts";

/* The plot's sizes are the design's fixed pixels: `buntingSizes`, and the flag widths here. */

/** Flag widths in px: desktop, then phone, each normal and dense. */
const FLAG_WIDTH = { desktop: [28, 24], phone: [20, 18] } as const;
/** A three-digit place widens its flag so the numeral clears the hoist and the notch. */
const WIDE_EXTRA = { desktop: 4, phone: 3 } as const;
/** Flag length in units of a 22-unit-wide flag: the place band. */
const BAND_LENGTH: Record<FlagBand, number> = { pod: 40, mid: 30, low: 20 };
/** How deep a provisional flag's swallowtail cuts into its length. */
const NOTCH_DEPTH: Record<FlagBand, number> = { pod: 8, mid: 7, low: 5 };

/** The flag outline in its own 22 × length box: pointed when settled, notched when provisional. */
function flagPath(band: FlagBand, isProvisional: boolean): string {
  const length = BAND_LENGTH[band];
  const end = length - 1;
  return isProvisional
    ? `M1 1H21V${end}L11 ${end - NOTCH_DEPTH[band]}L1 ${end}Z`
    : `M1 1H21V${end - 5}L11 ${end}L1 ${end - 5}Z`;
}

/** The rank graphic: a string per pool, a flag at each season's close. */
export function Bunting({ layout }: { layout: BuntingLayout }) {
  const density = layout.isDense ? 1 : 0;
  const isSlack = layout.rows.length > 0 && layout.rows.every((row) => row.isUnrated);
  const isScrolling = layout.scrollWidth > 1;

  return (
    <VStack gap={0} xstyle={styles.bunting}>
      <Stack xstyle={[styles.track, isScrolling && styles.scroller]}>
        {layout.rows.map((row) => (
          <BuntingRowView
            density={density}
            isScrolling={isScrolling}
            key={row.pool}
            layout={layout}
            row={row}
          />
        ))}
        <Stack
          aria-hidden="true"
          xstyle={[styles.axis, isScrolling && styles.scrollColumns(layout.scrollWidth)]}
        >
          <Stack as="span" xstyle={[styles.axisSpacer, isScrolling && styles.pinned]} />
          <Stack as="span" xstyle={[styles.axisLabels, isSlack && styles.slackWidth]}>
            {layout.axis.map((label) => (
              <Stack
                as="span"
                key={label.season}
                xstyle={[
                  figureStyles.figure,
                  styles.axisLabel,
                  styles.at(label.x),
                  label.isAlternate && styles.notPhone,
                ]}
              >
                S{label.season}
              </Stack>
            ))}
          </Stack>
        </Stack>
      </Stack>
      {layout.rows.map((row) => (
        <SeasonTable key={row.pool} row={row} />
      ))}
    </VStack>
  );
}

function BuntingRowView({
  density,
  isScrolling,
  layout,
  row,
}: {
  density: 0 | 1;
  isScrolling: boolean;
  layout: BuntingLayout;
  row: BuntingRow;
}) {
  const poolName = rankedPoolCopy(row.pool).name;
  const [selected, setSelected] = useState<number | null>(null);
  // A mouse hover shows its season but does not announce it.
  const [isHovered, setIsHovered] = useState(false);
  const plotRef = useRef<HTMLElement>(null);
  const selectedRef = useRef<HTMLElement>(null);
  const isStepped = useRef(false);
  const mark = selected === null ? null : (row.marks[selected] ?? null);

  // A key can select a season scrolled out of view; bring it back in.
  useEffect(() => {
    if (!isStepped.current) return;
    isStepped.current = false;
    if (isScrolling) selectedRef.current?.scrollIntoView({ block: "nearest", inline: "center" });
  }, [isScrolling, selected]);

  function pick(event: PointerEvent<HTMLElement>) {
    const box = plotRef.current?.getBoundingClientRect();
    if (!box || box.width === 0 || row.marks.length === 0) return;
    isStepped.current = false;
    setIsHovered(event.type === "pointermove" && event.pointerType === "mouse");
    setSelected(nearestMark(row.marks, ((event.clientX - box.left) / box.width) * 100));
  }

  function step(event: KeyboardEvent<HTMLElement>) {
    const last = row.marks.length - 1;
    const from = selected ?? last;
    const next =
      event.key === "ArrowLeft"
        ? Math.max(0, from - 1)
        : event.key === "ArrowRight"
          ? Math.min(last, from + 1)
          : event.key === "Home"
            ? 0
            : event.key === "End"
              ? last
              : null;
    if (event.key === "Escape") setSelected(null);
    if (next === null) return;
    event.preventDefault();
    isStepped.current = true;
    setIsHovered(false);
    setSelected(next);
  }

  return (
    <Stack xstyle={[styles.row, isScrolling && styles.scrollColumns(layout.scrollWidth)]}>
      <HStack as="span" xstyle={[styles.head, isScrolling && [styles.pinned, styles.pinnedHead]]}>
        <b {...stylex.props(styles.pool)}>{poolName}</b>
        <HStack as="span" xstyle={styles.stand}>
          {row.place !== null ? (
            <Figure size="lg">#{row.place}</Figure>
          ) : (
            <Text color="inherit" type="inherit" xstyle={styles.unranked}>
              Unranked
            </Text>
          )}
          {row.rating !== null ? (
            <Figure tone="secondary" xstyle={styles.rating}>
              {formatRating(row.rating, row.isProvisional)}
            </Figure>
          ) : null}
        </HStack>
      </HStack>
      <Stack
        as="span"
        aria-label={`${poolName} by season`}
        onBlur={() => setSelected(null)}
        onFocus={() => {
          setIsHovered(false);
          setSelected((current) => current ?? row.marks.length - 1);
        }}
        onKeyDown={step}
        onPointerDown={(event) => {
          if (event.pointerType !== "mouse") event.currentTarget.setPointerCapture(event.pointerId);
          pick(event);
        }}
        onPointerLeave={(event) => {
          if (event.pointerType === "mouse" && document.activeElement !== event.currentTarget) {
            setSelected(null);
          }
        }}
        onPointerMove={(event) => {
          if (event.pointerType === "mouse" || event.buttons > 0) pick(event);
        }}
        role="group"
        tabIndex={0}
        xstyle={[
          styles.plot,
          row.isUnrated && styles.plotUnrated,
          isScrolling && styles.plotScrolls,
        ]}
      >
        <Stack as="span" ref={plotRef} xstyle={[styles.inner, row.isUnrated && styles.innerSlack]}>
          {layout.washFrom !== null && !row.isUnrated ? (
            <Stack as="span" xstyle={[styles.wash, washStart(layout.washFrom, density)]} />
          ) : null}
          <svg
            aria-hidden="true"
            preserveAspectRatio="none"
            viewBox="0 0 1000 100"
            {...stylex.props(styles.svg)}
          >
            {row.aheadPath ? <path d={row.aheadPath} {...stylex.props(styles.ahead)} /> : null}
            <path
              d={row.stringPath}
              {...stylex.props(row.isUnrated ? styles.slack : styles.string)}
            />
          </svg>
          <Stack as="span" xstyle={[styles.pin, styles.at(row.pin.x, row.pin.y)]} />
          {row.peak ? (
            <Stack as="span" xstyle={[styles.peak, styles.at(row.peak.x, row.peak.y)]}>
              {/* Its own season's chip says the season; the figure would sit under it. */}
              {mark?.season !== row.peak.season ? (
                <Stack as="span" xstyle={[figureStyles.figure, styles.peakFigure]}>
                  {row.peak.rating}
                </Stack>
              ) : null}
            </Stack>
          ) : null}
          {row.marks.map((entry, index) => (
            <Mark
              density={density}
              isSelected={index === selected}
              key={entry.season}
              mark={entry}
              markRef={index === selected ? selectedRef : undefined}
            />
          ))}
          {mark ? <SeasonChip mark={mark} /> : null}
        </Stack>
        <VisuallyHidden aria-live="polite">
          {mark && !isHovered ? `Season ${mark.season}: ${markSummary(mark, formatRating)}` : ""}
        </VisuallyHidden>
      </Stack>
    </Stack>
  );
}

function Mark({
  density,
  isSelected,
  mark,
  markRef,
}: {
  density: 0 | 1;
  isSelected: boolean;
  mark: BuntingMark;
  markRef?: Ref<HTMLElement>;
}) {
  if (mark.kind === "hook") {
    return (
      <Stack
        as="span"
        ref={markRef}
        xstyle={[styles.hook, styles.at(mark.x, mark.y), isSelected && styles.selected]}
      />
    );
  }

  const isWide = mark.place !== null && mark.place >= 100;
  const desktop = FLAG_WIDTH.desktop[density] + (isWide ? WIDE_EXTRA.desktop : 0);
  const phone = FLAG_WIDTH.phone[density] + (isWide ? WIDE_EXTRA.phone : 0);
  const length = BAND_LENGTH[mark.band];
  // The numeral box, in flag units: under the hoist, clear of a swallowtail's notch.
  const numeralTop = mark.isProvisional ? 2.5 : 3;
  const numeralHeight = mark.isProvisional
    ? Math.min(13, length - 1 - NOTCH_DEPTH[mark.band] - numeralTop)
    : 14;
  const tone = mark.kind === "unraised" ? "unraised" : mark.isGold ? "gold" : mark.band;

  return (
    <Stack
      as="span"
      ref={markRef}
      xstyle={[
        styles.flag,
        mark.band !== "low" && mark.kind === "flag" && styles.flagShadow,
        styles.at(mark.x, mark.y),
        styles.flagSize(desktop, phone, length / 22),
        mark.isOpen && styles.batten,
        isSelected && styles.selected,
      ]}
    >
      <svg aria-hidden="true" viewBox={`0 0 22 ${length}`} {...stylex.props(styles.flagSvg)}>
        <path d={flagPath(mark.band, mark.isProvisional)} {...stylex.props(fills[tone])} />
      </svg>
      {mark.kind === "flag" ? (
        <Stack
          as="span"
          xstyle={[
            styles.numeral,
            numerals[tone],
            styles.numeralBox(
              (desktop * numeralTop) / 22,
              (phone * numeralTop) / 22,
              (desktop * numeralHeight) / 22,
              (phone * numeralHeight) / 22,
            ),
            density === 1 && styles.numeralDense,
            isWide && styles.numeralWide,
          ]}
        >
          {mark.place}
        </Stack>
      ) : null}
    </Stack>
  );
}

/** The selected season, floating above the plot; it holds to the plot's edge near either end. */
function SeasonChip({ mark }: { mark: BuntingMark }) {
  const anchor = mark.x < 28 ? styles.chipStart : mark.x > 72 ? styles.chipEnd : styles.chipCenter;
  return (
    <Stack as="span" aria-hidden="true" xstyle={[styles.chip, styles.at(mark.x), anchor]}>
      <Text color="inherit" type="inherit" xstyle={styles.chipSeason}>
        Season {mark.season}
      </Text>
      <Figure xstyle={styles.chipFigures}>{markSummary(mark, formatRating)}</Figure>
    </Stack>
  );
}

/** What the plot draws, as a table for a screen reader. */
function SeasonTable({ row }: { row: BuntingRow }) {
  return (
    <VisuallyHidden>
      <table>
        <caption>{rankedPoolCopy(row.pool).name}</caption>
        <thead>
          <tr>
            <th scope="col">Season</th>
            <th scope="col">Place</th>
            <th scope="col">Rating</th>
            <th scope="col">Games</th>
          </tr>
        </thead>
        <tbody>
          {row.marks.map((mark) => (
            <tr key={mark.season}>
              <th scope="row">
                Season {mark.season}
                {mark.isOpen ? " (now)" : ""}
              </th>
              <td>
                {mark.place !== null ? `#${mark.place}` : mark.games > 0 ? "No place" : "No games"}
              </td>
              <td>{mark.rating !== null ? formatRating(mark.rating, mark.isProvisional) : ""}</td>
              <td>{mark.games}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </VisuallyHidden>
  );
}

/** The open season's wash starts just past the last closed season's flag. */
function washStart(x: number, density: 0 | 1) {
  return styles.washFrom(
    `calc(${x}% + ${FLAG_WIDTH.desktop[density] / 2 + 4}px)`,
    `calc(${x}% + ${FLAG_WIDTH.phone[density] / 2 + 4}px)`,
  );
}

const inset = {
  default: buntingSizes.plotInset,
  [pageLayout.phoneMedia]: buntingSizes.plotInsetPhone,
} as const;
const outset = {
  default: `calc(-1 * ${buntingSizes.plotInset})`,
  [pageLayout.phoneMedia]: `calc(-1 * ${buntingSizes.plotInsetPhone})`,
} as const;

const styles = stylex.create({
  bunting: {
    display: "grid",
    paddingBlockEnd: {
      default: awbrnVars.spacing3_5,
      [pageLayout.phoneMedia]: spacingVars["--spacing-3"],
    },
    paddingInline: {
      default: spacingVars["--spacing-5"],
      [pageLayout.phoneMedia]: spacingVars["--spacing-4"],
    },
  },
  track: {
    display: "grid",
  },
  // Past twelve seasons the rows scroll together under their pinned heads. A
  // right-to-left scroller opens at its end, on the current season, before
  // any script runs; the rows in it read left to right (`scrollColumns`).
  scroller: {
    direction: "rtl",
    overflowX: "auto",
    overflowY: "hidden",
    containerType: "inline-size",
    paddingBlockStart: buntingSizes.chipRoom,
    marginBlockStart: `calc(-1 * ${buntingSizes.chipRoom})`,
  },
  /** `width`: the plot over the width in view (`BuntingLayout.scrollWidth`). */
  scrollColumns: (width: number) => ({
    direction: "ltr",
    gridTemplateColumns: {
      default: `${buntingSizes.headWidth} calc((100cqi - ${buntingSizes.headWidth} - ${spacingVars["--spacing-4"]}) * ${width})`,
      [pageLayout.phoneMedia]: `calc(100cqi * ${width})`,
    },
  }),
  // Pinned at the start; on a desktop it also covers the gap beside it.
  pinned: {
    position: "sticky",
    insetInlineStart: 0,
    zIndex: 3,
    alignSelf: "stretch",
    marginInlineEnd: {
      default: `calc(-1 * ${spacingVars["--spacing-4"]})`,
      [pageLayout.phoneMedia]: 0,
    },
    paddingInlineEnd: {
      default: spacingVars["--spacing-4"],
      [pageLayout.phoneMedia]: 0,
    },
    backgroundColor: awbrnVars.sheet,
  },
  // A phone's head holds the width in view, so its figure stays at the right.
  pinnedHead: {
    alignContent: "center",
    inlineSize: {
      default: null,
      [pageLayout.phoneMedia]: "100cqi",
    },
  },
  row: {
    display: "grid",
    gridTemplateColumns: {
      default: `${buntingSizes.headWidth} minmax(0, 1fr)`,
      [pageLayout.phoneMedia]: "minmax(0, 1fr)",
    },
    alignItems: "center",
    gap: {
      default: spacingVars["--spacing-4"],
      [pageLayout.phoneMedia]: spacingVars["--spacing-1"],
    },
    paddingBlock: {
      default: spacingVars["--spacing-2"],
      [pageLayout.phoneMedia]: `${spacingVars["--spacing-2"]} ${spacingVars["--spacing-1"]}`,
    },
    borderBlockStartColor: awbrnVars.rule,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: borderVars["--border-width"],
  },
  head: {
    display: {
      default: "grid",
      [pageLayout.phoneMedia]: "flex",
    },
    alignItems: "baseline",
    justifyContent: "space-between",
    gap: {
      default: spacingVars["--spacing-1-5"],
      [pageLayout.phoneMedia]: spacingVars["--spacing-3"],
    },
    minInlineSize: 0,
  },
  pool: {
    fontSize: awbrnVars.text15,
    fontWeight: 700,
    lineHeight: 1.2,
  },
  stand: {
    display: "flex",
    alignItems: "baseline",
    gap: spacingVars["--spacing-2"],
    whiteSpace: "nowrap",
  },
  rating: {
    fontSize: awbrnVars.text13,
    lineHeight: 1,
  },
  unranked: {
    fontSize: awbrnVars.text15,
    fontWeight: 650,
  },
  plot: {
    position: "relative",
    display: "block",
    blockSize: {
      default: buntingSizes.plotHeight,
      [pageLayout.phoneMedia]: buntingSizes.plotHeightPhone,
    },
    borderRadius: radiusVars["--radius-inner"],
    // A sideways drag picks seasons; an upward one still scrolls the page.
    touchAction: "pan-y",
    outlineColor: awbrnVars.blue,
    outlineOffset: focusVars["--focus-outline-offset"],
    outlineStyle: { default: "none", ":focus-visible": "solid" },
    outlineWidth: focusVars["--focus-outline-width"],
  },
  // A sideways drag scrolls a plot too long to show whole; a tap still picks.
  plotScrolls: {
    touchAction: "pan-x pan-y",
  },
  plotUnrated: {
    blockSize: {
      default: buntingSizes.unratedHeight,
      [pageLayout.phoneMedia]: buntingSizes.unratedHeightPhone,
    },
  },
  inner: {
    position: "absolute",
    insetBlock: 0,
    insetInline: inset,
  },
  // A slack string hangs 1.5 seasons wide: beside the head on a desktop, at
  // the right under the head's figure on a phone.
  innerSlack: {
    insetInlineStart: {
      default: buntingSizes.plotInset,
      [pageLayout.phoneMedia]: "auto",
    },
    insetInlineEnd: {
      default: "auto",
      [pageLayout.phoneMedia]: buntingSizes.plotInsetPhone,
    },
    inlineSize: {
      default: `min(calc(100% - 2 * ${buntingSizes.plotInset}), ${buntingSizes.unratedWidth})`,
      [pageLayout.phoneMedia]: `min(calc(100% - 2 * ${buntingSizes.plotInsetPhone}), ${buntingSizes.unratedWidth})`,
    },
  },
  wash: {
    position: "absolute",
    insetBlock: buntingSizes.washInset,
    insetInlineEnd: outset,
    borderRadius: radiusVars["--radius-inner"],
    backgroundColor: awbrnVars.blueWash,
  },
  washFrom: (desktop: string, phone: string) => ({
    insetInlineStart: {
      default: desktop,
      [pageLayout.phoneMedia]: phone,
    },
  }),
  svg: {
    position: "absolute",
    inset: 0,
    inlineSize: "100%",
    blockSize: "100%",
    overflow: "visible",
  },
  string: {
    fill: "none",
    stroke: awbrnVars.ink600,
    strokeWidth: 1.5,
    strokeLinejoin: "round",
    strokeLinecap: "round",
    vectorEffect: "non-scaling-stroke",
  },
  ahead: {
    fill: "none",
    stroke: awbrnVars.ink300,
    strokeWidth: 1.5,
    strokeDasharray: "1 4",
    strokeLinecap: "round",
    vectorEffect: "non-scaling-stroke",
  },
  slack: {
    fill: "none",
    stroke: awbrnVars.ink300,
    strokeWidth: 1.5,
    strokeDasharray: "3 3",
    vectorEffect: "non-scaling-stroke",
  },
  at: (x: number, y?: number) => ({
    insetInlineStart: `${x}%`,
    insetBlockStart: y === undefined ? 0 : `${y}%`,
  }),
  // The first rating, filled, so it never reads as the empty no-games hook.
  pin: {
    position: "absolute",
    inlineSize: buntingSizes.mark,
    blockSize: buntingSizes.mark,
    borderRadius: "50%",
    backgroundColor: awbrnVars.ink600,
    transform: "translate(-50%, -50%)",
  },
  peak: {
    position: "absolute",
    zIndex: 1,
    display: {
      default: "block",
      [pageLayout.phoneMedia]: "none",
    },
    "::before": {
      content: '""',
      position: "absolute",
      insetInlineStart: `calc(${buntingSizes.mark} / -2)`,
      insetBlockEnd: buntingSizes.caretGap,
      inlineSize: buntingSizes.mark,
      blockSize: buntingSizes.caretHeight,
      backgroundColor: awbrnVars.ink900,
      clipPath: "polygon(0 0, 100% 0, 50% 100%)",
    },
  },
  peakFigure: {
    position: "absolute",
    insetInlineStart: 0,
    insetBlockEnd: buntingSizes.peakFigureLift,
    transform: "translateX(-50%)",
    color: awbrnVars.textTertiary,
    fontSize: awbrnVars.textMicro,
    fontWeight: 450,
    lineHeight: 1,
  },
  hook: {
    position: "absolute",
    inlineSize: buntingSizes.mark,
    blockSize: buntingSizes.mark,
    borderRadius: "50%",
    backgroundColor: awbrnVars.sheet,
    boxShadow: `inset 0 0 0 ${buntingSizes.stroke} ${awbrnVars.ink600}`,
    transform: `translate(-50%, ${buntingSizes.batten})`,
    "::before": {
      content: '""',
      position: "absolute",
      insetInlineStart: `calc((${buntingSizes.mark} - ${buntingSizes.stroke}) / 2)`,
      insetBlockStart: `calc(-1 * ${buntingSizes.batten})`,
      inlineSize: buntingSizes.stroke,
      blockSize: buntingSizes.batten,
      backgroundColor: awbrnVars.ink600,
    },
  },
  flag: {
    position: "absolute",
    transform: `translate(-50%, calc(-1 * ${buntingSizes.flagLift}))`,
  },
  flagShadow: {
    filter: awbrnVars.shadowFlag,
  },
  flagSize: (desktop: number, phone: number, ratio: number) => ({
    inlineSize: {
      default: `${desktop}px`,
      [pageLayout.phoneMedia]: `${phone}px`,
    },
    blockSize: {
      default: `${desktop * ratio}px`,
      [pageLayout.phoneMedia]: `${phone * ratio}px`,
    },
  }),
  flagSvg: {
    display: "block",
    inlineSize: "100%",
    blockSize: "100%",
    overflow: "visible",
  },
  // The open season's flag still flies: a blue batten across its hoist.
  batten: {
    "::before": {
      content: '""',
      position: "absolute",
      zIndex: 1,
      insetInline: `calc(-1 * ${buntingSizes.battenOverhang})`,
      insetBlockStart: `calc(-1 * ${buntingSizes.flagLift})`,
      blockSize: buntingSizes.batten,
      borderRadius: `calc(${buntingSizes.batten} / 2)`,
      backgroundColor: awbrnVars.blue,
    },
  },
  selected: {
    borderRadius: radiusVars["--radius-inner"],
    outlineColor: awbrnVars.blue,
    outlineOffset: buntingSizes.selectedOffset,
    outlineStyle: "solid",
    outlineWidth: focusVars["--focus-outline-width"],
  },
  numeral: {
    position: "absolute",
    insetInline: `calc(-1 * ${buntingSizes.numeralBleed})`,
    display: "grid",
    placeItems: "center",
    fontFamily: awbrnVars.fontUi,
    fontSize: awbrnVars.text14,
    fontStretch: "62%",
    fontVariantNumeric: "tabular-nums",
    fontWeight: 850,
    lineHeight: 1,
  },
  numeralBox: (desktopTop: number, phoneTop: number, desktop: number, phone: number) => ({
    insetBlockStart: {
      default: `${desktopTop}px`,
      [pageLayout.phoneMedia]: `${phoneTop}px`,
    },
    blockSize: {
      default: `${desktop}px`,
      [pageLayout.phoneMedia]: `${phone}px`,
    },
  }),
  numeralDense: {
    fontSize: {
      default: awbrnVars.text14,
      [pageLayout.phoneMedia]: awbrnVars.text13,
    },
  },
  numeralWide: {
    fontSize: awbrnVars.textMicro,
  },
  chip: {
    position: "absolute",
    zIndex: 2,
    display: "inline-grid",
    gap: buntingSizes.chipGap,
    paddingBlock: spacingVars["--spacing-2"],
    paddingInline: awbrnVars.spacing2_5,
    borderRadius: radiusVars["--radius-inner"],
    backgroundColor: awbrnVars.ink900,
    boxShadow: awbrnVars.shadowSheet,
    color: awbrnVars.textOnInkStrong,
    whiteSpace: "nowrap",
    pointerEvents: "none",
  },
  chipCenter: {
    transform: `translate(-50%, calc(-100% - ${buntingSizes.chipLift}))`,
  },
  chipStart: {
    transform: `translate(calc(-1 * ${buntingSizes.chipEdge}), calc(-100% - ${buntingSizes.chipLift}))`,
  },
  chipEnd: {
    transform: `translate(calc(-100% + ${buntingSizes.chipEdge}), calc(-100% - ${buntingSizes.chipLift}))`,
  },
  chipSeason: {
    fontSize: awbrnVars.text13,
    fontWeight: 650,
  },
  chipFigures: {
    color: awbrnVars.textOnInkSecondary,
    fontSize: awbrnVars.text12,
    fontWeight: 400,
  },
  axis: {
    display: "grid",
    gridTemplateColumns: {
      default: `${buntingSizes.headWidth} minmax(0, 1fr)`,
      [pageLayout.phoneMedia]: "minmax(0, 1fr)",
    },
    gap: spacingVars["--spacing-4"],
    paddingBlockStart: spacingVars["--spacing-2"],
    borderBlockStartColor: awbrnVars.rule,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: borderVars["--border-width"],
  },
  axisSpacer: {
    display: {
      default: "block",
      [pageLayout.phoneMedia]: "none",
    },
  },
  axisLabels: {
    position: "relative",
    display: "block",
    blockSize: buntingSizes.axisHeight,
    marginInline: inset,
  },
  slackWidth: {
    maxInlineSize: buntingSizes.unratedWidth,
    marginInlineStart: {
      default: buntingSizes.plotInset,
      [pageLayout.phoneMedia]: "auto",
    },
  },
  axisLabel: {
    position: "absolute",
    transform: "translateX(-50%)",
    color: awbrnVars.textTertiary,
    fontSize: awbrnVars.textMicro,
    lineHeight: 1,
  },
  notPhone: {
    display: {
      default: null,
      [pageLayout.phoneMedia]: "none",
    },
  },
});

const fills = stylex.create({
  pod: { fill: awbrnVars.ink900 },
  gold: { fill: awbrnVars.gold },
  mid: { fill: awbrnVars.ink300 },
  low: {
    fill: awbrnVars.ink200,
    stroke: awbrnVars.ink600,
    strokeWidth: 1.5,
    vectorEffect: "non-scaling-stroke",
  },
  unraised: {
    fill: awbrnVars.sheet,
    stroke: awbrnVars.ink300,
    strokeWidth: 1.5,
    strokeDasharray: "3 2",
    vectorEffect: "non-scaling-stroke",
  },
});

const numerals = stylex.create({
  pod: { color: awbrnVars.textOnInkStrong },
  gold: { color: awbrnVars.ink950 },
  mid: { color: awbrnVars.ink950 },
  low: { color: awbrnVars.ink800 },
  unraised: {},
});
