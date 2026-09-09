/**
 * The shape of the board, and what it is covered with.
 *
 * Resizing is not applied as the figures are typed. A map maker changing 20 to
 * 32 passes through 2 and 3 on the way, and a board that redrew at each of
 * them would drop most of the map before it reached the size they were asking
 * for. The figures are read, and the board is rebuilt when they are pressed.
 */

import { NumberInput } from "@astryxdesign/core/NumberInput";
import { Selector } from "@astryxdesign/core/Selector";
import { VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useState } from "react";
import { Button } from "#/ui/Button.tsx";
import { MAP_EDITOR_MAX_SIZE, MAP_EDITOR_MIN_SIZE } from "#/maps/schemas.ts";
import type { Brush, ResizeAnchor } from "#/wasm/awbrn_wasm.js";

/**
 * Where the board a resize keeps stays put.
 *
 * This is a select rather than three keys in a row. The anchor is read once,
 * while the board is being resized, and never scanned: it is the one control
 * on this panel that does not have to be visible to be usable. Three keys in a
 * 340px column ran wider than the column and grew it a sideways scrollbar,
 * which is a defect a settings column can never earn.
 *
 * It is on the panel only while a resize is waiting to be made. A control that
 * sits under two figures at rest reads as a third thing about the board rather
 * than as the one question a resize asks, and the answer to "what is this for"
 * should be that it appears when the thing it is for does.
 */
const ANCHOR_OPTIONS: { value: ResizeAnchor; label: string }[] = [
  { value: "top-left", label: "Top left" },
  { value: "center", label: "The middle" },
  { value: "bottom-right", label: "Bottom right" },
];

export function BoardShapePanel({
  height,
  loadedBrush,
  onFill,
  onResize,
  width,
}: {
  height: number;
  /** The brush the rail has loaded, which is what a fill would use. */
  loadedBrush: Brush | null;
  onFill: (terrain: number) => void;
  onResize: (width: number, height: number, anchor: ResizeAnchor) => void;
  width: number;
}) {
  const [draftWidth, setDraftWidth] = useState(width);
  const [draftHeight, setDraftHeight] = useState(height);
  const [anchor, setAnchor] = useState<ResizeAnchor>("top-left");

  // The board can change shape without this panel asking, through an undo.
  useEffect(() => setDraftWidth(width), [width]);
  useEffect(() => setDraftHeight(height), [height]);

  const isChanged = draftWidth !== width || draftHeight !== height;
  const fillTerrain = loadedBrush?.kind === "terrain" ? loadedBrush.terrain : null;

  return (
    <VStack gap={3}>
      <VStack gap={2}>
        <div {...stylex.props(styles.pair)}>
          <NumberInput
            label="Width"
            max={MAP_EDITOR_MAX_SIZE}
            min={MAP_EDITOR_MIN_SIZE}
            onChange={(value) => setDraftWidth(value ?? width)}
            size="sm"
            value={draftWidth}
            width="100%"
          />
          <NumberInput
            label="Height"
            max={MAP_EDITOR_MAX_SIZE}
            min={MAP_EDITOR_MIN_SIZE}
            onChange={(value) => setDraftHeight(value ?? height)}
            size="sm"
            value={draftHeight}
            width="100%"
          />
        </div>

        {isChanged ? (
          <Selector
            label="Keep the drawing at"
            onChange={(value) => setAnchor(value as ResizeAnchor)}
            options={ANCHOR_OPTIONS}
            size="sm"
            value={anchor}
            width="100%"
          />
        ) : null}

        <Button
          clickAction={() => onResize(draftWidth, draftHeight, anchor)}
          isDisabled={!isChanged}
          label={isChanged ? `Resize to ${draftWidth} × ${draftHeight}` : "Resize the board"}
          size="sm"
          variant="secondary"
          width="100%"
        />
      </VStack>

      <VStack gap={1}>
        <Button
          clickAction={() => {
            if (fillTerrain !== null) onFill(fillTerrain);
          }}
          isDisabled={fillTerrain === null}
          label="Cover the board with this tile"
          size="sm"
          variant="secondary"
          width="100%"
        />
        {/* The one line kept here says why the key is out of reach. Everything
            else this panel used to explain, its own labels already say. */}
        {fillTerrain === null ? (
          <Text color="secondary" type="supporting">
            Load a tile from the rail to cover the board with it.
          </Text>
        ) : null}
      </VStack>
    </VStack>
  );
}

const styles = stylex.create({
  // Two figures that belong together, each taking half of whatever the column
  // has. A field in this system is 200px wide when it is not told otherwise,
  // and two of those in a 340px column ran off the end of it and grew the
  // settings a sideways scrollbar. The columns are `minmax(0, 1fr)` rather
  // than `1fr` so the pair shrinks with the column instead of setting a floor
  // under it.
  pair: {
    display: "grid",
    gap: spacingVars["--spacing-2"],
    gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
  },
});
