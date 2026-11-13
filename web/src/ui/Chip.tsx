import { Stack } from "@astryxdesign/core/Stack";
import { Icon } from "@astryxdesign/core/Icon";
import { ToggleButton } from "@astryxdesign/core/ToggleButton";
import { radiusVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import type { ReactNode } from "react";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { IconCheck } from "#/ui/icons.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";

/** A pill that toggles one filter. Pressed, it shows a check, so the state never rides on color alone. */
export function Chip({
  isPressed,
  label,
  onPressedChange,
}: {
  isPressed: boolean;
  label: string;
  onPressedChange: (isPressed: boolean) => void;
}) {
  return (
    <ToggleButton
      icon={isPressed ? <Icon icon={IconCheck} size="sm" /> : undefined}
      isPressed={isPressed}
      label={label}
      onPressedChange={onPressedChange}
      size="sm"
      xstyle={[styles.chip, isPressed && styles.pressed]}
    >
      {label}
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
