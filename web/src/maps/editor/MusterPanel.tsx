/**
 * What the board holds, army by army.
 *
 * A fold keeps the ground equal and says nothing about whether the map is a
 * match. This is the other half: who is seated, who can build, and what is
 * still missing. It is a readout and not a gate — a board part way through is
 * not an error — so it names what is left rather than refusing anything.
 *
 * It reads across rather than down, under the board it is describing. A map
 * maker checks the muster against the map, and a column beside the map would
 * cost the board the width its own readout is written in.
 */

import { HStack, VStack } from "@astryxdesign/core/Stack";
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
import { getFactionByCode } from "#/factions.ts";
import { boardNotes } from "#/maps/map_editor.ts";
import type { EditorArmy, EditorStateChanged } from "#/wasm/awbrn_wasm.js";

export function MusterPanel({ state }: { state: EditorStateChanged }) {
  const notes = boardNotes(state);

  return (
    <HStack align="start" gap={4} justify="between" wrap="wrap" xstyle={styles.strip}>
      {state.armies.length === 0 ? (
        <Text color="secondary" type="supporting">
          Nothing on this board belongs to anybody yet. Paint a headquarters to seat an army.
        </Text>
      ) : (
        <HStack align="stretch" as="ul" gap={2} wrap="wrap" xstyle={styles.armies}>
          {state.armies.map((army) => (
            <ArmyTile army={army} key={army.faction} />
          ))}
        </HStack>
      )}

      <VStack as="ul" gap={1} xstyle={styles.notes}>
        {notes.map((note) => (
          <HStack align="start" as="li" gap={1.5} key={note.message} xstyle={styles.note}>
            <span
              aria-hidden="true"
              {...stylex.props(styles.noteMark, note.tone === "ready" && styles.noteMarkReady)}
            />
            <Text color={note.tone === "ready" ? "primary" : "secondary"} type="supporting">
              {note.message}
            </Text>
          </HStack>
        ))}
      </VStack>
    </HStack>
  );
}

/**
 * One army and what it holds.
 *
 * The counts are the questions a map maker asks of a seat: can it build, how
 * much ground does it hold, and what does it start with. What the board is
 * worth is not among them — that is one figure about the whole board and it is
 * printed once, in the strip above this one, rather than divided up here into
 * a guess about which half of the map each neutral building will end up in.
 *
 * The headquarters is a mark rather than a count. One is the playable case;
 * none and two are both faults, and the notes name those in words, so a mark
 * that tried to carry three states would be a puzzle.
 */
function ArmyTile({ army }: { army: EditorArmy }) {
  return (
    <VStack
      as="li"
      gap={0}
      style={{ "--army": `var(--color-faction-${army.faction}-accent)` } as React.CSSProperties}
      xstyle={styles.army}
    >
      <span aria-hidden="true" {...stylex.props(styles.armyBar)} />
      <VStack gap={0.5} xstyle={styles.armyBody}>
        <HStack align="center" as="span" gap={1} justify="between" xstyle={styles.armyName}>
          <span>{getFactionByCode(army.faction)?.displayName ?? army.faction}</span>
          {army.headquarters === 1 ? <span {...stylex.props(styles.hq)}>HQ</span> : null}
        </HStack>
        <HStack as="dl" gap={2} xstyle={styles.counts}>
          <Count label="Build" value={army.production} />
          <Count label="Prop" value={army.properties} />
          <Count label="Unit" value={army.units} />
        </HStack>
      </VStack>
    </VStack>
  );
}

function Count({ label, value }: { label: string; value: number }) {
  return (
    <HStack align="center" as="div" gap={0.5}>
      <VStack as="dt" gap={0} xstyle={styles.countLabel}>
        {label}
      </VStack>
      <VStack as="dd" gap={0} xstyle={styles.countValue}>
        {value}
      </VStack>
    </HStack>
  );
}

const styles = stylex.create({
  strip: {
    borderBlockStartWidth: borderVars["--border-width"],
    borderBlockStartStyle: "solid",
    borderBlockStartColor: colorVars["--color-border-emphasized"],
    backgroundColor: colorVars["--color-background-surface"],
    paddingBlock: spacingVars["--spacing-2"],
    paddingInline: spacingVars["--spacing-3"],
  },
  armies: {
    listStyle: "none",
    margin: 0,
    padding: 0,
  },
  // The army wears its colour as a bar across the top, the way every faction
  // surface in this system does, and still says which army it is in words.
  army: {
    borderWidth: borderVars["--border-width"],
    borderStyle: "solid",
    borderColor: colorVars["--color-border-emphasized"],
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: colorVars["--color-background-muted"],
    overflow: "hidden",
  },
  armyBar: {
    backgroundColor: "var(--army)",
    blockSize: spacingVars["--spacing-1-5"],
    inlineSize: "100%",
  },
  armyBody: {
    paddingBlock: spacingVars["--spacing-1"],
    paddingInline: spacingVars["--spacing-1-5"],
  },
  armyName: {
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-xs"],
    letterSpacing: "0.06em",
    textTransform: "uppercase",
  },
  // The seat is playable when it holds exactly one headquarters. None and two
  // are both faults, and the notes name them in words; a mark that tried to
  // carry three states would be a puzzle. So the mark is only ever the good
  // case, and its absence is the prompt to read the note.
  hq: {
    backgroundColor: colorVars["--color-background-inverted"],
    color: colorVars["--color-background-surface"],
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-2xs"],
    letterSpacing: "0.08em",
    lineHeight: 1,
    paddingBlock: "0.2em",
    paddingInline: "0.35em",
  },
  counts: {
    margin: 0,
    color: colorVars["--color-text-secondary"],
  },
  countLabel: {
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-xs"],
    letterSpacing: "0.04em",
    textTransform: "uppercase",
  },
  countValue: {
    color: colorVars["--color-text-primary"],
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-xs"],
    margin: 0,
  },
  notes: {
    flexGrow: 1,
    flexBasis: "18rem",
    listStyle: "none",
    margin: 0,
    padding: 0,
  },
  note: {
    lineHeight: 1.3,
  },
  // A note is a square in the HUD, not a bullet: an amber one for what is left
  // to do, a green one for a board that is ready to play.
  noteMark: {
    backgroundColor: colorVars["--color-warning"],
    blockSize: spacingVars["--spacing-2"],
    inlineSize: spacingVars["--spacing-2"],
    flexShrink: 0,
    marginBlockStart: "0.35em",
  },
  noteMarkReady: {
    backgroundColor: colorVars["--color-success"],
  },
});
