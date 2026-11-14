import { HStack, Stack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import {
  borderVars,
  colorVars,
  focusVars,
  radiusVars,
  spacingVars,
  textSizeVars,
  typographyVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useCallback, useState } from "react";
import {
  BoardMenuShell,
  boardMenuStyles,
  followPointerCursor,
  MenuCaret,
  menuRowMarker,
  type BoardMenuPresentation,
} from "#/matches/components/BoardMenu.tsx";
import { Icon } from "@astryxdesign/core/Icon";
import { IconClose } from "#/ui/icons.tsx";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { boardMenuLayout } from "#/matches/components/boardMenuLayout.stylex.ts";
import { AttackEngagement, Attacker } from "#/matches/components/AttackEngagement.tsx";
import { engagementLabel } from "#/matches/components/attack_forecast.ts";
import type { UnitActionOption, UnitBadge } from "#/wasm/awbrn_wasm.js";

interface UnitActionMenuProps {
  /** Where the board was pressed, in surface pixels. Null when no pointer opened it. */
  anchor: { x: number; y: number } | null;
  /** The unit being commanded, named at the head of a menu that forecasts. */
  attacker?: UnitBadge;
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
  /** `dock`: the phone order dock, in place of the tile row and the dock. */
  presentation: BoardMenuPresentation | "dock";
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
  attacker,
  disabledReason,
  isEnabled,
  onChoose,
  onDismiss,
  onRestoreFocus,
  options,
  preselected,
  presentation,
}: UnitActionMenuProps) {
  // Which order is waiting on a second press, if any. It is held by identity
  // rather than by index: the engine may re-offer the orders while the question
  // is on screen, and a remembered index would then commit whatever moved into
  // that row.
  const [pendingKey, setPendingKey] = useState<string | null>(null);

  const handleChoose = useCallback(
    (index: number) => {
      const option = options[index];
      if (option && needsConfirmation(option)) {
        setPendingKey(rowKey(option));
        return;
      }
      onChoose(index);
    },
    [onChoose, options],
  );

  const pendingIndex =
    pendingKey === null ? -1 : options.findIndex((o) => rowKey(o) === pendingKey);
  const pending = pendingIndex === -1 ? undefined : options[pendingIndex];

  const cancelConfirmation = useCallback(() => setPendingKey(null), []);
  const confirmPending = useCallback(() => {
    if (isEnabled && pendingIndex !== -1) onChoose(pendingIndex);
  }, [isEnabled, onChoose, pendingIndex]);

  if (presentation === "dock") {
    return (
      <OrderDock
        attacker={attacker}
        disabledReason={disabledReason}
        isEnabled={isEnabled}
        onCancelPending={cancelConfirmation}
        onChoose={handleChoose}
        onConfirmPending={confirmPending}
        onDismiss={onDismiss}
        options={options}
        pending={pending}
      />
    );
  }

  return (
    <BoardMenuShell
      anchor={anchor}
      // The sheet answers its own question at the bottom edge: the commit, then
      // the way back to the orders. The stock Cancel would dismiss the whole
      // menu and lose the move the player already planned. The header rule
      // already divides these from what is above them, so the footer carries no
      // rule of its own here.
      footer={
        pending ? (
          <VStack gap={2} paddingBlock={3} paddingInline={3}>
            <Button
              clickAction={confirmPending}
              isDisabled={!isEnabled}
              label={pending.name}
              size="lg"
              variant="destructive"
              width="100%"
              xstyle={styles.key}
            />
            <Button
              clickAction={cancelConfirmation}
              label="Cancel"
              size="lg"
              variant="secondary"
              width="100%"
              xstyle={styles.key}
            />
          </VStack>
        ) : undefined
      }
      label="Orders"
      onDismiss={onDismiss}
      // A menu with an engagement on it needs the width to hold one. Orders
      // that are only words keep the narrow menu they have always had.
      inlineSize={
        options.some((option) => option.forecast)
          ? boardMenuLayout.actionForecastInlineSize
          : boardMenuLayout.actionInlineSize
      }
      onRestoreFocus={onRestoreFocus}
      presentation={presentation}
    >
      {({ isSheet, spriteScale }) => (
        <VStack gap={0} xstyle={boardMenuStyles.body}>
          <HStack align="center" gap={3} justify="between" xstyle={boardMenuStyles.header}>
            {/* The unit is the same for every order below, so it is said once
                here rather than repeated down the list. */}
            {attacker && !pending ? (
              <Attacker badge={attacker} spriteScale={1} />
            ) : (
              <Text type="label" xstyle={styles.heading}>
                {pending ? pending.name : "Orders"}
              </Text>
            )}
          </HStack>

          {pending ? (
            <ConfirmOrder
              isEnabled={isEnabled}
              onCancel={cancelConfirmation}
              onConfirm={confirmPending}
              option={pending}
              presentation={presentation}
            />
          ) : (
            <VStack gap={0} xstyle={isSheet ? undefined : boardMenuStyles.list}>
              {options.map((option, index) => (
                <OrderRow
                  index={index}
                  isEnabled={isEnabled}
                  // A sheet opens with nothing chosen: a finger did not ask for
                  // a cursor, and the chosen row is the cursor.
                  isPreselected={!isSheet && index === preselected}
                  key={rowKey(option)}
                  onChoose={handleChoose}
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
      )}
    </BoardMenuShell>
  );
}

/**
 * Orders the menu asks about twice.
 *
 * Everything else a unit can do is either reversible or ends in a unit still
 * standing on the board. Delete is the one order whose result cannot be
 * reconsidered — and if it takes the owner's last unit, it ends the match — so
 * it is the one order that costs a second press.
 */
function needsConfirmation(option: UnitActionOption): boolean {
  return option.action.type === "delete";
}

/**
 * The second press: the way out and the way through.
 *
 * It carries no warning. The header names the order, the key that commits it is
 * the one red key in the system, and a menu that asks again is already saying
 * the order is final; a sentence repeating that is a sentence a player learns
 * to skip. The one thing delete does that a player cannot see is take a
 * transport's cargo with it, and the order does not say whether this unit is
 * loaded, so that warning waits for an order that does rather than being shown
 * on every delete until it stops being read.
 *
 * It replaces the list inside the same menu rather than opening a window over
 * it, so the board menu stays one object under the cursor and the sheet stays
 * one sheet under the thumb. The two presentations part company on where the
 * answers go: the board keeps them as menu keys under the cursor, and the sheet
 * hands them to its own footer, where a thumb already expects the commands and
 * where the sheet would otherwise offer a second Cancel of its own.
 */
function ConfirmOrder({
  isEnabled,
  onCancel,
  onConfirm,
  option,
  presentation,
}: {
  isEnabled: boolean;
  onCancel: () => void;
  onConfirm: () => void;
  option: UnitActionOption;
  presentation: BoardMenuPresentation;
}) {
  if (presentation === "sheet") return null;

  return (
    <VStack gap={0} xstyle={boardMenuStyles.list}>
      <button
        // The board menu carries a cursor, and it opens on the harmless answer,
        // so the key that committed nothing a moment ago still commits nothing.
        autoFocus
        onClick={onCancel}
        onPointerMove={followPointerCursor}
        type="button"
        // These keys stand where the orders stood, in a frame that did not
        // change size, so they are the same key. A row that grew on the second
        // press would read as the menu moving under the player.
        {...stylex.props(boardMenuStyles.row)}
      >
        <Text color="inherit" type="inherit" xstyle={boardMenuStyles.rowName}>
          Cancel
        </Text>
      </button>
      <button
        disabled={!isEnabled}
        onClick={onConfirm}
        onPointerMove={followPointerCursor}
        type="button"
        {...stylex.props(boardMenuStyles.row, styles.rowCommit)}
      >
        <Text color="inherit" type="inherit" xstyle={boardMenuStyles.rowName}>
          {option.name}
        </Text>
      </button>
    </VStack>
  );
}

/**
 * One order, named the way the game names it.
 *
 * Targeted orders carry their tile because a unit may have more than one
 * destination or target from the same square and the order name alone would
 * not say which. An attack that AWVM could forecast carries the engagement
 * instead: a coordinate says which enemy only to a player willing to go and
 * look, and the tile is the least of what they need to know about it.
 */
function OrderRow({
  index,
  isEnabled,
  isPreselected,
  isWide = false,
  onChoose,
  option,
  spriteScale,
  title,
}: {
  index: number;
  isEnabled: boolean;
  isPreselected: boolean;
  /** The phone order dock: the forecast sits at the end of the row. */
  isWide?: boolean;
  onChoose: (index: number) => void;
  option: UnitActionOption;
  spriteScale: 1 | 2;
  title?: string;
}) {
  const target =
    option.action.type === "unload"
      ? option.action.position
      : option.action.type === "move" &&
          (option.action.action.type === "attack" || option.action.action.type === "launch")
        ? option.action.action.target
        : null;
  const forecast = option.forecast;

  return (
    <button
      aria-label={forecast ? engagementLabel(option.name, forecast) : undefined}
      // The shell moves the cursor here when the menu opens, so the order the
      // player has already indicated is the one under their thumb.
      data-preselected={isPreselected ? "" : undefined}
      disabled={!isEnabled}
      onClick={() => onChoose(index)}
      // The cursor follows the pointer rather than doubling it: entering a row
      // moves the one cursor there, so hover and keyboard never light two rows.
      onPointerMove={followPointerCursor}
      title={isEnabled ? undefined : title}
      type="button"
      {...stylex.props(
        menuRowMarker,
        boardMenuStyles.row,
        spriteScale === 2 && boardMenuStyles.rowSpacious,
        isWide && styles.dockRow,
        isWide && styles.dockOrder,
        needsConfirmation(option) && styles.rowDestructive,
        needsConfirmation(option) && styles.rowSeparated,
        !isEnabled && boardMenuStyles.rowInert,
      )}
    >
      <MenuCaret />
      <HStack as="span" xstyle={[styles.orderBody, isWide && styles.orderBodyWide]}>
        <Text color="inherit" type="inherit" xstyle={boardMenuStyles.rowName}>
          {option.name}
        </Text>
        {forecast ? (
          <HStack as="span" xstyle={[styles.forecast, isWide && styles.forecastWide]}>
            <AttackEngagement forecast={forecast} spriteScale={1} />
          </HStack>
        ) : null}
      </HStack>
      {target && !forecast ? (
        <Text color="inherit" type="inherit" xstyle={styles.target}>
          {target.x}, {target.y}
        </Text>
      ) : null}
    </button>
  );
}

/** What names one row of the menu: the order, and what it acts on. */
function rowKey(option: UnitActionOption): string {
  return `${option.name}-${orderKey(option)}`;
}

/**
 * A stable key for an order.
 *
 * Targeted entries can share a name, so their identity includes the target (and
 * cargo where applicable) to prevent React reusing the wrong row.
 */
function orderKey(option: UnitActionOption): string {
  if (option.action.type === "delete") {
    return "delete";
  }
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

/**
 * The phone order dock: the orders replace the tile row and the dock, so the
 * board stays tappable for another target.
 */
function OrderDock({
  attacker,
  disabledReason,
  isEnabled,
  onCancelPending,
  onChoose,
  onConfirmPending,
  onDismiss,
  options,
  pending,
}: {
  attacker?: UnitBadge;
  disabledReason?: string;
  isEnabled: boolean;
  onCancelPending: () => void;
  onChoose: (index: number) => void;
  onConfirmPending: () => void;
  onDismiss: () => void;
  options: UnitActionOption[];
  pending: UnitActionOption | undefined;
}) {
  return (
    <section
      aria-label={attacker ? `${attacker.name}: orders` : "Orders"}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          onDismiss();
        }
      }}
      {...stylex.props(styles.dock)}
    >
      {/* Armed in place: the head names the order. */}
      {pending ? <p {...stylex.props(styles.dockHead)}>{pending.name}</p> : null}
      {!isEnabled && disabledReason ? (
        <p {...stylex.props(styles.dockNotice)}>{disabledReason}</p>
      ) : null}
      <Stack as="span" xstyle={styles.dockRows}>
        {pending ? (
          <>
            <button
              // A confirmation opens on the safe key.
              autoFocus
              onClick={onCancelPending}
              type="button"
              {...stylex.props(boardMenuStyles.row, styles.dockRow)}
            >
              <Text color="inherit" type="inherit" xstyle={boardMenuStyles.rowName}>
                Keep {attacker?.name ?? "the unit"}
              </Text>
            </button>
            <button
              disabled={!isEnabled}
              onClick={onConfirmPending}
              type="button"
              {...stylex.props(boardMenuStyles.row, styles.dockRow, styles.rowCommit)}
            >
              <Text color="inherit" type="inherit" xstyle={boardMenuStyles.rowName}>
                {pending.name}
              </Text>
            </button>
          </>
        ) : (
          options.map((option, index) => (
            <OrderRow
              index={index}
              isEnabled={isEnabled}
              // The dock opens with nothing chosen, as a sheet does.
              isPreselected={false}
              isWide
              key={rowKey(option)}
              onChoose={onChoose}
              option={option}
              spriteScale={2}
              title={disabledReason}
            />
          ))
        )}
      </Stack>
      <Button
        icon={<Icon icon={IconClose} />}
        isIconOnly
        label="Back out of these orders"
        onClick={onDismiss}
        tooltip="Back"
        variant="on-ink"
        xstyle={styles.dockCancel}
      />
    </section>
  );
}

/** Two visible frames: halfway, then there. */
const land = stylex.keyframes({
  from: { opacity: 0 },
  to: { opacity: 1 },
});

const styles = stylex.create({
  orderBody: {
    display: "grid",
    gap: spacingVars["--spacing-1-5"],
    flexGrow: 1,
    minInlineSize: 0,
  },
  orderBodyWide: {
    display: "flex",
    alignItems: "center",
    gap: spacingVars["--spacing-3"],
  },
  forecast: {
    display: "flex",
    minInlineSize: 0,
  },
  forecastWide: {
    marginInlineStart: "auto",
    inlineSize: "auto",
  },
  dockOrder: {
    minBlockSize: "56px",
  },
  dock: {
    animationName: {
      default: land,
      [pageLayout.reducedMotionMedia]: "none",
    },
    animationDuration: "90ms",
    animationTimingFunction: awbrnVars.easeStep,
    display: "flex",
    alignItems: "flex-end",
    flexWrap: "wrap",
    gap: spacingVars["--spacing-2"],
    paddingBlock: awbrnVars.spacing2_5,
    paddingInline: spacingVars["--spacing-3"],
    paddingBlockEnd: `calc(${spacingVars["--spacing-3"]} + env(safe-area-inset-bottom, 0px))`,
    backgroundColor: awbrnVars.ink900,
    borderBlockStartColor: awbrnVars.ink800,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: borderVars["--border-width"],
    color: colorVars["--color-text-primary"],
  },
  dockRows: {
    display: "grid",
    gap: spacingVars["--spacing-2"],
    flexGrow: 1,
    flexBasis: 0,
    minInlineSize: 0,
  },
  dockRow: {
    minBlockSize: "52px",
    borderRadius: radiusVars["--radius-element"],
    borderBlockStartWidth: 0,
    fontSize: awbrnVars.text17,
  },
  dockCancel: {
    flexShrink: 0,
    inlineSize: "52px",
    minInlineSize: "52px",
    blockSize: "52px",
    paddingInline: 0,
  },
  dockNotice: {
    flexBasis: "100%",
    margin: 0,
    color: awbrnVars.textOnInkSecondary,
    fontSize: awbrnVars.text13,
  },
  // An order that destroys something reads in the error color, so it is not
  // one more word in a list of words. Under the cursor it keeps the orange
  // fill every other row wears, and the name inverts to stay legible on it.
  rowDestructive: {
    color: {
      default: colorVars["--color-error"],
      ":focus": colorVars["--color-on-accent"],
    },
  },
  // The armed commit: Danger, filled now that the order is armed (it was
  // outlined in the list). It is the last press before a unit is gone, so the
  // cursor on it is a ring, not the wash every other row wears.
  rowCommit: {
    backgroundColor: colorVars["--color-error"],
    color: colorVars["--color-on-error"],
    outline: {
      default: "none",
      ":focus-visible": `2px solid ${focusVars["--focus-outline-color"]}`,
    },
    outlineOffset: "-4px",
  },
  dockHead: {
    flexBasis: "100%",
    margin: 0,
    color: awbrnVars.textOnInkStrong,
    fontSize: awbrnVars.text15,
    fontWeight: 750,
  },
  // The stock large button is shorter than a thumb needs, and these two are the
  // only commands on the sheet.
  key: {
    minBlockSize: boardMenuLayout.actionRowSpaciousMinBlockSize,
  },
  // The rule the list draws between rows is soft enough to walk past. This one
  // is the frame's own rule, so the last order is visibly apart from Wait
  // rather than the next line under it.
  rowSeparated: {
    borderBlockStartWidth: borderVars["--border-width"],
    borderBlockStartStyle: "solid",
    borderBlockStartColor: colorVars["--color-border-emphasized"],
  },
  heading: {
    fontSize: awbrnVars.text15,
    fontWeight: 750,
  },
  // Which tile this order acts on, when the order alone does not say.
  target: {
    flex: "0 0 auto",
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    fontVariantNumeric: "tabular-nums",
    color: colorVars["--color-text-secondary"],
  },
});
