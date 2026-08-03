import { Card } from "@astryxdesign/core/Card";
import { HStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { Token, type TokenProps } from "@astryxdesign/core/Token";
import type { StyleXStyles } from "@stylexjs/stylex";
import type { ReactNode } from "react";
import { getFactionByCode } from "#/factions.ts";

export function PlayerHeader({
  factionCode,
  name,
  trailing,
  xstyle,
}: {
  factionCode: string;
  name: string;
  trailing?: ReactNode;
  xstyle?: StyleXStyles;
}) {
  return (
    <Card
      padding={2}
      className={factionVariant(factionCode)}
      variant="default"
      width="100%"
      xstyle={xstyle}
    >
      <HStack align="center" gap={2} justify="between">
        <Text maxLines={1} weight="bold">
          {name}
        </Text>
        {trailing ? (
          <HStack align="center" gap={2}>
            {trailing}
          </HStack>
        ) : null}
      </HStack>
    </Card>
  );
}

export function FactionBadge({
  factionCode,
  isLabelHidden = false,
  title,
}: {
  factionCode: string;
  isLabelHidden?: boolean;
  title?: string;
}) {
  const faction = getFactionByCode(factionCode);
  return (
    <Token
      color={factionVariant(factionCode)}
      description={title ? `Faction: ${title}` : undefined}
      isLabelHidden={isLabelHidden}
      label={title ?? faction?.displayName ?? factionCode.toUpperCase()}
      size="sm"
    />
  );
}

function factionVariant(factionCode: string): TokenProps["color"] {
  const faction = getFactionByCode(factionCode);
  return faction ? (`faction-${faction.code}` as TokenProps["color"]) : "default";
}
