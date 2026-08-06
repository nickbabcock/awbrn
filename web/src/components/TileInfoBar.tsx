import { useMediaQuery } from "@astryxdesign/core/hooks";
import { Icon } from "@astryxdesign/core/Icon";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { VisuallyHidden } from "@astryxdesign/core/VisuallyHidden";
import {
  borderVars,
  colorVars,
  radiusVars,
  shadowVars,
  spacingVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useRef, useState, type RefObject, type SVGProps } from "react";
import type { HoveredCargoUnit, HoveredTile, HoveredUnit } from "#/wasm/awbrn_wasm.js";
import {
  terrainSpriteStyle,
  uiAtlasSpriteStyle,
  unitSpriteStyle,
} from "#/components/game_sprites.ts";
import { useGameStore } from "#/engine/store.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";

const TERRAIN_STAR = uiAtlasSpriteStyle("TerrainStar.png");
const HP = uiAtlasSpriteStyle("HP.png");
const AMMO = uiAtlasSpriteStyle("Ammo.png");
const FUEL = uiAtlasSpriteStyle("Fuel.png");
// The income icon: a building earning. A capture is a building changing hands,
// which is the same event one turn earlier.
const CAPTURE = uiAtlasSpriteStyle("BuildingsCaptured.png");

/** What a property costs to take, per `spec/`. */
const CAPTURE_POINTS = 20;

/** Whether the primary pointer is a finger, which cannot hover. */
const COARSE_POINTER_MEDIA = "(pointer: coarse)";

/**
 * The share of the board the readout is treated as covering. A pointer inside
 * this corner sends the window to the opposite one, so the tile being read is
 * never the tile the readout is standing on.
 */
const HOME_CORNER_INLINE = 0.45;
const HOME_CORNER_BLOCK = 0.6;

/** Cargo shown as sprites before the rest is counted rather than drawn. */
const CARGO_SPRITE_LIMIT = 4;

/**
 * The terrain and unit details for one tile, as a window on the board itself.
 *
 * This is the game's own terrain window: a small readout docked in a corner of
 * the battlefield, which hops to the other corner when the player reads a tile
 * behind it. It costs the page no height, and it never intercepts a pointer, so
 * the board underneath it stays fully playable.
 *
 * Which tile it reports is the engine's answer, not this component's: a mouse
 * reports what it hovers, and a finger reports the tile it last tapped or is
 * dragging across. The bar is deliberately not a live region: it changes with
 * every tile a pointer crosses, which a screen reader would read as an unbroken
 * stream.
 *
 * With no tile to report there is nothing to draw. A mouse discovers the
 * readout by moving, so a panel telling it to do that is a panel standing on
 * the board for nothing. A finger has no such accident to have, so the one
 * pointer that needs telling is the one that is told.
 */
export function TileInfoBar() {
  const tile = useGameStore((state) => state.hoveredTile);
  const isCoarsePointer = useMediaQuery(COARSE_POINTER_MEDIA);
  const windowRef = useRef<HTMLDivElement>(null);
  const dock = useReadoutDock(windowRef);

  // The window stays in the board's markup even with nothing to say, because
  // it is what the docking watch is measured against.
  return (
    <VStack
      gap={0}
      ref={windowRef}
      xstyle={[
        styles.window,
        dock === "end" && styles.windowEnd,
        tile === null && styles.hint,
        tile === null && !isCoarsePointer && styles.absent,
      ]}
    >
      {tile === null ? (
        <Text type="label" xstyle={styles.hintText}>
          Tap a tile
        </Text>
      ) : (
        <>
          <TerrainLines tile={tile} />
          {tile.unit === undefined ? null : <UnitLines unit={tile.unit} />}
        </>
      )}
    </VStack>
  );
}

/**
 * What the tile is, how well it shelters, where it is, and what it still owes
 * before it changes hands.
 *
 * The block is built the same way as the unit block below it — art in a column
 * on the left, readings stacked beside it — because the two rows are the same
 * kind of object and the window is narrower for holding one shape rather than
 * two. The name says the terrain alone; the army holding a property is read
 * from the colours the tile is drawn in, so the owner is named in the
 * description of the sprite rather than spent on a line.
 */
function TerrainLines({ tile }: { tile: HoveredTile }) {
  const terrainLabel =
    tile.terrainOwner === undefined ? tile.terrainName : `${tile.terrainOwner} ${tile.terrainName}`;

  return (
    <HStack align="center" gap={2} xstyle={styles.line}>
      <Icon
        aria-label={terrainLabel}
        icon={SpriteImage}
        role="img"
        style={terrainSpriteStyle(tile.terrainSpriteIndex)}
        xstyle={styles.sprite}
      />
      <VStack gap={0} xstyle={styles.details}>
        <Text type="label" xstyle={[styles.readout, styles.name]}>
          {tile.terrainName}
        </Text>
        {/* Both of these are facts about the property, and a capture in
            progress is the one with a deadline, so it appears only while it is
            live. It counts down, because the number a player is waiting for is
            the one that reaches zero. */}
        <HStack align="center" gap={2} xstyle={styles.statLine}>
          <DefenseStars count={tile.defenseStars} />
          {tile.captureRemaining === undefined ? null : (
            <Resource
              description={`Capture: ${tile.captureRemaining} of 20 points left`}
              icon={CAPTURE}
              label="Capture"
              maximum={CAPTURE_POINTS}
              showMaximum={false}
              value={tile.captureRemaining}
            />
          )}
        </HStack>
        {/* The coordinates take a line of their own rather than sitting beside
            the stars. They are the widest reading in the block and the least
            urgent, and paired with anything they set the width of the whole
            window — which is board the player cannot see. */}
        <Text hasTabularNumbers type="label" xstyle={[styles.readout, styles.coordinates]}>
          {tile.x + 1},{tile.y + 1}
        </Text>
      </VStack>
    </HStack>
  );
}

/**
 * The unit standing there: the sprite, and what it has left.
 *
 * The sprite names the unit — it is the same art the player is already reading
 * on the board, in the army's own colours — so the readout spends its width on
 * the three numbers instead. Those numbers stack in one column with their icons
 * aligned, which is the game's own status window: three short lines a player
 * can read the shape of without reading the digits.
 */
function UnitLines({ unit }: { unit: HoveredUnit }) {
  return (
    <HStack align="center" gap={2} xstyle={[styles.line, styles.unitLine]}>
      <UnitSprite unit={unit} />
      <VStack gap={0} xstyle={styles.details}>
        <Resource
          critical={unit.health <= 3}
          icon={HP}
          label="Health"
          maximum={10}
          showMaximum={false}
          value={unit.health}
        />
        {unit.ammoDisplay === "none" ? null : (
          <Resource
            icon={AMMO}
            label="Ammo"
            maximum={unit.maxAmmo}
            unlimited={unit.ammoDisplay === "unlimited"}
            value={unit.ammo}
          />
        )}
        <Resource
          icon={FUEL}
          label="Fuel"
          maximum={unit.maxFuel}
          showMaximum={false}
          value={unit.fuel}
        />
      </VStack>
      <Cargo units={unit.loadedUnits} />
    </HStack>
  );
}

/** The terrain's shelter, counted in the game's own stars. */
function DefenseStars({ count }: { count: number }) {
  return (
    <HStack
      align="center"
      aria-label={`${count} defense ${count === 1 ? "star" : "stars"}`}
      gap={0}
      role="img"
      xstyle={styles.stars}
    >
      {Array.from({ length: count }, (_, index) => (
        <Icon
          aria-hidden="true"
          icon={SpriteImage}
          key={index}
          style={TERRAIN_STAR ?? undefined}
          xstyle={styles.sprite}
        />
      ))}
    </HStack>
  );
}

/**
 * What a transport is carrying, drawn rather than listed.
 *
 * Cargo is at most eight units and usually one or two, so the sprites say it
 * faster than names would and in a fraction of the width. Past what fits, the
 * remainder is counted.
 */
function Cargo({ units }: { units: HoveredCargoUnit[] }) {
  if (units.length === 0) {
    return null;
  }

  const shown = units.slice(0, CARGO_SPRITE_LIMIT);
  const remainder = units.length - shown.length;

  return (
    <HStack align="center" gap={0}>
      <VisuallyHidden>Carrying {units.map((unit) => unit.name).join(", ")}</VisuallyHidden>
      {shown.map((cargo, index) => (
        <UnitSprite key={`${cargo.unit}-${index}`} unit={cargo} />
      ))}
      {remainder === 0 ? null : (
        <Text aria-hidden="true" hasTabularNumbers type="label" xstyle={styles.readout}>
          +{remainder}
        </Text>
      )}
    </HStack>
  );
}

function UnitSprite({ unit }: { unit: HoveredUnit | HoveredCargoUnit }) {
  return (
    <Icon
      aria-label={`${unit.name} (${unit.factionCode.toUpperCase()})`}
      icon={SpriteImage}
      role="img"
      style={unitSpriteStyle(unit.unit, unit.factionCode) ?? undefined}
      xstyle={styles.sprite}
    />
  );
}

/**
 * One line of the unit's stock: the game's own icon, then the number.
 *
 * A weapon that never runs out reads as an infinity sign, and a resource the
 * viewer cannot know reads as a dash.
 *
 * Only ammunition prints its ceiling. Health has one ceiling in the whole game,
 * and a unit's fuel tank is a property of the kind of unit it is rather than
 * something the player weighs a move against; what a move turns on is how much
 * is left. The denominator stays in the description either way, so the reading
 * is complete for anyone who cannot see the column.
 */
function Resource({
  critical = false,
  description,
  icon,
  label,
  maximum,
  showMaximum = true,
  unlimited = false,
  value,
}: {
  critical?: boolean;
  /** Replaces the spoken reading where "x of y" is not what the number means. */
  description?: string;
  icon: ReturnType<typeof uiAtlasSpriteStyle>;
  label: string;
  maximum: number;
  showMaximum?: boolean;
  unlimited?: boolean;
  value: number | undefined;
}) {
  const amount = unlimited
    ? "∞"
    : value === undefined
      ? "—"
      : showMaximum
        ? `${value}/${maximum}`
        : `${value}`;
  const reading =
    description ??
    (unlimited
      ? `${label}: unlimited`
      : value === undefined
        ? `${label}: unknown`
        : `${label}: ${value} of ${maximum}`);

  return (
    <HStack align="center" aria-label={reading} gap={1} role="img" xstyle={styles.statLine}>
      <Icon
        aria-hidden="true"
        icon={SpriteImage}
        style={icon ?? undefined}
        xstyle={styles.statIcon}
      />
      <Text
        aria-hidden="true"
        hasTabularNumbers
        type="label"
        xstyle={[styles.readout, critical && styles.critical]}
      >
        {amount}
      </Text>
    </HStack>
  );
}

/**
 * Which corner the readout stands in.
 *
 * The window keeps the leading corner and gives it up only while the pointer is
 * working in it, which is what the source game does with the same readout. The
 * decision is made from the pointer rather than from the readout's own bounds,
 * because a window that moves out of its own way would move straight back and
 * flicker between the two corners.
 */
function useReadoutDock(windowRef: RefObject<HTMLElement | null>): "start" | "end" {
  const [dock, setDock] = useState<"start" | "end">("start");

  useEffect(() => {
    // The readout is positioned inside the board frame it was rendered into.
    // Reading that frame from the DOM keeps this correct on the first commit,
    // with no ref to thread through two pages.
    const surface = windowRef.current?.parentElement;
    if (!surface) return;

    const follow = (event: PointerEvent) => {
      const bounds = surface.getBoundingClientRect();
      if (bounds.width === 0 || bounds.height === 0) return;

      const inHomeCorner =
        (event.clientX - bounds.left) / bounds.width < HOME_CORNER_INLINE &&
        (event.clientY - bounds.top) / bounds.height > HOME_CORNER_BLOCK;
      setDock(inHomeCorner ? "end" : "start");
    };
    const goHome = () => setDock("start");

    surface.addEventListener("pointermove", follow);
    surface.addEventListener("pointerdown", follow);
    surface.addEventListener("pointerleave", goHome);

    return () => {
      surface.removeEventListener("pointermove", follow);
      surface.removeEventListener("pointerdown", follow);
      surface.removeEventListener("pointerleave", goHome);
    };
  }, [windowRef]);

  return dock;
}

function SpriteImage(props: SVGProps<SVGSVGElement>) {
  return <svg {...props} />;
}

const styles = stylex.create({
  // A window the game opened on the battlefield: the standard outlined panel,
  // docked to a bottom corner of the board and never in the way of a pointer.
  window: {
    position: "absolute",
    insetBlockEnd: spacingVars["--spacing-2"],
    insetInlineStart: spacingVars["--spacing-2"],
    minInlineSize: "96px",
    // A long terrain name shortens rather than widening the window. The board
    // underneath is what the player is reading; a readout that grows to hold
    // "Missile Silo" would take three quarters of a phone's battlefield to do
    // it.
    maxInlineSize: `min(15rem, calc(100% - ${spacingVars["--spacing-4"]}))`,
    borderWidth: borderVars["--border-width"],
    borderStyle: "solid",
    borderColor: colorVars["--color-border-emphasized"],
    borderRadius: radiusVars["--radius-container"],
    backgroundColor: colorVars["--color-background-surface"],
    boxShadow: shadowVars["--shadow-med"],
    color: colorVars["--color-text-primary"],
    pointerEvents: "none",
    userSelect: "none",
    overflow: "hidden",
    zIndex: 1,
  },
  // The hop is instant. The readout is a window being redrawn where it is now
  // needed, not an object sliding across the board.
  windowEnd: {
    insetInlineStart: "auto",
    insetInlineEnd: spacingVars["--spacing-2"],
  },
  // With nothing to report the window shrinks to the one instruction a finger
  // is owed, and a mouse is shown nothing at all.
  hint: {
    minInlineSize: 0,
    paddingBlock: spacingVars["--spacing-1"],
    paddingInline: spacingVars["--spacing-2"],
  },
  absent: {
    display: "none",
  },
  hintText: {
    color: colorVars["--color-text-secondary"],
  },
  line: {
    paddingBlock: spacingVars["--spacing-1"],
    paddingInline: spacingVars["--spacing-2"],
  },
  // The second line of the same readout, so it divides with the soft rule
  // rather than with the panel outline.
  unitLine: {
    borderBlockStartWidth: borderVars["--border-width"],
    borderBlockStartStyle: "solid",
    borderBlockStartColor: awbrnVars.colorBorderSoft,
  },
  // The readings beside the art, in both blocks.
  details: {
    flex: "0 1 auto",
    minInlineSize: 0,
  },
  // Three readings in a column, set tight enough to read as one block of
  // stock rather than three separate facts.
  statLine: {
    minBlockSize: 0,
  },
  // The three icons are drawn the same width in the game's own atlas, so the
  // numbers beside them line up down the column with nothing to correct. The
  // box is never widened: each icon is a window onto the atlas, and a wider
  // window would show the sprite next to it.
  statIcon: {
    display: "block",
    flex: "0 0 auto",
  },
  readout: {
    lineHeight: 1.2,
    whiteSpace: "nowrap",
  },
  name: {
    flex: "0 1 auto",
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  // Health that a single shot can end is the one number in the readout that
  // changes a decision, so it is the one number that changes colour.
  critical: {
    color: colorVars["--color-text-red"],
  },
  stars: {
    flex: "0 0 auto",
  },
  coordinates: {
    flex: "0 0 auto",
    color: colorVars["--color-text-secondary"],
  },
  sprite: {
    display: "block",
    flex: "0 0 auto",
  },
});
