import { HStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { colorVars, textSizeVars } from "@astryxdesign/core/theme/tokens.stylex";
import { VisuallyHidden } from "@astryxdesign/core/VisuallyHidden";
import * as stylex from "@stylexjs/stylex";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { getCoById } from "#/co_roster.ts";
import { CoStamp, StampFrame } from "#/components/CoStamp.tsx";
import { FactionLogo } from "#/components/FactionLogo.tsx";
import { getFactionById } from "#/factions.ts";
import { factionColorVar } from "#/ui/faction_colors.ts";
import { figureStyles } from "#/ui/Figure.tsx";

/** One army across the table from the viewer. */
export interface OpponentSeat {
  slotIndex: number;
  factionId: number;
  coId: number | null;
  userName: string;
  /** A ranked pairing withholds the other player until both are ready. */
  isHidden?: boolean;
}

/** How many stamps a crowded table shows before the rest become a count. */
export const STACKED_STAMPS = 4;

/**
 * Who the viewer faces, as stamps: one opponent reads as crest, stamp and
 * name; a crowded table stacks the stamps and counts the rest (`+3`).
 */
export function Opponents({ seats }: { seats: readonly OpponentSeat[] }) {
  if (seats.length === 0) return null;

  if (seats.length === 1) {
    const seat = seats[0]!;
    const faction = getFactionById(seat.factionId);
    return (
      <HStack as="span" xstyle={styles.vs}>
        {faction ? <FactionLogo factionCode={faction.code} isLabelHidden size={14} /> : null}
        {seat.isHidden ? (
          // The game's "no CO" portrait: an opponent not revealed yet, not a
          // seat still choosing.
          <CoStamp isDecorative />
        ) : (
          <SeatStamp coId={seat.coId} factionCode={faction?.code ?? null} />
        )}
        {seat.isHidden ? (
          <VisuallyHidden>Opponent not revealed yet</VisuallyHidden>
        ) : (
          <Text color="inherit" type="inherit" xstyle={styles.name}>
            {seat.userName}
          </Text>
        )}
      </HStack>
    );
  }

  const shown = seats.slice(0, STACKED_STAMPS);
  const rest = seats.length - shown.length;
  return (
    <HStack
      as="span"
      aria-label={`Against ${seats.map((seat) => seat.userName).join(", ")}`}
      role="img"
      xstyle={styles.vs}
    >
      <HStack as="span" xstyle={styles.stack}>
        {shown.map((seat) => {
          const faction = getFactionById(seat.factionId);
          return (
            <SeatStamp
              coId={seat.coId}
              factionCode={faction?.code ?? null}
              isDecorative
              key={seat.slotIndex}
              xstyle={styles.stacked}
            />
          );
        })}
      </HStack>
      {rest > 0 ? (
        <Text
          color="inherit"
          type="inherit"
          aria-hidden="true"
          xstyle={[figureStyles.figure, styles.more]}
        >
          +{rest}
        </Text>
      ) : null}
    </HStack>
  );
}

/**
 * A seat's commander as a stamp. A commander not chosen, or sealed until the
 * match starts, is its army's grayed crest: the game's "no CO" portrait is a
 * red cross, which at stamp size reads as a fallen army.
 */
export function SeatStamp({
  coId,
  factionCode,
  isDecorative = false,
  isYours = false,
  label,
  xstyle,
}: {
  coId: number | null;
  factionCode: string | null;
  isDecorative?: boolean;
  /** The viewer's own seat, whose commander comes later: the crest in full on the army's field. */
  isYours?: boolean;
  /** Names a stamp with no commander; a chosen commander names itself. */
  label?: string;
  xstyle?: stylex.StyleXStyles;
}) {
  const co = getCoById(coId);
  if (co) {
    return (
      <CoStamp
        coKey={co.key}
        factionCode={factionCode}
        isDecorative={isDecorative}
        xstyle={xstyle}
      />
    );
  }
  const name = label ?? "No commander yet";
  return (
    <StampFrame
      aria-hidden={isDecorative ? true : undefined}
      aria-label={isDecorative ? undefined : name}
      fieldXstyle={[
        styles.field,
        isYours && factionCode !== null && styles.fieldColor(factionColorVar(factionCode, "field")),
      ]}
      isSmall
      role={isDecorative ? undefined : "img"}
      title={isDecorative ? undefined : name}
      xstyle={xstyle}
    >
      {factionCode ? (
        <HStack as="span" xstyle={[styles.crest, !isYours && styles.grayed]}>
          <FactionLogo factionCode={factionCode} isLabelHidden size={16} />
        </HStack>
      ) : null}
    </StampFrame>
  );
}

const styles = stylex.create({
  vs: {
    display: "inline-flex",
    alignItems: "center",
    gap: "6px",
    minInlineSize: 0,
    color: colorVars["--color-text-primary"],
    fontWeight: 600,
    fontSize: awbrnVars.text13_5,
  },
  name: {
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  stack: {
    display: "inline-flex",
    alignItems: "center",
  },
  stacked: {
    marginInlineStart: {
      default: "-6px",
      ":first-child": 0,
    },
  },
  field: {
    inlineSize: "32px",
    blockSize: "32px",
  },
  fieldColor: (color: string) => ({
    backgroundColor: color,
  }),
  crest: {
    display: "inline-grid",
  },
  grayed: {
    filter: "grayscale(1) opacity(0.5)",
  },
  more: {
    fontSize: textSizeVars["--font-size-sm"],
    fontWeight: 600,
    color: colorVars["--color-text-secondary"],
  },
});
