/*
 * Sides in a team match: the letter tag, the host's key that moves a seat
 * between sides, and the match's shape figure. No team colors: army colors
 * stay the only hues on a row, and filled against quiet says which side is
 * yours.
 */

import { DropdownMenu, DropdownMenuItem } from "@astryxdesign/core/DropdownMenu";
import { Icon } from "@astryxdesign/core/Icon";
import { HStack } from "@astryxdesign/core/Stack";
import { colorVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { FactionLogo } from "#/components/FactionLogo.tsx";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { IconCheck, IconChevronDown } from "#/ui/icons.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { teamChoices, teamLetter, teamShape } from "../teams.ts";

/** A side's letter. Yours is filled. */
export function TeamTag({
  isDecorative = false,
  isMine = false,
  isNew = false,
  onInk = false,
  size = 20,
  team,
}: {
  isDecorative?: boolean;
  isMine?: boolean;
  /** The side a seat would open (dashed). */
  isNew?: boolean;
  onInk?: boolean;
  size?: 18 | 20;
  team: number;
}) {
  const letter = teamLetter(team);
  return (
    <HStack
      aria-hidden={isDecorative ? true : undefined}
      aria-label={isDecorative ? undefined : `Team ${letter}${isMine ? ", yours" : ""}`}
      as="span"
      role={isDecorative ? undefined : "img"}
      xstyle={[
        styles.tag,
        size === 18 && styles.tagSmall,
        onInk ? styles.tagOnInk : styles.tagQuiet,
        isMine && (onInk ? styles.tagMineOnInk : styles.tagMine),
        isNew && styles.tagNew,
      ]}
    >
      {letter}
    </HStack>
  );
}

/** The match's shape as a figure: `4v1`. */
export function TeamShape({
  teams,
  xstyle,
}: {
  teams: readonly number[];
  xstyle?: stylex.StyleXStyles;
}) {
  const shape = teamShape(teams);
  return (
    <HStack aria-label={`Shape ${shape}`} as="span" xstyle={[styles.shape, xstyle]}>
      {shape}
    </HStack>
  );
}

/**
 * The host's key at the end of a seat: its tag and a chevron, opening the
 * sides with their armies, then a new side.
 */
export function TeamKey({
  factionCodes,
  isDisabled = false,
  onChange,
  slotIndex,
  teams,
  viewerTeam,
}: {
  /** Every seat's army, by slot index. */
  factionCodes: readonly string[];
  isDisabled?: boolean;
  onChange: (team: number) => void;
  slotIndex: number;
  teams: readonly number[];
  viewerTeam: number | null;
}) {
  const team = teams[slotIndex]!;
  return (
    <DropdownMenu
      alignment="end"
      menuWidth={248}
      presentation="popover"
      renderTrigger={(props) => (
        <button
          {...props}
          aria-label={`Team ${teamLetter(team)}, seat ${slotIndex + 1}. Change team`}
          disabled={isDisabled}
          title="Change team"
          type="button"
          {...stylex.props(styles.key)}
        >
          <TeamTag isDecorative isMine={team === viewerTeam} team={team} />
          <Icon icon={IconChevronDown} size="sm" />
        </button>
      )}
    >
      {teamChoices(teams, slotIndex).map((choice) => (
        <DropdownMenuItem
          endContent={
            choice.isCurrent ? <Icon icon={IconCheck} size="sm" xstyle={styles.check} /> : undefined
          }
          icon={
            <TeamTag
              isDecorative
              isMine={choice.team === viewerTeam}
              isNew={choice.isNew}
              team={choice.team}
            />
          }
          isDisabled={choice.isDisabled}
          key={choice.team}
          label={
            choice.isNew ? (
              "New"
            ) : (
              <HStack
                aria-label={`Team ${teamLetter(choice.team)}, seats ${choice.slots.map((slot) => slot + 1).join(", ")}`}
                as="span"
                xstyle={styles.crests}
              >
                {choice.slots.map((slot) => (
                  <FactionLogo
                    factionCode={factionCodes[slot] ?? "os"}
                    isLabelHidden
                    key={slot}
                    size={14}
                  />
                ))}
              </HStack>
            )
          }
          onClick={() => {
            if (!choice.isCurrent) onChange(choice.team);
          }}
          xstyle={[styles.row, choice.isCurrent && styles.rowCurrent]}
        />
      ))}
    </DropdownMenu>
  );
}

const styles = stylex.create({
  tag: {
    display: "inline-grid",
    placeItems: "center",
    flexShrink: 0,
    inlineSize: "20px",
    blockSize: "20px",
    borderRadius: "4px",
    borderStyle: "solid",
    borderWidth: "1px",
    fontFamily: awbrnVars.fontUi,
    fontSize: awbrnVars.text12_5,
    fontWeight: 800,
    lineHeight: 1,
  },
  tagSmall: {
    inlineSize: "18px",
    blockSize: "18px",
  },
  tagQuiet: {
    borderColor: awbrnVars.wellHairline,
    backgroundColor: awbrnVars.paperWell,
    color: colorVars["--color-text-secondary"],
  },
  tagMine: {
    borderColor: awbrnVars.ink900,
    backgroundColor: awbrnVars.ink900,
    color: awbrnVars.textOnInkStrong,
  },
  tagOnInk: {
    borderColor: awbrnVars.ink700,
    backgroundColor: awbrnVars.ink700,
    color: awbrnVars.textOnInkStrong,
  },
  tagMineOnInk: {
    borderColor: awbrnVars.sheet,
    backgroundColor: awbrnVars.sheet,
    color: awbrnVars.ink900,
  },
  tagNew: {
    borderStyle: "dashed",
    borderColor: awbrnVars.ruleStrong,
    backgroundColor: "transparent",
  },
  shape: {
    fontFamily: awbrnVars.fontFigure,
    fontSize: awbrnVars.text13,
    fontWeight: 600,
    color: colorVars["--color-text-secondary"],
    whiteSpace: "nowrap",
  },
  key: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    gap: "4px",
    flexShrink: 0,
    minBlockSize: {
      default: "36px",
      [pageLayout.phoneMedia]: "44px",
    },
    paddingInline: "8px",
    borderColor: awbrnVars.ruleStrong,
    borderRadius: "6px",
    borderStyle: "solid",
    borderWidth: "1px",
    backgroundColor: {
      default: awbrnVars.sheet,
      ":hover": { default: null, [pageLayout.hoverMedia]: awbrnVars.paper },
    },
    color: colorVars["--color-text-secondary"],
    cursor: "pointer",
  },
  crests: {
    display: "inline-flex",
    alignItems: "center",
    gap: "4px",
  },
  row: {
    minBlockSize: "48px",
  },
  rowCurrent: {
    backgroundColor: awbrnVars.redWash,
  },
  check: {
    color: awbrnVars.red,
  },
});
