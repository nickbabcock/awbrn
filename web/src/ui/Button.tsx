import { Button as AstryxButton, type ButtonProps } from "@astryxdesign/core/Button";
import * as stylex from "@stylexjs/stylex";

export type { ButtonProps } from "@astryxdesign/core/Button";

/** Button with the AWBRN pressed and disabled states. */
export function Button({ isDisabled, xstyle, ...props }: ButtonProps) {
  return (
    <AstryxButton
      {...props}
      isDisabled={isDisabled}
      xstyle={[styles.root, styles.reducedMotion, isDisabled && styles.disabled, xstyle]}
    />
  );
}

const styles = stylex.create({
  root: {
    backgroundColor: {
      default: null,
      ":disabled": "var(--color-background-muted)",
    },
    borderColor: {
      default: null,
      ":disabled": "var(--color-border-disabled)",
    },
    boxShadow: {
      default: null,
      ":active": "none",
      ":disabled": "none",
    },
    color: {
      default: null,
      ":disabled": "var(--color-text-disabled)",
    },
    transform: {
      default: null,
      ":active": "translate(var(--offset-control-pressed), var(--offset-control-pressed))",
    },
    transitionDuration: {
      default: null,
      ":active": "var(--duration-fast-min)",
    },
  },
  reducedMotion: {
    transform: {
      default: null,
      "@media (prefers-reduced-motion: reduce)": "none",
    },
  },
  disabled: {
    backgroundColor: "var(--color-background-muted)",
    borderColor: "var(--color-border-disabled)",
    boxShadow: "none",
    color: "var(--color-text-disabled)",
  },
});
