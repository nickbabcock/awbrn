import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { VisuallyHidden } from "@astryxdesign/core/VisuallyHidden";
import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { uiAtlasSpriteStyle, unitSpriteStyle } from "#/components/game_sprites.ts";
import {
  BoardMenuShell,
  boardMenuStyles,
  type BoardMenuPresentation,
} from "#/matches/components/BoardMenu.tsx";
import type { ProductionOption, ProductionSite, UnitKind } from "#/wasm/awbrn_wasm.js";

interface BuildMenuProps {
  /** Where the board was pressed, in surface pixels. Null when no pointer opened it. */
  anchor: { x: number; y: number } | null;
  /** Why the commands are inert, when they are. */
  disabledReason?: string;
  factionCode: string;
  funds: number | null;
  isEnabled: boolean;
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
  disabledReason,
  factionCode,
  funds,
  isEnabled,
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
      label={`Build at ${facilityLabel(site)}`}
      onDismiss={onDismiss}
      onRestoreFocus={onRestoreFocus}
      presentation={presentation}
    >
      {({ spriteScale }) => (
        <BuildMenuBody
          disabledReason={disabledReason}
          factionCode={factionCode}
          funds={funds}
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
  isEnabled,
  onBuild,
  options,
  site,
  spriteScale,
}: {
  disabledReason?: string;
  factionCode: string;
  funds: number | null;
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
        paddingBlock={spriteScale === 2 ? 2 : 1}
        paddingInline={3}
        xstyle={boardMenuStyles.header}
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
        <VStack gap={0} xstyle={boardMenuStyles.list}>
          {options.map((option) => (
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
          ))}
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
      <span aria-hidden="true" style={coinStyle ?? undefined} {...stylex.props(styles.coin)} />
      <Text type="label" xstyle={styles.funds}>
        <VisuallyHidden>Funds</VisuallyHidden>
        {funds === null ? "--" : funds.toLocaleString()}
      </Text>
      {remaining === null ? null : (
        <Text type="label" xstyle={styles.fundsAfter}>
          <VisuallyHidden>, leaving</VisuallyHidden>
          <span aria-hidden="true">→ </span>
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
 * rather than as a catalogue, and the row under the cursor wears the game's
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
  const spriteStyle = unitSpriteStyle(option.unit, factionCode, spriteScale);
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
      onPointerEnter={(event) => event.currentTarget.focus({ preventScroll: true })}
      title={isEnabled ? undefined : title}
      type="button"
      {...stylex.props(
        boardMenuStyles.row,
        spriteScale === 2 && boardMenuStyles.rowSpacious,
        !isEnabled && boardMenuStyles.rowInert,
        !option.affordable && styles.rowUnaffordable,
      )}
    >
      <span
        aria-hidden="true"
        style={spriteStyle ?? undefined}
        {...stylex.props(boardMenuStyles.sprite)}
      />
      <span {...stylex.props(boardMenuStyles.rowName, !option.affordable && styles.rowStruck)}>
        {option.name}
      </span>
      <span {...stylex.props(styles.rowCost, !option.affordable && styles.rowStruck)}>
        <VisuallyHidden>costs</VisuallyHidden>
        {option.cost.toLocaleString()}
      </span>
    </button>
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
  facility: {
    color: "var(--color-text-secondary)",
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  fundsLine: {
    flex: "0 0 auto",
    minBlockSize: "var(--size-build-funds-line)",
  },
  coin: {
    display: "block",
    flex: "0 0 auto",
  },
  funds: {
    color: "var(--color-text-primary)",
    fontVariantNumeric: "tabular-nums",
  },
  fundsAfter: {
    color: "var(--color-text-accent)",
    fontVariantNumeric: "tabular-nums",
  },
  // A price beyond the treasury is struck through rather than hidden, which is
  // how the source game says it and how a player reads what one more turn of
  // income would buy. The sprite keeps its colors: the unit is still the thing
  // being identified, and only the order is unavailable.
  rowUnaffordable: {
    cursor: "not-allowed",
    color: "var(--color-text-secondary)",
  },
  rowStruck: {
    textDecorationLine: "line-through",
    textDecorationThickness: "var(--border-width)",
  },
  rowCost: {
    flex: "0 0 auto",
    fontFamily: "var(--font-family-code)",
    fontSize: "var(--font-size-sm)",
    letterSpacing: "0.06em",
    fontVariantNumeric: "tabular-nums",
  },
  empty: {
    // An empty state inside a panel is a recessed well, never a second outline.
    padding: "var(--spacing-3)",
    borderWidth: "var(--border-width)",
    borderStyle: "dashed",
    borderColor: "var(--color-border-disabled)",
    borderRadius: "var(--radius-element)",
    backgroundColor: "var(--color-background-muted)",
  },
  notice: {
    borderBlockStartWidth: "var(--border-width)",
    borderBlockStartStyle: "solid",
    borderBlockStartColor: "var(--color-border-soft)",
    backgroundColor: "var(--color-background-muted)",
  },
});
