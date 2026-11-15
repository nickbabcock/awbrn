import { Heading } from "@astryxdesign/core/Heading";
import { Icon } from "@astryxdesign/core/Icon";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import {
  colorVars,
  radiusVars,
  spacingVars,
  typeScaleVars,
  typographyVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import { VisuallyHidden } from "@astryxdesign/core/VisuallyHidden";
import * as stylex from "@stylexjs/stylex";
import { getCoAbilities, type CoAbilities, type CoPowerText } from "#/co_abilities.ts";
import { coRoster, DEFAULT_CO_PORTRAIT_KEY, getCoById } from "#/co_roster.ts";
import { CoStamp } from "#/components/CoStamp.tsx";
import { UiSprite } from "#/components/UiSprite.tsx";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { IconChevronDown, IconChevronUp } from "#/ui/icons.tsx";
import { useState, type ReactNode } from "react";

const everyBrief = coRoster.map((co) => ({ key: co.key, abilities: getCoAbilities(co.key) }));

/**
 * The briefing for one commander. `coId` is the seat's own, or one the player
 * is pointing at before choosing.
 */
export function CoBrief({
  coId,
  factionCode,
  headingId,
}: {
  coId: number | null;
  factionCode: string | null;
  headingId?: string;
}) {
  const co = getCoById(coId);
  const abilities = co ? getCoAbilities(co.key) : null;
  const [isPowersOpen, setIsPowersOpen] = useState(false);

  return (
    <VStack xstyle={styles.brief}>
      <HStack xstyle={styles.head}>
        <CoStamp
          coKey={co?.key ?? DEFAULT_CO_PORTRAIT_KEY}
          factionCode={co ? factionCode : null}
          isDecorative
          size="lg"
        />
        <Heading id={headingId} level={2} xstyle={[styles.name, co ? null : styles.noCo]}>
          {co ? co.displayName : "No commander"}
        </Heading>
      </HStack>
      {/* Each part is as tall as the longest commander's, so pointing from one
          stamp to the next never moves the keys below the brief. */}
      <dl {...stylex.props(styles.abilities)}>
        <VStack>
          {abilities?.dayToDay ? (
            <dt>
              <VisuallyHidden>Day to day</VisuallyHidden>
            </dt>
          ) : null}
          <dd {...stylex.props(styles.text)}>
            <EveryCo current={co?.key} render={(brief) => brief.dayToDay} />
          </dd>
        </VStack>
      </dl>
      {/* Held, unseen, while no commander is chosen, for the same reason. */}
      <details
        onToggle={(event) => setIsPowersOpen(event.currentTarget.open)}
        open={isPowersOpen}
        {...stylex.props(styles.more, co === null && styles.unseen)}
      >
        <summary {...stylex.props(styles.summary)}>
          <Text color="inherit" type="inherit">
            <EveryCo
              current={co?.key}
              render={(brief) =>
                [brief.power?.name, brief.superPower.name].filter(Boolean).join(" · ")
              }
            />
          </Text>
          <Icon
            icon={isPowersOpen ? IconChevronUp : IconChevronDown}
            size="sm"
            xstyle={styles.chevron}
          />
        </summary>
        {/* Only the closed brief is held still: a player who opened the
            powers is reading, and the longest of them all would leave a
            short one in a well of blank space. */}
        {abilities ? (
          <dl {...stylex.props(styles.abilities, styles.powers)}>
            {abilities.power ? <Power label="Power" power={abilities.power} /> : null}
            <Power isSuper label="Super power" power={abilities.superPower} />
          </dl>
        ) : null}
      </details>
    </VStack>
  );
}

/**
 * One part of the brief drawn for every commander in the same cell, with only
 * `current`'s seen, so the cell keeps the height of the longest.
 */
function EveryCo({
  current,
  render,
}: {
  current: string | undefined;
  render: (abilities: CoAbilities) => ReactNode;
}) {
  return (
    <HStack as="span" xstyle={styles.everyCo}>
      {everyBrief.map(({ key, abilities }) =>
        abilities ? (
          <HStack as="span" key={key} xstyle={[styles.layer, key !== current && styles.unseen]}>
            {render(abilities)}
          </HStack>
        ) : null,
      )}
    </HStack>
  );
}

function Power({
  isSuper = false,
  label,
  power,
}: {
  isSuper?: boolean;
  label: string;
  power: CoPowerText;
}) {
  return (
    <VStack>
      <dt {...stylex.props(styles.powerKey)}>
        {label}
        <HStack
          as="span"
          aria-label={`${power.stars} ${power.stars === 1 ? "star" : "stars"}`}
          role="img"
          xstyle={styles.stars}
        >
          {Array.from({ length: power.stars }, (_, index) => (
            <UiSprite key={index} name="TerrainStar.png" scale={isSuper ? 2 : 1} />
          ))}
        </HStack>
      </dt>
      <dd {...stylex.props(styles.text)}>
        <b>{power.name}.</b> {power.effect}
      </dd>
    </VStack>
  );
}

const styles = stylex.create({
  brief: {
    display: "grid",
    gap: spacingVars["--spacing-4"],
  },
  head: {
    alignItems: "center",
    gap: awbrnVars.spacing3_5,
  },
  name: {
    minInlineSize: 0,
    overflowWrap: "anywhere",
  },
  // No commander yet: a quiet title-size line beside the open-seat stamp,
  // not a CO name in display type.
  noCo: {
    color: colorVars["--color-text-secondary"],
    fontFamily: typographyVars["--font-family-body"],
    fontSize: typeScaleVars["--text-heading-3-size"],
    fontWeight: typeScaleVars["--text-heading-3-weight"],
    lineHeight: typeScaleVars["--text-heading-3-leading"],
    fontStretch: "100%",
    letterSpacing: "normal",
    wordSpacing: "normal",
  },
  everyCo: {
    display: "grid",
  },
  layer: {
    display: "block",
    gridArea: "1 / 1",
  },
  unseen: {
    visibility: "hidden",
  },
  abilities: {
    display: "grid",
    gap: awbrnVars.spacing3_5,
    margin: 0,
  },
  powers: {
    paddingBlock: `${spacingVars["--spacing-3"]} ${spacingVars["--spacing-1"]}`,
  },
  text: {
    margin: 0,
    fontSize: awbrnVars.text14_5,
    lineHeight: 1.45,
    textWrap: "pretty",
  },
  more: {
    borderRadius: radiusVars["--radius-element"],
  },
  summary: {
    display: "flex",
    alignItems: "center",
    gap: awbrnVars.spacing2_5,
    minBlockSize: "48px",
    paddingInline: spacingVars["--spacing-3"],
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: awbrnVars.paper,
    fontWeight: typeScaleVars["--text-label-weight"],
    listStyle: "none",
    cursor: "pointer",
    "::-webkit-details-marker": {
      display: "none",
    },
  },
  chevron: {
    marginInlineStart: "auto",
    color: colorVars["--color-text-secondary"],
  },
  powerKey: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: spacingVars["--spacing-2"],
    color: colorVars["--color-text-secondary"],
    fontSize: typeScaleVars["--text-heading-5-size"],
    fontWeight: typeScaleVars["--text-heading-5-weight"],
    letterSpacing: "0.06em",
    textTransform: "uppercase",
    fontStretch: "85%",
  },
  stars: {
    display: "inline-flex",
    alignItems: "flex-end",
    gap: "1px",
  },
});
