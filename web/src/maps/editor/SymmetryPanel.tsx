/**
 * How the board is folded.
 *
 * Symmetry is the whole reason a competitive map takes a week to draw by hand:
 * every tile has to be placed as many times as the map has armies, and the one
 * that is placed wrong is the one that decides the match. Setting it here
 * makes the fold a property of the board rather than a discipline the map
 * maker has to keep, so a stroke on the left is a stroke on the right and the
 * building at the far end of it belongs to the army at the far end of it.
 *
 * The modes the board cannot hold are still shown, and say why: the answer to
 * "where is the quarter turn" is "make the board square", and hiding the key
 * hides the answer with it.
 *
 * What each fold does is drawn rather than written. Eight diagrams with a name
 * under each already say it, and a line of prose under them said it a second
 * time in words; the sentence is kept on the key itself, where somebody who
 * wants it can rest on it.
 */

import { NumberInput } from "@astryxdesign/core/NumberInput";
import { VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import {
  borderVars,
  colorVars,
  radiusVars,
  spacingVars,
  textSizeVars,
  typographyVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { SymmetryGlyph } from "./SymmetryGlyph.tsx";
import {
  MAX_ROSTER,
  MIN_ROSTER,
  rosterOfSize,
  SQUARE_ONLY_REASON,
  SYMMETRY_BLURBS,
  SYMMETRY_LABELS,
  SYMMETRY_MODES,
} from "#/maps/map_editor.ts";
import type { Symmetry } from "#/wasm/awbrn_wasm.js";

/** Names the reason the quarter turns are shut, for the keys that carry it. */
const SQUARE_ONLY_ID = "symmetry-square-only";

export function SymmetryPanel({
  available,
  onRosterChange,
  onSymmetryChange,
  roster,
  symmetry,
}: {
  available: readonly Symmetry[];
  onRosterChange: (factionCodes: string[]) => void;
  onSymmetryChange: (symmetry: Symmetry) => void;
  roster: readonly string[];
  symmetry: Symmetry;
}) {
  return (
    <VStack gap={3}>
      <ul {...stylex.props(styles.grid)}>
        {SYMMETRY_MODES.map((mode) => {
          const isAvailable = available.includes(mode);
          return (
            <li key={mode} {...stylex.props(styles.gridItem)}>
              <button
                aria-describedby={isAvailable ? undefined : SQUARE_ONLY_ID}
                aria-disabled={isAvailable ? undefined : true}
                aria-pressed={mode === symmetry}
                onClick={() => {
                  if (isAvailable) onSymmetryChange(mode);
                }}
                title={isAvailable ? SYMMETRY_BLURBS[mode] : SQUARE_ONLY_REASON}
                type="button"
                {...stylex.props(
                  styles.key,
                  mode === symmetry && styles.keySelected,
                  !isAvailable && styles.keyDisabled,
                )}
              >
                <SymmetryGlyph mode={mode} />
                <VStack as="span" gap={0} xstyle={styles.keyLabel}>
                  {SYMMETRY_LABELS[mode]}
                </VStack>
              </button>
            </li>
          );
        })}
      </ul>

      {available.length === SYMMETRY_MODES.length ? null : (
        <Text color="secondary" id={SQUARE_ONLY_ID} type="supporting">
          {SQUARE_ONLY_REASON}
        </Text>
      )}

      <NumberInput
        label="Armies"
        max={MAX_ROSTER}
        min={MIN_ROSTER}
        onChange={(value) => onRosterChange(rosterOfSize(roster, value ?? MIN_ROSTER))}
        size="sm"
        value={roster.length}
      />
    </VStack>
  );
}

const styles = stylex.create({
  // Eight keys on one keypad: four to a row, every key the same size whatever
  // its name is, so the pad reads as a set of choices rather than a paragraph.
  grid: {
    display: "grid",
    gap: spacingVars["--spacing-1"],
    gridTemplateColumns: "repeat(4, minmax(0, 1fr))",
    listStyle: "none",
    margin: 0,
    padding: 0,
  },
  gridItem: {
    display: "grid",
  },
  key: {
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    gap: spacingVars["--spacing-1"],
    blockSize: "100%",
    inlineSize: "100%",
    paddingBlock: spacingVars["--spacing-1-5"],
    paddingInline: spacingVars["--spacing-0-5"],
    borderWidth: borderVars["--border-width"],
    borderStyle: "solid",
    borderColor: colorVars["--color-border-emphasized"],
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: {
      default: colorVars["--color-background-surface"],
      ":hover": { "@media (hover: hover)": colorVars["--color-background-muted"] },
    },
    color: colorVars["--color-text-primary"],
    cursor: "pointer",
    outline: {
      default: null,
      ":focus-visible": `2px solid ${colorVars["--color-accent"]}`,
    },
    outlineOffset: { default: null, ":focus-visible": "2px" },
  },
  keySelected: {
    backgroundColor: {
      default: colorVars["--color-accent"],
      ":hover": { "@media (hover: hover)": colorVars["--color-accent"] },
    },
    color: colorVars["--color-on-accent"],
  },
  // A fold the board cannot hold keeps its outline, softened. It is a key that
  // is out of reach, not a key that is gone.
  keyDisabled: {
    backgroundColor: colorVars["--color-background-muted"],
    borderColor: "var(--color-border-disabled)",
    color: colorVars["--color-text-disabled"],
    cursor: "not-allowed",
  },
  keyLabel: {
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-xs"],
    letterSpacing: "0.04em",
    lineHeight: 1.2,
    textAlign: "center",
    textTransform: "uppercase",
  },
});
