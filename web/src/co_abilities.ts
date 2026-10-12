/**
 * What each commanding officer does, in the words a player reads on the board.
 *
 * Three layers per CO. The day-to-day ability is always on. The power is
 * charged with stars and fired on the owner's turn. The super power is the
 * larger charge, with a larger effect. Star counts are the base cost from
 * `spec/rulesets/awbw/2026-07-10/commander-powers.json`; each use of a power
 * raises the charge needed for the next one, so these are the first-use costs.
 *
 * Effect text is plain English written against that ruleset. The spec remains
 * the source of truth for the numbers; `co_abilities.test.ts` checks the star
 * counts and which layers exist against the same data.
 */

export interface CoPowerText {
  /** The power's name, as the CO's own power is called. */
  name: string;
  /** Base star cost. */
  stars: number;
  /** One or two sentences a player can read at a glance. */
  effect: string;
}

export interface CoAbilities {
  /** Always-on ability, or null when the CO has none. */
  dayToDay: string | null;
  /** The first power, or null when the CO has only a super power. */
  power: CoPowerText | null;
  superPower: CoPowerText;
}

export const coAbilities: Readonly<Record<string, CoAbilities>> = {
  adder: {
    dayToDay: null,
    power: { name: "Sideslip", stars: 2, effect: "All your units gain +1 movement." },
    superPower: { name: "Sidewinder", stars: 5, effect: "All your units gain +2 movement." },
  },
  andy: {
    dayToDay: null,
    power: { name: "Hyper Repair", stars: 3, effect: "Heal all your units 2 HP." },
    superPower: {
      name: "Hyper Upgrade",
      stars: 6,
      effect: "Heal all your units 5 HP. Units gain +10% attack and +1 movement.",
    },
  },
  colin: {
    dayToDay: "Units cost 20% less, but have -10% attack.",
    power: { name: "Gold Rush", stars: 2, effect: "Your funds are multiplied by 1.5." },
    superPower: {
      name: "Power of Money",
      stars: 6,
      effect: "Units gain attack as your funds grow.",
    },
  },
  drake: {
    dayToDay:
      "Naval units gain +1 movement and +10% defense. Air units lose 30% attack. Rain does not slow your units.",
    power: {
      name: "Tsunami",
      stars: 4,
      effect: "Enemy units lose 1 HP and half their fuel.",
    },
    superPower: {
      name: "Typhoon",
      stars: 7,
      effect: "Enemy units lose 2 HP and half their fuel. Rain for a turn.",
    },
  },
  eagle: {
    dayToDay:
      "Air units gain +15% attack and +10% defense, and burn 2 less fuel per day. Naval units lose 30% attack.",
    power: {
      name: "Lightning Drive",
      stars: 3,
      effect: "Air units gain +5% attack and +10% defense.",
    },
    superPower: {
      name: "Lightning Strike",
      stars: 9,
      effect:
        "Air units gain +5% attack and +10% defense. Units other than infantry and mechs can act again.",
    },
  },
  flak: {
    dayToDay: "Luck on attacks ranges from -9% to +24%.",
    power: { name: "Brute Force", stars: 3, effect: "Luck ranges from -19% to +49%." },
    superPower: { name: "Barbaric Blow", stars: 6, effect: "Luck ranges from -39% to +89%." },
  },
  grimm: {
    dayToDay: "Units gain +30% attack, but lose 20% defense.",
    power: { name: "Knuckleduster", stars: 3, effect: "Units gain +20% attack." },
    superPower: { name: "Haymaker", stars: 6, effect: "Units gain +50% attack." },
  },
  grit: {
    dayToDay:
      "Indirect units gain +1 range and +20% attack. Direct units other than infantry lose 20% attack.",
    power: {
      name: "Snipe Attack",
      stars: 3,
      effect: "Indirect units gain +1 range and +20% attack.",
    },
    superPower: {
      name: "Super Snipe",
      stars: 6,
      effect: "Indirect units gain +2 range and +20% attack.",
    },
  },
  hachi: {
    dayToDay: "Units cost 10% less.",
    power: { name: "Barter", stars: 3, effect: "Units cost half price." },
    superPower: {
      name: "Merchant Union",
      stars: 5,
      effect: "Units cost half price. Cities can deploy infantry and mechs.",
    },
  },
  hawke: {
    dayToDay: "Units gain +10% attack.",
    power: {
      name: "Black Wave",
      stars: 5,
      effect: "Your units heal 1 HP. Enemy units lose 1 HP but cannot be killed.",
    },
    superPower: {
      name: "Black Storm",
      stars: 9,
      effect: "Your units heal 2 HP. Enemy units lose 2 HP but cannot be killed.",
    },
  },
  jake: {
    dayToDay: "All units gain +10% attack on plains.",
    power: {
      name: "Beat Down",
      stars: 3,
      effect: "Ground indirect units gain +1 range. Plains bonus rises to +20%.",
    },
    superPower: {
      name: "Block Rock",
      stars: 6,
      effect:
        "Ground indirect units gain +1 range. Plains bonus rises to +40%. Vehicles gain +2 movement.",
    },
  },
  jess: {
    dayToDay: "Vehicles gain +10% attack. Other units lose 10% attack.",
    power: {
      name: "Turbo Charge",
      stars: 3,
      effect: "Vehicles gain +1 movement and +10% attack. All units resupply.",
    },
    superPower: {
      name: "Overdrive",
      stars: 6,
      effect: "Vehicles gain +2 movement and +30% attack. All units resupply.",
    },
  },
  jugger: {
    dayToDay: "Luck on attacks ranges from -14% to +29%.",
    power: { name: "Overclock", stars: 3, effect: "Luck ranges from -24% to +54%." },
    superPower: { name: "System Crash", stars: 7, effect: "Luck ranges from -44% to +94%." },
  },
  javier: {
    dayToDay:
      "Units gain +20% defense against indirect attacks. Each comm tower adds +10% defense to all units.",
    power: {
      name: "Tower Shield",
      stars: 3,
      effect: "Indirect defense rises to +40%. Comm tower bonuses are doubled.",
    },
    superPower: {
      name: "Tower of Power",
      stars: 6,
      effect: "Indirect defense rises to +80%. Comm tower bonuses are tripled.",
    },
  },
  kanbei: {
    dayToDay: "Units cost 20% more, but gain +30% attack and defense.",
    power: { name: "Morale Boost", stars: 4, effect: "Units gain +10% attack." },
    superPower: {
      name: "Samurai Spirit",
      stars: 7,
      effect: "Units gain +10% attack and +20% defense. Counterattacks deal 50% more damage.",
    },
  },
  kindle: {
    dayToDay: "Units gain +40% attack while on urban terrain.",
    power: {
      name: "Urban Blight",
      stars: 3,
      effect: "Enemy units on urban terrain lose 3 HP. Urban bonus rises to +80%.",
    },
    superPower: {
      name: "High Society",
      stars: 6,
      effect: "Urban bonus rises to +130%. Units gain +3% attack for each urban property you own.",
    },
  },
  koal: {
    dayToDay: "Units gain +10% attack on roads.",
    power: {
      name: "Forced March",
      stars: 3,
      effect: "All units gain +1 movement. Road bonus rises to +20%.",
    },
    superPower: {
      name: "Trail of Woe",
      stars: 5,
      effect: "All units gain +2 movement. Road bonus rises to +30%.",
    },
  },
  lash: {
    dayToDay: "Units gain +10% attack for every terrain star they stand on.",
    power: {
      name: "Terrain Tactics",
      stars: 4,
      effect: "Movement costs 1 on all terrain, except snow.",
    },
    superPower: {
      name: "Prime Tactics",
      stars: 7,
      effect: "Terrain stars count double. Movement costs 1 on all terrain, except snow.",
    },
  },
  max: {
    dayToDay: "Direct units gain +20% attack. Indirect units lose 10% attack and 1 range.",
    power: {
      name: "Max Force",
      stars: 3,
      effect: "Direct units gain +10% attack. Tanks, air and naval units gain +1 movement.",
    },
    superPower: {
      name: "Max Blast",
      stars: 6,
      effect: "Direct units gain +30% attack. Tanks, air and naval units gain +2 movement.",
    },
  },
  nell: {
    dayToDay: "Luck on attacks ranges from 0% to +19%.",
    power: { name: "Lucky Star", stars: 3, effect: "Luck ranges from 0% to +59%." },
    superPower: { name: "Lady Luck", stars: 6, effect: "Luck ranges from 0% to +99%." },
  },
  olaf: {
    dayToDay: "Unaffected by snow. Rain slows his units the way snow does.",
    power: { name: "Blizzard", stars: 3, effect: "The weather becomes snow for a turn." },
    superPower: {
      name: "Winter Fury",
      stars: 7,
      effect: "Enemy units lose 2 HP. The weather becomes snow for a turn.",
    },
  },
  rachel: {
    dayToDay: "Units repair 1 extra HP. Repairs still cost funds.",
    power: { name: "Lucky Lass", stars: 3, effect: "Luck ranges from 0% to +39%." },
    superPower: {
      name: "Covering Fire",
      stars: 6,
      effect: "Missiles strike enemy units across the board for 3 HP each.",
    },
  },
  sami: {
    dayToDay:
      "Infantry and mechs gain +30% attack and capture faster. Other direct units lose 10% attack. Transports gain +1 movement.",
    power: {
      name: "Double Time",
      stars: 3,
      effect: "Infantry and mechs gain +1 movement and +20% attack.",
    },
    superPower: {
      name: "Victory March",
      stars: 8,
      effect: "Infantry and mechs gain +2 movement and +40% attack.",
    },
  },
  sasha: {
    dayToDay: "Earn +100 funds from each income property. Labs and comm towers do not count.",
    power: {
      name: "Market Crash",
      stars: 2,
      effect: "Enemy power bars shrink by 10% for every 5,000 of your funds.",
    },
    superPower: {
      name: "War Bonds",
      stars: 6,
      effect: "Earn funds equal to half the value of the damage you deal to enemy units.",
    },
  },
  sensei: {
    dayToDay:
      "B-copters gain +50% attack and infantry and mechs gain +40%. Other vehicles and sea units lose 10% attack. Transports gain +1 movement.",
    power: {
      name: "Copter Command",
      stars: 2,
      effect: "B-copters gain +15% attack. Place an infantry on each empty city you own.",
    },
    superPower: {
      name: "Airborne Assault",
      stars: 6,
      effect: "B-copters gain +15% attack. Place a mech on each empty city you own.",
    },
  },
  sonja: {
    dayToDay:
      "Units gain +1 vision in fog. Enemy HP is hidden. Counterattacks deal 50% more damage. Bad luck is capped at 9%.",
    power: {
      name: "Enhanced Vision",
      stars: 3,
      effect: "Units gain +1 vision and see into forests and reefs.",
    },
    superPower: {
      name: "Counter Break",
      stars: 5,
      effect:
        "Units gain +1 vision and see into forests and reefs. An attacked unit strikes first.",
    },
  },
  sturm: {
    dayToDay:
      "Movement costs 1 on all terrain, except snow. Units lose 20% attack and gain 20% defense.",
    power: {
      name: "Meteor Strike",
      stars: 6,
      effect: "A missile deals 4 HP to the enemy unit in the greatest concentration of unit value.",
    },
    superPower: {
      name: "Meteor Strike II",
      stars: 10,
      effect: "A missile deals 8 HP to the enemy unit in the greatest concentration of unit value.",
    },
  },
  "von-bolt": {
    dayToDay: "Units gain +10% attack and +10% defense.",
    power: null,
    superPower: {
      name: "Ex Machina",
      stars: 10,
      effect: "A missile deals 3 HP and keeps affected enemy units from acting next turn.",
    },
  },
};

/** The abilities a CO has, or null when the roster has no entry for `key`. */
export function getCoAbilities(key: string): CoAbilities | null {
  return coAbilities[key] ?? null;
}
