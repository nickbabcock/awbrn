/*
 * Per-seat CO bans: the editor (a row of scope chips over the tier board) and
 * the read-back (one line per distinct list).
 */

import { Icon } from "@astryxdesign/core/Icon";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { colorVars, radiusVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useState, type ReactNode } from "react";
import { getCoById } from "#/co_roster.ts";
import { coTierBands, type CoTierList } from "#/co_tiers.ts";
import { CoStamp } from "#/components/CoStamp.tsx";
import { FactionLogo } from "#/components/FactionLogo.tsx";
import { TierPicker } from "#/play/TierPicker.tsx";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { figureStyles } from "#/ui/Figure.tsx";
import { IconChevronUp } from "#/ui/icons.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { onRadioGroupKeyDown } from "#/ui/radioGroup.ts";
import { banLines, setBans, type BanDraft, type SeatBanLists } from "../co_bans.ts";
import { sidesOf, slotsOn, teamLetter } from "../teams.ts";
import { TeamTag } from "./Teams.tsx";

/** Stamps a read-back line shows before its `+N` key. */
const LINE_STAMPS = { desktop: 4, phone: 3 };

type Scope = { kind: "all" } | { kind: "side"; team: number } | { kind: "seat"; slot: number };

/** Where each CO stands in a tier list, strongest first. */
function tierOrder(tierList: CoTierList): (coId: number) => number {
  const order = new Map(
    coTierBands(tierList)
      .flatMap((band) => band.cos)
      .map((co, index) => [co.awbwId, index]),
  );
  return (coId) => order.get(coId) ?? Number.MAX_SAFE_INTEGER;
}

/**
 * The ban editor: All, then a chip per side of two or more editable seats (a
 * shortcut that writes to each of them), then a chip per seat. In the lobby
 * All and the seats people hold are locked.
 */
export function BanEditor({
  draft,
  factionCodes,
  isAllLocked = false,
  lockedSlots,
  names,
  onChange,
  teams,
  tierList,
}: {
  draft: BanDraft;
  /** Every seat's army, by slot index. */
  factionCodes: readonly string[];
  isAllLocked?: boolean;
  lockedSlots?: ReadonlySet<number>;
  /** Who holds each seat, where someone does. */
  names: readonly (string | null)[];
  onChange: (draft: BanDraft) => void;
  teams: readonly number[];
  tierList: CoTierList;
}) {
  const seatCount = draft.seats.length;
  const isLocked = (slot: number) => lockedSlots?.has(slot) ?? false;
  const editable = (slots: readonly number[]) => slots.filter((slot) => !isLocked(slot));
  const firstEditable = editable(draft.seats.map((_, slot) => slot))[0];
  // Players can claim every open seat while the editor is open, and then
  // nothing here can change.
  const hasTarget = !isAllLocked || firstEditable !== undefined;
  const fallback: Scope = isAllLocked
    ? { kind: "seat", slot: firstEditable ?? 0 }
    : { kind: "all" };
  const [chosen, setScope] = useState<Scope>(fallback);
  const sides = sidesOf(teams).filter((team) => editable(slotsOn(teams, team)).length > 1);
  // The lobby keeps polling while the editor is open, so a player can claim
  // the chosen seat, or enough of a side, out from under it.
  const isChosenLive =
    chosen.kind === "all"
      ? !isAllLocked
      : chosen.kind === "side"
        ? sides.includes(chosen.team)
        : !isLocked(chosen.slot);
  const scope = isChosenLive ? chosen : fallback;
  const target: "all" | number[] =
    scope.kind === "all"
      ? "all"
      : scope.kind === "side"
        ? editable(slotsOn(teams, scope.team))
        : [scope.slot];
  const isQuiet = seatCount === 2;

  const all = new Set(draft.all);
  const own = draft.seats.map((list) => new Set(list));
  const reach = target === "all" ? draft.seats.map((_, slot) => slot) : target;
  const banned = new Set<number>(
    target === "all"
      ? draft.all
      : [...own[reach[0]!]!].filter((coId) => reach.every((slot) => own[slot]!.has(coId))),
  );
  const partialMarks = new Map<number, { label: string; mark: ReactNode }>();
  for (const coId of new Set(reach.flatMap((slot) => [...own[slot]!]))) {
    if (all.has(coId) || banned.has(coId)) continue;
    const holders = reach.filter((slot) => own[slot]!.has(coId));
    const side = wholeSide(holders, teams);
    partialMarks.set(coId, {
      label:
        side !== null
          ? `Team ${teamLetter(side)}`
          : holders.length === 1
            ? `seat ${holders[0]! + 1}`
            : `${holders.length} seats`,
      mark: <HoldersMark factionCodes={factionCodes} holders={holders} side={side} />,
    });
  }

  function chip(
    key: string,
    isOn: boolean,
    isChipLocked: boolean,
    label: string,
    onPress: () => void,
    body: ReactNode,
    xstyle?: stylex.StyleXStyles,
  ) {
    return (
      <button
        aria-checked={isOn}
        aria-disabled={isChipLocked ? true : undefined}
        aria-label={label}
        key={key}
        onClick={isChipLocked ? undefined : onPress}
        role="radio"
        tabIndex={isOn ? 0 : -1}
        type="button"
        {...stylex.props(
          scopeChipStyles.chip,
          isQuiet && styles.chipQuiet,
          xstyle,
          isOn && scopeChipStyles.chipOn,
          isChipLocked && styles.chipLocked,
        )}
      >
        {body}
      </button>
    );
  }

  const chips = [
    chip("all", scope.kind === "all", isAllLocked, "All", () => setScope({ kind: "all" }), "All"),
    ...sides.map((team) => {
      const slots = editable(slotsOn(teams, team));
      return chip(
        `side-${team}`,
        scope.kind === "side" && scope.team === team,
        false,
        `Team ${teamLetter(team)}, seats ${slots.map((slot) => slot + 1).join(", ")}`,
        () => setScope({ kind: "side", team }),
        <>
          <TeamTag isDecorative team={team} />
          <HStack as="span" xstyle={[styles.crests, styles.phoneHidden]}>
            {slots.map((slot) => (
              <FactionLogo
                factionCode={factionCodes[slot] ?? "os"}
                isLabelHidden
                key={slot}
                size={14}
              />
            ))}
          </HStack>
        </>,
      );
    }),
    ...draft.seats.map((_, slot) => {
      const name = names[slot] ?? null;
      return chip(
        `seat-${slot}`,
        scope.kind === "seat" && scope.slot === slot,
        isLocked(slot),
        `Seat ${slot + 1}${name ? `, ${name}` : ""}`,
        () => setScope({ kind: "seat", slot }),
        <>
          <Text color="inherit" type="inherit" xstyle={figureStyles.figure}>
            {slot + 1}
          </Text>
          <FactionLogo factionCode={factionCodes[slot] ?? "os"} isLabelHidden size={14} />
          {name && !isQuiet ? (
            <Text color="inherit" type="inherit" xstyle={[styles.chipName, styles.phoneHidden]}>
              {name}
            </Text>
          ) : null}
        </>,
        scope.kind === "side" && target !== "all" && target.includes(slot)
          ? styles.chipReach
          : undefined,
      );
    }),
  ];

  return (
    <VStack xstyle={styles.editor}>
      <HStack
        aria-label="Banned for"
        onKeyDown={onRadioGroupKeyDown}
        role="radiogroup"
        xstyle={scopeChipStyles.chips}
      >
        {chips}
      </HStack>
      <VStack xstyle={styles.tiers}>
        <TierPicker
          bannedCoIds={banned}
          inheritedCoIds={target === "all" ? undefined : all}
          isDisabled={!hasTarget}
          mode="ban"
          onSetTierBanned={(coIds, isBanned) => onChange(setBans(draft, target, coIds, isBanned))}
          onToggleBan={(coId) => onChange(setBans(draft, target, [coId], !banned.has(coId)))}
          partialMarks={partialMarks}
          tierList={tierList}
        />
      </VStack>
    </VStack>
  );
}

/** The side the seats make up exactly, when it holds more than one seat; else null. */
function wholeSide(holders: readonly number[], teams: readonly number[]): number | null {
  const side = teams.length > 0 ? teams[holders[0]!]! : null;
  const sideSlots = side === null ? [] : slotsOn(teams, side);
  return side !== null &&
    sideSlots.length > 1 &&
    sideSlots.every((slot) => holders.includes(slot)) &&
    holders.length === sideSlots.length
    ? side
    : null;
}

/**
 * Which seats ban a CO: the side's tag for a whole side, the seat's crest for
 * one, else how many. Decorative: the key names the seats in words.
 */
function HoldersMark({
  factionCodes,
  holders,
  side,
}: {
  factionCodes: readonly string[];
  holders: readonly number[];
  side: number | null;
}) {
  if (side !== null) {
    return (
      <HStack as="span" xstyle={styles.sideMark}>
        <TeamTag isDecorative team={side} />
      </HStack>
    );
  }
  if (holders.length === 1) {
    return (
      <HStack as="span" xstyle={styles.badge}>
        <FactionLogo factionCode={factionCodes[holders[0]!] ?? "os"} isLabelHidden size={14} />
      </HStack>
    );
  }
  return (
    <HStack as="span" xstyle={[styles.badge, figureStyles.figure]}>
      {holders.length}
    </HStack>
  );
}

/**
 * The read-back: one line per distinct list, each labeled by its side's tag
 * and its seats' crests, and openable in place to its whole list.
 */
export function BanLines({
  factionCodes,
  lists,
  teams,
  tierList,
  viewerSlots = [],
  viewerTeam = null,
}: {
  factionCodes: readonly string[];
  lists: SeatBanLists;
  teams: readonly number[];
  tierList: CoTierList;
  /** The viewer's seats: their line takes the YOU fill. */
  viewerSlots?: readonly number[];
  viewerTeam?: number | null;
}) {
  const [openLine, setOpenLine] = useState<number | null>(null);
  const lines = banLines(lists, teams, factionCodes.length, tierOrder(tierList));
  return (
    <VStack aria-label="Banned, by seat" role="list" xstyle={styles.lines}>
      {lines.map((line, index) => {
        const isOpen = openLine === index;
        const isWholeSide =
          line.team !== null && slotsOn(teams, line.team).length === line.slots.length;
        const isMine = line.slots.some((slot) => viewerSlots.includes(slot));
        const label = `${line.team === null ? "" : `Team ${teamLetter(line.team)}, `}seats ${line.slots.map((slot) => slot + 1).join(", ")}${isMine ? ", yours" : ""}`;
        const shown = isOpen ? line.coIds : line.coIds.slice(0, LINE_STAMPS.desktop);
        const more = (count: number) => line.coIds.length - count;
        return (
          <HStack
            key={index}
            role="listitem"
            xstyle={[styles.line, isOpen && styles.lineOpen, isMine && styles.lineMine]}
          >
            <HStack aria-label={label} as="span" role="img" xstyle={styles.lineLabel}>
              {line.team !== null ? (
                <TeamTag isDecorative isMine={line.team === viewerTeam} team={line.team} />
              ) : null}
              {isWholeSide ? null : (
                <HStack as="span" xstyle={styles.crests}>
                  {line.slots.map((slot) => (
                    <FactionLogo
                      factionCode={factionCodes[slot] ?? "os"}
                      isLabelHidden
                      key={slot}
                      size={14}
                    />
                  ))}
                </HStack>
              )}
            </HStack>
            <HStack as="span" xstyle={[styles.lineStamps, isOpen && styles.lineStampsOpen]}>
              {line.coIds.length === 0 ? (
                <Text color="secondary">None</Text>
              ) : (
                shown.map((coId, position) => {
                  const co = getCoById(coId);
                  return co ? (
                    <CoStamp
                      coKey={co.key}
                      key={coId}
                      xstyle={[
                        styles.lineStamp,
                        position === line.allStart && position > 0 && styles.allGap,
                        !isOpen && position === LINE_STAMPS.phone && styles.phoneHidden,
                      ]}
                    />
                  ) : null;
                })
              )}
            </HStack>
            <HStack as="span" xstyle={styles.moreCell}>
              {isOpen ? (
                <button
                  aria-expanded="true"
                  aria-label="Show fewer"
                  onClick={() => setOpenLine(null)}
                  type="button"
                  {...stylex.props(styles.more)}
                >
                  <Icon icon={IconChevronUp} size="sm" />
                </button>
              ) : (
                <>
                  {more(LINE_STAMPS.desktop) > 0 ? (
                    <MoreKey
                      count={more(LINE_STAMPS.desktop)}
                      onOpen={() => setOpenLine(index)}
                      total={line.coIds.length}
                      xstyle={styles.phoneHidden}
                    />
                  ) : null}
                  {more(LINE_STAMPS.phone) > 0 ? (
                    <MoreKey
                      count={more(LINE_STAMPS.phone)}
                      onOpen={() => setOpenLine(index)}
                      total={line.coIds.length}
                      xstyle={styles.desktopHidden}
                    />
                  ) : null}
                </>
              )}
            </HStack>
          </HStack>
        );
      })}
    </VStack>
  );
}

function MoreKey({
  count,
  onOpen,
  total,
  xstyle,
}: {
  count: number;
  onOpen: () => void;
  total: number;
  xstyle: stylex.StyleXStyles;
}) {
  return (
    <button
      aria-expanded="false"
      aria-label={`Show all ${total} bans`}
      onClick={onOpen}
      type="button"
      {...stylex.props(styles.more, figureStyles.figure, xstyle)}
    >
      +{count}
    </button>
  );
}

/** The editor's chip row: a scope here, a brush in the Units tab. */
export const scopeChipStyles = stylex.create({
  // The chips hold still under the sheet's head while the tiers scroll.
  chips: {
    position: "sticky",
    insetBlockStart: awbrnVars.sheetHead,
    zIndex: 1,
    flexWrap: "wrap",
    gap: {
      default: "8px",
      [pageLayout.phoneMedia]: "4px",
    },
    paddingBlock: "14px",
    paddingInline: {
      default: "20px",
      [pageLayout.phoneMedia]: "16px",
    },
    borderBlockEndColor: awbrnVars.rule,
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: "1px",
    backgroundColor: awbrnVars.sheet,
  },
  chip: {
    display: "inline-flex",
    alignItems: "center",
    gap: "6px",
    minBlockSize: {
      default: "34px",
      [pageLayout.phoneMedia]: "44px",
    },
    paddingInline: {
      default: "12px",
      [pageLayout.phoneMedia]: "8px",
    },
    borderColor: awbrnVars.ruleStrong,
    borderRadius: radiusVars["--radius-full"],
    borderStyle: "solid",
    borderWidth: "1px",
    backgroundColor: {
      default: awbrnVars.sheet,
      ":hover": { default: null, [pageLayout.hoverMedia]: awbrnVars.paper },
    },
    color: colorVars["--color-text-primary"],
    font: "inherit",
    fontSize: awbrnVars.text13_5,
    fontWeight: 650,
    cursor: "pointer",
  },
  chipOn: {
    borderColor: awbrnVars.ink900,
    backgroundColor: {
      default: awbrnVars.ink900,
      ":hover": awbrnVars.ink900,
    },
    color: awbrnVars.textOnInkStrong,
  },
});

const styles = stylex.create({
  editor: {
    display: "grid",
  },
  chipQuiet: {
    borderColor: awbrnVars.rule,
  },
  chipReach: {
    backgroundColor: awbrnVars.ink100,
  },
  // Locked, in the one look every locked thing in the ban editor takes.
  chipLocked: {
    borderColor: "transparent",
    backgroundColor: {
      default: awbrnVars.paperWell,
      ":hover": awbrnVars.paperWell,
    },
    color: awbrnVars.textTertiary,
    cursor: "default",
  },
  chipName: {
    maxInlineSize: "120px",
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  crests: {
    display: "inline-flex",
    alignItems: "center",
    gap: "3px",
  },
  tiers: {
    paddingBlock: "8px 20px",
    paddingInline: {
      default: "20px",
      [pageLayout.phoneMedia]: "16px",
    },
  },
  badge: {
    display: "inline-grid",
    placeItems: "center",
    minInlineSize: "20px",
    blockSize: "20px",
    paddingInline: "3px",
    borderColor: awbrnVars.ruleStrong,
    borderRadius: "4px",
    borderStyle: "solid",
    borderWidth: "1px",
    backgroundColor: awbrnVars.sheet,
    color: colorVars["--color-text-secondary"],
    fontSize: awbrnVars.text12,
  },
  sideMark: {
    borderRadius: "6px",
    boxShadow: `0 0 0 2px ${awbrnVars.stampWhite}`,
  },
  lines: {
    display: "grid",
    gridTemplateColumns: "auto minmax(0, 1fr) 40px",
    columnGap: "12px",
    rowGap: "4px",
    padding: "6px",
    borderRadius: "6px",
    backgroundColor: awbrnVars.paper,
  },
  line: {
    display: "grid",
    gridColumn: "1 / -1",
    gridTemplateColumns: "subgrid",
    alignItems: "center",
    minBlockSize: {
      default: "40px",
      [pageLayout.phoneMedia]: "48px",
    },
    paddingInline: "6px",
    borderRadius: "4px",
  },
  lineOpen: {
    alignItems: "start",
    paddingBlock: "4px",
  },
  lineMine: {
    backgroundColor: awbrnVars.ink100,
  },
  lineLabel: {
    display: "inline-flex",
    alignItems: "center",
    gap: "6px",
  },
  lineStamps: {
    display: "flex",
    alignItems: "center",
    gap: "4px",
    minInlineSize: 0,
    overflow: "hidden",
  },
  lineStampsOpen: {
    flexWrap: "wrap",
  },
  lineStamp: {
    flexShrink: 0,
  },
  allGap: {
    marginInlineStart: "8px",
  },
  moreCell: {
    display: "flex",
    justifyContent: "flex-end",
  },
  more: {
    display: "inline-grid",
    placeItems: "center",
    inlineSize: "40px",
    blockSize: {
      default: "32px",
      [pageLayout.phoneMedia]: "44px",
    },
    padding: 0,
    borderColor: awbrnVars.ruleStrong,
    borderRadius: "6px",
    borderStyle: "solid",
    borderWidth: "1px",
    backgroundColor: awbrnVars.sheet,
    color: colorVars["--color-text-primary"],
    fontSize: awbrnVars.text13,
    fontWeight: 650,
    cursor: "pointer",
  },
  phoneHidden: {
    display: {
      default: null,
      [pageLayout.phoneMedia]: "none",
    },
  },
  desktopHidden: {
    display: {
      default: "none",
      [pageLayout.phoneMedia]: "inline-grid",
    },
  },
});
