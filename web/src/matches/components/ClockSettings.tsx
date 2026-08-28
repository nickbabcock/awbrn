import { Grid } from "@astryxdesign/core/Grid";
import { Heading } from "@astryxdesign/core/Heading";
import { NumberInput } from "@astryxdesign/core/NumberInput";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { SelectableCard } from "@astryxdesign/core/SelectableCard";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
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

/** Narrow enough that the three custom fields sit on one row on a laptop. */
const CUSTOM_GRID_MIN_WIDTH = 180;

/** Wide enough for a pace to keep its name, its terms, and its line. */
const PRESET_GRID_MIN_WIDTH = 240;

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
 * The pace the match is played at.
 *
 * A host is choosing a pace, not three durations: whether this is a match
 * played at the board or over days is the decision, and the three numbers
 * behind it are the consequence. So the paces are dealt as keys on the same
 * board the maps were chosen from, the chosen one wears the same cursor, and
 * the fields only appear for the host who wants terms nobody named.
 *
 * Each key says the least that distinguishes it: what the pace is called, what
 * it costs in time, and a few words on who plays at it. A key a host has to
 * read a sentence of is a key that has stopped being a choice and started
 * being a manual.
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
    <VStack gap={4}>
      <VStack gap={1}>
        <Heading level={3}>Match clock</Heading>
        {/* The terms are on the keys and the fields name their own jobs, so the
            one line here is for the rule none of them state: what the clock is
            for. */}
        <Text color="secondary">An army whose clock runs out is removed from the match.</Text>
      </VStack>

      <Grid
        align="stretch"
        columns={{ minWidth: PRESET_GRID_MIN_WIDTH, max: 3, repeat: "fit" }}
        gap={3}
      >
        {CLOCK_PRESETS.map((option) => (
          <SelectableCard
            isSelected={isPresetChosen(option.id)}
            key={option.id}
            label={`${option.name}: ${formatClockTerms(option.clock)}. ${option.brief}`}
            onChange={() => {
              setIsCustom(false);
              onChange(option.clock);
            }}
            padding={3}
          >
            <PaceFace
              brief={option.brief}
              name={option.name}
              terms={formatClockTerms(option.clock)}
            />
          </SelectableCard>
        ))}
        <SelectableCard
          isSelected={isCustom}
          label="Custom clock: set the bank, the increment, and the ceiling yourself."
          onChange={() => {
            setIsCustom(true);
            onChange(toClock(draft));
          }}
          padding={3}
        >
          <PaceFace
            brief="Set your own."
            name="Custom"
            terms={isCustom ? formatClockTerms(clock) : "Your terms"}
          />
        </SelectableCard>
      </Grid>

      {isCustom ? (
        <Grid
          align="start"
          columns={{ minWidth: CUSTOM_GRID_MIN_WIDTH, max: 3, repeat: "fit" }}
          gap={4}
        >
          <DurationField
            description="What an army opens with."
            label="Starting bank"
            onChange={(initial) => editDraft({ ...draft, initial })}
            value={draft.initial}
          />
          <DurationField
            description="Given back each turn."
            label="Added a turn"
            minCount={0}
            onChange={(increment) => editDraft({ ...draft, increment })}
            value={draft.increment}
          />
          <DurationField
            description="The most it can hold."
            label="Bank ceiling"
            onChange={(max) => editDraft({ ...draft, max })}
            value={draft.max}
          />
        </Grid>
      ) : null}
    </VStack>
  );
}

/**
 * A pace, as it reads on its key: what it is called, what it costs in time,
 * and who plays at it, in the three voices and in that order.
 */
function PaceFace({ brief, name, terms }: { brief: string; name: string; terms: string }) {
  return (
    <VStack gap={1}>
      <HStack align="center" gap={2} justify="between" wrap="wrap">
        <Text weight="bold">{name}</Text>
        <Text type="label">{terms}</Text>
      </HStack>
      <Text type="supporting">{brief}</Text>
    </VStack>
  );
}

/**
 * One term of the clock: a count and the unit it is counted in.
 *
 * Changing the unit keeps the number and changes what it means, because a host
 * who set seven and wants hours is correcting the unit, not the count.
 */
function DurationField({
  description,
  label,
  minCount = 1,
  onChange,
  value,
}: {
  description: string;
  label: string;
  minCount?: number;
  onChange: (value: DurationDraft) => void;
  value: DurationDraft;
}) {
  return (
    <VStack gap={2}>
      <NumberInput
        description={description}
        isIntegerOnly
        isRequired
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
        <SegmentedControlItem label="Minutes" value="minutes" />
        <SegmentedControlItem label="Hours" value="hours" />
        <SegmentedControlItem label="Days" value="days" />
      </SegmentedControl>
    </VStack>
  );
}
