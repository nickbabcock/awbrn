import { Icon } from "@astryxdesign/core/Icon";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { Tooltip } from "@astryxdesign/core/Tooltip";
import { VisuallyHidden } from "@astryxdesign/core/VisuallyHidden";
import * as stylex from "@stylexjs/stylex";
import type { SVGProps } from "react";
import type { PlayerRosterEntry } from "#/wasm/awbrn_wasm.js";
import { readPowerMeter, type PowerMeterReading } from "./power_meter.ts";
import { uiAtlasSpriteStyle } from "./roster_icons";

const STAR = uiAtlasSpriteStyle("TerrainStar.png");

/**
 * A CO power meter drawn in stars, at the length that CO's powers actually run.
 *
 * Nothing here is normalized to a shared width. A meter is exactly as long as
 * the CO's stars make it, so Sami's five-star bar is visibly a different
 * commitment from Von Bolt's ten, and the two zones use different star sizes:
 * the normal power is a run of small stars and the super power a run of large
 * ones, parted by an ink rule. The step in height is the boundary — a player
 * sees where the normal power ends without reading anything.
 */
export function PowerMeter({ player }: { player: PlayerRosterEntry }) {
  const meter = readPowerMeter(player);
  // A seat taken from the match record knows its CO but not yet its charge. The
  // row keeps the meter's height so it does not jump when the engine reports.
  if (!meter) return <PendingMeter />;

  const { charged, cop, level, scop, totalStars } = meter;
  const superStars = scop === null ? 0 : scop.stars - (cop?.stars ?? 0);

  return (
    <Tooltip content={<PowerBreakdown meter={meter} />} focusTrigger="always" placement="above">
      <HStack align="end" gap={2} tabIndex={0} xstyle={styles.meter}>
        <HStack
          align="end"
          aria-label="CO power"
          aria-valuemax={totalStars}
          aria-valuemin={0}
          aria-valuenow={charged}
          aria-valuetext={describe(meter)}
          gap={0}
          role="meter"
        >
          {cop === null ? null : (
            <Zone
              charged={charged}
              isJoined={superStars > 0}
              isReady={level !== "charging"}
              offset={0}
              size="cop"
              stars={cop.stars}
            />
          )}
          {superStars === 0 ? null : (
            <Zone
              charged={charged}
              isJoined={cop !== null}
              isReady={level === "scop"}
              offset={cop?.stars ?? 0}
              size="scop"
              stars={superStars}
            />
          )}
        </HStack>

        <Text
          color={level === "charging" ? "secondary" : "accent"}
          hasTabularNumbers
          type="label"
          xstyle={styles.readout}
        >
          <Icon
            aria-hidden="true"
            icon={SpriteImage}
            style={STAR ?? undefined}
            xstyle={styles.sprite}
          />
          {formatStars(charged)}/{totalStars}
        </Text>
      </HStack>
    </Tooltip>
  );
}

/** The meter before the engine has said what this CO's powers cost. */
function PendingMeter() {
  return (
    <HStack align="end" gap={2} xstyle={styles.meter}>
      <VisuallyHidden>CO power data pending</VisuallyHidden>
      <HStack gap={0} xstyle={[styles.zone, styles.zoneScop, styles.zonePending]} />
      <Text aria-hidden="true" hasTabularNumbers type="label" xstyle={styles.readout}>
        <Icon
          aria-hidden="true"
          icon={SpriteImage}
          style={STAR ?? undefined}
          xstyle={styles.sprite}
        />
        --
      </Text>
    </HStack>
  );
}

/**
 * One side of the meter. `offset` is how many stars come before this zone, so
 * each cell knows which star it draws and how much of that star is charged.
 */
function Zone({
  charged,
  isJoined,
  isReady,
  offset,
  size,
  stars,
}: {
  charged: number;
  /** Whether the other power's zone runs alongside this one. */
  isJoined: boolean;
  isReady: boolean;
  offset: number;
  size: "cop" | "scop";
  stars: number;
}) {
  return (
    <HStack
      align="end"
      gap={0}
      xstyle={[
        styles.zone,
        size === "cop" ? styles.zoneCop : styles.zoneScop,
        isJoined && (size === "cop" ? styles.zoneJoinedStart : styles.zoneJoinedEnd),
      ]}
    >
      {Array.from({ length: stars }, (_, index) => (
        <HStack align="end" gap={0} key={index} xstyle={styles.cell}>
          <HStack
            gap={0}
            style={{ inlineSize: `${clamp(charged - offset - index) * 100}%` }}
            xstyle={[styles.fill, isReady && styles.fillReady]}
          />
        </HStack>
      ))}
    </HStack>
  );
}

/** The charge, and the two numbers it is running toward. Nothing else. */
function PowerBreakdown({ meter }: { meter: PowerMeterReading }) {
  return (
    <VStack gap={0} xstyle={styles.breakdown}>
      <BreakdownRow label="Charge" value={meter.charge} />
      {meter.cop === null ? null : <BreakdownRow label="Power" value={meter.cop.charge} />}
      {meter.scop === null ? null : <BreakdownRow label="Super" value={meter.scop.charge} />}
    </VStack>
  );
}

function BreakdownRow({ label, value }: { label: string; value: number }) {
  return (
    <HStack gap={3} justify="between">
      <Text type="label">{label}</Text>
      <Text color="primary" hasTabularNumbers type="label">
        {value.toLocaleString()}
      </Text>
    </HStack>
  );
}

/** Lets Astryx Icon host a CSS-atlas sprite while retaining its image semantics. */
function SpriteImage(props: SVGProps<SVGSVGElement>) {
  return <svg {...props} />;
}

/** A partial star is worth reporting; a whole one should not read as "3.0". */
function formatStars(charged: number): string {
  const truncated = Math.trunc(charged * 10) / 10;
  return truncated % 1 === 0 ? String(truncated) : truncated.toFixed(1);
}

function describe({ charged, level, totalStars }: PowerMeterReading): string {
  const stars = `${formatStars(charged)} of ${totalStars} stars`;
  switch (level) {
    case "scop":
      return `${stars} — super CO power ready`;
    case "cop":
      return `${stars} — CO power ready`;
    case "charging":
      return `${stars} — charging`;
  }
}

function clamp(value: number): number {
  return Math.min(Math.max(value, 0), 1);
}

const styles = stylex.create({
  meter: {
    // The bar is as wide as the CO's stars make it and no wider.
    alignSelf: "start",
    maxInlineSize: "100%",
    outlineOffset: 2,
  },
  readout: {
    display: "inline-flex",
    alignItems: "center",
    flex: "0 0 auto",
    gap: "var(--spacing-1)",
  },
  sprite: {
    display: "block",
    flex: "0 0 auto",
    imageRendering: "pixelated",
    backgroundRepeat: "no-repeat",
  },
  zone: {
    backgroundColor: "var(--color-track)",
    borderColor: "var(--color-border-emphasized)",
    borderStyle: "solid",
    borderWidth: "var(--border-width)",
    borderRadius: "var(--radius-inner)",
    overflow: "hidden",
  },
  zoneCop: {
    blockSize: "var(--size-power-star-cop)",
    "--star-size": "var(--size-power-star-cop)",
  },
  zoneScop: {
    blockSize: "var(--size-power-star-scop)",
    "--star-size": "var(--size-power-star-scop)",
  },
  // A dashed outline is how the system says a value is not there yet, rather
  // than drawing a full bar the engine has not confirmed.
  zonePending: {
    inlineSize: "var(--size-power-meter-pending)",
    backgroundColor: "transparent",
    borderStyle: "dashed",
    borderColor: "var(--color-border-disabled)",
  },
  // The two zones meet on one rule instead of two touching outlines. The super
  // zone owns it, so the line runs the full height of the taller bar: it parts
  // two different commands, not two units of the same one.
  zoneJoinedStart: {
    borderInlineEndWidth: 0,
  },
  zoneJoinedEnd: {
    borderInlineStartWidth: "var(--border-width-power-zone-joined)",
  },
  cell: {
    position: "relative",
    inlineSize: "var(--star-size)",
    flex: "0 0 auto",
    blockSize: "100%",
    borderInlineEndWidth: "var(--border-width)",
    borderInlineEndStyle: "solid",
    borderInlineEndColor: "var(--color-border-soft)",
    ":last-child": {
      borderInlineEndWidth: 0,
    },
  },
  fill: {
    blockSize: "100%",
    backgroundColor: "var(--color-border)",
  },
  // A charged power is the one thing on this row worth looking up for, so its
  // stars take the accent the system reserves for what matters.
  fillReady: {
    backgroundColor: "var(--color-text-accent)",
  },
  breakdown: {
    minInlineSize: "var(--size-power-breakdown)",
  },
});
