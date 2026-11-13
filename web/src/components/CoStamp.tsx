import { Stack } from "@astryxdesign/core/Stack";
import * as stylex from "@stylexjs/stylex";
import type { ReactNode } from "react";
import { FactionLogo } from "#/components/FactionLogo.tsx";
import {
  type CoPortraitEntry,
  DEFAULT_CO_PORTRAIT_KEY,
  resolveCoPortrait,
} from "#/components/co_portraits.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { factionColorVar } from "#/ui/faction_colors.ts";

/**
 * The CO stamp: a commander's pixel portrait on its army's field, in a white
 * perforated margin. A commander is a stamp everywhere in AWBRN.
 *
 * Sizes draw the 32px portrait at a whole scale: `sm` 32px (1x), `md` 64px
 * (2x), `lg` 96px (3x). Never resampled.
 *
 * States:
 * - `default`: the portrait on the army's field (`factionCode`).
 * - `canceled`: a fallen army. Grayed under violet cancellation lines.
 * - `banned`: a banned commander in a picker. Grayed and faded under the lines.
 * - `isOpenSeat`: an empty seat. A dashed outline holding the army's crest,
 *   grayed; pass `factionCode` for the crest. Only a seat someone can still
 *   take draws this way.
 *
 * A seat whose commander is not known (an opponent not yet revealed, or no
 * `coKey`, or the "no CO" key) is a stamp too: the game's "no CO" portrait on
 * the well's field, named "No commander". It is filled, so it never reads as
 * a seat to take.
 *
 * The stamp names the commander for assistive tech (`label` overrides the
 * name). Pass `isDecorative` when a name beside the stamp already says it.
 *
 *   <CoStamp coKey="sami" factionCode="os" size="md" />
 *   <CoStamp factionCode="bm" isOpenSeat size="sm" />
 *   <CoStamp coKey="kanbei" factionCode="yc" state="canceled" />
 */
export function CoStamp({
  coKey,
  factionCode,
  isDecorative = false,
  isOpenSeat = false,
  label,
  size = "sm",
  state = "default",
  xstyle,
}: {
  coKey?: string | null;
  factionCode?: string | null;
  isDecorative?: boolean;
  isOpenSeat?: boolean;
  label?: string;
  size?: CoStampSize;
  state?: "default" | "canceled" | "banned";
  xstyle?: stylex.StyleXStyles;
}) {
  const scale = STAMP_SCALE[size];
  const portrait = isOpenSeat ? null : resolveCoPortrait(null, coKey);

  if (portrait === null) {
    return (
      <Stack
        aria-hidden={isDecorative ? true : undefined}
        aria-label={isDecorative ? undefined : (label ?? "Open seat")}
        as="span"
        role={isDecorative ? undefined : "img"}
        xstyle={[
          styles.openSeat,
          styles.openSeatSize(32 * scale + 2 * (size === "sm" ? 4 : 5)),
          xstyle,
        ]}
      >
        {factionCode ? (
          <Stack as="span" xstyle={styles.grayed}>
            <FactionLogo
              factionCode={factionCode}
              isFramed={false}
              isLabelHidden
              size={14 * scale}
            />
          </Stack>
        ) : null}
      </Stack>
    );
  }

  const isHidden = portrait.key === DEFAULT_CO_PORTRAIT_KEY;
  const name = label ?? (isHidden ? "No commander" : portrait.displayName);
  // A commander not yet known stands on the well, whatever the army.
  const field = factionCode && !isHidden ? factionColorVar(factionCode, "field") : null;

  return (
    <StampFrame
      aria-hidden={isDecorative ? true : undefined}
      aria-label={isDecorative ? undefined : name}
      fieldXstyle={[styles.fieldSize(32 * scale), field !== null && styles.fieldColor(field)]}
      isSmall={size === "sm"}
      isUsed={state !== "default"}
      overlay={state !== "default" ? <CancelMark xstyle={styles.cancelOverStamp} /> : null}
      role={isDecorative ? undefined : "img"}
      title={isDecorative ? undefined : name}
      xstyle={xstyle}
    >
      <PortraitSprite
        portrait={portrait}
        scale={scale}
        xstyle={state === "banned" && styles.banned}
      />
    </StampFrame>
  );
}

/**
 * A commander's portrait cell from the sheet, drawn at `scale`. Scaling the
 * background box keeps the cell on whole pixels, so the art stays crisp
 * instead of being resampled by a transform.
 */
export function PortraitSprite({
  portrait,
  scale,
  xstyle,
  ...aria
}: {
  "aria-label"?: string;
  portrait: CoPortraitEntry;
  role?: "img";
  scale: number;
  title?: string;
  xstyle?: stylex.StyleXStyles;
}) {
  return (
    // A pixel-sprite crop of the portrait sheet: a bare box no component draws.
    <span
      {...aria}
      {...stylex.props(
        styles.sprite,
        styles.spriteAt(
          portrait.sheetUrl,
          Math.round(portrait.sheetWidth * scale),
          Math.round(portrait.sheetHeight * scale),
          Math.round(portrait.x * scale),
          Math.round(portrait.y * scale),
          Math.round(portrait.width * scale),
          Math.round(portrait.height * scale),
        ),
        xstyle,
      )}
    />
  );
}

/**
 * The stamp itself: a white perforated margin around a field, with the
 * stamp's shadow. The one definition of the stamp's paper; `CoStamp` fills it
 * with a portrait at a whole scale, and `CoPortrait` reuses it for portraits
 * sized freely. `overlay` draws over the stamp (the cancellation lines).
 */
export function StampFrame({
  children,
  fieldXstyle,
  isSmall = false,
  isUsed = false,
  overlay,
  xstyle,
  ...aria
}: {
  "aria-hidden"?: boolean;
  "aria-label"?: string;
  children: ReactNode;
  fieldXstyle?: stylex.StyleXStyles;
  isSmall?: boolean;
  isUsed?: boolean;
  overlay?: ReactNode;
  role?: "img";
  title?: string;
  xstyle?: stylex.StyleXStyles;
}) {
  return (
    <Stack {...aria} as="span" xstyle={[styles.stamp, xstyle]}>
      <Stack as="span" xstyle={[styles.perf, isSmall && styles.perfSmall, isUsed && styles.used]}>
        <Stack as="span" xstyle={[styles.field, fieldXstyle]}>
          {children}
        </Stack>
      </Stack>
      {overlay}
    </Stack>
  );
}

export type CoStampSize = "sm" | "md" | "lg";

const STAMP_SCALE: Record<CoStampSize, number> = { sm: 1, md: 2, lg: 3 };

/**
 * Violet cancellation lines: the wavy strokes a post office prints across a
 * used stamp. Over a fallen army's stamp, a banned commander, or beside a
 * postmark. Decorative; the surrounding words carry the meaning.
 */
export function CancelMark({ xstyle }: { xstyle?: stylex.StyleXStyles }) {
  return (
    <svg aria-hidden="true" viewBox="0 0 46 30" {...stylex.props(styles.cancel, xstyle)}>
      <path d="M0 6c6-4 9 4 15 0s9-4 15 0 9 4 16 0" />
      <path d="M0 15c6-4 9 4 15 0s9-4 15 0 9 4 16 0" />
      <path d="M0 24c6-4 9 4 15 0s9-4 15 0 9 4 16 0" />
    </svg>
  );
}

const styles = stylex.create({
  // The shadow sits on the wrapper: a filter on the masked element would be
  // cut away by its own mask.
  stamp: {
    position: "relative",
    display: "inline-grid",
    flexShrink: 0,
    filter: awbrnVars.shadowStamp,
  },
  // A used stamp is grayed; the cancellation lines over it stay violet.
  used: {
    filter: "grayscale(1)",
  },
  // The white margin, perforated: 2px holes at a 6px pitch around the edge.
  perf: {
    display: "inline-grid",
    placeItems: "center",
    padding: "5px",
    backgroundColor: awbrnVars.stampWhite,
    mask: "linear-gradient(#000 0 0) content-box, radial-gradient(circle at center, transparent 1.7px, #000 2.1px) -3px -3px / 6px 6px round",
  },
  perfSmall: {
    padding: "4px",
  },
  field: {
    display: "grid",
    placeItems: "center",
    overflow: "hidden",
    backgroundColor: awbrnVars.paperWell,
    boxShadow: `inset 0 0 0 1px ${awbrnVars.inkHairline}`,
  },
  fieldSize: (size: number) => ({
    inlineSize: `${size}px`,
    blockSize: `${size}px`,
  }),
  fieldColor: (color: string) => ({
    backgroundColor: color,
  }),
  sprite: {
    display: "block",
    flexShrink: 0,
    backgroundRepeat: "no-repeat",
    imageRendering: "pixelated",
  },
  spriteAt: (
    url: string,
    sheetWidth: number,
    sheetHeight: number,
    x: number,
    y: number,
    width: number,
    height: number,
  ) => ({
    inlineSize: `${width}px`,
    blockSize: `${height}px`,
    backgroundImage: `url(${url})`,
    backgroundSize: `${sheetWidth}px ${sheetHeight}px`,
    backgroundPosition: `-${x}px -${y}px`,
  }),
  banned: {
    opacity: 0.45,
  },
  // An open seat: the stamp's outline, waiting for a commander.
  openSeat: {
    display: "inline-grid",
    placeItems: "center",
    flexShrink: 0,
    borderColor: awbrnVars.ruleStrong,
    borderRadius: "2px",
    borderStyle: "dashed",
    borderWidth: "1.5px",
    color: awbrnVars.textTertiary,
  },
  openSeatSize: (size: number) => ({
    inlineSize: `${size}px`,
    blockSize: `${size}px`,
  }),
  grayed: {
    display: "inline-grid",
    filter: "grayscale(1) opacity(0.5)",
  },
  cancel: {
    flexShrink: 0,
    inlineSize: "46px",
    blockSize: "30px",
    color: awbrnVars.violet,
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 2,
    opacity: 0.85,
  },
  cancelOverStamp: {
    position: "absolute",
    insetBlockStart: "50%",
    insetInlineStart: "50%",
    inlineSize: "120%",
    blockSize: "auto",
    transform: "translate(-50%, -50%)",
    opacity: 1,
  },
});
