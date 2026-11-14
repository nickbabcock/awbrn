import { VStack } from "@astryxdesign/core/Stack";
import { useQuery } from "@tanstack/react-query";
import * as stylex from "@stylexjs/stylex";
import { Fragment, useMemo, type CSSProperties } from "react";
import { uiAtlasSpriteStyle, unitSpriteStyle } from "#/components/game_sprites.ts";
import { defaultFactionIdForSlot, getFactionById } from "#/factions.ts";
import { mapScreenshotSize } from "#/maps/map_screenshot.ts";
import { mapRevisionQueryOptions } from "#/maps/maps.queries.ts";
import type { MapCatalogEntry } from "#/maps/schemas.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { placeSkirmish, type DepictedSkirmish } from "./depicted.ts";

/** A map tile in the full picture, in its own pixels. */
const TILE = 16;
/** The board's opening scale. A 2dppx screen raises it to the touch floor (2.5x). */
const SCALE = 2;

/** The depicted board's armies and skirmish, once the map's terrain arrives. */
export function useDepiction(map: MapCatalogEntry) {
  const { data: document } = useQuery(mapRevisionQueryOptions(map.mapId, map.revision));
  return useMemo(() => {
    const code = (slot: number) =>
      getFactionById(document?.slotFactionIds[slot] ?? defaultFactionIdForSlot(slot))?.code ??
      (slot === 0 ? "os" : "bm");
    return {
      factionCodes: [code(0), code(1)] as const,
      skirmish: document ? placeSkirmish(document) : null,
    };
  }, [document]);
}

/**
 * A depicted board: the map's own render centered on the depicted cursor,
 * with the skirmish's units on it and a fade on every cropped edge.
 */
export function DepictedBoard({
  factionCodes,
  map,
  skirmish,
}: {
  factionCodes: readonly [string, string];
  map: MapCatalogEntry;
  skirmish: DepictedSkirmish | null;
}) {
  const picture = mapScreenshotSize("full", map.width, map.height);
  const focus = skirmish?.focus ?? {
    x: Math.floor(map.width / 2),
    y: Math.floor(map.height / 2),
  };
  // The full picture carries one row of overhang above the first tile row.
  const focusX = (focus.x * TILE + TILE / 2) * SCALE;
  const focusY = ((focus.y + 1) * TILE + TILE / 2) * SCALE;

  return (
    <VStack as="span" xstyle={styles.box}>
      <VStack as="span" xstyle={styles.anchor}>
        <VStack
          as="span"
          xstyle={[
            styles.board,
            styles.boardAt(-focusX, -focusY, picture.width * SCALE, picture.height * SCALE),
          ]}
        >
          <img
            alt=""
            decoding="async"
            height={picture.height}
            src={map.screenshot.full}
            width={picture.width}
            {...stylex.props(styles.picture)}
          />
          {skirmish?.units.map((unit) => {
            const sprite = unitSpriteStyle(unit.unit, factionCodes[unit.side], SCALE);
            if (!sprite) return null;
            const left = unit.x * TILE * SCALE;
            const tileTop = (unit.y + 1) * TILE * SCALE;
            return (
              <Fragment key={`${unit.x},${unit.y}`}>
                <Sprite
                  left={left}
                  sprite={sprite}
                  // The unit's 19px window stands on the tile's bottom edge.
                  top={tileTop + (TILE - 19) * SCALE}
                />
                {unit.hp < 10 ? (
                  <Sprite
                    left={left + (TILE / 2) * SCALE}
                    sprite={uiAtlasSpriteStyle(`Healthv2/${unit.hp}.png`, SCALE)}
                    top={tileTop + 9 * SCALE}
                  />
                ) : null}
              </Fragment>
            );
          })}
          {skirmish ? (
            <Sprite
              left={(focus.x * TILE - 5) * SCALE}
              sprite={uiAtlasSpriteStyle("Effects/TileCursor.png", SCALE)}
              top={((focus.y + 1) * TILE - 5) * SCALE}
            />
          ) : null}
        </VStack>
      </VStack>
      <VStack as="span" xstyle={[styles.fade, styles.fadeLeft]} />
      <VStack as="span" xstyle={[styles.fade, styles.fadeRight]} />
      <VStack as="span" xstyle={[styles.fade, styles.fadeTop]} />
      <VStack as="span" xstyle={[styles.fade, styles.fadeBottom]} />
    </VStack>
  );
}

function Sprite({
  left,
  sprite,
  top,
}: {
  left: number;
  sprite: CSSProperties | null;
  top: number;
}) {
  if (!sprite) return null;
  const placed = stylex.props(styles.sprite, styles.spriteAt(left, top));
  return (
    // A crop of the sprite atlas: its background position and size come from
    // the atlas as an inline style object, which no Astryx component takes.
    <span aria-hidden="true" className={placed.className} style={{ ...sprite, ...placed.style }} />
  );
}

/** The table's fade: 16px at 55% ink, toward the edge where more map lies. */
const FADE = `color-mix(in srgb, ${awbrnVars.ink950} 55%, transparent)`;

const styles = stylex.create({
  box: {
    position: "relative",
    display: "block",
    inlineSize: "100%",
    blockSize: "100%",
    overflow: "hidden",
    backgroundColor: awbrnVars.ink950,
  },
  // The box's middle. Scaling about it keeps the cursor's tile centered at
  // 2.5x as at 2x; at 2dppx 2.5x is a whole five device pixels a map pixel.
  anchor: {
    position: "absolute",
    insetInlineStart: "50%",
    insetBlockStart: "50%",
    transformOrigin: "0 0",
    transform: {
      default: null,
      "@media (min-resolution: 2dppx)": "scale(1.25)",
    },
  },
  board: {
    position: "absolute",
  },
  boardAt: (left: number, top: number, width: number, height: number) => ({
    insetInlineStart: `${left}px`,
    insetBlockStart: `${top}px`,
    inlineSize: `${width}px`,
    blockSize: `${height}px`,
  }),
  picture: {
    display: "block",
    inlineSize: "100%",
    blockSize: "100%",
    maxInlineSize: "none",
    imageRendering: "pixelated",
  },
  sprite: {
    position: "absolute",
    display: "block",
  },
  spriteAt: (left: number, top: number) => ({
    insetInlineStart: `${left}px`,
    insetBlockStart: `${top}px`,
  }),
  fade: {
    position: "absolute",
    pointerEvents: "none",
  },
  fadeLeft: {
    insetBlock: 0,
    insetInlineStart: 0,
    inlineSize: "16px",
    backgroundImage: `linear-gradient(90deg, ${FADE}, transparent)`,
  },
  fadeRight: {
    insetBlock: 0,
    insetInlineEnd: 0,
    inlineSize: "16px",
    backgroundImage: `linear-gradient(-90deg, ${FADE}, transparent)`,
  },
  fadeTop: {
    insetInline: 0,
    insetBlockStart: 0,
    blockSize: "16px",
    backgroundImage: `linear-gradient(180deg, ${FADE}, transparent)`,
  },
  fadeBottom: {
    insetInline: 0,
    insetBlockEnd: 0,
    blockSize: "16px",
    backgroundImage: `linear-gradient(0deg, ${FADE}, transparent)`,
  },
});
