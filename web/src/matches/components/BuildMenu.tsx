import { HStack, Stack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { Tooltip } from "@astryxdesign/core/Tooltip";
import { VisuallyHidden } from "@astryxdesign/core/VisuallyHidden";
import { borderVars, radiusVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { uiAtlasSpriteStyle, unitSpriteStyle } from "#/components/game_sprites.ts";
import {
  BoardMenuShell,
  boardMenuStyles,
  followPointerCursor,
  type BoardMenuPresentation,
} from "#/matches/components/BoardMenu.tsx";
import { boardMenuLayout } from "#/matches/components/boardMenuLayout.stylex.ts";
import { LAB_RULE, LabTag } from "#/matches/components/UnitRules.tsx";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import type { ProductionOption, ProductionSite, UnitKind } from "#/wasm/awbrn_wasm.js";

interface BuildMenuProps {
  /** Where the board was pressed, in surface pixels. Null when no pointer opened it. */
  anchor: { x: number; y: number } | null;
  /** How far the menu stays from the anchor, to leave its tile in view. */
  anchorClearance?: number;
  /** Why the commands are inert, when they are. */
  disabledReason?: string;
  factionCode: string;
  funds: number | null;
  isEnabled: boolean;
  /** The sheet only: false while it leaves. */
  isOpen?: boolean;
  onBuild: (unit: UnitKind) => void;
  onDismiss: () => void;
  /** Hands the keyboard back to the board once the menu has closed. */
  onRestoreFocus: () => void;
  options: ProductionOption[];
  presentation: BoardMenuPresentation;
  site: ProductionSite;
}

export function BuildMenu({
  anchor,
  anchorClearance,
  disabledReason,
  factionCode,
  funds,
  isEnabled,
  isOpen,
  onBuild,
  onDismiss,
  onRestoreFocus,
  options,
  presentation,
  site,
}: BuildMenuProps) {
  return (
    <BoardMenuShell
      anchor={anchor}
      anchorClearance={anchorClearance}
      isOpen={isOpen}
      label={`Build at ${facilityLabel(site)}`}
      onDismiss={onDismiss}
      onRestoreFocus={onRestoreFocus}
      presentation={presentation}
    >
      {({ isSheet, spriteScale }) => (
        <BuildMenuBody
          disabledReason={disabledReason}
          factionCode={factionCode}
          funds={funds}
          isSheet={isSheet}
          isEnabled={isEnabled}
          onBuild={onBuild}
          options={options}
          site={site}
          spriteScale={spriteScale}
        />
      )}
    </BoardMenuShell>
  );
}

/**
 * The readout and the orders, shared by both presentations.
 *
 * Only the sprite scale and the row height differ between a menu read at arm's
 * length with a mouse and one pressed with a thumb; the content, the order, and
 * the wording are the same command in both.
 */
function BuildMenuBody({
  disabledReason,
  factionCode,
  funds,
  isSheet,
  isEnabled,
  onBuild,
  options,
  site,
  spriteScale,
}: {
  disabledReason?: string;
  factionCode: string;
  funds: number | null;
  isSheet: boolean;
  isEnabled: boolean;
  onBuild: (unit: UnitKind) => void;
  options: ProductionOption[];
  site: ProductionSite;
  spriteScale: 1 | 2;
}) {
  const [previewCost, setPreviewCost] = useState<number | null>(null);

  return (
    <VStack gap={0} xstyle={boardMenuStyles.body}>
      {/* One line: what this building is, and what the treasury holds. The
          facility is anchored to the leading edge and the readout to the
          trailing one, so the remainder appearing under the cursor grows into
          the gap between them instead of shifting either. */}
      <HStack
        align="center"
        gap={3}
        justify="between"
        wrap="wrap"
        xstyle={[boardMenuStyles.header, styles.header, isSheet && styles.sheetHeader]}
      >
        <Text type="label" xstyle={styles.facility}>
          {facilityLabel(site)}
        </Text>
        <FundsLine funds={funds} previewCost={previewCost} />
      </HStack>

      {options.length === 0 ? (
        <VStack gap={0} paddingBlock={3} paddingInline={3}>
          <Text color="secondary" type="supporting" xstyle={styles.empty}>
            Nothing can be built here under this match's rules.
          </Text>
        </VStack>
      ) : (
        <VStack gap={0} xstyle={isSheet ? undefined : [boardMenuStyles.list, styles.listCap]}>
          {options.map((option) =>
            option.labLocked ? (
              <LockedBuildRow
                factionCode={factionCode}
                key={option.unit}
                option={option}
                spriteScale={spriteScale}
              />
            ) : (
              <BuildRow
                factionCode={factionCode}
                isEnabled={isEnabled}
                key={option.unit}
                onBuild={onBuild}
                onPreview={setPreviewCost}
                option={option}
                spriteScale={spriteScale}
                title={disabledReason}
              />
            ),
          )}
        </VStack>
      )}

      {isEnabled || !disabledReason ? null : (
        <VStack gap={0} paddingBlock={2} paddingInline={3} xstyle={boardMenuStyles.notice}>
          <Text color="secondary" type="supporting">
            {disabledReason}
          </Text>
        </VStack>
      )}
    </VStack>
  );
}

/**
 * The treasury, and what the order under the cursor would leave of it.
 *
 * The remainder is the number a player is actually doing in their head while
 * they read the list, so the menu does the subtraction as the cursor moves.
 */
function FundsLine({ funds, previewCost }: { funds: number | null; previewCost: number | null }) {
  const remaining = funds !== null && previewCost !== null ? funds - previewCost : null;
  const coinStyle = uiAtlasSpriteStyle("Coin.png", 2);

  return (
    <HStack align="center" gap={2} xstyle={styles.fundsLine}>
      {/* Raw span: an inline atlas crop, which no Astryx component takes. */}
      <span aria-hidden="true" style={coinStyle ?? undefined} {...stylex.props(styles.coin)} />
      <Text type="label" xstyle={styles.funds}>
        <VisuallyHidden>Funds</VisuallyHidden>
        {funds === null ? "--" : funds.toLocaleString()}
      </Text>
      {remaining === null ? null : (
        <Text type="label" xstyle={styles.fundsAfter}>
          <VisuallyHidden>, leaving</VisuallyHidden>
          <Text color="inherit" type="inherit" aria-hidden="true">
            →{" "}
          </Text>
          {remaining.toLocaleString()}
        </Text>
      )}
    </HStack>
  );
}

/**
 * One order: the unit as the army will build it, its name, and its price.
 *
 * The sprite is the army's own, so the list reads as this player's roster
 * rather than as a catalog, and the row under the cursor wears the game's
 * orange selection the way a chosen menu key does everywhere else.
 */
function BuildRow({
  factionCode,
  isEnabled,
  onBuild,
  onPreview,
  option,
  spriteScale,
  title,
}: {
  factionCode: string;
  isEnabled: boolean;
  onBuild: (unit: UnitKind) => void;
  onPreview: (cost: number | null) => void;
  option: ProductionOption;
  spriteScale: 1 | 2;
  title?: string;
}) {
  const spriteStyle = unitSpriteStyle(option.unit, factionCode, 2);
  const isOrderable = isEnabled && option.affordable;

  return (
    <button
      disabled={!isOrderable}
      aria-label={
        option.affordable
          ? undefined
          : `${option.name}, ${option.cost.toLocaleString()}, more than your funds`
      }
      onBlur={() => onPreview(null)}
      onClick={() => onBuild(option.unit)}
      onFocus={() => onPreview(option.cost)}
      // The cursor follows the pointer rather than doubling it: entering a row
      // moves the one cursor there, so hover and keyboard never light two rows.
      onPointerMove={followPointerCursor}
      title={isEnabled ? undefined : title}
      type="button"
      {...stylex.props(
        boardMenuStyles.row,
        styles.row,
        spriteScale === 2 && boardMenuStyles.rowSpacious,
        !isEnabled && boardMenuStyles.rowInert,
        !option.affordable && styles.rowUnaffordable,
      )}
    >
      {/* Raw span: an inline atlas crop, which no Astryx component takes. */}
      <span
        aria-hidden="true"
        style={spriteStyle ?? undefined}
        {...stylex.props(boardMenuStyles.sprite, !option.affordable && styles.spriteOff)}
      />
      <Text color="inherit" type="inherit" xstyle={boardMenuStyles.rowName}>
        {option.name}
      </Text>
      <Stack as="span" xstyle={styles.rowCost}>
        <VisuallyHidden>costs</VisuallyHidden>
        {option.cost.toLocaleString("en-US")}
      </Stack>
    </button>
  );
}

/**
 * A Lab unit the player cannot build until they own a Lab: the reward of a
 * capture, in view where it is spent. It stays focusable and tappable, which
 * is how its tooltip names the rule.
 */
function LockedBuildRow({
  factionCode,
  option,
  spriteScale,
}: {
  factionCode: string;
  option: ProductionOption;
  spriteScale: 1 | 2;
}) {
  const spriteStyle = unitSpriteStyle(option.unit, factionCode, 2);
  return (
    <Tooltip content={LAB_RULE} focusTrigger="always" placement="end" touchTrigger="tap">
      <button
        aria-disabled="true"
        aria-label={`${option.name}, ${option.cost.toLocaleString()}`}
        onPointerMove={followPointerCursor}
        type="button"
        {...stylex.props(
          boardMenuStyles.row,
          styles.row,
          spriteScale === 2 && boardMenuStyles.rowSpacious,
          styles.rowLocked,
        )}
      >
        {/* Raw span: an inline atlas crop, which no Astryx component takes. */}
        <span
          aria-hidden="true"
          style={spriteStyle ?? undefined}
          {...stylex.props(boardMenuStyles.sprite, styles.spriteOff)}
        />
        <VStack as="span" xstyle={styles.lockedName}>
          <Text color="inherit" type="inherit" xstyle={boardMenuStyles.rowName}>
            {option.name}
          </Text>
          <LabTag isLocked />
        </VStack>
        <Stack as="span" xstyle={styles.rowCost}>
          {option.cost.toLocaleString("en-US")}
        </Stack>
      </button>
    </Tooltip>
  );
}

/** "Base", "Airport", "Port" — the facility as the game names it. */
function facilityLabel(site: ProductionSite): string {
  return site.facility
    .split("-")
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");
}

const styles = stylex.create({
  header: {
    rowGap: spacingVars["--spacing-1"],
  },
  sheetHeader: {
    paddingInlineEnd: "56px",
    minBlockSize: "56px",
  },
  facility: {
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    color: awbrnVars.textOnInkStrong,
    fontSize: awbrnVars.text15,
    fontWeight: 750,
  },
  fundsLine: {
    flex: "0 0 auto",
  },
  coin: {
    display: "block",
    flex: "0 0 auto",
  },
  funds: {
    color: awbrnVars.textOnInkStrong,
    fontFamily: awbrnVars.fontFigure,
    fontSize: awbrnVars.text15,
    fontStretch: "87.5%",
    fontWeight: 600,
    fontVariantNumeric: "tabular-nums",
  },
  fundsAfter: {
    color: awbrnVars.textOnInkSecondary,
    fontFamily: awbrnVars.fontFigure,
    fontSize: awbrnVars.text15,
    fontStretch: "87.5%",
    fontWeight: 600,
    fontVariantNumeric: "tabular-nums",
  },
  listCap: {
    maxBlockSize: boardMenuLayout.buildListMaxBlockSize,
  },
  row: {
    display: "grid",
    gridTemplateColumns: "34px minmax(0, 1fr) auto",
    gap: awbrnVars.spacing2_5,
    paddingInlineStart: awbrnVars.spacing3_5,
  },
  rowUnaffordable: {
    cursor: "not-allowed",
    color: awbrnVars.textTertiary,
    backgroundColor: awbrnVars.sheet,
  },
  // The tooltip's wrapper makes each locked row a first child, which would
  // drop the divider every other row has.
  rowLocked: {
    borderBlockStartWidth: borderVars["--border-width"],
    cursor: "default",
    color: awbrnVars.textTertiary,
    backgroundColor: awbrnVars.sheet,
  },
  lockedName: {
    alignItems: "start",
    gap: spacingVars["--spacing-1"],
    paddingBlock: spacingVars["--spacing-1-5"],
  },
  spriteOff: {
    filter: "grayscale(1) opacity(0.55)",
  },
  rowCost: {
    display: "grid",
    justifyItems: "end",
    fontFamily: awbrnVars.fontFigure,
    fontSize: awbrnVars.text13_5,
    fontStretch: "87.5%",
    fontWeight: 600,
    fontVariantNumeric: "tabular-nums",
    lineHeight: 1.05,
  },
  empty: {
    padding: spacingVars["--spacing-3"],
    borderWidth: borderVars["--border-width"],
    borderStyle: "dashed",
    borderColor: awbrnVars.ruleStrong,
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: awbrnVars.paperWell,
  },
});
