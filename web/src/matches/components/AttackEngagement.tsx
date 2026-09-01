import { HStack } from "@astryxdesign/core/Stack";
import { spacingVars, textSizeVars, typographyVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { uiAtlasSpriteStyle, unitSpriteSize, unitSpriteStyle } from "#/components/game_sprites.ts";
import { formatBracket } from "#/matches/components/attack_forecast.ts";
import { boardMenuLayout } from "#/matches/components/boardMenuLayout.stylex.ts";
import type { AttackForecast, UnitBadge } from "#/wasm/awbrn_wasm.js";

/**
 * The engagement one attack would open: what this order deals, and what comes
 * back.
 *
 * The two figures are named. Two passes tried to avoid naming them — first with
 * arrows, then with the sprite of whoever took the damage — and both failed the
 * same way, which is worth recording so it is not tried a third time. An arrow
 * needs two things to point between, and this row has one. A sprite beside a
 * number reads as *that unit's* number, and a unit's number means what it deals
 * long before it means what it suffers; putting the target's art next to the
 * damage done to it had readers concluding the target was the attacker.
 *
 * Direction is not a thing a picture can say here. `DEAL` and `TAKE` say it in
 * eight characters, from the seat the player is sitting in, and nothing about
 * them has to be learned. The width they cost was paid for by dropping the
 * army, the unit name and the standalone health column.
 *
 * The two figures are read together, not separately. A counter is scored from
 * whatever the strike left standing, so the good outcome is the top of `DEAL`
 * with the bottom of `TAKE`, and the bad one is the other pair.
 *
 * Nothing is written where nothing comes back: a row with no reply is one line
 * high. An indirect gets no answer from anything it can reach, so a menu that
 * spelled that out would say it on every row and mean it once.
 *
 * Damage is in percentage points, uncapped, the way AWBW's own calculator
 * reports it: 160 against a whole unit is an overkill worth sending something
 * cheaper at, and clamping it to 100 would hide that.
 *
 * The same block serves the aim and the order: the numbers a player reads
 * while aiming must be the numbers they read while committing, in the same
 * shape, or the two are two forecasts rather than one.
 */
export function AttackEngagement({
  forecast,
  spriteScale,
}: {
  forecast: AttackForecast;
  spriteScale: 1 | 2;
}) {
  const target = forecast.target;

  return (
    <span {...stylex.props(styles.engagement)}>
      <span {...stylex.props(styles.exchangeLine)}>
        {target.type === "unit" ? (
          <UnitSprite badge={target} scale={spriteScale} />
        ) : (
          // A destructible tile has no sprite to stand for it, so it says its
          // own name. It is the one target on the board that is not a unit.
          <span {...stylex.props(styles.tileName)}>{target.name}</span>
        )}
        <span {...stylex.props(styles.exchangeLabel)}>Deal</span>
        <span {...stylex.props(styles.exchangeValue)}>{formatBracket(forecast.damage)}</span>
      </span>

      {forecast.counter ? (
        <span {...stylex.props(styles.exchangeLine)}>
          <span
            {...stylex.props(styles.exchangeIndent)}
            style={{ inlineSize: `${unitSpriteSize(spriteScale).width}px` }}
          />
          {/* A commander who answers first inverts the exchange: this unit
              takes the hit before it fires, and what it deals then depends on
              surviving. The label is the only place that can be said, because
              the order of two lines is not something a reader is owed. */}
          <span {...stylex.props(styles.exchangeLabel)}>
            {forecast.counterFirst ? "Take 1st" : "Take"}
          </span>
          <span {...stylex.props(styles.exchangeValue)}>{formatBracket(forecast.counter)}</span>
        </span>
      ) : null}
    </span>
  );
}

/**
 * The unit whose orders these are.
 *
 * It stands where the word "Orders" stands, because on a menu of engagements
 * the useful thing to say at the head is not that these are orders — the panel
 * is plainly a menu — but which unit is about to spend itself, and with how
 * much health left to spend.
 */
export function Attacker({ badge, spriteScale }: { badge: UnitBadge; spriteScale: 1 | 2 }) {
  return (
    <HStack align="center" gap={2} xstyle={styles.attacker}>
      <UnitSprite badge={badge} scale={spriteScale} />
      <span {...stylex.props(styles.attackerName)}>{badge.name}</span>
    </HStack>
  );
}

/**
 * A unit as the board draws it: its sprite, wearing its health.
 *
 * The digit is the game's own `Healthv2` art sitting in the sprite's corner,
 * exactly where the board puts it, and it follows the board's rule of appearing
 * only when the unit is not at full strength. That rule is what makes it worth
 * having here: a menu where most units are whole shows almost no digits, and
 * the ones it does show are the units whose health is about to matter.
 */
export function UnitSprite({ badge, scale }: { badge: UnitBadge; scale: 1 | 2 }) {
  const sprite = unitSpriteStyle(badge.unit, badge.factionCode, scale);
  const digit =
    badge.health === undefined
      ? uiAtlasSpriteStyle("Healthv2/Question.png", scale)
      : badge.health < FULL_HEALTH
        ? uiAtlasSpriteStyle(`Healthv2/${badge.health}.png`, scale)
        : null;

  if (!sprite) return null;

  return (
    <span {...stylex.props(styles.unitSprite)} style={unitSpriteSize(scale)}>
      <span aria-hidden="true" style={sprite} {...stylex.props(styles.sprite)} />
      {digit ? (
        <span aria-hidden="true" style={digit} {...stylex.props(styles.healthDigit)} />
      ) : null}
    </span>
  );
}

/** The health at which the board stops drawing a number on a unit. */
const FULL_HEALTH = 10;

const styles = stylex.create({
  engagement: {
    display: "flex",
    flexDirection: "column",
    gap: spacingVars["--spacing-1"],
    inlineSize: "100%",
    minInlineSize: 0,
  },
  // The target, then what happens, then how much. The label and the figure sit
  // in fixed columns so the numbers stack exactly down the whole menu, which is
  // what makes two engagements comparable without reading either of them.
  exchangeLine: {
    display: "flex",
    alignItems: "center",
    gap: spacingVars["--spacing-2"],
    minInlineSize: 0,
  },
  // The reply line has no art of its own: the target was named once, above it.
  // This holds the art's place so the two labels line up, and takes its width
  // from the sprite rather than from a constant, because a thumb reads the
  // menu at twice the scale a cursor does.
  exchangeIndent: {
    flex: "0 0 auto",
  },
  // What happens, in the game's own voice and from the player's own seat. It
  // recedes by opacity rather than by a second colour, which is this system's
  // rule for receding and is also what lets the key invert under the orange
  // cursor without a value going illegible. The step stops at 0.8, where it
  // still clears 4.5:1 on that orange, the harder of its two grounds.
  exchangeLabel: {
    flex: "0 0 auto",
    inlineSize: boardMenuLayout.forecastLabelInlineSize,
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    textTransform: "uppercase",
    opacity: 0.8,
  },
  // The figures are the only thing on the key at full strength.
  exchangeValue: {
    flex: "0 0 auto",
    marginInlineStart: "auto",
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    fontVariantNumeric: "tabular-nums",
  },
  // The one target with no sprite to stand for it.
  tileName: {
    flex: "0 1 auto",
    minInlineSize: 0,
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    textTransform: "uppercase",
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  sprite: {
    display: "block",
    flex: "0 0 auto",
  },
  // The unit whose orders these are, standing at the head of the menu.
  attacker: {
    minInlineSize: 0,
  },
  attackerName: {
    flex: "0 1 auto",
    minInlineSize: 0,
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    textTransform: "uppercase",
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  // The sprite and the number it wears. Sized by the caller to the art itself,
  // so the digit has a corner to sit in.
  unitSprite: {
    position: "relative",
    display: "block",
    flex: "0 0 auto",
  },
  // The board hangs the health off the unit's lower right, half over the art
  // and half over the tile. Doing anything else here would make the same unit
  // read as two different objects in the same turn.
  healthDigit: {
    position: "absolute",
    insetInlineEnd: 0,
    insetBlockEnd: 0,
    display: "block",
  },
});
