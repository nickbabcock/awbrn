import { Button as AstryxButton, type ButtonProps } from "@astryxdesign/core/Button";
import { Icon } from "@astryxdesign/core/Icon";
import { HStack } from "@astryxdesign/core/Stack";
import * as stylex from "@stylexjs/stylex";
import { spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import { useEffect, useState, type MouseEvent } from "react";
import { IconSpinner } from "#/ui/icons.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";

export type { ButtonProps } from "@astryxdesign/core/Button";

/**
 * A key. Every command in AWBRN is one of these.
 *
 * Variants (the theme draws them; see `awbrnTheme.ts`):
 * - `primary`: Airmail red, the next command. **One Red Key**: at most one per
 *   viewport. End turn is `secondary` (or `on-ink`) while units remain and
 *   turns `primary` only when the turn is spent.
 * - `secondary` (default): a sheet-colored key with a strong rule.
 * - `ink`: a firm key that is not the next command (an empty state's one key).
 * - `on-ink`: a raised key on the ink table or a navy bar (ink 850).
 * - `ghost`, `ghost-on-ink`: quiet keys on paper or on ink.
 * - `destructive`: Resign, Decline. Outlined in danger until armed.
 *
 * Sizes: `sm` 32px, `md` 40px, `lg` 48px; on a coarse pointer every size is
 * at least 44px and `lg` is the dock's 52px. Icon keys pass `isIconOnly`, an
 * `icon` from `#/ui/icons.tsx`, a `label` (the accessible name) and a
 * `tooltip`.
 *
 * **Busy.** A key that sent a request (`isLoading`, or a `clickAction` whose
 * promise is pending) keeps its label and its width and ignores presses until
 * the answer. After a short hold, so a quick answer never flashes, the drawn
 * spinner takes the icon's place and turns a spoke at a time; it holds still
 * under reduced motion. A key with no icon takes a small spinner beside its
 * label and gives it room from its own padding, so it holds its width (a key
 * that sets its own padding grows by the spinner's slot instead).
 *
 *   <Button label="Play turn" size="lg" variant="primary" />
 *   <Button isLoading={isEnding} label="End turn" onClick={endTurn} variant="primary" />
 *   <Button icon={<Icon icon={IconCalculator} />} isIconOnly label="Calculator"
 *     tooltip="Calculator" variant="on-ink" />
 *
 * Route links use `RouterButton` from `#/ui/astryx-links.tsx`, which takes the
 * same props plus TanStack Router's `to`.
 */
export function Button({
  clickAction,
  icon,
  isLoading = false,
  onClick,
  xstyle,
  ...props
}: ButtonProps) {
  const [isPending, setIsPending] = useState(false);
  const isBusy = isLoading || isPending;
  const showsSpinner = useHeldFlag(isBusy, SPINNER_HOLD_MS);
  const hasIcon = icon != null && icon !== false;
  const spinsBesideLabel = showsSpinner && !hasIcon && !props.isIconOnly;

  const press = (event: MouseEvent<HTMLButtonElement>) => {
    // A busy key ignores presses, a form's submit included.
    if (isBusy) {
      event.preventDefault();
      return;
    }
    onClick?.(event);
    const result = clickAction?.(event);
    if (result instanceof Promise) {
      setIsPending(true);
      result.finally(() => setIsPending(false)).catch(() => {});
    }
  };

  return (
    <AstryxButton
      {...props}
      aria-busy={isBusy || props["aria-busy"] || undefined}
      // A key with an icon turns the spinner in the icon's place. A key with
      // none sets a small one beside its label, inside the label slot, so
      // the key's own icon box does not open.
      icon={showsSpinner && hasIcon ? <Icon icon={IconSpinner} xstyle={styles.spinner} /> : icon}
      {...(spinsBesideLabel
        ? {
            children: (
              <HStack align="center" as="span" gap={1}>
                <Icon icon={IconSpinner} size="sm" xstyle={styles.spinner} />
                {props.children ?? props.label}
              </HStack>
            ),
          }
        : {})}
      onClick={press}
      xstyle={[styles.root, isBusy && styles.busy, spinsBesideLabel && styles.spinnerRoom, xstyle]}
    />
  );
}

/** Astryx's own hold before a busy key shows its spinner (`--duration-medium-min`). */
const SPINNER_HOLD_MS = 85;

/** True once `value` has stayed true for `ms`; false at once when it drops. */
function useHeldFlag(value: boolean, ms: number): boolean {
  const [held, setHeld] = useState(false);
  useEffect(() => {
    if (!value) {
      setHeld(false);
      return;
    }
    const timer = window.setTimeout(() => setHeld(true), ms);
    return () => window.clearTimeout(timer);
  }, [ms, value]);
  return held && value;
}

const turn = stylex.keyframes({
  from: { transform: "rotate(0deg)" },
  to: { transform: "rotate(360deg)" },
});

const styles = stylex.create({
  // Disabled keys fade rather than gray out, so the variant stays readable.
  root: {
    opacity: {
      default: null,
      ":disabled": 0.45,
    },
    cursor: {
      default: null,
      ":disabled": "not-allowed",
    },
  },
  // A busy key keeps its look: it is waiting, not refused.
  busy: {
    cursor: "progress",
  },
  // The spinner's slot (a 16px icon and the 4px gap) comes out of the key's
  // own padding (Astryx's 12px), half from each side, so the box holds still.
  // The spinner cannot hang into the padding instead: the label slot clips.
  spinnerRoom: {
    paddingInline: `calc(${spacingVars["--spacing-3"]} - (1rem + ${spacingVars["--spacing-1"]}) / 2)`,
  },
  spinner: {
    animationName: {
      default: turn,
      [pageLayout.reducedMotionMedia]: "none",
    },
    animationDuration: "800ms",
    animationIterationCount: "infinite",
    animationTimingFunction: "steps(8, jump-start)",
  },
});
