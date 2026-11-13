import { Text } from "@astryxdesign/core/Text";
import { BottomSheet } from "@astryxdesign/core/BottomSheet";
import { Dialog } from "@astryxdesign/core/Dialog";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { borderVars, colorVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useId, type ReactNode } from "react";
import { UiSprite } from "#/components/UiSprite.tsx";
import { infantrySpriteStyle } from "#/components/game_sprites.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { figureStyles } from "#/ui/Figure.tsx";
import { overlayMotion } from "#/ui/overlayMotion.ts";
import { useCompactTouch } from "#/ui/useCompactTouch.ts";
import type { TurnReadinessChanged } from "#/wasm/awbrn_wasm.js";

/** What ending the turn leaves: idle units, bases that can still build, a power. */
function residueLabel(readiness: TurnReadinessChanged, isPowerReady: boolean): string {
  const parts: string[] = [];
  if (readiness.idleUnits > 0) {
    parts.push(`${readiness.idleUnits} ${readiness.idleUnits === 1 ? "unit" : "units"} idle`);
  }
  if (readiness.freeSites > 0) {
    parts.push(`${readiness.freeSites} ${readiness.freeSites === 1 ? "base" : "bases"} can build`);
  }
  if (isPowerReady) parts.push("CO power ready");
  return parts.join(", ");
}

/** The residue as sprites and counts: `[infantry] 2 [base] 1 [POWER]`. */
export function TurnResidue({
  factionCode,
  isPowerReady = false,
  readiness,
  size = "sm",
}: {
  factionCode: string;
  isPowerReady?: boolean;
  readiness: TurnReadinessChanged;
  size?: "sm" | "lg";
}) {
  const infantry = infantrySpriteStyle(factionCode);
  return (
    <HStack
      as="span"
      aria-label={residueLabel(readiness, isPowerReady)}
      role="img"
      xstyle={[styles.residue, size === "lg" && styles.residueLarge]}
    >
      {readiness.idleUnits > 0 ? (
        <HStack as="span" xstyle={styles.residueItem}>
          {/* Raw span: an inline atlas crop, which no Astryx component takes. */}
          <span aria-hidden="true" style={infantry ?? undefined} {...stylex.props(styles.sprite)} />
          <b {...stylex.props(figureStyles.figure, styles.count)}>{readiness.idleUnits}</b>
        </HStack>
      ) : null}
      {readiness.freeSites > 0 ? (
        <HStack as="span" xstyle={styles.residueItem}>
          <UiSprite name="BuildingsCaptured.png" scale={size === "lg" ? 2 : 1} />
          <b {...stylex.props(figureStyles.figure, styles.count)}>{readiness.freeSites}</b>
        </HStack>
      ) : null}
      {isPowerReady && size === "lg" ? <UiSprite name="NormalPower.png" /> : null}
    </HStack>
  );
}

/**
 * End turn. It carries what ending leaves (`[infantry] 2 [base] 1`); when
 * nothing is left it turns red and drops the counts. One Red Key: it is
 * secondary on ink while units remain.
 */
export function EndTurnKey({
  factionCode,
  isDisabled,
  isLoading,
  isSpent,
  onPress,
  readiness,
  size,
  tooltip,
}: {
  factionCode: string;
  isDisabled: boolean;
  isLoading: boolean;
  isSpent: boolean;
  onPress: () => void;
  readiness: TurnReadinessChanged | undefined;
  size: "rail" | "dock";
  tooltip?: string;
}) {
  const showResidue = !isSpent && readiness !== undefined && !isDisabled;
  return (
    <Button
      isDisabled={isDisabled}
      isLoading={isLoading}
      label="End turn"
      onClick={onPress}
      tooltip={tooltip}
      variant={isSpent && !isDisabled ? "primary" : "on-ink"}
      xstyle={[styles.key, size === "dock" && styles.keyDock]}
    >
      <HStack as="span" xstyle={[styles.keyBody, size === "dock" && styles.keyBodyDock]}>
        <Text color="inherit" type="inherit">
          End turn
        </Text>
        {showResidue ? <TurnResidue factionCode={factionCode} readiness={readiness} /> : null}
      </HStack>
    </Button>
  );
}

/**
 * A paper dialog (desktop) or a hugging bottom sheet (phone): a title in the
 * body, what it is about, then keys on a paper foot. The title names it.
 *
 * Motion is the overlays' own (`overlayMotion`): the dialog appears in place
 * over 120ms and leaves over 90ms, both stepped; the sheet rises half way,
 * then all the way. Nothing moves under reduced motion. A confirmation opens
 * on its safe key: mark that key `data-autofocus`.
 */
export function PaperDialog({
  children,
  footer,
  headEnd,
  headStart,
  isOpen,
  onOpenChange,
  purpose = "info",
  title,
}: {
  children?: ReactNode;
  footer: ReactNode;
  headEnd?: ReactNode;
  headStart?: ReactNode;
  isOpen: boolean;
  onOpenChange: (isOpen: boolean) => void;
  purpose?: "info" | "form" | "required";
  title: string;
}) {
  const isCompactTouch = useCompactTouch();
  const titleId = useId();
  const content = (
    <VStack xstyle={styles.dialog}>
      <header {...stylex.props(styles.dialogHead)}>
        {headStart}
        <h2 id={titleId} {...stylex.props(styles.dialogTitle)}>
          {title}
        </h2>
        {headEnd}
      </header>
      {children ? <VStack xstyle={styles.dialogBody}>{children}</VStack> : null}
      <footer {...stylex.props(styles.dialogFoot)}>{footer}</footer>
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
        xstyle={[styles.sheetPanel, overlayMotion.sheet, !isOpen && overlayMotion.sheetLeaving]}
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
      width={420}
      xstyle={[overlayMotion.dialog, isOpen && overlayMotion.dialogOpen]}
    >
      {content}
    </Dialog>
  );
}

/**
 * End your turn? What is left as sprites and counts, that it cannot be undone,
 * Keep playing and End turn.
 */
export function EndTurnDialog({
  factionCode,
  isEnding,
  isOpen,
  isPowerReady,
  onConfirm,
  onOpenChange,
  readiness,
}: {
  factionCode: string;
  isEnding: boolean;
  isOpen: boolean;
  isPowerReady: boolean;
  onConfirm: () => void;
  onOpenChange: (isOpen: boolean) => void;
  readiness: TurnReadinessChanged | undefined;
}) {
  return (
    <PaperDialog
      footer={
        <>
          <Button
            data-autofocus
            label="Keep playing"
            onClick={() => onOpenChange(false)}
            variant="secondary"
          />
          <Button isLoading={isEnding} label="End turn" onClick={onConfirm} variant="primary" />
        </>
      }
      isOpen={isOpen}
      onOpenChange={onOpenChange}
      title="End your turn?"
    >
      {readiness ? (
        <TurnResidue
          factionCode={factionCode}
          isPowerReady={isPowerReady}
          readiness={readiness}
          size="lg"
        />
      ) : null}
      <p {...stylex.props(styles.undone)}>This cannot be undone.</p>
    </PaperDialog>
  );
}

const styles = stylex.create({
  residue: {
    display: "inline-flex",
    alignItems: "center",
    gap: awbrnVars.spacing2_5,
  },
  residueLarge: {
    gap: awbrnVars.spacing3_5,
    fontSize: awbrnVars.text17,
  },
  residueItem: {
    display: "inline-flex",
    alignItems: "center",
    gap: spacingVars["--spacing-1"],
  },
  sprite: {
    display: "inline-block",
    flexShrink: 0,
  },
  count: {
    fontSize: "inherit",
    fontWeight: 600,
  },
  key: {
    flexGrow: 1,
    minInlineSize: 0,
    blockSize: "48px",
  },
  keyDock: {
    blockSize: "52px",
  },
  keyBody: {
    display: "inline-flex",
    alignItems: "center",
    gap: spacingVars["--spacing-3"],
    fontSize: awbrnVars.text15,
    fontWeight: 700,
  },
  keyBodyDock: {
    flexDirection: "column",
    gap: spacingVars["--spacing-0-5"],
    fontSize: awbrnVars.text14,
  },
  dialog: {
    display: "grid",
    backgroundColor: awbrnVars.sheet,
    color: colorVars["--color-text-primary"],
  },
  dialogHead: {
    display: "flex",
    alignItems: "center",
    gap: awbrnVars.spacing3_5,
    paddingBlock: `${spacingVars["--spacing-5"]} ${spacingVars["--spacing-2"]}`,
    paddingInline: "22px",
  },
  dialogTitle: {
    flexGrow: 1,
    margin: 0,
    fontSize: awbrnVars.text26,
    fontStretch: "68%",
    fontWeight: 800,
    lineHeight: 1.05,
    wordSpacing: "0.1em",
  },
  dialogBody: {
    display: "grid",
    gap: awbrnVars.spacing2_5,
    paddingBlock: `${spacingVars["--spacing-2"]} ${awbrnVars.spacing4_5}`,
    paddingInline: "22px",
  },
  // Astryx pads the panel by the home-indicator inset, under the paper foot.
  sheetPanel: {
    backgroundColor: awbrnVars.paper,
  },
  dialogFoot: {
    display: "flex",
    justifyContent: "flex-end",
    flexWrap: "wrap",
    gap: awbrnVars.spacing2_5,
    paddingBlock: awbrnVars.spacing3_5,
    paddingInline: "22px",
    borderBlockStartColor: awbrnVars.rule,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: borderVars["--border-width"],
    backgroundColor: awbrnVars.paper,
  },
  undone: {
    margin: 0,
    color: colorVars["--color-text-secondary"],
    fontSize: awbrnVars.text14,
  },
});
