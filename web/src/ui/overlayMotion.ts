import * as stylex from "@stylexjs/stylex";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";

/**
 * How what floats arrives and leaves: in over 120ms, out over 90ms, both in
 * two visible steps (`--ease-step`). A dialog or a popover appears in place;
 * nothing travels. Under reduced motion it cuts.
 *
 * A dialog's scrim takes the dialog's opacity, so it steps with it. A bottom
 * sheet's scrim is Astryx's own `::backdrop`, out of reach of these styles: it
 * steps in over the theme's `--duration-medium` (120ms) on `--ease-standard`
 * (the step), with the sheet, but Astryx fades it out linearly, cut when the
 * sheet has left at 90ms.
 *
 * `@starting-style` gives the first frame; `transition-behavior:
 * allow-discrete` on `display` and `overlay` keeps a closing layer in the top
 * layer until its last step. Only a browser that can transition `overlay`
 * holds `display` too: one that can hold only `display` (Safari) would drop
 * the closing dialog out of the top layer, uncentered, for its last 90ms, so
 * it hides the layer at once instead, as a browser with neither does.
 *
 * Pass `dialog` and `dialogOpen` to a native `<dialog>` (Astryx `Dialog`), the
 * second only while it is open:
 *
 *   <Dialog xstyle={[overlayMotion.dialog, isOpen && overlayMotion.dialogOpen]} />
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
  // A bottom sheet (Astryx `BottomSheet`) rises on its own transition: half
  // risen in the first frame, then there. Pass `sheetLeaving` once it is
  // closed, so it drops in 90ms. A swipe still follows the finger.
  sheet: {
    transitionTimingFunction: awbrnVars.easeStep,
    transitionDuration: {
      default: "120ms",
      [pageLayout.reducedMotionMedia]: "0s",
    },
  },
  sheetLeaving: {
    transitionDuration: {
      default: "90ms",
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
