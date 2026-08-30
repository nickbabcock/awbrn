/*
 * THE SLOT METER
 *
 * THESIS: the one thing a ranked player needs to read at a glance is how much
 *   of their own attention is already committed. So the surface's focal
 *   element is a readout of five sockets, not a number in a sentence: filled
 *   sockets are games in play, the sunken socket is the one the seek fills
 *   next, and the flat tan sockets are capacity the player has not asked for.
 * OWN-WORLD: the meter borrows the CO intel readout. Sockets are square, take
 *   the one ink outline, and use the depth model in reverse to say what they
 *   mean: a game in play rises off the panel, an empty socket sinks into it.
 * A11Y: the sockets are decoration for a screen reader. The meter carries one
 *   label that says the same thing in words.
 */

import * as stylex from "@stylexjs/stylex";
import {
  borderVars,
  colorVars,
  radiusVars,
  spacingVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import type { SlotState } from "#/matchmaking/ranked_display.ts";

export function SlotMeter({ slots }: { slots: readonly SlotState[] }) {
  const taken = slots.filter((slot) => slot === "in-play").length;
  const searching = slots.filter((slot) => slot === "searching").length;

  return (
    <div
      {...stylex.props(styles.meter)}
      role="img"
      aria-label={meterLabel(taken, searching, slots.length)}
    >
      {slots.map((slot, index) => (
        <span
          aria-hidden="true"
          key={index}
          {...stylex.props(
            styles.socket,
            slot === "in-play" && styles.inPlay,
            slot === "searching" && styles.searching,
            slot === "spare" && styles.spare,
          )}
        />
      ))}
    </div>
  );
}

function meterLabel(taken: number, searching: number, total: number): string {
  const held = taken === 1 ? "1 slot taken" : `${taken} slots taken`;
  const rest =
    searching > 0
      ? searching === 1
        ? ", 1 slot searching"
        : `, ${searching} slots searching`
      : "";
  return `${held}${rest}, out of ${total} possible ranked games.`;
}

const SOCKET_SIZE = "1.75rem";

const styles = stylex.create({
  meter: {
    display: "flex",
    gap: spacingVars["--spacing-2"],
  },
  socket: {
    blockSize: SOCKET_SIZE,
    inlineSize: SOCKET_SIZE,
    borderRadius: radiusVars["--radius-element"],
    borderWidth: borderVars["--border-width"],
    borderStyle: "solid",
  },
  inPlay: {
    backgroundColor: colorVars["--color-accent"],
    borderColor: colorVars["--color-border-emphasized"],
    boxShadow: `2px 2px 0 0 ${colorVars["--color-shadow"]}`,
  },
  searching: {
    backgroundColor: colorVars["--color-background-surface"],
    borderColor: colorVars["--color-border-emphasized"],
    borderStyle: "dashed",
    boxShadow: `inset 2px 2px 0 0 ${colorVars["--color-shadow"]}`,
  },
  spare: {
    backgroundColor: colorVars["--color-background-muted"],
    borderColor: awbrnVars.colorBorderDisabled,
  },
});
