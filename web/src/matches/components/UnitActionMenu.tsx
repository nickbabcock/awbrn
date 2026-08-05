import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import * as stylex from "@stylexjs/stylex";
import {
  BoardMenuShell,
  boardMenuStyles,
  type BoardMenuPresentation,
} from "#/matches/components/BoardMenu.tsx";
import type { UnitActionOption } from "#/wasm/awbrn_wasm.js";

interface UnitActionMenuProps {
  /** Where the board was pressed, in surface pixels. Null when no pointer opened it. */
  anchor: { x: number; y: number } | null;
  destination: { x: number; y: number };
  /** Why the orders are inert, when they are. */
  disabledReason?: string;
  isEnabled: boolean;
  onChoose: (index: number) => void;
  onDismiss: () => void;
  onRestoreFocus: () => void;
  options: UnitActionOption[];
  /**
   * Which order the menu opens on. A drag released on an enemy has already said
   * what the player meant, so the menu says it back.
   */
  preselected?: number;
  presentation: BoardMenuPresentation;
}

/**
 * What the unit does where it is going: the menu the source game opens at the
 * end of a move.
 *
 * This is the only thing on the board that commits an order. Arriving somewhere
 * decides nothing, which is what makes a mis-tap cost a step back rather than a
 * unit. Every entry here was accepted by the AWVM reducer against this player's
 * own observation; the interface never decides what a unit may do.
 */
export function UnitActionMenu({
  anchor,
  destination,
  disabledReason,
  isEnabled,
  onChoose,
  onDismiss,
  onRestoreFocus,
  options,
  preselected,
  presentation,
}: UnitActionMenuProps) {
  return (
    <BoardMenuShell
      anchor={anchor}
      label={`Orders at ${destination.x}, ${destination.y}`}
      onDismiss={onDismiss}
      inlineSize="var(--size-action-menu)"
      onRestoreFocus={onRestoreFocus}
      presentation={presentation}
    >
      {({ spriteScale }) => (
        <VStack gap={0} xstyle={boardMenuStyles.body}>
          <HStack
            align="center"
            gap={3}
            justify="between"
            paddingBlock={spriteScale === 2 ? 2 : 0}
            paddingInline={2}
            xstyle={boardMenuStyles.header}
          >
            <Text type="label" xstyle={styles.heading}>
              Orders
            </Text>
            <Text type="label" xstyle={styles.coordinate}>
              {destination.x}, {destination.y}
            </Text>
          </HStack>

          <VStack gap={0} xstyle={boardMenuStyles.list}>
            {options.map((option, index) => (
              <OrderRow
                index={index}
                isEnabled={isEnabled}
                isPreselected={index === preselected}
                key={`${option.name}-${orderKey(option)}`}
                onChoose={onChoose}
                option={option}
                spriteScale={spriteScale}
                title={disabledReason}
              />
            ))}
          </VStack>

          {isEnabled || !disabledReason ? null : (
            <VStack gap={0} paddingBlock={2} paddingInline={3} xstyle={boardMenuStyles.notice}>
              <Text color="secondary" type="supporting">
                {disabledReason}
              </Text>
            </VStack>
          )}
        </VStack>
      )}
    </BoardMenuShell>
  );
}

/**
 * One order, named the way the game names it.
 *
 * Targeted orders carry their tile because a unit may have more than one
 * destination or target from the same square and the order name alone would
 * not say which.
 */
function OrderRow({
  index,
  isEnabled,
  isPreselected,
  onChoose,
  option,
  spriteScale,
  title,
}: {
  index: number;
  isEnabled: boolean;
  isPreselected: boolean;
  onChoose: (index: number) => void;
  option: UnitActionOption;
  spriteScale: 1 | 2;
  title?: string;
}) {
  const target =
    option.action.type === "unload"
      ? option.action.position
      : option.action.action.type === "attack" || option.action.action.type === "launch"
        ? option.action.action.target
        : null;

  return (
    <button
      // The shell moves the cursor here when the menu opens, so the order the
      // player has already indicated is the one under their thumb.
      data-preselected={isPreselected ? "" : undefined}
      disabled={!isEnabled}
      onClick={() => onChoose(index)}
      // The cursor follows the pointer rather than doubling it: entering a row
      // moves the one cursor there, so hover and keyboard never light two rows.
      onPointerEnter={(event) => event.currentTarget.focus({ preventScroll: true })}
      title={isEnabled ? undefined : title}
      type="button"
      {...stylex.props(
        boardMenuStyles.row,
        styles.row,
        spriteScale === 2 && styles.rowSpacious,
        !isEnabled && boardMenuStyles.rowInert,
      )}
    >
      <span {...stylex.props(boardMenuStyles.rowName)}>{option.name}</span>
      {target ? (
        <span {...stylex.props(styles.target)}>
          {target.x}, {target.y}
        </span>
      ) : null}
    </button>
  );
}

/**
 * A stable key for an order.
 *
 * Targeted entries can share a name, so their identity includes the target (and
 * cargo where applicable) to prevent React reusing the wrong row.
 */
function orderKey(option: UnitActionOption): string {
  if (option.action.type === "unload") {
    return `unload-${option.action.cargo_id}-${option.action.position.x},${option.action.position.y}`;
  }
  const action = option.action.action;
  if (action.type === "attack" || action.type === "launch") {
    return `${action.type}-${action.target.x},${action.target.y}`;
  }
  if (action.type === "repair") {
    return `repair-${action.target_id}`;
  }
  return action.type;
}

const styles = stylex.create({
  // An order is a word, not a unit: there is no art in the row to make room
  // for, so the row is a line of the HUD face and one step either side.
  row: {
    gap: "var(--spacing-2)",
    minBlockSize: "var(--size-action-row)",
    paddingBlock: 0,
    paddingInline: "var(--spacing-2)",
  },
  // A thumb still needs a real target, so the sheet keeps its height.
  rowSpacious: {
    minBlockSize: "var(--size-action-row-spacious)",
    paddingInline: "var(--spacing-3)",
  },
  heading: {
    color: "var(--color-text-secondary)",
  },
  coordinate: {
    color: "var(--color-text-secondary)",
    fontVariantNumeric: "tabular-nums",
  },
  // Which tile this order acts on, when the order alone does not say.
  target: {
    flex: "0 0 auto",
    fontFamily: "var(--font-family-code)",
    fontSize: "var(--font-size-sm)",
    letterSpacing: "0.06em",
    fontVariantNumeric: "tabular-nums",
    color: "var(--color-text-secondary)",
  },
});
