import { NumberInput } from "@astryxdesign/core/NumberInput";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { colorVars, textSizeVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import {
  CLOCK_PRESETS,
  DAY_MS,
  HOUR_MS,
  MINUTE_MS,
  findClockPreset,
  formatClockTerms,
} from "#/matches/match_clock.ts";
import { MAX_CLOCK_MS, type MatchClock } from "#/matches/schemas.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { figureStyles } from "#/ui/Figure.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";

type ClockUnit = "minutes" | "hours" | "days";

const UNIT_MS: Record<ClockUnit, number> = {
  minutes: MINUTE_MS,
  hours: HOUR_MS,
  days: DAY_MS,
};

/** One of the clock's three terms, as the host is typing it. */
interface DurationDraft {
  count: number;
  unit: ClockUnit;
}

interface ClockDraft {
  initial: DurationDraft;
  increment: DurationDraft;
  max: DurationDraft;
}

/**
 * A span in the largest unit that divides it whole.
 *
 * A host who set two days and comes back to the field reads two days, not
 * 2,880 minutes.
 */
function toDraft(ms: number): DurationDraft {
  if (ms > 0 && ms % DAY_MS === 0) return { count: ms / DAY_MS, unit: "days" };
  if (ms > 0 && ms % HOUR_MS === 0) return { count: ms / HOUR_MS, unit: "hours" };
  return { count: Math.round(ms / MINUTE_MS), unit: "minutes" };
}

const toMs = (draft: DurationDraft): number => Math.round(draft.count * UNIT_MS[draft.unit]);

const toClock = (draft: ClockDraft): MatchClock => ({
  initialMs: toMs(draft.initial),
  incrementMs: toMs(draft.increment),
  maxBankMs: toMs(draft.max),
});

const draftFrom = (clock: MatchClock): ClockDraft => ({
  initial: toDraft(clock.initialMs),
  increment: toDraft(clock.incrementMs),
  max: toDraft(clock.maxBankMs),
});

/** Why a clock cannot be used, or null when it can. */
export function validateClock(clock: MatchClock): string | null {
  const terms = [clock.initialMs, clock.incrementMs, clock.maxBankMs];
  if (!terms.every((term) => Number.isSafeInteger(term) && term >= 0)) {
    return "Every part of the clock has to be a whole number of minutes, hours, or days.";
  }
  if (clock.initialMs < MINUTE_MS || clock.maxBankMs < MINUTE_MS) {
    return "Give each army at least a minute to start with.";
  }
  if (clock.maxBankMs < clock.initialMs) {
    return "An army cannot bank less time than it starts with, so raise the ceiling.";
  }
  if (terms.some((term) => term > MAX_CLOCK_MS)) {
    return "No part of the clock may run past 30 days.";
  }
  return null;
}

/**
 * The pace the match is played at, as three option cards: Live (`5m +2m`),
 * Async (`7d +2d`), and Custom, whose three fields appear only once it is
 * chosen. A host is choosing a pace, not three durations.
 */
export function ClockSettings({
  clock,
  onChange,
}: {
  clock: MatchClock;
  onChange: (clock: MatchClock) => void;
}) {
  const preset = findClockPreset(clock);
  const [draft, setDraft] = useState<ClockDraft>(() => draftFrom(clock));
  // A host who opens the fields, tries a pace, and comes back expects to find
  // the terms they were writing, so the draft outlives a trip through a preset.
  const [isCustom, setIsCustom] = useState(() => preset === null);
  const isPresetChosen = (id: string) => !isCustom && preset?.id === id;

  const editDraft = (next: ClockDraft): void => {
    setDraft(next);
    onChange(toClock(next));
  };

  return (
    <VStack xstyle={styles.root}>
      <VStack aria-label="Clock" role="radiogroup" xstyle={styles.options}>
        {CLOCK_PRESETS.map((option) => (
          <OptionCard
            isChosen={isPresetChosen(option.id)}
            key={option.id}
            name={option.name}
            onChoose={() => {
              setIsCustom(false);
              onChange(option.clock);
            }}
            terms={formatClockTerms(option.clock)}
          />
        ))}
        <OptionCard
          isChosen={isCustom}
          name="Custom"
          onChoose={() => {
            setIsCustom(true);
            onChange(toClock(draft));
          }}
          terms={isCustom ? formatClockTerms(clock) : "–"}
        />
      </VStack>

      {isCustom ? (
        <VStack xstyle={styles.fields}>
          <DurationField
            label="Starting bank"
            onChange={(initial) => editDraft({ ...draft, initial })}
            value={draft.initial}
          />
          <DurationField
            label="Added a turn"
            minCount={0}
            onChange={(increment) => editDraft({ ...draft, increment })}
            value={draft.increment}
          />
          <DurationField
            label="Bank ceiling"
            onChange={(max) => editDraft({ ...draft, max })}
            value={draft.max}
          />
        </VStack>
      ) : null}
    </VStack>
  );
}

/** A pace as a card: its name over its terms in mono. */
function OptionCard({
  isChosen,
  name,
  onChoose,
  terms,
}: {
  isChosen: boolean;
  name: string;
  onChoose: () => void;
  terms: string;
}) {
  return (
    <button
      aria-checked={isChosen}
      onClick={onChoose}
      role="radio"
      type="button"
      {...stylex.props(styles.card, isChosen && styles.cardChosen)}
    >
      <b {...stylex.props(styles.cardName)}>{name}</b>
      <Text color="inherit" type="inherit" xstyle={[figureStyles.figure, styles.cardTerms]}>
        {terms}
      </Text>
    </button>
  );
}

/**
 * One term of the clock: a count and the unit it is counted in.
 *
 * Changing the unit keeps the number and changes what it means, because a host
 * who set seven and wants hours is correcting the unit, not the count.
 */
function DurationField({
  label,
  minCount = 1,
  onChange,
  value,
}: {
  label: string;
  minCount?: number;
  onChange: (value: DurationDraft) => void;
  value: DurationDraft;
}) {
  return (
    <VStack xstyle={styles.field}>
      <NumberInput
        isIntegerOnly
        label={label}
        max={Math.floor(MAX_CLOCK_MS / UNIT_MS[value.unit])}
        min={minCount}
        onChange={(count) => onChange({ ...value, count })}
        value={value.count}
      />
      <SegmentedControl
        label={`${label} unit`}
        layout="fill"
        onChange={(unit) => onChange({ ...value, unit: unit as ClockUnit })}
        size="sm"
        value={value.unit}
      >
        <SegmentedControlItem label="Min" value="minutes" />
        <SegmentedControlItem label="Hours" value="hours" />
        <SegmentedControlItem label="Days" value="days" />
      </SegmentedControl>
    </VStack>
  );
}

const styles = stylex.create({
  root: {
    display: "grid",
    gap: "16px",
  },
  options: {
    display: "grid",
    gridTemplateColumns: "repeat(3, minmax(0, 1fr))",
    gap: "8px",
  },
  card: {
    display: "flex",
    flexDirection: "column",
    alignItems: "flex-start",
    gap: "2px",
    minInlineSize: 0,
    minBlockSize: "44px",
    paddingBlock: {
      default: "12px",
      [pageLayout.phoneMedia]: "10px",
    },
    paddingInline: {
      default: "14px",
      [pageLayout.phoneMedia]: "10px",
    },
    borderColor: awbrnVars.ruleStrong,
    borderRadius: "6px",
    borderStyle: "solid",
    borderWidth: "1px",
    backgroundColor: colorVars["--color-background-card"],
    color: colorVars["--color-text-primary"],
    textAlign: "start",
    cursor: "pointer",
    transitionProperty: "border-color, background-color",
    transitionDuration: {
      default: "90ms",
      [pageLayout.reducedMotionMedia]: "0s",
    },
    transitionTimingFunction: awbrnVars.easeStep,
  },
  cardChosen: {
    borderColor: awbrnVars.ink900,
    backgroundColor: awbrnVars.ink100,
    boxShadow: `inset 0 0 0 1px ${awbrnVars.ink900}`,
  },
  cardName: {
    fontSize: textSizeVars["--font-size-base"],
    fontWeight: 700,
  },
  cardTerms: {
    fontSize: awbrnVars.text12_5,
    color: colorVars["--color-text-primary"],
  },
  fields: {
    display: "grid",
    gridTemplateColumns: "repeat(auto-fit, minmax(110px, 1fr))",
    gap: "12px",
  },
  field: {
    display: "grid",
    gap: "8px",
    alignContent: "start",
  },
});
