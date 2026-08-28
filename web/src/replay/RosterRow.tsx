import { Badge } from "@astryxdesign/core/Badge";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { StatusDot } from "@astryxdesign/core/StatusDot";
import { Text } from "@astryxdesign/core/Text";
import { VisuallyHidden } from "@astryxdesign/core/VisuallyHidden";
import { borderVars, colorVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import type { ReactNode } from "react";
import { CoPortrait } from "#/components/CoPortrait.tsx";
import type { CoPortraitCatalog } from "#/components/co_portraits.ts";
import { FactionCrest } from "#/components/FactionCrest.tsx";
import { ROSTER_MEDIA_SIZE } from "#/ui/layout.ts";
import { rosterLayout } from "#/ui/rosterLayout.stylex.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import type { PlayerRosterEntry } from "#/wasm/awbrn_wasm.js";
import { PowerMeter } from "./PowerMeter.tsx";
import type { ActivatablePowerLevel } from "./power_meter.ts";
import { infantrySpriteStyle, uiAtlasSpriteStyle } from "#/components/game_sprites.ts";

const formatMoney = (value: number | null | undefined) =>
  value == null ? "--" : value.toLocaleString();
const formatCount = (value: number | null | undefined) =>
  value == null ? "--" : value.toLocaleString();

/**
 * The armies, edge to edge inside their panel.
 *
 * Beside the board the roster is one narrow column. Under the board it has the
 * page's whole width, so the rows pair up rather than running down a long
 * single file the board has already pushed off screen. Every row carries a rule
 * on its top and leading edges and the list is shifted one rule up and back, so
 * the outermost rules fall outside the panel's clip and the same row works at
 * either count.
 */
export function RosterList({ children }: { children: ReactNode }) {
  return (
    <VStack gap={0} xstyle={styles.list}>
      {children}
    </VStack>
  );
}

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

/**
 * A number and the sprite it is counted in, the way the game's own HUD reports
 * it. The sprite trails the number, so the digits close on their unit the way
 * an amount closes on its currency, and the room a short number leaves opens
 * ahead of it rather than between the two.
 *
 * The word is carried in the accessible name rather than set beside the value:
 * these four sprites are the icons the audience already reads funds and unit
 * strength from, and spelling each one out costs the row a line it does not
 * have to spare.
 */
function Readout({ icon, label, value }: { icon: ReactNode; label: string; value: string }) {
  return (
    <HStack align="center" gap={1} xstyle={styles.readout}>
      <Text hasTabularNumbers maxLines={1} type="label" xstyle={styles.readoutValue}>
        <VisuallyHidden>{label}: </VisuallyHidden>
        {value}
      </Text>
      {icon}
    </HStack>
  );
}

/**
 * One army, on one row.
 *
 * The crest identifies the army by shape as well as by color, so nothing here
 * depends on hue alone and the name does not have to be repeated in text. The
 * row is built to be read against the board rather than instead of it: three
 * bands, each one line tall, in the order a player checks them — who is up,
 * what they can do next, and what they hold.
 */
export function RosterRow({
  activatingPower,
  clock,
  isActive,
  isViewer = false,
  name,
  onActivatePower,
  onFactionChange,
  player,
  portraitCatalog,
}: {
  activatingPower?: ActivatablePowerLevel;
  /**
   * What the match clock leaves this army, when the match runs on one. A
   * replay has no clock and passes nothing, and the row closes over the slot.
   */
  clock?: ReactNode;
  isActive: boolean;
  /** Marks the army the viewer is playing. A reviewer plays none of them. */
  isViewer?: boolean;
  name: string;
  onActivatePower?: (level: ActivatablePowerLevel) => void;
  onFactionChange?: (factionId: number) => void | Promise<void>;
  player: PlayerRosterEntry;
  portraitCatalog: CoPortraitCatalog;
}) {
  // The army is not named here: the crest carries it, and the picker behind the
  // crest is where every army is spelled out.
  const team = player.team ? `Team ${player.team}` : null;

  return (
    <VStack
      gap={2}
      paddingBlock={2}
      paddingInline={3}
      xstyle={[
        styles.row,
        styles.rowWash(player.displayFactionCode, isActive),
        player.eliminated && styles.rowEliminated,
      ]}
    >
      <HStack align="center" gap={2} justify="between">
        <HStack align="center" gap={2} xstyle={styles.clip}>
          <FactionCrest
            factionCode={player.displayFactionCode}
            onChange={onFactionChange}
            size={ROSTER_MEDIA_SIZE.crest}
          />
          <CoPortrait
            catalog={portraitCatalog}
            coKey={player.coKey}
            fallbackLabel={player.coName ?? "?"}
            size={ROSTER_MEDIA_SIZE.portrait}
          />
          {player.tagCoKey ? (
            <CoPortrait
              catalog={portraitCatalog}
              coKey={player.tagCoKey}
              fallbackLabel={player.tagCoName ?? "?"}
              size={ROSTER_MEDIA_SIZE.portrait}
            />
          ) : null}
          <HStack align="center" gap={1} xstyle={styles.clip}>
            {isActive ? <StatusDot label="Active turn" variant="accent" /> : null}
            <Text maxLines={1} weight="bold" xstyle={styles.clip}>
              {name}
            </Text>
            {/* Neutral, not the accent: command orange is reserved for the one
                action in the system, and this marks identity rather than a
                thing to press. */}
            {isViewer ? <Badge label="You" variant="neutral" /> : null}
            {player.eliminated ? <Badge label="Out" variant="error" /> : null}
          </HStack>
        </HStack>

        {/* The team and the clock run down the trailing edge rather than
            trailing the name. Both are read as a column — who is with whom,
            and who is about to run out — and a name of any length now shortens
            without ever pushing those columns around. Time sits beside the
            army whose turn it measures, because a bank is a fact about a seat
            and not about the match. */}
        <HStack align="center" gap={2} xstyle={styles.keep}>
          {team ? (
            <Text maxLines={1} type="label">
              {team}
            </Text>
          ) : null}
          {clock}
        </HStack>
      </HStack>

      <PowerMeter
        activatingPower={activatingPower}
        onActivatePower={onActivatePower}
        player={player}
      />

      <div {...stylex.props(styles.statGrid)}>
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
          label="Unit value"
          value={formatMoney(player.stats.unitValue)}
        />
      </div>
    </VStack>
  );
}

const styles = stylex.create({
  list: {
    display: "grid",
    gridTemplateColumns: {
      default: "minmax(0, 1fr)",
      // Only while the roster sits under the board. Above this the rail is one
      // column wide by design.
      [rosterLayout.pairedRowsMedia]: "repeat(2, minmax(0, 1fr))",
    },
    marginBlockStart: `calc(-1 * ${borderVars["--border-width"]})`,
    marginInlineStart: `calc(-1 * ${borderVars["--border-width"]})`,
  },
  row: {
    borderBlockStartWidth: borderVars["--border-width"],
    borderInlineStartWidth: borderVars["--border-width"],
    borderStyle: "solid",
    borderColor: awbrnVars.colorBorderSoft,
    // The same row sits in the rail, in a pair under the board, and alone on a
    // phone, so how much width it has is its own fact rather than the window's.
    containerType: "inline-size",
    containerName: "roster-row",
  },
  // The army is only known at runtime, so its color arrives as a token name.
  // The army whose turn it is wears its own color at full strength rather than
  // as a wash; the dot beside its name says the same thing in a second way.
  rowWash: (factionCode: string, isActive: boolean) => ({
    backgroundColor: isActive
      ? `var(--color-faction-${factionCode}-soft, ${colorVars["--color-background-muted"]})`
      : `var(--color-faction-${factionCode}-wash, ${colorVars["--color-background-muted"]})`,
  }),
  // A defeated army stays on the roster, because who is left is the fact a
  // player is checking, but it stops competing with the armies still playing.
  rowEliminated: {
    opacity: 0.55,
  },
  // Only the name gives up width when the line runs out; the dot, the team,
  // and the badges are the parts a player is scanning for.
  clip: {
    minWidth: 0,
  },
  keep: {
    flex: "0 0 auto",
  },
  // The four numbers hold their block tightly, one step inside the spacing
  // that separates the block from the name and the power meter above it.
  statGrid: {
    display: "grid",
    // The three widths are written so that no two of them can apply at once,
    // because which of two matching at-rules wins is not worth depending on.
    gridTemplateColumns: {
      default: "repeat(2, minmax(0, 1fr))",
      // Too narrow to hold two of the longest readouts, which is a squeezed
      // rail or a deep zoom. One number to a line is the last arrangement that
      // still reads; below it the numbers shorten rather than push each other.
      "@container roster-row (max-width: 215px)": "minmax(0, 1fr)",
      // Under the board a row can be wide enough to give four readouts their
      // full width, and on that line the four are read left to right.
      "@container roster-row (min-width: 440px)": "repeat(4, minmax(0, 1fr))",
    },
    columnGap: "var(--spacing-2)",
    rowGap: "var(--spacing-1)",
  },
  readout: {
    minWidth: 0,
  },
  // The number is set in a box seven glyphs wide, which is what the largest
  // figure the roster reports needs: `999,999`, grouped. The box holds that
  // width for a short value too, so the digits of two armies land on the same
  // places, and a longer figure widens the box rather than being cut off.
  // `ch` measures the glyph alone, so the HUD face's tracking is added back.
  //
  // The seven glyphs are what the number asks for, not what it takes: the box
  // never claims more than the line leaves beside the sprite. A row too narrow
  // to grant the request shortens the number, which is the same failure the
  // roster has always had at that width, rather than running it into the
  // reading beside it.
  readoutValue: {
    minWidth:
      "min(calc(7ch + 0.42em), calc(100% - var(--size-roster-stat-icon) - var(--spacing-1)))",
    textAlign: "right",
  },
  // The sprites differ in natural size, so each one is centered in a fixed box
  // and the numbers beside them stay on one line.
  statIconStack: {
    position: "relative",
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    width: rosterLayout.rosterStatIconSize,
    height: rosterLayout.rosterStatIconSize,
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
    width: rosterLayout.rosterStatIconOverlaySize,
    height: rosterLayout.rosterStatIconOverlaySize,
  },
});
