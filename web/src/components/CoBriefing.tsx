/**
 * THE CO BRIEFING
 *
 * The intel card for one commanding officer: the face at twice its
 * native size, the name, the tier, and what the CO does, in the order a player
 * meets it: the ability that never switches off, then the power that is
 * charged with stars, then the super power that costs more and hits harder.
 *
 * It names the CO it describes and says how that CO stands with the seat:
 * already the seat's commander, only being read, or banned. A player who points
 * at Adder while Sami holds the seat must never read Adder's powers as Sami's.
 */

import { Grid } from "@astryxdesign/core/Grid";
import { Heading } from "@astryxdesign/core/Heading";
import { Section } from "@astryxdesign/core/Section";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { VisuallyHidden } from "@astryxdesign/core/VisuallyHidden";
import { borderVars, colorVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useMemo, type ReactNode } from "react";
import { getCoAbilities, type CoPowerText } from "#/co_abilities.ts";
import { DEFAULT_CO_PORTRAIT_KEY, getCoById } from "#/co_roster.ts";
import { coTierNames, coTierOf, type CoTierList } from "#/co_tiers.ts";
import { CoPortrait } from "#/components/CoPortrait.tsx";
import { loadCoPortraitCatalog } from "#/components/co_portraits.ts";
import { uiAtlasSpriteStyle } from "#/components/game_sprites.ts";

// The power meter's own star, so a cost reads the way the game draws it. It is
// drawn at its own size, so a ten-star super power still fits one line.
const STAR = uiAtlasSpriteStyle("TerrainStar.png", 1);

/** Two screen pixels to each pixel of the 32-pixel face. */
const BRIEFING_PORTRAIT_SIZE = 64;

type CoBriefingProps = {
  /** The CO to describe: the one being read, or else the seat's own. */
  coId: number | null;
  /** The CO the seat holds now, which may already be a pick still being saved. */
  chosenCoId: number | null;
  /** The COs this match has taken away. */
  bannedCoIds: ReadonlySet<number>;
  /** The seat cannot change its commander right now. */
  isLocked: boolean;
  /** A pick is on its way to the server. */
  isSaving: boolean;
  /** The tier list that fits this match. */
  tierList: CoTierList;
};

export function CoBriefing({
  bannedCoIds,
  chosenCoId,
  coId,
  isLocked,
  isSaving,
  tierList,
}: CoBriefingProps) {
  const catalog = useMemo(() => loadCoPortraitCatalog(), []);
  const co = getCoById(coId);
  const abilities = co === null ? null : getCoAbilities(co.key);

  if (co === null || abilities === null) {
    return (
      <VStack gap={4} xstyle={styles.card}>
        <HStack align="center" gap={4}>
          <Section padding={0} variant="muted" xstyle={styles.portraitWell}>
            <CoPortrait
              catalog={catalog}
              coKey={DEFAULT_CO_PORTRAIT_KEY}
              fallbackLabel="No commander"
              hasFrame={false}
              size={BRIEFING_PORTRAIT_SIZE}
            />
          </Section>
          <VStack gap={1}>
            <Text color="secondary" type="label">
              No commander yet
            </Text>
            <Heading level={3}>Pick a CO</Heading>
          </VStack>
        </HStack>
      </VStack>
    );
  }

  const tier = coTierOf(co.key, tierList);
  const isBanned = bannedCoIds.has(co.awbwId);
  const isChosen = co.awbwId === chosenCoId;
  const standing = briefingStanding({ isBanned, isChosen, isLocked, isSaving });

  return (
    <VStack gap={3} xstyle={styles.card}>
      <HStack align="center" gap={4}>
        <Section
          padding={0}
          variant="muted"
          xstyle={[styles.portraitWell, isChosen && styles.portraitWellChosen]}
        >
          <CoPortrait
            catalog={catalog}
            coKey={co.key}
            fallbackLabel={co.displayName}
            hasFrame={false}
            size={BRIEFING_PORTRAIT_SIZE}
          />
        </Section>
        <VStack gap={1} xstyle={styles.identity}>
          <Text color={standing.tone} type="label">
            {standing.label}
          </Text>
          <Heading level={3}>{co.displayName}</Heading>
          {tier === null ? null : (
            <Text aria-label={`Tier ${tier}, ${coTierNames[tier]}`} color="secondary" type="label">
              Tier {tier}
            </Text>
          )}
        </VStack>
      </HStack>

      {/* The two powers stand side by side, so their costs and effects read
          as one comparison. A CO with no day-to-day ability shows no row. */}
      <VStack as="dl" gap={3} xstyle={styles.abilities}>
        {abilities.dayToDay === null ? null : (
          <AbilityRow label="Day to day">
            <Text type="supporting">{abilities.dayToDay}</Text>
          </AbilityRow>
        )}
        <Grid columns={{ minWidth: 180, repeat: "fit" }} gap={3}>
          {abilities.power ? (
            <AbilityRow label="Power" stars={abilities.power.stars}>
              <PowerReadout power={abilities.power} />
            </AbilityRow>
          ) : null}
          <AbilityRow label="Super power" stars={abilities.superPower.stars}>
            <PowerReadout power={abilities.superPower} />
          </AbilityRow>
        </Grid>
      </VStack>
    </VStack>
  );
}

/** How the described CO stands with the seat, in one line above the name. */
function briefingStanding({
  isBanned,
  isChosen,
  isLocked,
  isSaving,
}: {
  isBanned: boolean;
  isChosen: boolean;
  isLocked: boolean;
  isSaving: boolean;
}): { label: string; tone: "accent" | "secondary" } {
  if (isChosen) {
    return { label: isSaving ? "Saving your commander" : "Your commander", tone: "accent" };
  }
  if (isBanned) return { label: "Banned in this match", tone: "secondary" };
  return { label: isLocked ? "Preview, locked" : "Preview", tone: "secondary" };
}

/** One layer of what a CO does: its name in the HUD face, the stars it costs. */
function AbilityRow({
  children,
  label,
  stars,
}: {
  children: ReactNode;
  label: string;
  stars?: number;
}) {
  return (
    <VStack gap={1}>
      <HStack align="center" as="dt" gap={2}>
        <Text color="secondary" type="label">
          {label}
        </Text>
        {stars === undefined ? null : <PowerStars count={stars} />}
      </HStack>
      <VStack as="dd" gap={0} xstyle={styles.value}>
        {children}
      </VStack>
    </VStack>
  );
}

function PowerReadout({ power }: { power: CoPowerText }) {
  return (
    <VStack gap={0.5}>
      <Text weight="bold">{power.name}</Text>
      <Text type="supporting">{power.effect}</Text>
    </VStack>
  );
}

function PowerStars({ count }: { count: number }) {
  return (
    <HStack align="center" gap={0}>
      <VisuallyHidden>{`${count} stars`}</VisuallyHidden>
      {Array.from({ length: count }, (_, index) => (
        <HStack key={index} style={STAR ?? undefined} />
      ))}
    </HStack>
  );
}

const styles = stylex.create({
  card: {
    inlineSize: "100%",
  },
  // The face sits in a recessed well with the one black outline, the frame the
  // game's own CO screen draws around a portrait.
  portraitWell: {
    flexShrink: 0,
    lineHeight: 0,
    overflow: "hidden",
    borderColor: colorVars["--color-border-emphasized"],
    borderStyle: "solid",
    borderWidth: borderVars["--border-width"],
    borderRadius: "var(--radius-element)",
  },
  portraitWellChosen: {
    boxShadow: `0 0 0 ${borderVars["--border-width"]} ${colorVars["--color-accent"]}`,
  },
  identity: {
    minInlineSize: 0,
  },
  abilities: {
    margin: 0,
    paddingBlockStart: spacingVars["--spacing-3"],
    borderBlockStartColor: colorVars["--color-border-emphasized"],
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: borderVars["--border-width"],
  },
  value: {
    margin: 0,
  },
});
