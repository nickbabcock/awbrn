import { Card } from "@astryxdesign/core/Card";
import { HStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import type { TokenProps } from "@astryxdesign/core/Token";
import * as stylex from "@stylexjs/stylex";
import type { StyleXStyles } from "@stylexjs/stylex";
import type { ReactNode } from "react";
import { FactionCrest } from "#/components/FactionCrest.tsx";
import { getFactionByCode } from "#/factions.ts";

/**
 * The name of a seat, under the colors of the army holding it.
 *
 * The crest leads the row and is also how the army is chosen when the seat is
 * the viewer's to change. The army is not spelled out beside it: the picker the
 * crest opens names every army, which is where the mark is learned.
 */
export function PlayerHeader({
  factionCode,
  isFactionLocked = false,
  name,
  onFactionChange,
  trailing,
  xstyle,
}: {
  factionCode: string;
  isFactionLocked?: boolean;
  name: string;
  onFactionChange?: (factionId: number) => void | Promise<void>;
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
        <HStack align="center" gap={2} xstyle={styles.identity}>
          <FactionCrest
            factionCode={factionCode}
            isDisabled={isFactionLocked}
            onChange={onFactionChange}
          />
          <Text maxLines={1} weight="bold">
            {name}
          </Text>
        </HStack>
        {trailing ? (
          <HStack align="center" gap={2}>
            {trailing}
          </HStack>
        ) : null}
      </HStack>
    </Card>
  );
}

function factionVariant(factionCode: string): TokenProps["color"] {
  const faction = getFactionByCode(factionCode);
  return faction ? (`faction-${faction.code}` as TokenProps["color"]) : "default";
}

const styles = stylex.create({
  identity: {
    minWidth: 0,
  },
});
