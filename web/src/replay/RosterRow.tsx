import { Badge } from "@astryxdesign/core/Badge";
import { Grid } from "@astryxdesign/core/Grid";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { StatusDot } from "@astryxdesign/core/StatusDot";
import { Text } from "@astryxdesign/core/Text";
import * as stylex from "@stylexjs/stylex";
import type { ReactNode } from "react";
import { CoPortrait } from "#/components/CoPortrait.tsx";
import type { CoPortraitCatalog } from "#/components/co_portraits.ts";
import { FactionCrest } from "#/components/FactionCrest.tsx";
import type { PlayerRosterEntry } from "#/wasm/awbrn_wasm.js";
import { infantrySpriteStyle, uiAtlasSpriteStyle } from "./roster_icons";

const CO_PORTRAIT_SIZE = 40;
const FACTION_LOGO_SIZE = 28;

const formatMoney = (value: number | null | undefined) =>
  value == null ? "--" : value.toLocaleString();
const formatCount = (value: number | null | undefined) =>
  value == null ? "--" : value.toLocaleString();

/**
 * One statistic drawn from the game's own sprite sheets. `coinOverlay` marks
 * the value readout, which the game shows as a unit with a coin on it.
 */
export function StatIcon({
  spriteName,
  factionCode,
  coinOverlay = false,
}: {
  spriteName?: string;
  factionCode?: string;
  coinOverlay?: boolean;
}) {
  const baseStyle = spriteName
    ? uiAtlasSpriteStyle(spriteName)
    : factionCode
      ? infantrySpriteStyle(factionCode)
      : null;
  const coinStyle = coinOverlay ? uiAtlasSpriteStyle("Coin.png") : null;

  return (
    <span aria-hidden="true" {...stylex.props(styles.statIconStack)}>
      <span style={baseStyle ?? undefined} {...stylex.props(styles.statIcon)} />
      {coinStyle ? (
        <span style={coinStyle} {...stylex.props(styles.statIcon, styles.statIconCoin)} />
      ) : null}
    </span>
  );
}

/** A number under its own sprite and name, on the strip's shared column grid. */
function Readout({ icon, label, value }: { icon: ReactNode; label: string; value: string }) {
  return (
    <VStack gap={0}>
      <HStack align="center" gap={1}>
        {icon}
        <Text maxLines={1} type="supporting">
          {label}
        </Text>
      </HStack>
      <Text color="primary" hasTabularNumbers maxLines={1} size="base" type="supporting">
        {value}
      </Text>
    </VStack>
  );
}

/**
 * One army, on one row.
 *
 * The crest identifies the army by shape as well as by color, so nothing here
 * depends on hue alone and the name does not have to be repeated in text. All
 * five public statistics stay on the row, because a reviewer compares armies
 * against each other and cannot do that while scrolling.
 */
export function RosterRow({
  isActive,
  isViewer = false,
  name,
  onFactionChange,
  player,
  portraitCatalog,
}: {
  isActive: boolean;
  /** Marks the army the viewer is playing. A reviewer plays none of them. */
  isViewer?: boolean;
  name: string;
  onFactionChange?: (factionId: number) => void | Promise<void>;
  player: PlayerRosterEntry;
  portraitCatalog: CoPortraitCatalog;
}) {
  // The army is not named here: the crest carries it, and the picker behind the
  // crest is where every army is spelled out.
  const meta = player.team ? `Team ${player.team}` : null;

  return (
    <VStack
      gap={2}
      paddingBlock={2}
      paddingInline={3}
      xstyle={[styles.row, styles.rowWash(player.displayFactionCode)]}
    >
      <HStack align="center" gap={2}>
        <FactionCrest
          factionCode={player.displayFactionCode}
          onChange={onFactionChange}
          size={FACTION_LOGO_SIZE}
        />
        <CoPortrait
          catalog={portraitCatalog}
          coKey={player.coKey}
          fallbackLabel={player.coName ?? "?"}
          size={CO_PORTRAIT_SIZE}
        />
        {player.tagCoKey ? (
          <CoPortrait
            catalog={portraitCatalog}
            coKey={player.tagCoKey}
            fallbackLabel={player.tagCoName ?? "?"}
            size={CO_PORTRAIT_SIZE}
          />
        ) : null}
        <VStack gap={0} xstyle={styles.clip}>
          <HStack align="center" gap={1}>
            {isActive ? <StatusDot label="Active turn" variant="accent" /> : null}
            <Text maxLines={1} weight="bold">
              {name}
            </Text>
            {/* Neutral, not the accent: command orange is reserved for the one
                action in the system, and this marks identity rather than a
                thing to press. */}
            {isViewer ? <Badge label="You" variant="neutral" /> : null}
            {player.eliminated ? <Badge label="Out" variant="error" /> : null}
          </HStack>
          {meta ? (
            <Text maxLines={1} type="supporting">
              {meta}
            </Text>
          ) : null}
        </VStack>
      </HStack>

      <Grid align="start" columns={{ minWidth: 86, max: 5, repeat: "fill" }} gap={2}>
        <Readout
          icon={<StatIcon spriteName="Coin.png" />}
          label="Funds"
          value={formatMoney(player.stats.funds)}
        />
        <Readout
          icon={<StatIcon spriteName="BuildingsCaptured.png" />}
          label="Income"
          value={formatMoney(player.stats.income)}
        />
        <Readout
          icon={<StatIcon factionCode={player.displayFactionCode} />}
          label="Units"
          value={formatCount(player.stats.unitCount)}
        />
        <Readout
          icon={<StatIcon coinOverlay factionCode={player.displayFactionCode} />}
          label="Value"
          value={formatMoney(player.stats.unitValue)}
        />
        <Readout
          icon={<StatIcon spriteName="TerrainStar.png" />}
          label="Power"
          value={formatCount(player.powerCharge)}
        />
      </Grid>
    </VStack>
  );
}

const styles = stylex.create({
  row: {
    borderTopWidth: "var(--border-width)",
    borderTopStyle: "solid",
    borderTopColor: "var(--color-border-soft)",
    ":first-child": {
      borderTopWidth: 0,
    },
  },
  // The army is only known at runtime, so its color arrives as a token name.
  rowWash: (factionCode: string) => ({
    backgroundColor: `var(--color-faction-${factionCode}-wash, var(--color-background-muted))`,
  }),
  clip: {
    minWidth: 0,
  },
  // The sprites differ in natural size, so each one is centered in a fixed box
  // and the numbers beside them stay on one line.
  statIconStack: {
    position: "relative",
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    width: 18,
    height: 18,
    overflow: "hidden",
    flex: "0 0 auto",
  },
  statIcon: {
    display: "block",
    imageRendering: "pixelated",
    backgroundRepeat: "no-repeat",
  },
  statIconCoin: {
    position: "absolute",
    right: 0,
    bottom: 0,
    width: 10,
    height: 10,
  },
});
