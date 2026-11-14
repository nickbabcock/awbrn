import { Text } from "@astryxdesign/core/Text";
import { BottomSheet } from "@astryxdesign/core/BottomSheet";
import { Dialog } from "@astryxdesign/core/Dialog";
import { Heading } from "@astryxdesign/core/Heading";
import { Icon } from "@astryxdesign/core/Icon";
import { HStack, Stack, VStack } from "@astryxdesign/core/Stack";
import {
  borderVars,
  colorVars,
  durationVars,
  spacingVars,
  textSizeVars,
  typeScaleVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useId, type ReactNode } from "react";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { overlayMotion } from "#/ui/overlayMotion.ts";
import { Figure } from "#/ui/Figure.tsx";
import { IconClose } from "#/ui/icons.tsx";
import { useCompactTouch } from "#/ui/useCompactTouch.ts";

/**
 * A dialog on desktop and a bottom sheet on a compact touch screen. A
 * confirmation opens on its safe key: mark that key
 * `data-autofocus`.
 */
export function ModalSheet({
  children,
  footer,
  hasCloseKey = true,
  headMeta,
  headStart,
  isOpen,
  onOpenChange,
  purpose = "info",
  title,
  width = 420,
}: {
  children: ReactNode;
  footer?: ReactNode;
  hasCloseKey?: boolean;
  headMeta?: ReactNode;
  headStart?: ReactNode;
  isOpen: boolean;
  onOpenChange: (isOpen: boolean) => void;
  /** `form` blocks light dismiss. */
  purpose?: "info" | "form";
  title: string;
  width?: number;
}) {
  const isCompactTouch = useCompactTouch();
  const titleId = useId();

  const content = (
    <VStack gap={0}>
      <HStack align="center" gap={3} justify="between" xstyle={styles.head}>
        <HStack align="center" gap={2} xstyle={styles.headTitle}>
          {headStart}
          <Heading
            accessibilityLevel={2}
            color="inherit"
            id={titleId}
            level={4}
            xstyle={styles.title}
          >
            {title}
          </Heading>
          {headMeta ? <Figure tone="on-ink-secondary">{headMeta}</Figure> : null}
        </HStack>
        {hasCloseKey ? (
          <Button
            icon={<Icon icon={IconClose} />}
            isIconOnly
            label="Close"
            onClick={() => onOpenChange(false)}
            variant="ghost-on-ink"
            xstyle={styles.close}
          />
        ) : null}
      </HStack>
      <VStack gap={0} xstyle={styles.body}>
        {children}
      </VStack>
      {footer ? (
        <HStack align="center" gap={2} justify="end" wrap="wrap" xstyle={styles.footer}>
          {footer}
        </HStack>
      ) : null}
    </VStack>
  );

  if (isCompactTouch) {
    return (
      <BottomSheet
        height="hug"
        isOpen={isOpen}
        label={title}
        onOpenChange={onOpenChange}
        padding={0}
        purpose={purpose}
        xstyle={[
          styles.sheetPanel,
          Boolean(footer) && styles.sheetPanelUnderFooter,
          overlayMotion.sheet,
          !isOpen && overlayMotion.sheetLeaving,
        ]}
      >
        {content}
      </BottomSheet>
    );
  }

  return (
    <Dialog
      aria-labelledby={titleId}
      isOpen={isOpen}
      onOpenChange={onOpenChange}
      padding={0}
      purpose={purpose}
      width={width}
      xstyle={[overlayMotion.dialog, isOpen && overlayMotion.dialogOpen]}
    >
      {content}
    </Dialog>
  );
}

/** One row of a menu. `children` holds anything under the label. */
export function MenuRow({
  children,
  icon,
  isChosen = false,
  isDisabled = false,
  label,
  onClick,
}: {
  children?: ReactNode;
  /** A drawn icon or a sprite in place of the caret (the chosen caret then hides). */
  icon?: ReactNode;
  isChosen?: boolean;
  isDisabled?: boolean;
  label: ReactNode;
  onClick?: () => void;
}) {
  return (
    <button
      aria-current={isChosen ? "true" : undefined}
      disabled={isDisabled}
      onClick={onClick}
      type="button"
      {...stylex.props(styles.row, isChosen && styles.rowChosen, isDisabled && styles.rowOff)}
    >
      {icon ? (
        <Stack aria-hidden="true" as="span" xstyle={styles.rowIcon}>
          {icon}
        </Stack>
      ) : (
        <Stack aria-hidden="true" as="span" xstyle={[styles.caret, isChosen && styles.caretOn]}>
          <CaretGlyph />
        </Stack>
      )}
      <VStack as="span" gap={1.5} xstyle={styles.rowBody}>
        <Text type="inherit">{label}</Text>
        {children}
      </VStack>
    </button>
  );
}

function CaretGlyph() {
  return (
    <svg fill="currentColor" height="16" viewBox="0 0 16 16" width="16">
      <path d="M5 3l6 5-6 5z" />
    </svg>
  );
}

const rule = {
  borderBlockStartColor: awbrnVars.rule,
  borderBlockStartStyle: "solid",
  borderBlockStartWidth: borderVars["--border-width"],
} as const;

const styles = stylex.create({
  head: {
    minBlockSize: "56px",
    flexShrink: 0,
    paddingBlock: awbrnVars.spacing2_5,
    paddingInlineStart: awbrnVars.spacing3_5,
    paddingInlineEnd: spacingVars["--spacing-1-5"],
    backgroundColor: awbrnVars.ink900,
    color: awbrnVars.textOnInkStrong,
  },
  // In a bottom sheet the grip sits on the ink head, so the panel is ink and
  // the body lays paper over it.
  sheetPanel: {
    backgroundColor: awbrnVars.ink900,
    [colorVars["--color-background-surface"]]: awbrnVars.ink900,
  },
  // Astryx pads the panel by the home-indicator inset, below the content. Under
  // a footer that band is paper, so the foot runs to the screen's edge; the grip
  // still paints the surface (ink).
  sheetPanelUnderFooter: {
    backgroundColor: awbrnVars.paper,
  },
  body: {
    backgroundColor: awbrnVars.sheet,
    [colorVars["--color-background-surface"]]: awbrnVars.sheet,
  },
  headTitle: {
    minInlineSize: 0,
  },
  title: {
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  close: {
    color: awbrnVars.textOnInkStrong,
  },
  footer: {
    ...rule,
    paddingBlockStart: awbrnVars.spacing3_5,
    paddingBlockEnd: awbrnVars.spacing3_5,
    paddingInline: awbrnVars.spacing5_5,
    backgroundColor: awbrnVars.paper,
  },
  row: {
    display: "grid",
    gridTemplateColumns: `${spacingVars["--spacing-5"]} minmax(0, 1fr) auto`,
    alignItems: "center",
    gap: spacingVars["--spacing-2"],
    inlineSize: "100%",
    minBlockSize: {
      default: "48px",
      "@media (pointer: coarse)": "56px",
    },
    paddingBlock: spacingVars["--spacing-2"],
    paddingInlineStart: awbrnVars.spacing2_5,
    paddingInlineEnd: awbrnVars.spacing3_5,
    borderStyle: "none",
    ...rule,
    backgroundColor: {
      default: awbrnVars.sheet,
      ":hover": { default: null, [pageLayout.hoverMedia]: awbrnVars.paper },
      ":active": awbrnVars.paper,
    },
    color: "inherit",
    fontFamily: awbrnVars.fontUi,
    fontSize: {
      default: typeScaleVars["--text-body-size"],
      "@media (pointer: coarse)": textSizeVars["--font-size-lg"],
    },
    fontWeight: typeScaleVars["--text-label-weight"],
    textAlign: "start",
    cursor: "pointer",
    transitionProperty: "background-color",
    transitionDuration: {
      default: durationVars["--duration-fast"],
      [pageLayout.reducedMotionMedia]: "0s",
    },
    transitionTimingFunction: awbrnVars.easeStep,
  },
  rowChosen: {
    backgroundColor: awbrnVars.redWash,
  },
  rowOff: {
    backgroundColor: awbrnVars.sheet,
    color: awbrnVars.textTertiary,
    cursor: "not-allowed",
  },
  caret: {
    display: "inline-grid",
    placeItems: "center",
    color: awbrnVars.red,
    opacity: 0,
  },
  caretOn: {
    opacity: 1,
  },
  rowIcon: {
    display: "inline-grid",
    placeItems: "center",
    color: colorVars["--color-text-secondary"],
  },
  rowBody: {
    minInlineSize: 0,
  },
});
