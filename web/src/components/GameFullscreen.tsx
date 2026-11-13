import { Button } from "#/ui/Button.tsx";
import { useMediaQuery } from "@astryxdesign/core/hooks";
import { HStack } from "@astryxdesign/core/Stack";
import * as stylex from "@stylexjs/stylex";
// The corner-to-corner arrows, not the window frame. The frame glyph is a grid
// of 2px bars in a 24px box, and at icon size those bars close up into a solid
// block: an icon that does not survive the size it is drawn at.
import { Scale as ScaleIcon } from "pixelarticons/react/Scale";
import { useEffect, useState } from "react";
import type { GameFullscreenMode } from "#/canvas_courier/index.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";

/** Coarse pointers have no Esc, so the shortcut is only named where it exists. */
const KEYBOARD_MEDIA = "(pointer: fine)";

/** How long the immersive notice holds before it cuts away. */
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
 * The one thing native full screen supplies that the fallback cannot: the
 * browser's own notice that Esc is the way out.
 *
 * Without it, a board pinned over the page by CSS alone is a screen a player
 * has to guess their way out of, and the guess most reach for first is the back
 * button, which leaves the match. It withdraws on its own, because a standing
 * instruction is a standing obstruction.
 *
 * True from the moment the board takes the screen by the fallback until the
 * notice has been up long enough to read; it then cuts, as every notice does.
 */
export function useImmersiveNotice(mode: GameFullscreenMode): boolean {
  const isImmersive = mode === "immersive";
  const [isShowing, setIsShowing] = useState(false);

  useEffect(() => {
    setIsShowing(isImmersive);
    if (!isImmersive) return;
    const timer = setTimeout(() => setIsShowing(false), NOTICE_DURATION_MS);
    return () => clearTimeout(timer);
  }, [isImmersive]);

  return isShowing;
}

/**
 * The immersive notice as a line of text, for a screen that has a message slot
 * to put it in (the turn strip, the tile row) rather than over the board.
 */
export function ImmersiveNoticeLine({ xstyle }: { xstyle?: stylex.StyleXStyles }) {
  const hasKeyboard = useMediaQuery(KEYBOARD_MEDIA);
  return (
    <HStack align="center" as="output" gap={0} xstyle={[styles.line, xstyle]}>
      {noticeText(hasKeyboard)}
    </HStack>
  );
}

function noticeText(hasKeyboard: boolean): string {
  return hasKeyboard ? "Press Esc to leave full screen" : "Tap the full screen key to leave";
}

const styles = stylex.create({
  // The line takes the slot's own type; only the instruction is lifted.
  line: {
    color: awbrnVars.textOnInkStrong,
    fontWeight: 650,
  },
});
