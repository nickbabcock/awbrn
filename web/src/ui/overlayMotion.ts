import * as stylex from "@stylexjs/stylex";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";

/**
 * A bottom sheet's scrim is Astryx's own `::backdrop`, out of reach of these
 * styles: Astryx fades it out linearly, cut when the sheet has left.
 *
 * `transition-behavior: allow-discrete` on `display` and `overlay` keeps a
 * closing layer in the top layer until its last step. Only a browser that can
 * transition `overlay` holds `display` too: one that can hold only `display`
 * (Safari) would drop the closing dialog out of the top layer, uncentered, for
 * its last 90ms, so it hides the layer at once instead.
 *
 * Pass `dialogOpen` only while the dialog is open.
 */
const discrete = {
  transitionProperty: {
    default: "opacity",
    "@supports (overlay: auto)": "opacity, display, overlay",
  },
  transitionBehavior: "allow-discrete",
  transitionTimingFunction: awbrnVars.easeStep,
  // Leaving: the closed state's own duration.
  transitionDuration: {
    default: "90ms",
    [pageLayout.reducedMotionMedia]: "0s",
  },
} as const;

export const overlayMotion = stylex.create({
  dialog: {
    // The layer's own entrance (a travel and a scale) gives way to the step.
    animationName: "none",
    ...discrete,
    // The scrim takes the dialog's opacity as it steps, so the two move as
    // one; it needs no transition of its own.
    "::backdrop": {
      opacity: "inherit",
    },
  },
  // Pass `sheetLeaving` once the sheet is closed. A swipe still follows the
  // finger. A sheet travels too far to step: it glides up and settles, and
  // leaves faster than it came.
  sheet: {
    transitionTimingFunction: "cubic-bezier(0.32, 0.72, 0, 1)",
    transitionDuration: {
      default: "240ms",
      [pageLayout.reducedMotionMedia]: "0s",
    },
  },
  sheetLeaving: {
    transitionTimingFunction: "cubic-bezier(0.4, 0, 1, 1)",
    transitionDuration: {
      default: "160ms",
      [pageLayout.reducedMotionMedia]: "0s",
    },
  },
  dialogOpen: {
    opacity: {
      default: 1,
      "@starting-style": 0,
    },
    transitionDuration: {
      default: "120ms",
      [pageLayout.reducedMotionMedia]: "0s",
    },
  },
});
