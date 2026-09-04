import { VStack, HStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { borderVars, colorVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useCallback, useState } from "react";
import {
  BoardMenuShell,
  boardMenuStyles,
  followPointerCursor,
  type BoardMenuPresentation,
} from "#/matches/components/BoardMenu.tsx";
import { Button } from "#/ui/Button.tsx";
import { boardMenuLayout } from "#/matches/components/boardMenuLayout.stylex.ts";

interface MatchMenuProps {
  /** The day the match stands on, for the menu's own readout. */
  day: number | null;
  /** Why the commands are inert, when they are. */
  disabledReason?: string;
  isEnabled: boolean;
  onDismiss: () => void;
  onResign: () => void;
  onRestoreFocus: () => void;
  presentation: BoardMenuPresentation;
}

/**
 * What a player can do to the match itself rather than to a unit on it.
 *
 * The board already opens a menu for a production site and another for a
 * destination, and this is the third of the same object: the window the game
 * opens over the map when the player asks it something. It carries one command
 * today, and it still opens as a menu — a command that is sometimes a menu and
 * sometimes a bare key is a command a player has to look for twice.
 *
 * Resignation is the match-level twin of deleting a unit, so it is asked the
 * same way: the list is replaced in the frame it already occupies, the header
 * names what is being asked, and the one red key in the system commits it. What
 * it adds over a deleted unit is a line saying what leaving does, because a
 * seat leaving is the one order here whose result is not drawn on the board.
 */
export function MatchMenu({
  day,
  disabledReason,
  isEnabled,
  onDismiss,
  onResign,
  onRestoreFocus,
  presentation,
}: MatchMenuProps) {
  const [isConfirming, setIsConfirming] = useState(false);
  const confirm = useCallback(() => setIsConfirming(true), []);
  const cancelConfirmation = useCallback(() => setIsConfirming(false), []);
  const commit = useCallback(() => {
    if (isEnabled) onResign();
  }, [isEnabled, onResign]);

  return (
    <BoardMenuShell
      // A menu opened from the strip was not opened at a tile, so it stands in
      // the middle of the board rather than beside a square it has nothing to
      // do with.
      anchor={null}
      footer={
        isConfirming ? (
          <VStack gap={2} paddingBlock={3} paddingInline={3}>
            <Button
              clickAction={commit}
              isDisabled={!isEnabled}
              label="Resign"
              size="lg"
              variant="destructive"
              width="100%"
              xstyle={styles.key}
            />
            <Button
              clickAction={cancelConfirmation}
              label="Keep playing"
              size="lg"
              variant="secondary"
              width="100%"
              xstyle={styles.key}
            />
          </VStack>
        ) : undefined
      }
      inlineSize={boardMenuLayout.actionForecastInlineSize}
      label="Match commands"
      onDismiss={onDismiss}
      onRestoreFocus={onRestoreFocus}
      presentation={presentation}
    >
      {({ isSheet, spriteScale }) => (
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
              {isConfirming ? "Resign" : "Match"}
            </Text>
            {day === null ? null : (
              <Text type="label" xstyle={styles.day}>
                Day {day}
              </Text>
            )}
          </HStack>

          {isConfirming ? (
            <>
              {/* The one line of prose in the menu, and the reason it is here:
                  a deleted unit leaves the board where the player can see it,
                  and a resigned seat leaves a record they cannot. */}
              <VStack gap={0} paddingBlock={2} paddingInline={3} xstyle={styles.consequence}>
                <Text type="supporting">
                  Your army leaves the board. The seat cannot come back.
                </Text>
              </VStack>
              {/* The sheet answers the question in its own footer, where the
                  thumb already expects the commands. */}
              {isSheet ? null : (
                <VStack gap={0} xstyle={boardMenuStyles.list}>
                  <button
                    // The menu opens its confirmation on the harmless answer,
                    // so the key under the cursor still commits nothing.
                    autoFocus
                    onClick={cancelConfirmation}
                    onPointerMove={followPointerCursor}
                    type="button"
                    {...stylex.props(boardMenuStyles.row, styles.row)}
                  >
                    <Text color="inherit" type="inherit" xstyle={boardMenuStyles.rowName}>
                      Keep playing
                    </Text>
                  </button>
                  <button
                    disabled={!isEnabled}
                    onClick={commit}
                    onPointerMove={followPointerCursor}
                    title={isEnabled ? undefined : disabledReason}
                    type="button"
                    {...stylex.props(
                      boardMenuStyles.row,
                      styles.row,
                      styles.rowCommit,
                      !isEnabled && boardMenuStyles.rowInert,
                    )}
                  >
                    <Text color="inherit" type="inherit" xstyle={boardMenuStyles.rowName}>
                      Resign
                    </Text>
                  </button>
                </VStack>
              )}
            </>
          ) : (
            <VStack gap={0} xstyle={isSheet ? undefined : boardMenuStyles.list}>
              <button
                disabled={!isEnabled}
                onClick={confirm}
                onPointerMove={followPointerCursor}
                title={isEnabled ? undefined : disabledReason}
                type="button"
                {...stylex.props(
                  boardMenuStyles.row,
                  styles.row,
                  spriteScale === 2 && styles.rowSpacious,
                  styles.rowDestructive,
                  !isEnabled && boardMenuStyles.rowInert,
                )}
              >
                <Text color="inherit" type="inherit" xstyle={boardMenuStyles.rowName}>
                  Resign
                </Text>
              </button>
              {/* The board menu has no footer of its own, so the way out is a
                  key on the menu, the way the source game leaves one. */}
              {isSheet ? null : (
                <button
                  onClick={onDismiss}
                  onPointerMove={followPointerCursor}
                  type="button"
                  {...stylex.props(boardMenuStyles.row, styles.row)}
                >
                  <Text color="inherit" type="inherit" xstyle={boardMenuStyles.rowName}>
                    Cancel
                  </Text>
                </button>
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
      )}
    </BoardMenuShell>
  );
}

const styles = stylex.create({
  // A command is a word, not a unit: the row is a line of the HUD face and one
  // step either side, the same key the destination menu draws.
  row: {
    gap: spacingVars["--spacing-2"],
    minBlockSize: boardMenuLayout.actionRowMinBlockSize,
    paddingBlock: 0,
    paddingInline: spacingVars["--spacing-2"],
  },
  // A thumb still needs a real target, so the sheet keeps its height.
  rowSpacious: {
    minBlockSize: boardMenuLayout.actionRowSpaciousMinBlockSize,
    paddingInline: spacingVars["--spacing-3"],
  },
  // The command that ends a player's match reads in the error color, so it is
  // not one more word in a list of words. Under the cursor it keeps the orange
  // fill every other row wears, and the name inverts to stay legible on it.
  rowDestructive: {
    color: {
      default: colorVars["--color-error"],
      ":focus": colorVars["--color-on-accent"],
    },
  },
  // The commit on the confirmation is the one key in the system whose cursor is
  // not orange. It is the last press before the army is gone, the screen exists
  // only to say so, and a key that looks like every other key does not say it.
  rowCommit: {
    backgroundColor: { default: "transparent", ":focus": colorVars["--color-error"] },
    color: {
      default: colorVars["--color-error"],
      ":focus": colorVars["--color-on-error"],
    },
  },
  // The line above the answers is part of the question, so it is divided from
  // them by the frame's own rule rather than by the soft rule between rows.
  consequence: {
    borderBlockEndWidth: borderVars["--border-width"],
    borderBlockEndStyle: "solid",
    borderBlockEndColor: colorVars["--color-border-emphasized"],
  },
  // The stock large button is shorter than a thumb needs, and these two are the
  // only commands on the sheet.
  key: {
    minBlockSize: boardMenuLayout.actionRowSpaciousMinBlockSize,
  },
  heading: {
    color: colorVars["--color-text-secondary"],
  },
  day: {
    color: colorVars["--color-text-secondary"],
    fontVariantNumeric: "tabular-nums",
  },
});
