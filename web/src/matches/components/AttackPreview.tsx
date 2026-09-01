import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { colorVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useLayoutEffect, useRef, useState, type RefObject } from "react";
import { useGameStore } from "#/engine/store.ts";
import { AttackEngagement, Attacker } from "#/matches/components/AttackEngagement.tsx";
import { BoardAnchoredPanel, boardMenuStyles } from "#/matches/components/BoardMenu.tsx";
import { boardMenuLayout } from "#/matches/components/boardMenuLayout.stylex.ts";
import type { AttackPreviewChanged } from "#/wasm/awbrn_wasm.js";

/**
 * What the shot under the crosshair would cost, while the crosshair is on it.
 *
 * It is the destination menu's own block, in the destination menu's own frame,
 * one step earlier in the decision. Aiming is where a player chooses between
 * targets, and a forecast that arrives only after the unit has been committed
 * to a firing tile arrives after the choice it was meant to inform.
 *
 * The frame is the menu's rather than the terrain readout's for two reasons.
 * The numbers belong beside the tile they are about, not in a corner the eye
 * has to leave the target to read; and the panel that appears while aiming is
 * then the same object as the panel that appears on committing, so the second
 * one reads as the first one answering rather than as a new thing.
 *
 * It commits nothing and it takes nothing: no keyboard, no press, and no
 * pointer. The press that fires the shot goes through it to the board.
 */
export function AttackPreview({ surfaceRef }: { surfaceRef: RefObject<HTMLElement | null> }) {
  const preview = useGameStore((state) => state.attackPreview);
  const anchor = useAimAnchor(surfaceRef, preview);
  if (preview === null || preview.forecast === undefined) {
    return null;
  }

  return (
    <BoardAnchoredPanel
      anchor={anchor}
      inlineSize={boardMenuLayout.actionForecastInlineSize}
      xstyle={styles.panel}
    >
      <VStack gap={0} xstyle={boardMenuStyles.body}>
        <HStack
          align="center"
          gap={3}
          justify="between"
          paddingInline={2}
          xstyle={[boardMenuStyles.header, styles.header]}
        >
          {/* Who is firing, said the way the menu says it. The exchange below
              is read from this unit's seat, and the tile the panel stands
              beside is the target's. */}
          {preview.attacker === undefined ? (
            <Text type="label" xstyle={styles.heading}>
              Attack
            </Text>
          ) : (
            <Attacker badge={preview.attacker} spriteScale={1} />
          )}
        </HStack>
        <span {...stylex.props(styles.engagement)}>
          <AttackEngagement forecast={preview.forecast} spriteScale={1} />
        </span>
      </VStack>
    </BoardAnchoredPanel>
  );
}

/**
 * Where the panel stands: beside the pointer that took the aim.
 *
 * The pointer is remembered without a render, and read only when the engine
 * reports a different exchange. A panel that chased the cursor across a tile it
 * is already reporting on would be a moving target for the eye that came to
 * read it, and the aim only changes when the tile under the pointer does.
 */
function useAimAnchor(
  surfaceRef: RefObject<HTMLElement | null>,
  preview: AttackPreviewChanged | null,
) {
  const pointerRef = useRef<{ x: number; y: number } | null>(null);
  const [anchor, setAnchor] = useState<{ x: number; y: number } | null>(null);

  useEffect(() => {
    const surface = surfaceRef.current;
    if (!surface) return;

    const remember = (event: PointerEvent) => {
      const bounds = surface.getBoundingClientRect();
      pointerRef.current = { x: event.clientX - bounds.left, y: event.clientY - bounds.top };
    };

    // A finger has no hover: the tap that takes the aim is the only thing that
    // says where it was taken.
    surface.addEventListener("pointermove", remember, { passive: true });
    surface.addEventListener("pointerdown", remember, { passive: true });
    return () => {
      surface.removeEventListener("pointermove", remember);
      surface.removeEventListener("pointerdown", remember);
    };
  }, [surfaceRef]);

  // Before paint: the panel appears in the same frame as the aim it reports on,
  // and a frame spent at the old anchor is a frame of the panel in the wrong
  // place.
  useLayoutEffect(() => {
    setAnchor(pointerRef.current);
  }, [preview]);

  return anchor;
}

const styles = stylex.create({
  // The panel reports; it is not a surface. Every press on it is a press on the
  // board underneath, which is where the shot it is describing is fired from.
  panel: {
    pointerEvents: "none",
  },
  // The menu's own head, carrying the unit rather than the word "Orders". The
  // strip is the menu's so that the two panels are plainly the same object.
  header: {
    paddingBlock: spacingVars["--spacing-1"],
  },
  heading: {
    color: colorVars["--color-text-secondary"],
  },
  // The engagement sits where an order would, with the air that an order of two
  // lines is given.
  engagement: {
    display: "flex",
    paddingBlock: spacingVars["--spacing-2"],
    paddingInline: spacingVars["--spacing-2"],
  },
});
