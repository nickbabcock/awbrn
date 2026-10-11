/**
 * THE CO INTEL BOARD
 *
 * Every commanding officer at once, one face each, at the same size, set in
 * bands by how strong they play. A player knows these COs by face, the way the
 * game's own CO screen shows them, so the board is faces only. The briefing
 * names the face under the pointer or the focus, every tile carries its name
 * for a screen reader, and the browser's find in page finds a CO by name. The
 * tiles do not change shape between the screen that bans a CO and the screen
 * that picks one.
 *
 * The bands are the tier list that fits the match, strongest first. A player
 * reads them to learn what the match allows, and a host reads them to ban a
 * whole band at once.
 *
 * A banned CO is not hidden. It keeps its tile and wears the strike, because a
 * player looking for Grit has to find out that Grit is gone rather than fail
 * to find him.
 */

import { Grid } from "@astryxdesign/core/Grid";
import { SelectableCard } from "@astryxdesign/core/SelectableCard";
import { Section } from "@astryxdesign/core/Section";
import { HStack, Stack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { borderVars, colorVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useMemo, useRef, useState } from "react";
import { getCoById, type CoRosterEntry } from "#/co_roster.ts";
import { coTierBands, type CoTierBand, type CoTierList } from "#/co_tiers.ts";
import { CoBriefing } from "#/components/CoBriefing.tsx";
import { CoPortrait } from "#/components/CoPortrait.tsx";
import { loadCoPortraitCatalog } from "#/components/co_portraits.ts";
import { findableRef, useFindableLabels } from "#/components/useFindableLabels.ts";
import { Button } from "#/ui/Button.tsx";

/**
 * How wide a tile runs and how many screen pixels its portrait takes.
 *
 * Both sizes draw the 32-pixel portrait at exactly twice its size, so the art
 * stays crisp and a face is large enough to know without a name.
 */
const BOARD_SIZES = {
  md: { portrait: 64, tile: 66 },
  sm: { portrait: 64, tile: 68 },
} as const;

/** A device whose pointer can rest on a face without pressing it. */
const HOVER_MEDIA = "@media (hover: hover) and (pointer: fine)";

/** How many screen pixels a portrait takes in the banned strip. */
const STRIP_PORTRAIT_SIZE = 32;

type CoBoardProps = {
  /** The COs this match has taken away. */
  bannedCoIds: ReadonlySet<number>;
  /** Locks the board, for a seat that cannot change or a read-only board. */
  isDisabled?: boolean;
  /** How much room the board has. Defaults to the full-width briefing size. */
  size?: keyof typeof BOARD_SIZES;
  /** The tier list that fits this match. */
  tierList: CoTierList;
} & (
  | {
      /** Pressing a tile bans that CO, or gives it back. */
      mode: "ban";
      onToggleBan: (coId: number) => void;
    }
  | {
      /**
       * Pressing a tile makes that CO the seat's. Pointing at a tile, or
       * focusing it, shows that CO in the briefing without choosing them.
       */
      mode: "pick";
      selectedCoId: number | null;
      /** A pick is on its way to the server. */
      isSaving?: boolean;
      onPick: (coId: number) => void;
    }
);

export function CoBoard(props: CoBoardProps) {
  const { bannedCoIds, isDisabled = false, mode, size = "md", tierList } = props;
  const catalog = useMemo(() => loadCoPortraitCatalog(), []);
  const bands = useMemo(() => coTierBands(tierList), [tierList]);
  // The face under the pointer or the focus is what the briefing describes.
  // It is local, so reading a commander never touches the seat; only a press
  // claims one.
  const [previewCoId, setPreviewCoId] = useState<number | null>(null);
  const { portrait, tile } = BOARD_SIZES[size];
  const boardRef = useRef<HTMLDivElement>(null);
  // A CO found by find in page is read in the briefing, as if pointed at.
  useFindableLabels(boardRef, (value) => setPreviewCoId(value === null ? null : Number(value)));
  const chosenCoId = mode === "pick" ? props.selectedCoId : null;

  function pressTile(co: CoRosterEntry, isBanned: boolean) {
    if (mode === "ban") {
      props.onToggleBan(co.awbwId);
      return;
    }
    // A banned face can be read, but a press does not claim it. Pressing the
    // seat's own commander again keeps them; a pick is changed, not undone.
    setPreviewCoId(co.awbwId);
    if (isBanned || co.awbwId === chosenCoId) return;
    props.onPick(co.awbwId);
  }

  function toggleBand(band: CoTierBand) {
    if (mode !== "ban") return;
    const isWholeBandBanned = band.cos.every((co) => bannedCoIds.has(co.awbwId));
    for (const co of band.cos) {
      if (bannedCoIds.has(co.awbwId) === isWholeBandBanned) props.onToggleBan(co.awbwId);
    }
  }

  const board = (
    <VStack
      gap={mode === "ban" ? 4 : 1}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setPreviewCoId(null);
      }}
      onPointerLeave={() => setPreviewCoId(null)}
      ref={boardRef}
      xstyle={styles.board}
    >
      {bands.map((band) => {
        const bannedInBand = band.cos.filter((co) => bannedCoIds.has(co.awbwId)).length;
        const isWholeBandBanned = bannedInBand === band.cos.length;

        return (
          // A picking board sets each tier's number beside its row of faces, so
          // the bands cost no height of their own. A banning board keeps a head
          // row, because it holds the key that bans the band.
          <Stack
            as="section"
            aria-label={`Tier ${band.tier}, ${band.name}`}
            direction={mode === "ban" ? "vertical" : "horizontal"}
            gap={2}
            key={band.tier}
            xstyle={mode === "pick" && styles.bandRow}
          >
            {mode === "ban" ? (
              <HStack align="center" gap={2} justify="between" xstyle={styles.bandHead}>
                <Text type="label">Tier {band.tier}</Text>
                <Button
                  clickAction={() => toggleBand(band)}
                  isDisabled={isDisabled}
                  label={
                    isWholeBandBanned
                      ? `Allow every tier ${band.tier} CO`
                      : `Ban every tier ${band.tier} CO`
                  }
                  size="sm"
                  variant="secondary"
                >
                  {isWholeBandBanned ? "Allow tier" : "Ban tier"}
                </Button>
              </HStack>
            ) : (
              <Text type="label" xstyle={styles.bandLabel}>
                T{band.tier}
              </Text>
            )}
            <Grid columns={{ minWidth: tile, repeat: "fill" }} gap={1} xstyle={styles.bandGrid}>
              {band.cos.map((co) => {
                const isBanned = bannedCoIds.has(co.awbwId);
                const isChosen = mode === "pick" && chosenCoId === co.awbwId;
                const isSelected = mode === "ban" ? isBanned : isChosen;

                return (
                  <SelectableCard
                    isDisabled={isDisabled}
                    isSelected={isSelected}
                    key={co.awbwId}
                    label={tileLabel(co, { isBanned, mode })}
                    onChange={() => pressTile(co, isBanned)}
                    onFocus={() => setPreviewCoId(co.awbwId)}
                    onPointerEnter={(event) => {
                      // A touch press already claims the face it lands on.
                      if (event.pointerType === "mouse") setPreviewCoId(co.awbwId);
                    }}
                    padding={0}
                    xstyle={styles.tile}
                  >
                    <VStack align="center" gap={0}>
                      <Section
                        padding={0}
                        variant="muted"
                        xstyle={[
                          styles.well,
                          isChosen && styles.wellChosen,
                          isBanned && styles.wellBanned,
                        ]}
                      >
                        <CoPortrait
                          catalog={catalog}
                          coKey={co.key}
                          fallbackLabel={co.displayName}
                          hasFrame={false}
                          size={portrait}
                        />
                      </Section>
                    </VStack>
                    {/* The name shows only when find in page lands on it. It
                        has no box of its own while hidden, and the tile's
                        label already names the face to a screen reader. */}
                    <span
                      aria-hidden
                      data-find-label={co.awbwId}
                      hidden
                      ref={findableRef}
                      {...stylex.props(styles.findLabel)}
                    >
                      <Text color="inherit" type="label">
                        {co.displayName}
                      </Text>
                    </span>
                  </SelectableCard>
                );
              })}
            </Grid>
          </Stack>
        );
      })}
    </VStack>
  );

  if (mode === "ban") return board;

  const briefing = (
    <CoBriefing
      bannedCoIds={bannedCoIds}
      chosenCoId={chosenCoId}
      coId={previewCoId ?? chosenCoId}
      isLocked={isDisabled}
      isSaving={props.isSaving ?? false}
      tierList={tierList}
    />
  );

  // Nothing above the faces may change height as the readout changes, or a
  // face moves out from under the pointer. With a pointer the briefing sits
  // under the board. A touch screen has no hover and reads a press, so there
  // the briefing sits above the board, where the press shows it, at one height.
  return (
    <VStack gap={4}>
      {board}
      <VStack gap={0} xstyle={styles.briefingSlot}>
        {briefing}
      </VStack>
    </VStack>
  );
}

/**
 * The COs this match took away, named and struck.
 *
 * A player reads this before claiming a seat, so it lists only what is gone
 * rather than making them scan the whole roster for the strikes.
 */
export function BannedCoList({ bannedCoIds }: { bannedCoIds: readonly number[] }) {
  const catalog = useMemo(() => loadCoPortraitCatalog(), []);
  const banned = bannedCoIds
    .map((coId) => getCoById(coId))
    .filter((co): co is CoRosterEntry => co !== null);

  if (banned.length === 0) {
    return (
      <Text color="secondary" type="supporting">
        Every CO is available in this match.
      </Text>
    );
  }

  return (
    <HStack as="ul" gap={3} wrap="wrap" xstyle={styles.plainList}>
      {banned.map((co) => (
        <HStack align="center" as="li" gap={1.5} key={co.awbwId}>
          <Section
            padding={0}
            variant="muted"
            xstyle={[styles.well, styles.stripWell, styles.wellBanned]}
          >
            <CoPortrait
              catalog={catalog}
              coKey={co.key}
              fallbackLabel={co.displayName}
              hasFrame={false}
              size={STRIP_PORTRAIT_SIZE}
            />
          </Section>
          <Text hasStrikethrough type="label">
            {co.displayName}
          </Text>
        </HStack>
      ))}
    </HStack>
  );
}

function tileLabel(
  co: CoRosterEntry,
  { isBanned, mode }: { isBanned: boolean; mode: "ban" | "pick" },
): string {
  if (mode === "ban") return isBanned ? `${co.displayName}, banned` : `Ban ${co.displayName}`;
  return isBanned ? `${co.displayName}, banned in this match` : co.displayName;
}

const styles = stylex.create({
  board: {
    flexGrow: 1,
    minInlineSize: 0,
  },
  // A band's rule runs under its name, so the bands read as one list cut in
  // five rather than five boxes.
  bandHead: {
    minBlockSize: spacingVars["--spacing-8"],
    borderBlockEndColor: colorVars["--color-border-emphasized"],
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: borderVars["--border-width"],
    paddingBlockEnd: spacingVars["--spacing-1"],
  },
  briefingSlot: {
    order: {
      default: -1,
      [HOVER_MEDIA]: 0,
    },
    // One height for every CO, so neither the faces above nor Ready below
    // moves as the readout changes.
    minBlockSize: "17.5rem",
  },
  bandRow: {
    alignItems: "start",
  },
  // The tier number holds a fixed column, so every band's faces start on the
  // same line.
  bandLabel: {
    flexShrink: 0,
    inlineSize: "1.75rem",
    paddingBlockStart: spacingVars["--spacing-2"],
  },
  bandGrid: {
    flexGrow: 1,
    minInlineSize: 0,
  },
  // The tile lets its find label hang out over the next row.
  tile: {
    overflow: "visible",
    position: "relative",
  },
  // The seat's own commander wears a bar in the command color across the foot
  // of the face, a second mark beside the ring, so the chosen face reads by
  // shape and not by the ring's color alone.
  wellChosen: {
    "::after": {
      backgroundColor: colorVars["--color-accent"],
      blockSize: spacingVars["--spacing-1-5"],
      borderBlockStartColor: colorVars["--color-border-emphasized"],
      borderBlockStartStyle: "solid",
      borderBlockStartWidth: borderVars["--border-width"],
      content: "''",
      insetBlockEnd: 0,
      insetInline: 0,
      position: "absolute",
    },
  },
  // The name that find in page shows, hung under the face over the next row.
  findLabel: {
    position: "absolute",
    insetBlockStart: "100%",
    insetInlineStart: "50%",
    transform: "translateX(-50%)",
    zIndex: 1,
    marginBlockStart: spacingVars["--spacing-1"],
    paddingInline: spacingVars["--spacing-2"],
    paddingBlock: spacingVars["--spacing-1"],
    whiteSpace: "nowrap",
    pointerEvents: "none",
    color: colorVars["--color-on-dark"],
    backgroundColor: colorVars["--color-background-inverted"],
    borderRadius: "var(--radius-element)",
  },
  // The portrait already sits inside the tile's outline, so it takes the
  // recessed fill and none of the border.
  well: {
    borderRadius: "var(--radius-element)",
    lineHeight: 0,
    overflow: "hidden",
    position: "relative",
  },
  stripWell: {
    flex: "0 0 auto",
  },
  // A banned CO is washed out and struck corner to corner. Both marks are
  // drawn on the well rather than over the whole tile, so they land on the
  // face and leave the name under it at full strength. The wash goes under
  // the strike rather than over the portrait as one opacity, which would take
  // the strike down with it.
  wellBanned: {
    "::before": {
      backgroundColor: colorVars["--color-background-muted"],
      content: "''",
      inset: 0,
      opacity: 0.55,
      position: "absolute",
    },
    "::after": {
      backgroundImage: `linear-gradient(to bottom right, transparent calc(50% - ${borderVars["--border-width"]}), ${colorVars["--color-error"]} calc(50% - ${borderVars["--border-width"]}), ${colorVars["--color-error"]} calc(50% + ${borderVars["--border-width"]}), transparent calc(50% + ${borderVars["--border-width"]}))`,
      content: "''",
      inset: 0,
      position: "absolute",
    },
  },
  plainList: {
    listStyleType: "none",
    margin: 0,
    padding: 0,
  },
});
