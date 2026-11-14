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

/** Walking a game by actions or by turns, and the scrubber along it. */
export interface ReviewModel {
  /**
   * How far into the game the viewer is standing, or null when the game has
   * not said yet. A live match only counts its actions once somebody asks.
   */
  position: { index: number; total: number } | null;
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

function Position({ review }: { review: ReviewModel }) {
  return (
    <Text color="inherit" type="inherit" xstyle={[figureStyles.figure, styles.position]}>
      {positionLabel(review)}
    </Text>
  );
}

/** A range input over a track with a tick for each day. */
function Scrubber({
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
  // The thumb stays where it landed until the engine reaches that moment, so
  // it does not snap back to the old one for a frame.
  if (draft !== null && draft === index) setDraft(null);
  const commit = () => {
    if (draft !== null && draft !== index) review.onSeek?.(draft);
  };

  return (
    <HStack as="span" align="center" xstyle={[styles.scrub, size === "phone" && styles.scrubPhone]}>
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

/** Previous and next, by turn and by action. */
function TurnAndActionKeys({ review, size }: { review: ReviewModel; size: "foot" | "dock" }) {
  const busy = review.isBusy;
  return (
    <>
      <StepKey
        icon={IconTurnBack}
        isDisabled={busy || !review.canStepTurnBack}
        label="Previous turn"
        onPress={() => review.onTurnStep(-1)}
        size={size}
      />
      <StepKey
        icon={IconBack}
        isDisabled={busy || !review.canStepBack}
        label="Previous action"
        onPress={() => review.onStep(-1)}
        size={size}
      />
      <StepKey
        icon={IconNext}
        isDisabled={busy || !review.canStepForward}
        isLit={size === "dock"}
        label="Next action"
        onPress={() => review.onStep(1)}
        size={size}
      />
      <StepKey
        icon={IconTurnNext}
        isDisabled={busy || !review.canStepTurnForward}
        label="Next turn"
        onPress={() => review.onTurnStep(1)}
        size={size}
      />
    </>
  );
}

/** An icon key for the phone review bar, filling its cell (log, calculator). */
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

/** The desktop stage foot: step keys, the scrubber, the position. */
export function ReviewFoot({ review }: { review: ReviewModel }) {
  const busy = review.isBusy;
  return (
    <section aria-label="Review" {...stylex.props(styles.foot)}>
      <HStack
        as="span"
        aria-label="Step through the match"
        gap={1}
        role="group"
        xstyle={styles.keys}
      >
        <StepKey
          icon={IconFirst}
          isDisabled={busy || !review.canStepBack}
          label="Go to the first turn"
          onPress={review.onSeekStart}
          size="foot"
        />
        <TurnAndActionKeys review={review} size="foot" />
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
      <Position review={review} />
      {review.isAtLatest || review.hasLastKey ? null : (
        <Button
          isDisabled={busy}
          label="Back to live"
          onClick={review.onSeekLatest}
          size="sm"
          variant="on-ink"
        />
      )}
    </section>
  );
}

function PhoneBar({
  children,
  keysXstyle,
  review,
  top,
}: {
  children: ReactNode;
  keysXstyle?: stylex.StyleXStyles;
  review: ReviewModel;
  top?: ReactNode;
}) {
  return (
    <section aria-label="Review" {...stylex.props(styles.phoneBar)}>
      <HStack as="span" align="center" justify="between" xstyle={styles.phoneTop}>
        {top}
        <Position review={review} />
      </HStack>
      <Scrubber review={review} size="phone" />
      <Stack as="span" xstyle={[styles.phoneKeys, keysXstyle]}>
        <TurnAndActionKeys review={review} size="dock" />
        {children}
      </Stack>
    </section>
  );
}

/** Reviewing a live match on a phone, in place of the dock. */
export function PhoneReviewBar({ review }: { review: ReviewModel }) {
  return (
    <PhoneBar keysXstyle={styles.phoneKeysReview} review={review}>
      <Button
        isDisabled={review.isBusy}
        label="Back to live"
        onClick={review.onSeekLatest}
        variant="on-ink"
        xstyle={styles.dockWide}
      />
    </PhoneBar>
  );
}

/** A replay on a phone: the step keys and whatever the screen adds. */
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
  return (
    <PhoneBar
      review={review}
      top={
        <HStack as="span" xstyle={styles.said}>
          {said}
        </HStack>
      }
    >
      {extra}
    </PhoneBar>
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
