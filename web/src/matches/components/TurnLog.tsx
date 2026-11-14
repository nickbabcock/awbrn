import { Text } from "@astryxdesign/core/Text";
import { HStack, Stack } from "@astryxdesign/core/Stack";
import { Icon } from "@astryxdesign/core/Icon";
import {
  borderVars,
  focusVars,
  radiusVars,
  spacingVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useRef, useState } from "react";
import { FactionCrest } from "#/components/FactionCrest.tsx";
import { unitSpriteSize, unitSpriteStyle } from "#/components/game_sprites.ts";
import type { MatchArmy } from "#/matches/match_armies.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { figureStyles } from "#/ui/Figure.tsx";
import { IconBack, IconNext } from "#/ui/icons.tsx";
import { ModalSheet } from "#/ui/ModalSheet.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { actionDay, dayLines, logDays, type LogBoundary, type LogLine } from "./turn_log.ts";

/**
 * The turn log: the day's actions, one line each (number, sprite, verb,
 * coordinates), the moment being read lifted and the ones after it dimmed.
 * Every line is a review target. No head: the rail names the day elsewhere.
 */
export function TurnLog({
  armies,
  boundaries,
  currentIndex,
  isDisabled = false,
  onSeek,
}: {
  armies: readonly MatchArmy[];
  boundaries: readonly LogBoundary[];
  /** The boundary being read; the far end while the game is live. */
  currentIndex: number;
  isDisabled?: boolean;
  onSeek: (index: number) => void;
}) {
  const day = actionDay(boundaries, currentIndex);
  if (day === null) return null;
  const lines = dayLines(boundaries, day);
  if (lines.length === 0) return null;

  return (
    <section aria-label={`Day ${day}`} {...stylex.props(styles.log)}>
      <LogList
        armies={armies}
        currentIndex={currentIndex}
        isDisabled={isDisabled}
        lines={lines}
        onSeek={onSeek}
        tone="ink"
      />
    </section>
  );
}

/**
 * The log on a phone: a sheet over the board holding one day, `6 / 11` in its
 * head, and the days either side under it.
 */
export function TurnLogSheet({
  armies,
  boundaries,
  currentIndex,
  isDisabled = false,
  isOpen,
  onOpenChange,
  onSeek,
}: {
  armies: readonly MatchArmy[];
  boundaries: readonly LogBoundary[];
  currentIndex: number;
  isDisabled?: boolean;
  isOpen: boolean;
  onOpenChange: (isOpen: boolean) => void;
  onSeek: (index: number) => void;
}) {
  const currentDay = actionDay(boundaries, currentIndex);
  const [shownDay, setShownDay] = useState(currentDay);
  // The sheet opens on the day being read, and follows it while it is open.
  useEffect(() => {
    setShownDay(currentDay);
  }, [currentDay, isOpen]);

  const day = shownDay ?? currentDay;
  const days = logDays(boundaries);
  const at = day === null ? -1 : days.indexOf(day);
  const lines = day === null ? [] : dayLines(boundaries, day);
  const now = lines.find((line) => line.index === currentIndex) ?? null;
  const previousDay = at > 0 ? days[at - 1] : undefined;
  const nextDay = at >= 0 && at < days.length - 1 ? days[at + 1] : undefined;

  return (
    <ModalSheet
      footer={
        <Stack as="span" xstyle={styles.days}>
          <Button
            icon={<Icon icon={IconBack} />}
            isDisabled={previousDay === undefined}
            label={previousDay === undefined ? "Earlier day" : `Day ${previousDay}`}
            onClick={() => setShownDay(previousDay ?? null)}
            variant="secondary"
            xstyle={styles.dayKey}
          />
          <Button
            endContent={<Icon icon={IconNext} />}
            isDisabled={nextDay === undefined}
            label={nextDay === undefined ? "Later day" : `Day ${nextDay}`}
            onClick={() => setShownDay(nextDay ?? null)}
            variant="secondary"
            xstyle={styles.dayKey}
          />
        </Stack>
      }
      headMeta={now ? `${now.number} / ${lines.length}` : undefined}
      isOpen={isOpen}
      onOpenChange={onOpenChange}
      title={day === null ? "Log" : `Day ${day}`}
    >
      <LogList
        armies={armies}
        currentIndex={currentIndex}
        isDisabled={isDisabled}
        lines={lines}
        onSeek={onSeek}
        tone="paper"
      />
    </ModalSheet>
  );
}

/**
 * The moment being read, as the phone replay's action line says it:
 * `6 [sprite] Attacks`. Null at the opening, where nothing has happened yet.
 */
export function LogMoment({
  armies,
  boundaries,
  currentIndex,
}: {
  armies: readonly MatchArmy[];
  boundaries: readonly LogBoundary[];
  currentIndex: number;
}) {
  const day = actionDay(boundaries, currentIndex);
  if (day === null || currentIndex === 0) return null;
  const line = dayLines(boundaries, day).find((each) => each.index === currentIndex);
  if (!line) return null;
  const army = armyFor(armies, line.actingSlot);
  return (
    <HStack as="span" xstyle={styles.moment}>
      <Text color="inherit" type="inherit" xstyle={[figureStyles.figure, styles.momentNumber]}>
        {line.number}
      </Text>
      <LineArt army={army} line={line} />
      <Text color="inherit" type="inherit" xstyle={styles.verb}>
        {lineVerb(line, army)}
      </Text>
    </HStack>
  );
}

function LogList({
  armies,
  currentIndex,
  isDisabled,
  lines,
  onSeek,
  tone,
}: {
  armies: readonly MatchArmy[];
  currentIndex: number;
  isDisabled: boolean;
  lines: LogLine[];
  onSeek: (index: number) => void;
  tone: "ink" | "paper";
}) {
  const listRef = useRef<HTMLOListElement>(null);

  // The lifted line stays in view as the reading moves through the day.
  useEffect(() => {
    const row = listRef.current?.querySelector<HTMLElement>('[aria-current="step"]');
    row?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [currentIndex]);

  const isPaper = tone === "paper";
  return (
    <ol ref={listRef} {...stylex.props(styles.list, isPaper && styles.listPaper)}>
      {lines.map((line) => {
        const army = armyFor(armies, line.actingSlot);
        const verb = lineVerb(line, army);
        const isNow = line.index === currentIndex;
        const isLater = line.index > currentIndex;
        const place = line.summary?.tile ?? null;
        const tile = place ? `${place.x + 1},${place.y + 1}` : null;
        return (
          <li key={line.index}>
            <button
              aria-current={isNow ? "step" : undefined}
              aria-label={`${line.number}. ${verb}${tile ? ` at ${tile}` : ""}`}
              disabled={isDisabled}
              onClick={() => onSeek(line.index)}
              type="button"
              {...stylex.props(
                styles.row,
                isPaper && styles.rowPaper,
                isLater && (isPaper ? styles.laterPaper : styles.later),
                isNow && (isPaper ? styles.nowPaper : styles.now),
              )}
            >
              <Text
                color="inherit"
                type="inherit"
                xstyle={[figureStyles.figure, styles.small, isPaper && styles.smallPaper]}
              >
                {line.number}
              </Text>
              <HStack as="span" xstyle={[styles.art, isLater && styles.artLater]}>
                <LineArt army={army} line={line} />
              </HStack>
              <Text color="inherit" type="inherit" xstyle={styles.verb}>
                {verb}
              </Text>
              <Text
                color="inherit"
                type="inherit"
                xstyle={[figureStyles.figure, styles.small, isPaper && styles.smallPaper]}
              >
                {tile}
              </Text>
            </button>
          </li>
        );
      })}
    </ol>
  );
}

/** The unit that acted where the outline names it, else the army's crest. */
function LineArt({ army, line }: { army: MatchArmy | null; line: LogLine }) {
  const factionCode = army?.entry.displayFactionCode ?? null;
  const unit = line.summary?.unit ?? null;
  if (unit !== null && factionCode !== null) {
    const sprite = unitSpriteStyle(unit, factionCode, 1);
    if (sprite) {
      return (
        <Stack
          aria-hidden="true"
          as="span"
          height={unitSpriteSize(1).height}
          width={unitSpriteSize(1).width}
          xstyle={styles.sprite}
        >
          {/* Raw span: an inline atlas crop. */}
          <span style={sprite} {...stylex.props(styles.spriteArt)} />
        </Stack>
      );
    }
  }
  if (factionCode === null) return null;
  return <FactionCrest factionCode={factionCode} size={16} />;
}

/** What the line says: the action's verb, or who acted where it is not named. */
function lineVerb(line: LogLine, army: MatchArmy | null): string {
  return line.summary?.verb ?? army?.name ?? "Action";
}

function armyFor(armies: readonly MatchArmy[], slot: number | null): MatchArmy | null {
  if (slot === null) return null;
  return armies.find((army) => army.entry.playerId === slot) ?? null;
}

const styles = stylex.create({
  // The log takes what the rail has left under the armies (at least 180px)
  // and scrolls inside it, so the rail foot stays put.
  log: {
    display: "flex",
    flexDirection: "column",
    flexBasis: 0,
    flexGrow: 1,
    flexShrink: 0,
    minBlockSize: "180px",
    paddingBlockStart: awbrnVars.spacing2_5,
    borderBlockEndColor: awbrnVars.ink800,
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: borderVars["--border-width"],
  },
  list: {
    display: "grid",
    alignContent: "start",
    gap: spacingVars["--spacing-0-5"],
    minBlockSize: 0,
    margin: 0,
    paddingBlock: `0 ${spacingVars["--spacing-2"]}`,
    paddingInline: spacingVars["--spacing-2"],
    overflowY: "auto",
    listStyle: "none",
  },
  // On a phone the day scrolls under the sheet's head and above its day keys.
  listPaper: {
    maxBlockSize: "min(56dvh, 520px)",
    paddingBlock: spacingVars["--spacing-1-5"],
  },
  row: {
    display: "grid",
    gridTemplateColumns: "22px 16px minmax(0, 1fr) auto",
    alignItems: "center",
    gap: spacingVars["--spacing-2"],
    inlineSize: "100%",
    minBlockSize: {
      default: "36px",
      [pageLayout.coarseMedia]: "44px",
    },
    margin: 0,
    paddingBlock: spacingVars["--spacing-1"],
    paddingInline: spacingVars["--spacing-2"],
    borderStyle: "none",
    borderRadius: radiusVars["--radius-inner"],
    backgroundColor: {
      default: "transparent",
      ":hover:not(:disabled)": { default: null, [pageLayout.hoverMedia]: awbrnVars.ink850 },
      ":active:not(:disabled)": awbrnVars.ink850,
    },
    color: awbrnVars.textOnInk,
    fontFamily: "inherit",
    fontSize: awbrnVars.text13_5,
    textAlign: "start",
    cursor: {
      default: "pointer",
      ":disabled": "default",
    },
    transitionProperty: "background-color",
    transitionDuration: {
      default: "90ms",
      [pageLayout.reducedMotionMedia]: "0s",
    },
    transitionTimingFunction: awbrnVars.easeStep,
    outlineColor: awbrnVars.blueOnInk,
    outlineOffset: "-2px",
    outlineStyle: {
      default: "none",
      ":focus-visible": "solid",
    },
    outlineWidth: "2px",
  },
  rowPaper: {
    gap: awbrnVars.spacing2_5,
    minBlockSize: "48px",
    paddingInline: awbrnVars.spacing2_5,
    backgroundColor: {
      default: "transparent",
      ":hover:not(:disabled)": { default: null, [pageLayout.hoverMedia]: awbrnVars.paperWell },
      ":active:not(:disabled)": awbrnVars.paperWell,
    },
    color: awbrnVars.ink900,
    fontSize: awbrnVars.text14_5,
    outlineColor: focusVars["--focus-outline-color"],
  },
  now: {
    backgroundColor: {
      default: awbrnVars.ink800,
      ":hover:not(:disabled)": { default: null, [pageLayout.hoverMedia]: awbrnVars.ink800 },
      ":active:not(:disabled)": awbrnVars.ink800,
    },
    boxShadow: `inset 0 0 0 1px ${awbrnVars.ink600}`,
  },
  nowPaper: {
    backgroundColor: {
      default: awbrnVars.paper,
      ":hover:not(:disabled)": { default: null, [pageLayout.hoverMedia]: awbrnVars.paper },
      ":active:not(:disabled)": awbrnVars.paper,
    },
    boxShadow: `inset 0 0 0 1px ${awbrnVars.ink900}`,
    fontWeight: 650,
  },
  later: {
    color: awbrnVars.textOnInkSecondary,
  },
  laterPaper: {
    color: awbrnVars.textTertiary,
  },
  small: {
    color: awbrnVars.textOnInkSecondary,
    fontSize: awbrnVars.text11_5,
    fontWeight: 500,
    lineHeight: 1,
    textAlign: "end",
  },
  smallPaper: {
    color: awbrnVars.textTertiary,
  },
  art: {
    display: "inline-grid",
    placeItems: "center",
    inlineSize: "16px",
    blockSize: "19px",
    marginBlockStart: `calc(-1 * ${spacingVars["--spacing-1"]})`,
  },
  artLater: {
    opacity: 0.5,
  },
  sprite: {
    position: "relative",
    display: "inline-block",
    overflow: "hidden",
  },
  spriteArt: {
    position: "absolute",
    insetBlockStart: 0,
    insetInlineStart: 0,
  },
  verb: {
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  moment: {
    display: "inline-flex",
    alignItems: "center",
    gap: spacingVars["--spacing-2"],
    minInlineSize: 0,
  },
  momentNumber: {
    color: awbrnVars.textOnInkSecondary,
    fontSize: awbrnVars.text13,
  },
  days: {
    display: "grid",
    gridTemplateColumns: "1fr 1fr",
    gap: spacingVars["--spacing-2"],
    inlineSize: "100%",
  },
  dayKey: {
    blockSize: "48px",
  },
});
