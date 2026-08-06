import { Button as AstryxButton, type ButtonProps } from "@astryxdesign/core/Button";
import { colorVars, durationVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";

export type { ButtonProps } from "@astryxdesign/core/Button";

/**
 * Button with the AWBRN outline, depth, pressed state, and disabled state.
 *
 * Every command in this system is a key on a menu: a 2px ink outline and a
 * shadow it goes down into when pressed. The theme states that, but the
 * component library flattens buttons at a specificity no theme layer can reach,
 * so the outline and the shadow are stated again here, where they win. Ghost is
 * the one command that is not a raised key and keeps the flat treatment.
 */
export function Button({ isDisabled, variant, xstyle, ...props }: ButtonProps) {
  return (
    <AstryxButton
      {...props}
      isDisabled={isDisabled}
      variant={variant}
      xstyle={[
        styles.root,
        variant !== "ghost" && styles.raised,
        styles.reducedMotion,
        isDisabled && styles.disabled,
        xstyle,
      ]}
    />
  );
}

const styles = stylex.create({
  root: {
    backgroundColor: {
      default: null,
      ":disabled": colorVars["--color-background-muted"],
    },
    borderColor: {
      default: null,
      ":disabled": awbrnVars.colorBorderDisabled,
    },
    boxShadow: {
      default: null,
      ":active": "none",
      ":disabled": "none",
    },
    color: {
      default: null,
      ":disabled": colorVars["--color-text-disabled"],
    },
    transform: {
      default: null,
      ":active": `translate(${awbrnVars.offsetControlPressed}, ${awbrnVars.offsetControlPressed})`,
    },
    transitionDuration: {
      default: null,
      ":active": durationVars["--duration-fast-min"],
    },
  },
  // The key itself: the outline the system gives every control, and the shadow
  // the press goes down into.
  raised: {
    borderWidth: "var(--border-width)",
    borderStyle: "solid",
    borderColor: {
      default: "var(--color-border-emphasized)",
      ":disabled": "var(--color-border-disabled)",
    },
    boxShadow: {
      default: "var(--shadow-low)",
      ":active": "none",
      ":disabled": "none",
    },
  },
  reducedMotion: {
    transform: {
      default: null,
      "@media (prefers-reduced-motion: reduce)": "none",
    },
  },
  disabled: {
    backgroundColor: colorVars["--color-background-muted"],
    borderColor: awbrnVars.colorBorderDisabled,
    boxShadow: "none",
    color: colorVars["--color-text-disabled"],
  },
});
