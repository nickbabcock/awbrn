import { Button } from "@astryxdesign/core/Button";
import { useMediaQuery } from "@astryxdesign/core/hooks";
import { HStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import * as stylex from "@stylexjs/stylex";
import { Close as CloseIcon } from "pixelarticons/react/Close";
// The corner-to-corner arrows, not the window frame. The frame glyph is a grid
// of 2px bars in a 24px box, and at icon size those bars close up into a solid
// block: an icon that does not survive the size it is drawn at.
import { Scale as ScaleIcon } from "pixelarticons/react/Scale";
import { useEffect, useState } from "react";
import type { GameFullscreenMode } from "#/canvas_courier/index.ts";

/** Coarse pointers have no Esc, so the shortcut is only named where it exists. */
const KEYBOARD_MEDIA = "(pointer: fine)";

/** How long the immersive notice holds before it withdraws, matching its animation. */
const NOTICE_DURATION_MS = 4400;

/**
 * The command that gives the board the whole screen.
 *
 * It sits on the readout strip with the other commands rather than floating
 * over the map. A control parked on the battlefield covers the one thing the
 * screen is for, and the board already has a strip whose whole job is holding
 * what a player can do to it.
 */
export function GameFullscreenButton({ onEnter }: { onEnter: () => void }) {
  return (
    <Button
      clickAction={onEnter}
      icon={<ScaleIcon aria-hidden height={16} width={16} />}
      label="Full screen"
      size="sm"
      variant="secondary"
    />
  );
}

/**
 * Leaving full screen, and — for as long as it is needed — how.
 *
 * This is the only chrome the board carries while it holds the screen, so it
 * says what it does in words rather than wearing a bare cross. A cross on a
 * battlefield reads as closing the match, which is the one thing it must never
 * be mistaken for.
 */
export function BoardFullscreenExit({
  mode,
  onExit,
}: {
  mode: GameFullscreenMode;
  onExit: () => void;
}) {
  const hasKeyboard = useMediaQuery(KEYBOARD_MEDIA);

  return (
    <>
      {/* The key is placed by its own holder rather than by a style on the
          button. A button carries its own `position` from the design system,
          at a specificity a utility style does not reach, so a key positioned
          directly falls back into the column and off the bottom of the board. */}
      <HStack gap={0} xstyle={styles.exitKey}>
        <Button
          clickAction={onExit}
          icon={<CloseIcon aria-hidden height={16} width={16} />}
          label="Exit full screen"
          size="sm"
          variant="secondary"
        />
      </HStack>

      {mode === "immersive" ? <ImmersiveNotice hasKeyboard={hasKeyboard} /> : null}
    </>
  );
}

/**
 * The one thing native full screen supplies that the fallback cannot: the
 * browser's own notice that Esc is the way out.
 *
 * Without it, a board pinned over the page by CSS alone is a screen a player
 * has to guess their way out of, and the guess most reach for first is the back
 * button, which leaves the match. It withdraws on its own, because a standing
 * instruction on a battlefield is a standing obstruction.
 */
function ImmersiveNotice({ hasKeyboard }: { hasKeyboard: boolean }) {
  const [hasWithdrawn, setHasWithdrawn] = useState(false);

  useEffect(() => {
    const timer = setTimeout(() => setHasWithdrawn(true), NOTICE_DURATION_MS);
    return () => clearTimeout(timer);
  }, []);

  if (hasWithdrawn) return null;

  return (
    <HStack
      align="center"
      as="output"
      justify="center"
      paddingBlock={2}
      paddingInline={3}
      xstyle={styles.notice}
    >
      <Text type="label" xstyle={styles.noticeText}>
        {hasKeyboard ? "Press Esc to leave full screen" : "Tap Exit to leave full screen"}
      </Text>
    </HStack>
  );
}

/**
 * The notice holds still long enough to be read, then goes. It starts at full
 * presence rather than fading in: an instruction that arrives late is one the
 * player has already started hunting for.
 */
const withdraw = stylex.keyframes({
  "0%": { opacity: 1, transform: "translateY(0)" },
  "84%": { opacity: 1, transform: "translateY(0)" },
  "100%": { opacity: 0, transform: "translateY(var(--spacing-2))" },
});

/** The same timing without the movement, for a player who asked for less of it. */
const withdrawStill = stylex.keyframes({
  "0%": { opacity: 1 },
  "84%": { opacity: 1 },
  "100%": { opacity: 0 },
});

const styles = stylex.create({
  // The board is the screen now, so the key takes the corner the game's own HUD
  // leaves free, and stays clear of the destination menus, which open at the
  // point that was pressed.
  exitKey: {
    position: "absolute",
    insetBlockStart: "var(--spacing-3)",
    insetInlineEnd: "var(--spacing-3)",
    zIndex: 1,
  },
  // The far edge from the key, so the notice and the thing it names are never
  // reaching for the same corner.
  notice: {
    position: "absolute",
    insetBlockEnd: "var(--spacing-4)",
    // Centered with auto margins rather than a translate, because the
    // withdrawal animates `transform` and would otherwise take the centering
    // with it.
    insetInline: 0,
    marginInline: "auto",
    inlineSize: "fit-content",
    zIndex: 1,
    maxInlineSize: "calc(100% - var(--spacing-6))",
    backgroundColor: "var(--color-background-surface)",
    borderWidth: "var(--border-width)",
    borderStyle: "solid",
    borderColor: "var(--color-border-emphasized)",
    borderRadius: "var(--radius-element)",
    boxShadow: "var(--shadow-low)",
    // It reports; it never stands between the player and the board under it.
    pointerEvents: "none",
    animationName: {
      default: withdraw,
      "@media (prefers-reduced-motion: reduce)": withdrawStill,
    },
    animationDuration: "4.4s",
    animationTimingFunction: "cubic-bezier(0.2, 0, 0, 1)",
    animationFillMode: "forwards",
  },
  noticeText: {
    textAlign: "center",
  },
});
