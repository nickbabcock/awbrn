import { Stack } from "@astryxdesign/core/Stack";
import { Icon } from "@astryxdesign/core/Icon";
import { ToggleButton } from "@astryxdesign/core/ToggleButton";
import { radiusVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import type { ReactNode } from "react";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { IconCheck } from "#/ui/icons.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";

/**
 * A chip: a pill that toggles one filter or option. 34px tall, 44px on a
 * phone or a coarse pointer. Pressed, it turns ink and shows a check, so the
 * state never rides on color alone.
 *
 * Group chips with `ChipGroup`, which wraps them and names the group for
 * assistive tech (Filters groups are unlabeled on screen).
 *
 *   <ChipGroup label="Armies">
 *     <Chip isPressed={two} label="2P" onPressedChange={setTwo} />
 *   </ChipGroup>
 *
 * For one choice out of a few (Live / Async), use Astryx `SegmentedControl`;
 * the theme draws it as the paper-well track with a white chosen key.
 */
export function Chip({
  children,
  icon,
  isDisabled,
  isPressed,
  label,
  onPressedChange,
}: {
  children?: ReactNode;
  icon?: ReactNode;
  isDisabled?: boolean;
  isPressed: boolean;
  label: string;
  onPressedChange: (isPressed: boolean) => void;
}) {
  return (
    <ToggleButton
      icon={isPressed ? <Icon icon={IconCheck} size="sm" /> : icon}
      isDisabled={isDisabled}
      isPressed={isPressed}
      label={label}
      onPressedChange={(next) => onPressedChange(next)}
      size="sm"
      xstyle={[styles.chip, isPressed && styles.pressed]}
    >
      {children ?? label}
    </ToggleButton>
  );
}

/** A wrapping row of chips, named for assistive tech. */
export function ChipGroup({ children, label }: { children: ReactNode; label: string }) {
  return (
    <Stack aria-label={label} direction="horizontal" role="group" xstyle={styles.group}>
      {children}
    </Stack>
  );
}

const styles = stylex.create({
  chip: {
    minBlockSize: {
      default: "34px",
      [pageLayout.phoneMedia]: "44px",
      [pageLayout.coarseMedia]: "44px",
    },
    paddingInline: {
      default: "12px",
      [pageLayout.phoneMedia]: "14px",
    },
    borderColor: awbrnVars.ruleStrong,
    borderRadius: radiusVars["--radius-full"],
    borderStyle: "solid",
    borderWidth: "1px",
    backgroundColor: awbrnVars.sheet,
    fontSize: awbrnVars.text13_5,
    fontWeight: 600,
  },
  pressed: {
    borderColor: awbrnVars.ink900,
    backgroundColor: awbrnVars.ink900,
    color: awbrnVars.textOnInkStrong,
  },
  group: {
    display: "flex",
    flexWrap: "wrap",
    gap: "8px",
  },
});
