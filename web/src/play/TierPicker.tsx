import { Icon } from "@astryxdesign/core/Icon";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import {
  borderVars,
  colorVars,
  focusVars,
  radiusVars,
  sizeVars,
  spacingVars,
  typeScaleVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { coTierBands, type CoTierList } from "#/co_tiers.ts";
import { CoStamp } from "#/components/CoStamp.tsx";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { IconCheck, IconChevronDown } from "#/ui/icons.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";

/** How many tiers a phone shows before the rest fold behind "12 more". */
const PHONE_OPEN_TIERS = 3;
/** The tier number's column beside its row of stamps (desktop). */
const TIER_KEY_WIDTH = "76px";
/** A stamp's cell on desktop: the 64px stamp and its margin, with room for the name. */
const PICK_WIDTH = "80px";

type TierPickerProps = {
  bannedCoIds: ReadonlySet<number>;
  tierList: CoTierList;
  isDisabled?: boolean;
} & (
  | {
      /** Pressing a stamp makes that CO the seat's. */
      mode: "pick";
      chosenCoId: number | null;
      /** The army whose field the chosen stamp takes. */
      factionCode: string | null;
      onPick: (coId: number) => void;
      /**
       * Pointing at a stamp with a mouse, so the briefing can read it before it
       * is chosen. Not on focus: a tap focuses even when it misses the click,
       * and the briefing would then show a CO that is not the pick.
       */
      onPreview?: (coId: number | null) => void;
    }
  | {
      /** Pressing a stamp bans that CO, or gives it back; a tier number bans the tier. */
      mode: "ban";
      onToggleBan: (coId: number) => void;
      onSetTierBanned: (coIds: number[], isBanned: boolean) => void;
    }
);

/** Rows by tier number, stamps alphabetical. On a phone the weaker tiers fold behind one key. */
export function TierPicker(props: TierPickerProps) {
  const bands = coTierBands(props.tierList);
  const [isExpanded, setIsExpanded] = useState(false);
  // The fold starts below the pick, so the player always sees it marked.
  const chosenTier =
    props.mode === "pick"
      ? (bands.find((band) => band.cos.some((co) => co.awbwId === props.chosenCoId))?.tier ?? 0)
      : 0;
  const openTiers = Math.max(PHONE_OPEN_TIERS, chosenTier + 1);
  const folded = bands
    .filter((band) => band.tier >= openTiers)
    .reduce((count, band) => count + band.cos.length, 0);

  return (
    <VStack xstyle={styles.tiers}>
      {bands.map((band) => {
        const isFolded = !isExpanded && band.tier >= openTiers;
        const bandIds = band.cos.map((co) => co.awbwId);
        const isBandBanned = bandIds.every((coId) => props.bannedCoIds.has(coId));
        return (
          <VStack
            aria-label={`Tier ${band.tier}, ${band.name}`}
            key={band.tier}
            role="group"
            xstyle={[styles.tier, isFolded && styles.folded]}
          >
            <HStack as="span" xstyle={styles.tierKey}>
              {props.mode === "ban" ? (
                <button
                  aria-label={`${isBandBanned ? "Lift the bans on" : "Ban"} tier ${band.tier}`}
                  aria-pressed={isBandBanned}
                  disabled={props.isDisabled}
                  onClick={() => props.onSetTierBanned(bandIds, !isBandBanned)}
                  title={`${isBandBanned ? "Lift the bans on" : "Ban"} tier ${band.tier}`}
                  type="button"
                  {...stylex.props(styles.tierNumber, styles.key, styles.tierButton)}
                >
                  {band.tier}
                </button>
              ) : (
                <Text color="inherit" type="inherit" aria-hidden="true" xstyle={styles.tierNumber}>
                  {band.tier}
                </Text>
              )}
              <Text color="inherit" type="inherit" aria-hidden="true" xstyle={styles.tierName}>
                {band.name}
              </Text>
            </HStack>
            <HStack xstyle={styles.cos}>
              {band.cos.map((co) => {
                const isBanned = props.bannedCoIds.has(co.awbwId);
                if (props.mode === "ban") {
                  return (
                    <button
                      aria-label={`${co.displayName}${isBanned ? ", banned" : ""}`}
                      aria-pressed={isBanned}
                      disabled={props.isDisabled}
                      key={co.key}
                      onClick={() => props.onToggleBan(co.awbwId)}
                      type="button"
                      {...stylex.props(styles.key, styles.pick)}
                    >
                      <CoStamp
                        coKey={co.key}
                        isDecorative
                        size="md"
                        state={isBanned ? "banned" : "default"}
                      />
                      <Text
                        color="inherit"
                        type="inherit"
                        xstyle={[styles.name, isBanned && styles.nameBanned]}
                      >
                        {co.displayName}
                      </Text>
                    </button>
                  );
                }
                const isChosen = props.chosenCoId === co.awbwId;
                return (
                  <button
                    aria-label={`${co.displayName}${isBanned ? ", banned in this match" : ""}`}
                    aria-pressed={isChosen}
                    disabled={isBanned || props.isDisabled}
                    key={co.key}
                    onClick={() => props.onPick(co.awbwId)}
                    onPointerEnter={(event) => {
                      if (event.pointerType === "mouse") props.onPreview?.(co.awbwId);
                    }}
                    onPointerLeave={(event) => {
                      if (event.pointerType === "mouse") props.onPreview?.(null);
                    }}
                    type="button"
                    {...stylex.props(
                      styles.key,
                      styles.pick,
                      props.isDisabled && styles.pickInert,
                      isChosen && styles.picked,
                      isBanned && styles.pickBanned,
                    )}
                  >
                    <CoStamp
                      coKey={co.key}
                      factionCode={isChosen ? props.factionCode : null}
                      isDecorative
                      size="md"
                      state={isBanned ? "banned" : "default"}
                    />
                    <Text
                      color="inherit"
                      type="inherit"
                      xstyle={[styles.name, isBanned && styles.nameBanned]}
                    >
                      {co.displayName}
                    </Text>
                    {isChosen ? (
                      <VStack as="span" aria-hidden="true" xstyle={styles.check}>
                        <Icon icon={IconCheck} size="xsm" />
                      </VStack>
                    ) : null}
                  </button>
                );
              })}
            </HStack>
          </VStack>
        );
      })}
      {folded > 0 && !isExpanded ? (
        <Button
          endContent={<Icon icon={IconChevronDown} size="sm" />}
          label={`${folded} more`}
          onClick={() => setIsExpanded(true)}
          width="100%"
          xstyle={styles.more}
        />
      ) : null}
    </VStack>
  );
}

const styles = stylex.create({
  tiers: {
    display: "grid",
    gap: spacingVars["--spacing-1"],
  },
  tier: {
    display: "grid",
    gridTemplateColumns: {
      default: `${TIER_KEY_WIDTH} minmax(0, 1fr)`,
      [pageLayout.phoneMedia]: "minmax(0, 1fr)",
    },
    alignItems: "start",
    gap: {
      default: spacingVars["--spacing-3"],
      [pageLayout.phoneMedia]: spacingVars["--spacing-1-5"],
    },
    paddingBlockStart: {
      default: awbrnVars.spacing2_5,
      ":first-child": spacingVars["--spacing-1"],
    },
    paddingBlockEnd: awbrnVars.spacing2_5,
    borderBlockStartColor: awbrnVars.rule,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: {
      default: borderVars["--border-width"],
      ":first-child": 0,
    },
  },
  folded: {
    display: {
      default: "grid",
      [pageLayout.phoneMedia]: "none",
    },
  },
  tierKey: {
    flexDirection: {
      default: "column",
      [pageLayout.phoneMedia]: "row",
    },
    alignItems: {
      default: "start",
      [pageLayout.phoneMedia]: "baseline",
    },
    gap: spacingVars["--spacing-1-5"],
    paddingBlockStart: {
      default: spacingVars["--spacing-1-5"],
      [pageLayout.phoneMedia]: 0,
    },
  },
  tierNumber: {
    fontFamily: awbrnVars.fontUi,
    fontSize: typeScaleVars["--text-heading-2-size"],
    fontWeight: 850,
    fontStretch: "62%",
    lineHeight: 1,
    color: colorVars["--color-text-primary"],
  },
  tierName: {
    color: colorVars["--color-text-secondary"],
    fontSize: typeScaleVars["--text-heading-5-size"],
    fontWeight: typeScaleVars["--text-heading-5-weight"],
  },
  key: {
    borderStyle: "none",
    borderRadius: radiusVars["--radius-element"],
    // Hover tones only where a pointer hovers, so a tap leaves none behind;
    // touch gets the same tone while pressed.
    backgroundColor: {
      default: "transparent",
      ":hover": { default: null, [pageLayout.hoverMedia]: awbrnVars.paper },
      ":active": awbrnVars.paper,
    },
    outline: {
      default: null,
      ":focus-visible": `2px solid ${focusVars["--focus-outline-color"]}`,
    },
    outlineOffset: { default: null, ":focus-visible": "2px" },
    cursor: "pointer",
  },
  // The tier number as a key (ban mode): the touch floor, with its label
  // where the plain number sits.
  tierButton: {
    minInlineSize: sizeVars["--size-element-md"],
    minBlockSize: sizeVars["--size-element-md"],
    marginBlock: `calc(${spacingVars["--spacing-2"]} * -1)`,
    marginInlineStart: `calc(${awbrnVars.spacing2_5} * -1)`,
    paddingInline: awbrnVars.spacing2_5,
    textAlign: "start",
  },
  cos: {
    display: {
      default: "flex",
      [pageLayout.phoneMedia]: "grid",
    },
    flexWrap: "wrap",
    gridTemplateColumns: {
      default: null,
      [pageLayout.phoneMedia]: "repeat(3, minmax(0, 1fr))",
    },
    gap: spacingVars["--spacing-1"],
  },
  pick: {
    position: "relative",
    display: "grid",
    justifyItems: "center",
    gap: spacingVars["--spacing-1"],
    inlineSize: {
      default: PICK_WIDTH,
      [pageLayout.phoneMedia]: "auto",
    },
    minInlineSize: 0,
    paddingBlockStart: spacingVars["--spacing-1"],
    paddingBlockEnd: spacingVars["--spacing-1-5"],
    paddingInline: 0,
    color: colorVars["--color-text-primary"],
    transitionProperty: "background-color",
    transitionDuration: {
      default: "90ms",
      [pageLayout.reducedMotionMedia]: "0s",
    },
    transitionTimingFunction: awbrnVars.easeStep,
  },
  // A picker nobody can pick from yet (signed out, unseated, ready) shows the
  // roster without the hover tone and hand that promise a choice.
  pickInert: {
    backgroundColor: "transparent",
    cursor: "default",
  },
  picked: {
    backgroundColor: awbrnVars.ink900,
    color: awbrnVars.textOnInkStrong,
  },
  pickBanned: {
    cursor: "not-allowed",
    backgroundColor: "transparent",
  },
  name: {
    fontSize: typeScaleVars["--text-heading-5-size"],
    fontWeight: typeScaleVars["--text-heading-5-weight"],
    lineHeight: typeScaleVars["--text-heading-5-leading"],
  },
  nameBanned: {
    color: awbrnVars.textTertiary,
    textDecoration: "line-through",
  },
  check: {
    position: "absolute",
    insetBlockStart: spacingVars["--spacing-0-5"],
    insetInlineEnd: spacingVars["--spacing-1-5"],
    display: "grid",
    placeItems: "center",
    inlineSize: spacingVars["--spacing-5"],
    blockSize: spacingVars["--spacing-5"],
    borderRadius: radiusVars["--radius-full"],
    backgroundColor: awbrnVars.red,
    boxShadow: `0 0 0 2px ${awbrnVars.ink900}`,
    color: colorVars["--color-on-accent"],
  },
  more: {
    display: {
      default: "none",
      [pageLayout.phoneMedia]: "inline-flex",
    },
    marginBlockStart: spacingVars["--spacing-2"],
  },
});
