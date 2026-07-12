#!/usr/bin/env node
import { readFileSync, readdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
const root = resolve(dirname(fileURLToPath(import.meta.url)), "../rulesets/awbw/2026-07-10");
const load = (n) => JSON.parse(readFileSync(resolve(root, n), "utf8"));
const weapons = load("weapons.json"),
  damage = load("damage.json").damage;
const units = new Set(Object.keys(load("units.json").units)),
  failures = [];
let effective = 0;
for (const [a, slots] of Object.entries(weapons.units)) {
  if (!units.has(a)) failures.push(`unknown attacker: ${a}`);
  for (const slot of Object.keys(slots))
    if (!["ammo", "unlimited"].includes(slot)) failures.push(`${a}: unknown slot ${slot}`);
  if (slots.ammo && slots.ammo.ammo_cost < 1) failures.push(`${a}.ammo must consume positive ammo`);
  if (slots.unlimited && slots.unlimited.ammo_cost !== 0)
    failures.push(`${a}.unlimited must consume zero ammo`);
  const targets = new Set([
    ...Object.keys(slots.ammo?.damage ?? {}),
    ...Object.keys(slots.unlimited?.damage ?? {}),
  ]);
  for (const d of targets) {
    if (!units.has(d)) failures.push(`${a}: unknown defender ${d}`);
    const selected = slots.ammo?.damage?.[d] ?? slots.unlimited?.damage?.[d],
      expected = damage[a]?.[d];
    if (expected === undefined) failures.push(`${a}->${d}: absent from effective table`);
    else {
      effective++;
      if (selected !== expected)
        failures.push(`${a}->${d}: selected ${selected}, effective ${expected}`);
    }
  }
}
for (const [a, targets] of Object.entries(damage))
  for (const d of Object.keys(targets)) {
    const slots = weapons.units[a] ?? {};
    if (slots.ammo?.damage?.[d] === undefined && slots.unlimited?.damage?.[d] === undefined)
      failures.push(`${a}->${d}: no weapon entry`);
  }
if (effective !== 328) failures.push(`expected 328 effective matchups, found ${effective}`);

// Fog data: the vision fields and their traits must agree, and every
// elevated-vision-eligible kind must exist. See spec/semantics/fog.md.
const terrains = load("terrain.json").terrains,
  caps = load("unit-capabilities.json");
let visionBonuses = 0,
  visionLimits = 0;
for (const [name, t] of Object.entries(terrains)) {
  const elevated = t.traits.includes("elevated-vision"),
    conceals = t.traits.includes("conceals-in-fog");
  const destructible = t.traits.includes("destructible");
  if (elevated !== "vision_bonus" in t)
    failures.push(`${name}: elevated-vision trait and vision_bonus must agree`);
  if (conceals !== "vision_limit" in t)
    failures.push(`${name}: conceals-in-fog trait and vision_limit must agree`);
  if (destructible !== "destructible" in t)
    failures.push(`${name}: destructible trait and profile must agree`);
  if (t.destructible) {
    if (!units.has(t.destructible.target_kind))
      failures.push(`${name}: unknown destructible target kind ${t.destructible.target_kind}`);
    if (!(t.destructible.destruction_replacement in terrains))
      failures.push(
        `${name}: unknown destruction replacement ${t.destructible.destruction_replacement}`,
      );
  }
  if (elevated) visionBonuses++;
  if (conceals) visionLimits++;
}
if (visionBonuses < 1) failures.push("no terrain supplies a vision_bonus");
if (visionLimits < 1) failures.push("no terrain supplies a vision_limit");
for (const name of ["pipe", "pipe-seam"]) {
  if (!terrains[name]?.traits.includes("always-visible")) {
    failures.push(`${name}: AWBW pipes must carry the always-visible trait`);
  }
}
for (const kind of caps.elevated_vision)
  if (!units.has(kind)) failures.push(`elevated_vision: unknown kind ${kind}`);
if (caps.elevated_vision.length < 1) failures.push("elevated_vision must name at least one kind");

// Commander tables: referenced unit kinds must exist and luck replacement
// domains must be ordered. These are relational constraints outside JSON Schema.
const commanderCombat = load("commander-combat.json");
for (const [commander, profile] of Object.entries(commanderCombat.commanders)) {
  for (const state of ["day_to_day", "cop", "scop"]) {
    for (const rule of profile[state].rules) {
      for (const kind of rule.when.unit_kinds ?? []) {
        if (!units.has(kind)) failures.push(`${commander}.${state}: unknown unit kind ${kind}`);
      }
      const domain = rule.effect.domain;
      if (domain && domain.minimum > domain.maximum) {
        failures.push(`${commander}.${state}: reversed ${rule.effect.operator} domain`);
      }
    }
  }
}
const commanderProfiles = load("commander-profiles.json");
for (const [commander, profile] of Object.entries(commanderProfiles.commanders)) {
  for (const dimension of ["movement", "vision", "attack_range"]) {
    for (const state of ["day_to_day", "cop", "scop"]) {
      for (const rule of profile[dimension]?.[state] ?? []) {
        for (const kind of rule.unit_kinds ?? []) {
          if (!units.has(kind))
            failures.push(`${commander}.${dimension}.${state}: unknown unit kind ${kind}`);
        }
      }
    }
  }
}

const commanderPowers = load("commander-powers.json");
if (commanderPowers.base_star_charge <= 0)
  failures.push("commander powers: base star charge must be positive");
if (commanderPowers.use_cost_scaling.denominator <= 0)
  failures.push("commander powers: scaling denominator must be positive");
for (const [commander, profile] of Object.entries(commanderPowers.commanders)) {
  for (const level of ["cop", "scop"]) {
    const power = profile[level];
    if (!power) continue;
    if (power.stars <= 0) failures.push(`${commander}.${level}: stars must be positive`);
    for (const effect of power.instant_effects) {
      if (
        ![
          "heal-visual-hp",
          "heal-exact-hp",
          "damage-exact-hp",
          "set-weather",
          "drain-current-fuel-ratio",
          "fire-area-strikes",
          "reduce-power-charge-by-funds-ratio",
          "refresh-unit-action",
          "resupply-units",
          "spawn-units-on-owned-properties",
          "fire-targeted-area-strike",
          "fire-immobilizing-area-strike",
          "multiply-funds-ratio",
        ].includes(effect.operator)
      )
        failures.push(`${commander}.${level}: unsupported instant effect ${effect.operator}`);
      for (const kind of effect.exclude_unit_kinds ?? []) {
        if (!units.has(kind))
          failures.push(`${commander}.${level}: unknown excluded unit kind ${kind}`);
      }
      if (effect.unit_kind && !units.has(effect.unit_kind))
        failures.push(`${commander}.${level}: unknown spawned unit kind ${effect.unit_kind}`);
      for (const kind of effect.property_kinds ?? []) {
        if (!Object.values(terrains).some((terrain) => terrain.property_kind === kind))
          failures.push(`${commander}.${level}: unknown spawn property kind ${kind}`);
      }
    }
    for (const effect of power.strike_effects ?? []) {
      if (effect.operator !== "gain-funds-from-visual-hp-damage")
        failures.push(`${commander}.${level}: unsupported strike effect ${effect.operator}`);
    }
  }
}

// Commander capability completeness: the roster, revisioned tables, manifest,
// and executable fixtures must agree. Von Bolt is naturally handled here:
// his table has only SCOP, so no fictitious COP capability is required.
const manifestFeatures = load("manifest.json").features;
const manifest = new Set(manifestFeatures);
if (manifest.size !== manifestFeatures.length) failures.push("manifest features must be unique");
const fixtureRoot = resolve(root, "../../../fixtures");
const fixtureFeatures = new Set();
const visitFixtures = (directory) => {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = resolve(directory, entry.name);
    if (entry.isDirectory()) visitFixtures(path);
    else if (entry.name.endsWith(".json")) {
      const feature = JSON.parse(readFileSync(path, "utf8")).feature;
      if (feature) fixtureFeatures.add(feature);
    }
  }
};
visitFixtures(fixtureRoot);
const combatRoster = new Set(
  Object.keys(commanderCombat.commanders).filter((name) => name !== "neutral"),
);
const powerRoster = new Set(Object.keys(commanderPowers.commanders));
const effectiveRoster = new Set(
  Object.keys(commanderProfiles.commanders).filter((name) => name !== "neutral"),
);
for (const [name, table] of [
  ["combat", commanderCombat],
  ["effective-value", commanderProfiles],
  ["power", commanderPowers],
]) {
  if (table.status !== "complete") failures.push(`${name} commander table must be complete`);
}
for (const commander of combatRoster) {
  if (!powerRoster.has(commander)) failures.push(`${commander}: absent from commander power table`);
  if (!effectiveRoster.has(commander))
    failures.push(`${commander}: absent from commander effective-value table`);
  const dayToDay = `commander-combat-v1.${commander}.day-to-day`;
  if (!manifest.has(dayToDay)) failures.push(`${commander}: missing day-to-day combat capability`);
  for (const level of ["cop", "scop"]) {
    if (!commanderPowers.commanders[commander]?.[level]) continue;
    for (const feature of [
      `commander-combat-v1.${commander}.${level}`,
      `commander-power-v1.${commander}.${level}`,
    ]) {
      if (!manifest.has(feature)) failures.push(`${commander}.${level}: missing ${feature}`);
    }
  }
}
for (const commander of powerRoster) {
  if (!combatRoster.has(commander))
    failures.push(`${commander}: power profile is absent from combat roster`);
}
for (const commander of effectiveRoster) {
  if (!combatRoster.has(commander))
    failures.push(`${commander}: effective-value profile is absent from combat roster`);
}
const scalarDayToDayDimensions = [
  "air_upkeep_add",
  "ignores_rain_movement",
  "ignores_snow_movement",
  "rain_movement_as_snow",
  "repair_bars_add",
  "income_per_property_add",
];
for (const [commander, profile] of Object.entries(commanderProfiles.commanders)) {
  if (commander === "neutral") continue;
  const states = new Set();
  const collectStates = (value) => {
    if (Array.isArray(value)) {
      for (const item of value) collectStates(item);
    } else if (value && typeof value === "object") {
      for (const [key, child] of Object.entries(value)) {
        if (["day_to_day", "cop", "scop"].includes(key)) states.add(key.replaceAll("_", "-"));
        collectStates(child);
      }
    }
  };
  collectStates(profile);
  if (scalarDayToDayDimensions.some((dimension) => dimension in profile)) states.add("day-to-day");
  for (const state of states) {
    const feature = `commander-effective-values-v1.${commander}.${state}`;
    if (!manifest.has(feature)) failures.push(`${commander}.${state}: missing ${feature}`);
  }
}
for (const feature of manifest) {
  if (feature.startsWith("commander-") && !fixtureFeatures.has(feature)) {
    failures.push(`${feature}: advertised without an executable fixture`);
  }
}

if (failures.length) {
  console.error(failures.join("\n"));
  process.exitCode = 1;
} else
  console.log(
    `valid: ${effective} effective weapon matchups; ${visionBonuses} vision bonuses, ${visionLimits} vision limits, ${caps.elevated_vision.length} elevated-vision kinds; ${Object.keys(commanderCombat.commanders).length} combat, ${Object.keys(commanderProfiles.commanders).length} effective-value, and ${Object.keys(commanderPowers.commanders).length} power commander profiles`,
  );
