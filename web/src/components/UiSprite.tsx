import * as stylex from "@stylexjs/stylex";
import { uiAtlasSpriteStyle } from "#/components/game_sprites.ts";

/**
 * One sprite from the game's interface atlas (`Clock.png`, `Coin.png`,
 * `CO/Power.png`, `HP.png`, `Ammo.png`, `Fuel.png`, ...), drawn at a whole
 * scale: 1x, 2x or 3x, pixelated. Never smoothed, tinted or recolored.
 *
 * Decorative by default (the figure beside it carries the meaning); pass
 * `label` when the sprite stands alone.
 *
 *   <UiSprite name="Coin.png" />
 */
export function UiSprite({
  label,
  name,
  scale = 1,
  xstyle,
}: {
  label?: string;
  name: string;
  scale?: 1 | 2 | 3;
  xstyle?: stylex.StyleXStyles;
}) {
  const style = uiAtlasSpriteStyle(name, scale);
  if (!style) return null;
  return (
    // A pixel-sprite crop of the interface atlas: a bare box no component draws.
    <span
      aria-hidden={label ? undefined : true}
      aria-label={label}
      role={label ? "img" : undefined}
      style={style}
      {...stylex.props(styles.sprite, xstyle)}
    />
  );
}

const styles = stylex.create({
  sprite: {
    display: "inline-block",
    flexShrink: 0,
  },
});
