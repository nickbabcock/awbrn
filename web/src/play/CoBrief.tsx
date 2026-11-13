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
import { getCoAbilities, type CoPowerText } from "#/co_abilities.ts";
import { DEFAULT_CO_PORTRAIT_KEY, getCoById } from "#/co_roster.ts";
import { CoStamp } from "#/components/CoStamp.tsx";
import { UiSprite } from "#/components/UiSprite.tsx";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { IconChevronDown, IconChevronUp } from "#/ui/icons.tsx";
import { useState } from "react";

/**
 * The briefing for one commander: the large stamp and the name, what the CO
 * does day to day, and both powers behind one disclosure whose summary names
 * them (`Double Time · Victory March`).
 *
 * `coId` is the CO to read: the seat's own, or one the player is pointing at
 * before choosing. `factionCode` gives the stamp its army's field.
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
      {abilities?.dayToDay ? (
        <dl {...stylex.props(styles.abilities)}>
          <VStack>
            <dt>
              <VisuallyHidden>Day to day</VisuallyHidden>
            </dt>
            <dd {...stylex.props(styles.text)}>{abilities.dayToDay}</dd>
          </VStack>
        </dl>
      ) : null}
      {abilities ? (
        <details
          onToggle={(event) => setIsPowersOpen(event.currentTarget.open)}
          open={isPowersOpen}
          {...stylex.props(styles.more)}
        >
          <summary {...stylex.props(styles.summary)}>
            <Text color="inherit" type="inherit">
              {[abilities.power?.name, abilities.superPower.name].filter(Boolean).join(" · ")}
            </Text>
            <Icon
              icon={isPowersOpen ? IconChevronUp : IconChevronDown}
              size="sm"
              xstyle={styles.chevron}
            />
          </summary>
          <dl {...stylex.props(styles.abilities, styles.powers)}>
            {abilities.power ? <Power label="Power" power={abilities.power} /> : null}
            <Power isSuper label="Super power" power={abilities.superPower} />
          </dl>
        </details>
      ) : null}
    </VStack>
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
    display: "flex",
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
