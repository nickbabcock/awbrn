import { Text } from "@astryxdesign/core/Text";
import { HStack, Stack } from "@astryxdesign/core/Stack";
import { Icon } from "@astryxdesign/core/Icon";
import { borderVars, radiusVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useState, type ReactNode } from "react";
import { matchLayout } from "#/matches/components/matchLayout.stylex.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { figureStyles } from "#/ui/Figure.tsx";
import {
  IconBack,
  IconFirst,
  IconLast,
  IconNext,
  IconTurnBack,
  IconTurnNext,
} from "#/ui/icons.tsx";

/**
 * Walking a game, by actions or by turns, and the scrubber along it.
 *
 * An action is the unit a fight is read in and a turn is the unit a game is
 * read in, so both are one press. The scrubber jumps anywhere; a tick marks
 * each day where the game says where its days begin.
 */
export interface ReviewModel {
  /**
   * How far into the game the viewer is standing, or null when the game has
   * not said yet. A live match only counts its actions once somebody asks.
   */
  position: { index: number; total: number } | null;
  /** Whether the viewer is standing at the far end of the game. */
  isAtLatest: boolean;
  canStepBack: boolean;
  canStepForward: boolean;
  canStepTurnBack: boolean;
  canStepTurnForward: boolean;
  /** `Live` for a match still being played, `End` for an archive. */
  latestLabel: string;
  /** True while the game is answering the last step it was asked for. */
  isBusy: boolean;
  onSeekStart: () => void;
  onStep: (delta: number) => void;
  onTurnStep: (delta: number) => void;
  onSeekLatest: () => void;
  /** Jump to one moment; absent until the game has said how long it is. */
  onSeek?: (index: number) => void;
  /** Where each day begins, as action indexes. */
  dayStarts?: number[];
  /** An archive has an end to jump to; a live match ends at Live. */
  hasLastKey: boolean;
}

function positionLabel(review: ReviewModel): ReactNode {
  if (review.isAtLatest || review.position === null) return <b>{review.latestLabel}</b>;
  return (
    <>
      <b>{review.position.index}</b> / {review.position.total}
    </>
  );
}

/** The scrubber: a range input over a track with a tick for each day. */
export function Scrubber({
  review,
  size = "desktop",
}: {
  review: ReviewModel;
  size?: "desktop" | "phone";
}) {
  // The thumb follows a drag at once; the moment is asked for when it lands,
  // so a drag across a hundred actions is one question rather than a hundred.
  const [draft, setDraft] = useState<number | null>(null);
  const total = review.position?.total ?? 0;
  const index = review.isAtLatest ? total : (review.position?.index ?? total);
  const shown = draft ?? index;
  const percent = total === 0 ? 100 : (shown / total) * 100;
  const isLive = review.onSeek === undefined || total === 0;
  const commit = () => {
    if (draft === null) return;
    setDraft(null);
    if (draft !== index) review.onSeek?.(draft);
  };

  return (
    <HStack as="span" xstyle={[styles.scrub, size === "phone" && styles.scrubPhone]}>
      <Text color="inherit" type="inherit" aria-hidden="true" xstyle={styles.track}>
        <i {...stylex.props(styles.played, styles.playedTo(percent))} />
        {total === 0
          ? null
          : (review.dayStarts ?? []).map((start) => (
              <s key={start} {...stylex.props(styles.tick, styles.tickAt((start / total) * 100))} />
            ))}
      </Text>
      <input
        aria-label="Moment of the match"
        aria-valuetext={
          review.isAtLatest && draft === null ? review.latestLabel : `${shown} of ${total}`
        }
        disabled={isLive}
        max={Math.max(total, 1)}
        min={0}
        onBlur={commit}
        onChange={(event) => setDraft(Number(event.currentTarget.value))}
        onKeyUp={commit}
        onPointerUp={commit}
        step={1}
        type="range"
        value={total === 0 ? 1 : shown}
        {...stylex.props(styles.range)}
      />
    </HStack>
  );
}

function StepKey({
  icon,
  isDisabled,
  label,
  onPress,
  size,
  isLit = false,
}: {
  icon: typeof IconBack;
  isDisabled: boolean;
  label: string;
  onPress: () => void;
  size: "foot" | "dock";
  isLit?: boolean;
}) {
  return (
    <Button
      icon={<Icon icon={icon} xstyle={size === "dock" && styles.dockGlyph} />}
      isDisabled={isDisabled}
      isIconOnly
      label={label}
      onClick={onPress}
      tooltip={label}
      variant="on-ink"
      xstyle={[size === "foot" ? styles.footKey : styles.dockKey, isLit && styles.litKey]}
    />
  );
}

/** A 52px icon key for the phone review bar, filling its cell (log, calculator). */
export function ReviewBarKey({
  icon,
  isDisabled = false,
  label,
  onPress,
}: {
  icon: typeof IconBack;
  isDisabled?: boolean;
  label: string;
  onPress: () => void;
}) {
  return (
    <StepKey icon={icon} isDisabled={isDisabled} label={label} onPress={onPress} size="dock" />
  );
}

/**
 * The desktop stage foot (56px): step keys, the scrubber, the position
 * (`Live` or `214 / 486`).
 */
export function ReviewFoot({ end, review }: { end?: ReactNode; review: ReviewModel }) {
  const busy = review.isBusy;
  return (
    <section aria-label="Review" {...stylex.props(styles.foot)}>
      <HStack as="span" aria-label="Step through the match" role="group" xstyle={styles.keys}>
        <StepKey
          icon={IconFirst}
          isDisabled={busy || !review.canStepBack}
          label="Go to the first turn"
          onPress={review.onSeekStart}
          size="foot"
        />
        <StepKey
          icon={IconTurnBack}
          isDisabled={busy || !review.canStepTurnBack}
          label="Previous turn"
          onPress={() => review.onTurnStep(-1)}
          size="foot"
        />
        <StepKey
          icon={IconBack}
          isDisabled={busy || !review.canStepBack}
          label="Previous action"
          onPress={() => review.onStep(-1)}
          size="foot"
        />
        <StepKey
          icon={IconNext}
          isDisabled={busy || !review.canStepForward}
          label="Next action"
          onPress={() => review.onStep(1)}
          size="foot"
        />
        <StepKey
          icon={IconTurnNext}
          isDisabled={busy || !review.canStepTurnForward}
          label="Next turn"
          onPress={() => review.onTurnStep(1)}
          size="foot"
        />
        {review.hasLastKey ? (
          <StepKey
            icon={IconLast}
            isDisabled={busy || review.isAtLatest}
            label={`Go to the ${review.latestLabel.toLowerCase()}`}
            onPress={review.onSeekLatest}
            size="foot"
          />
        ) : null}
      </HStack>
      <Scrubber review={review} />
      <Text color="inherit" type="inherit" xstyle={[figureStyles.figure, styles.position]}>
        {positionLabel(review)}
      </Text>
      {review.isAtLatest || review.hasLastKey ? null : (
        <Button
          isDisabled={busy}
          label="Back to live"
          onClick={review.onSeekLatest}
          size="sm"
          variant="on-ink"
        />
      )}
      {end}
    </section>
  );
}

/**
 * Reviewing a live match on a phone: the position, the scrubber, four step
 * keys and Back to live, in place of the dock.
 */
export function PhoneReviewBar({ review }: { review: ReviewModel }) {
  const busy = review.isBusy;
  return (
    <section aria-label="Review" {...stylex.props(styles.phoneBar)}>
      <HStack as="span" xstyle={styles.phoneTop}>
        <Text color="inherit" type="inherit" xstyle={[figureStyles.figure, styles.position]}>
          {positionLabel(review)}
        </Text>
      </HStack>
      <Scrubber review={review} size="phone" />
      <Stack as="span" xstyle={[styles.phoneKeys, styles.phoneKeysReview]}>
        <StepKey
          icon={IconTurnBack}
          isDisabled={busy || !review.canStepTurnBack}
          label="Previous turn"
          onPress={() => review.onTurnStep(-1)}
          size="dock"
        />
        <StepKey
          icon={IconBack}
          isDisabled={busy || !review.canStepBack}
          label="Previous action"
          onPress={() => review.onStep(-1)}
          size="dock"
        />
        <StepKey
          icon={IconNext}
          isDisabled={busy || !review.canStepForward}
          isLit
          label="Next action"
          onPress={() => review.onStep(1)}
          size="dock"
        />
        <StepKey
          icon={IconTurnNext}
          isDisabled={busy || !review.canStepTurnForward}
          label="Next turn"
          onPress={() => review.onTurnStep(1)}
          size="dock"
        />
        <Button
          isDisabled={busy}
          label="Back to live"
          onClick={review.onSeekLatest}
          variant="on-ink"
          xstyle={styles.dockWide}
        />
      </Stack>
    </section>
  );
}

/**
 * A replay on a phone: the scrubber with its position, then the step keys and
 * whatever the screen adds (the calculator), all 52px.
 */
export function PhoneReplayBar({
  extra,
  review,
  said,
}: {
  extra?: ReactNode;
  review: ReviewModel;
  /** What happened at this moment, when the game can say. */
  said?: ReactNode;
}) {
  const busy = review.isBusy;
  return (
    <section aria-label="Review" {...stylex.props(styles.phoneBar)}>
      <HStack as="span" xstyle={styles.phoneTop}>
        <HStack as="span" xstyle={styles.said}>
          {said}
        </HStack>
        <Text color="inherit" type="inherit" xstyle={[figureStyles.figure, styles.position]}>
          {positionLabel(review)}
        </Text>
      </HStack>
      <Scrubber review={review} size="phone" />
      <Stack as="span" xstyle={styles.phoneKeys}>
        <StepKey
          icon={IconTurnBack}
          isDisabled={busy || !review.canStepTurnBack}
          label="Previous turn"
          onPress={() => review.onTurnStep(-1)}
          size="dock"
        />
        <StepKey
          icon={IconBack}
          isDisabled={busy || !review.canStepBack}
          label="Previous action"
          onPress={() => review.onStep(-1)}
          size="dock"
        />
        <StepKey
          icon={IconNext}
          isDisabled={busy || !review.canStepForward}
          isLit
          label="Next action"
          onPress={() => review.onStep(1)}
          size="dock"
        />
        <StepKey
          icon={IconTurnNext}
          isDisabled={busy || !review.canStepTurnForward}
          label="Next turn"
          onPress={() => review.onTurnStep(1)}
          size="dock"
        />
        {extra}
      </Stack>
    </section>
  );
}

const styles = stylex.create({
  foot: {
    display: "flex",
    alignItems: "center",
    gap: spacingVars["--spacing-3"],
    minInlineSize: 0,
    blockSize: "100%",
    paddingBlock: spacingVars["--spacing-2"],
    paddingInline: spacingVars["--spacing-4"],
    backgroundColor: awbrnVars.ink900,
    borderBlockStartColor: awbrnVars.ink800,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: borderVars["--border-width"],
    color: awbrnVars.textOnInk,
  },
  keys: {
    display: "flex",
    gap: spacingVars["--spacing-1"],
    flexShrink: 0,
  },
  footKey: {
    inlineSize: "36px",
    minInlineSize: "36px",
    blockSize: "36px",
    paddingInline: 0,
  },
  dockKey: {
    inlineSize: "100%",
    minInlineSize: "52px",
    blockSize: "52px",
    paddingInline: 0,
  },
  dockGlyph: {
    width: matchLayout.dockGlyph,
    height: matchLayout.dockGlyph,
  },
  litKey: {
    backgroundColor: awbrnVars.ink100,
    color: awbrnVars.ink950,
  },
  dockWide: {
    blockSize: "52px",
    minInlineSize: 0,
  },
  scrub: {
    position: "relative",
    display: "flex",
    alignItems: "center",
    flexGrow: 1,
    minInlineSize: "80px",
    blockSize: "36px",
  },
  scrubPhone: {
    blockSize: "44px",
  },
  track: {
    position: "absolute",
    insetInline: 0,
    insetBlockStart: "50%",
    blockSize: "6px",
    marginBlockStart: `calc(-1 * ${spacingVars["--spacing-1"]})`,
    borderRadius: radiusVars["--radius-inner"],
    backgroundColor: awbrnVars.ink800,
  },
  played: {
    position: "absolute",
    insetBlock: 0,
    insetInlineStart: 0,
    borderRadius: radiusVars["--radius-inner"],
    backgroundColor: awbrnVars.ink300,
  },
  playedTo: (percent: number) => ({
    inlineSize: `${percent}%`,
  }),
  tick: {
    position: "absolute",
    insetBlockStart: "-5px",
    inlineSize: "2px",
    blockSize: "16px",
    marginInlineStart: "-1px",
    backgroundColor: awbrnVars.ink600,
  },
  tickAt: (percent: number) => ({
    insetInlineStart: `${percent}%`,
  }),
  range: {
    position: "relative",
    zIndex: 1,
    inlineSize: "100%",
    blockSize: "100%",
    margin: 0,
    appearance: "none",
    backgroundColor: "transparent",
    cursor: {
      default: "pointer",
      ":disabled": "default",
    },
    "::-webkit-slider-runnable-track": {
      blockSize: "18px",
      backgroundColor: "transparent",
    },
    "::-moz-range-track": {
      backgroundColor: "transparent",
    },
    "::-webkit-slider-thumb": {
      appearance: "none",
      inlineSize: "18px",
      blockSize: "18px",
      borderRadius: "50%",
      borderStyle: "none",
      backgroundColor: awbrnVars.textOnInkStrong,
      boxShadow: `0 0 0 2px ${awbrnVars.ink950}`,
      cursor: "pointer",
    },
    "::-moz-range-thumb": {
      appearance: "none",
      inlineSize: "18px",
      blockSize: "18px",
      borderRadius: "50%",
      borderStyle: "none",
      backgroundColor: awbrnVars.textOnInkStrong,
      boxShadow: `0 0 0 2px ${awbrnVars.ink950}`,
      cursor: "pointer",
    },
  },
  position: {
    flexShrink: 0,
    color: awbrnVars.textOnInkSecondary,
    fontSize: awbrnVars.text13,
    fontWeight: 500,
    whiteSpace: "nowrap",
  },
  phoneBar: {
    display: "grid",
    gap: spacingVars["--spacing-1"],
    paddingBlock: spacingVars["--spacing-2"],
    paddingInline: spacingVars["--spacing-3"],
    paddingBlockEnd: `calc(${spacingVars["--spacing-3"]} + env(safe-area-inset-bottom, 0px))`,
    backgroundColor: awbrnVars.ink900,
    borderBlockStartColor: awbrnVars.ink800,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: borderVars["--border-width"],
    color: awbrnVars.textOnInk,
  },
  phoneTop: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: awbrnVars.spacing2_5,
    minBlockSize: "20px",
  },
  said: {
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    color: awbrnVars.textOnInkStrong,
    fontSize: awbrnVars.text13_5,
  },
  phoneKeys: {
    display: "grid",
    gridAutoColumns: "minmax(0, 1fr)",
    gridAutoFlow: "column",
    gap: spacingVars["--spacing-2"],
  },
  phoneKeysReview: {
    gridTemplateColumns: "repeat(4, 52px) minmax(0, 1fr)",
  },
});
