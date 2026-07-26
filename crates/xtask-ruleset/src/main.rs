//! Generates the AWVM ruleset tables from the checked-in specification.
//!
//! `spec/rulesets/awbw/2026-07-10/**` is the normative source. AWVM used to
//! parse those documents at runtime, once per table per reducer call; this
//! xtask lowers them to dense `static` tables keyed by `#[repr(u8)]` enums so
//! the reducer only indexes.
//!
//! Usage:
//!
//! ```text
//! cargo xtask-ruleset           # regenerate crates/awvm/src/generated/ruleset.rs
//! cargo xtask-ruleset --check   # fail if the checked-in file is stale
//! ```
//!
//! A stale table is a silent correctness bug in a conformance claim, so
//! `--check` runs in CI.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::{env, fs, process};

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;

/// The ruleset revision lowered into `static` tables.
const RULESET_ID: &str = "awbw";
const RULESET_REVISION: &str = "2026-07-10";

fn main() -> Result<()> {
    let check = match env::args().nth(1).as_deref() {
        None => false,
        Some("--check") => true,
        Some(other) => {
            eprintln!("Usage: cargo xtask-ruleset [--check]\nunknown argument {other}");
            process::exit(1);
        }
    };

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let source_dir = repo_root
        .join("spec/rulesets")
        .join(RULESET_ID)
        .join(RULESET_REVISION);
    let output_path = repo_root.join("crates/awvm/src/generated/ruleset.rs");

    let ruleset = Ruleset::load(&source_dir)?;
    let rendered = render(&ruleset)?;

    if check {
        let current = fs::read_to_string(&output_path)
            .with_context(|| format!("Reading {}", output_path.display()))?;
        if current != rendered {
            bail!(
                "{} is stale.\nRegenerate it with `cargo xtask-ruleset` and commit the result.",
                output_path.display()
            );
        }
        println!("{} is up to date", output_path.display());
        return Ok(());
    }

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("Creating {}", parent.display()))?;
    }
    fs::write(&output_path, rendered)
        .with_context(|| format!("Writing {}", output_path.display()))?;
    println!("Wrote {}", output_path.display());
    Ok(())
}

// ---------------------------------------------------------------------------
// Specification documents
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct UnitsDocument {
    units: BTreeMap<String, UnitEntry>,
}

#[derive(Debug, Deserialize)]
struct UnitEntry {
    awbw_id: u32,
    domain: String,
    cost: u64,
    #[serde(rename = "move")]
    movement: u64,
    movement_class: String,
    max_fuel: u64,
    max_ammo: u64,
    fuel_per_turn: FuelPerTurn,
    vision: i64,
    indirect_range: Option<AttackRange>,
}

#[derive(Debug, Deserialize)]
struct FuelPerTurn {
    normal: u64,
    hidden: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct AttackRange {
    min: u64,
    max: u64,
}

#[derive(Debug, Deserialize)]
struct TerrainDocument {
    terrains: BTreeMap<String, TerrainEntry>,
}

#[derive(Debug, Deserialize)]
struct TerrainEntry {
    defense_stars: u8,
    property_kind: Option<String>,
    traits: Vec<String>,
    vision_bonus: Option<i64>,
    vision_limit: Option<usize>,
    elimination_replacement: Option<String>,
    destructible: Option<DestructibleEntry>,
}

#[derive(Debug, Deserialize)]
struct DestructibleEntry {
    maximum_hp: u64,
    target_kind: String,
    destruction_replacement: String,
}

#[derive(Debug, Deserialize)]
struct MovementCostDocument {
    weather: Vec<String>,
    movement_classes: Vec<String>,
    terrains: BTreeMap<String, BTreeMap<String, BTreeMap<String, Option<u8>>>>,
}

#[derive(Debug, Deserialize)]
struct CapabilitiesDocument {
    capture: BTreeSet<String>,
    elevated_vision: BTreeSet<String>,
    transport: BTreeMap<String, TransportEntry>,
    supply: BTreeMap<String, SupplyEntry>,
    repair: BTreeMap<String, RepairEntry>,
    concealment: BTreeMap<String, ConcealmentEntry>,
    special_actions: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct TransportEntry {
    capacity: usize,
    cargo: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SupplyEntry {
    trigger: String,
    relation: String,
    targets: String,
    refill: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RepairEntry {
    command: String,
    relation: String,
    targets: String,
    exact_hp: u8,
    cost_percent: u64,
    also_refills: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ConcealmentEntry {
    mode: String,
    enter_command: String,
    exit_command: String,
}

#[derive(Debug, Deserialize)]
struct CombatProfileDocument {
    units: BTreeMap<String, CombatProfileEntry>,
}

#[derive(Debug, Deserialize)]
struct CombatProfileEntry {
    fire_mode: String,
    weapon_policy: String,
}

#[derive(Debug, Deserialize)]
struct WeaponsDocument {
    selection: WeaponSelection,
    units: BTreeMap<String, BTreeMap<String, WeaponEntry>>,
}

#[derive(Debug, Deserialize)]
struct WeaponSelection {
    order: Vec<String>,
    requires_available_ammo: bool,
}

#[derive(Debug, Deserialize)]
struct WeaponEntry {
    ammo_cost: u64,
    damage: BTreeMap<String, u8>,
}

#[derive(Debug, Deserialize)]
struct CommanderProfileDocument {
    commanders: BTreeMap<String, serde_json::Value>,
}

struct Ruleset {
    units: UnitsDocument,
    terrain: TerrainDocument,
    movement: MovementCostDocument,
    capabilities: CapabilitiesDocument,
    combat_profiles: CombatProfileDocument,
    weapons: WeaponsDocument,
    commanders: CommanderProfileDocument,
}

impl Ruleset {
    fn load(dir: &Path) -> Result<Self> {
        Ok(Self {
            units: read_json(dir, "units.json")?,
            terrain: read_json(dir, "terrain.json")?,
            movement: read_json(dir, "movement-costs.json")?,
            capabilities: read_json(dir, "unit-capabilities.json")?,
            combat_profiles: read_json(dir, "combat-profiles.json")?,
            weapons: read_json(dir, "weapons.json")?,
            commanders: read_json(dir, "commander-profiles.json")?,
        })
    }
}

fn read_json<T: serde::de::DeserializeOwned>(dir: &Path, name: &str) -> Result<T> {
    let path: PathBuf = dir.join(name);
    let contents =
        fs::read_to_string(&path).with_context(|| format!("Reading {}", path.display()))?;
    serde_json::from_str(&contents).with_context(|| format!("Parsing {}", path.display()))
}

// ---------------------------------------------------------------------------
// Vocabulary enums
// ---------------------------------------------------------------------------

/// A `#[repr(u8)]` enum whose variants are exactly the identifiers the ruleset
/// documents use. Regenerating after a specification change adds or removes
/// variants, so every `match` on a vocabulary fails to compile until it is
/// brought back in line.
struct Vocabulary {
    name: &'static str,
    doc: &'static str,
    values: Vec<String>,
}

impl Vocabulary {
    fn new<I>(name: &'static str, doc: &'static str, values: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        Self {
            name,
            doc,
            values: values.into_iter().map(Into::into).collect(),
        }
    }

    fn variant(&self, id: &str) -> Result<String> {
        if !self.values.iter().any(|value| value == id) {
            bail!("{} has no variant for {id:?}", self.name);
        }
        Ok(variant_name(id))
    }

    fn path(&self, id: &str) -> Result<String> {
        Ok(format!("{}::{}", self.name, self.variant(id)?))
    }

    fn optional_path(&self, id: Option<&String>) -> Result<String> {
        match id {
            Some(id) => Ok(format!("Some({})", self.path(id)?)),
            None => Ok("None".to_owned()),
        }
    }
}

fn variant_name(id: &str) -> String {
    id.split(['-', '_'])
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn render_vocabulary(vocabulary: &Vocabulary, out: &mut String) {
    let name = vocabulary.name;
    let count = vocabulary.values.len();
    let variants: Vec<String> = vocabulary
        .values
        .iter()
        .map(|value| variant_name(value))
        .collect();

    let _ = writeln!(out, "/// {}", vocabulary.doc);
    let _ = writeln!(
        out,
        "#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]"
    );
    let _ = writeln!(out, "#[repr(u8)]");
    let _ = writeln!(out, "pub enum {name} {{");
    for (value, variant) in vocabulary.values.iter().zip(&variants) {
        let _ = writeln!(out, "    #[serde(rename = \"{value}\")]");
        let _ = writeln!(out, "    {variant},");
    }
    let _ = writeln!(out, "}}");
    let _ = writeln!(out);

    let _ = writeln!(out, "impl {name} {{");
    let _ = writeln!(
        out,
        "    /// Number of variants, and the length of any table keyed by this vocabulary."
    );
    let _ = writeln!(out, "    pub const COUNT: usize = {count};");
    let _ = writeln!(out);
    let _ = writeln!(out, "    /// Every variant, in table order.");
    let _ = writeln!(out, "    pub const ALL: [Self; {count}] = [");
    for variant in &variants {
        let _ = writeln!(out, "        Self::{variant},");
    }
    let _ = writeln!(out, "    ];");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "    /// The identifier this variant is written as in the specification and on the wire."
    );
    let _ = writeln!(out, "    pub const fn as_str(self) -> &'static str {{");
    let _ = writeln!(out, "        match self {{");
    for (value, variant) in vocabulary.values.iter().zip(&variants) {
        let _ = writeln!(out, "            Self::{variant} => \"{value}\",");
    }
    let _ = writeln!(out, "        }}");
    let _ = writeln!(out, "    }}");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "    /// Parses an identifier. `None` means the value is outside this ruleset."
    );
    let _ = writeln!(out, "    pub fn from_id(id: &str) -> Option<Self> {{");
    let _ = writeln!(out, "        match id {{");
    for (value, variant) in vocabulary.values.iter().zip(&variants) {
        let _ = writeln!(out, "            \"{value}\" => Some(Self::{variant}),");
    }
    let _ = writeln!(out, "            _ => None,");
    let _ = writeln!(out, "        }}");
    let _ = writeln!(out, "    }}");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "    /// Dense index into tables keyed by this vocabulary."
    );
    let _ = writeln!(out, "    pub const fn index(self) -> usize {{");
    let _ = writeln!(out, "        self as usize");
    let _ = writeln!(out, "    }}");
    let _ = writeln!(out, "}}");
    let _ = writeln!(out);
    let _ = writeln!(out, "impl fmt::Display for {name} {{");
    let _ = writeln!(
        out,
        "    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {{"
    );
    let _ = writeln!(out, "        formatter.write_str(self.as_str())");
    let _ = writeln!(out, "    }}");
    let _ = writeln!(out, "}}");
    let _ = writeln!(out);
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn render(ruleset: &Ruleset) -> Result<String> {
    let unit_kinds = Vocabulary::new(
        "UnitKind",
        "Unit kinds defined by `units.json`, ordered as the damage matrices are keyed.",
        ruleset.units.units.keys().cloned(),
    );
    let terrains = Vocabulary::new(
        "Terrain",
        "Terrain kinds defined by `terrain.json`.",
        ruleset.terrain.terrains.keys().cloned(),
    );
    let movement_classes = Vocabulary::new(
        "MovementClass",
        "Movement classes `movement-costs.json` is keyed by.",
        ruleset.movement.movement_classes.clone(),
    );
    let weather = Vocabulary::new(
        "WeatherKind",
        "Weather conditions that select a movement-cost column.",
        ruleset.movement.weather.clone(),
    );
    let domains = Vocabulary::new(
        "Domain",
        "Unit domains defined by `units.json`.",
        distinct(ruleset.units.units.values().map(|unit| unit.domain.clone())),
    );
    let fire_modes = Vocabulary::new(
        "FireMode",
        "Fire modes defined by `combat-profiles.json`.",
        distinct(
            ruleset
                .combat_profiles
                .units
                .values()
                .map(|profile| profile.fire_mode.clone()),
        ),
    );
    let weapon_policies = Vocabulary::new(
        "WeaponPolicy",
        "Weapon policies defined by `combat-profiles.json`.",
        distinct(
            ruleset
                .combat_profiles
                .units
                .values()
                .map(|profile| profile.weapon_policy.clone()),
        ),
    );
    let weapon_slots = Vocabulary::new(
        "WeaponSlot",
        "Weapon slots, in the selection order `weapons.json` mandates.",
        ruleset.weapons.selection.order.clone(),
    );
    let commanders = Vocabulary::new(
        "CommanderKind",
        "Commanders defined by `commander-profiles.json`.",
        ruleset.commanders.commanders.keys().cloned(),
    );
    let terrain_traits = Vocabulary::new(
        "TerrainTrait",
        "Terrain traits defined by `terrain.json`.",
        distinct(
            ruleset
                .terrain
                .terrains
                .values()
                .flat_map(|terrain| terrain.traits.iter().cloned()),
        ),
    );
    let property_kinds = Vocabulary::new(
        "PropertyKind",
        "Property kinds defined by `terrain.json`.",
        distinct(
            ruleset
                .terrain
                .terrains
                .values()
                .filter_map(|terrain| terrain.property_kind.clone()),
        ),
    );
    let resources = Vocabulary::new(
        "Resource",
        "Consumable unit resources that supply and repair operators refill.",
        distinct(
            ruleset
                .capabilities
                .supply
                .values()
                .flat_map(|supply| supply.refill.iter().cloned())
                .chain(
                    ruleset
                        .capabilities
                        .repair
                        .values()
                        .flat_map(|repair| repair.also_refills.iter().cloned()),
                ),
        ),
    );
    let supply_triggers = Vocabulary::new(
        "SupplyTrigger",
        "When a supply operator fires.",
        distinct(
            ruleset
                .capabilities
                .supply
                .values()
                .map(|supply| supply.trigger.clone()),
        ),
    );
    let relations = Vocabulary::new(
        "Relation",
        "Spatial relation between an operator's source unit and its targets.",
        distinct(
            ruleset
                .capabilities
                .supply
                .values()
                .map(|supply| supply.relation.clone())
                .chain(
                    ruleset
                        .capabilities
                        .repair
                        .values()
                        .map(|repair| repair.relation.clone()),
                ),
        ),
    );
    let targets = Vocabulary::new(
        "TargetSet",
        "Which units a supply or repair operator applies to.",
        distinct(
            ruleset
                .capabilities
                .supply
                .values()
                .map(|supply| supply.targets.clone())
                .chain(
                    ruleset
                        .capabilities
                        .repair
                        .values()
                        .map(|repair| repair.targets.clone()),
                ),
        ),
    );
    let commands = Vocabulary::new(
        "Command",
        "Commands a capability makes available to a unit.",
        distinct(
            ruleset
                .capabilities
                .repair
                .values()
                .map(|repair| repair.command.clone())
                .chain(
                    ruleset
                        .capabilities
                        .concealment
                        .values()
                        .flat_map(|hiding| {
                            [hiding.enter_command.clone(), hiding.exit_command.clone()]
                        }),
                )
                .chain(
                    ruleset
                        .capabilities
                        .special_actions
                        .values()
                        .flat_map(|actions| actions.iter().cloned()),
                ),
        ),
    );
    let concealment_modes = Vocabulary::new(
        "ConcealmentMode",
        "How a concealed unit hides.",
        distinct(
            ruleset
                .capabilities
                .concealment
                .values()
                .map(|hiding| hiding.mode.clone()),
        ),
    );

    let vocabularies = [
        &unit_kinds,
        &terrains,
        &movement_classes,
        &weather,
        &domains,
        &fire_modes,
        &weapon_policies,
        &weapon_slots,
        &commanders,
        &terrain_traits,
        &property_kinds,
        &resources,
        &supply_triggers,
        &relations,
        &targets,
        &commands,
        &concealment_modes,
    ];

    let mut out = String::new();
    let _ = writeln!(out, "// This file is @generated by xtask-ruleset.");
    let _ = writeln!(
        out,
        "// Source: spec/rulesets/{RULESET_ID}/{RULESET_REVISION}/**"
    );
    let _ = writeln!(
        out,
        "// Edit the specification and run `cargo xtask-ruleset`, never this file."
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "/// Identifier of the ruleset these tables were generated from."
    );
    let _ = writeln!(out, "pub const RULESET_ID: &str = \"{RULESET_ID}\";");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "/// Revision of the ruleset these tables were generated from."
    );
    let _ = writeln!(
        out,
        "pub const RULESET_REVISION: &str = \"{RULESET_REVISION}\";"
    );
    let _ = writeln!(out);

    for vocabulary in vocabularies {
        render_vocabulary(vocabulary, &mut out);
    }

    render_unit_profiles(
        ruleset,
        &unit_kinds,
        &domains,
        &movement_classes,
        &fire_modes,
        &weapon_policies,
        &weapon_slots,
        &resources,
        &supply_triggers,
        &relations,
        &targets,
        &commands,
        &concealment_modes,
        &mut out,
    )?;
    render_damage(ruleset, &unit_kinds, &weapon_slots, &mut out)?;
    render_terrain_profiles(
        ruleset,
        &terrains,
        &unit_kinds,
        &property_kinds,
        &terrain_traits,
        &mut out,
    )?;
    render_movement_costs(ruleset, &terrains, &weather, &movement_classes, &mut out)?;
    render_selection(ruleset, &weapon_slots, &mut out)?;

    Ok(out)
}

fn distinct<I: IntoIterator<Item = String>>(values: I) -> Vec<String> {
    values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn render_unit_profiles(
    ruleset: &Ruleset,
    unit_kinds: &Vocabulary,
    domains: &Vocabulary,
    movement_classes: &Vocabulary,
    fire_modes: &Vocabulary,
    weapon_policies: &Vocabulary,
    weapon_slots: &Vocabulary,
    resources: &Vocabulary,
    supply_triggers: &Vocabulary,
    relations: &Vocabulary,
    targets: &Vocabulary,
    commands: &Vocabulary,
    concealment_modes: &Vocabulary,
    out: &mut String,
) -> Result<()> {
    let _ = writeln!(
        out,
        "/// Everything the ruleset says about a unit kind, keyed by [`UnitKind::index`]."
    );
    let _ = writeln!(out, "///");
    let _ = writeln!(
        out,
        "/// Merged from `units.json`, `combat-profiles.json`, `weapons.json` and"
    );
    let _ = writeln!(out, "/// `unit-capabilities.json`.");
    let _ = writeln!(
        out,
        "pub static UNIT_PROFILES: [UnitProfile; UnitKind::COUNT] = ["
    );

    for (kind, unit) in &ruleset.units.units {
        let combat = ruleset
            .combat_profiles
            .units
            .get(kind)
            .ok_or_else(|| anyhow!("combat-profiles.json is missing {kind}"))?;
        let weapons = ruleset.weapons.units.get(kind);

        let _ = writeln!(out, "    UnitProfile {{");
        let _ = writeln!(out, "        kind: {},", unit_kinds.path(kind)?);
        let _ = writeln!(out, "        awbw_id: {},", unit.awbw_id);
        let _ = writeln!(out, "        domain: {},", domains.path(&unit.domain)?);
        let _ = writeln!(out, "        cost: {},", unit.cost);
        let _ = writeln!(out, "        movement: {},", unit.movement);
        let _ = writeln!(
            out,
            "        movement_class: {},",
            movement_classes.path(&unit.movement_class)?
        );
        let _ = writeln!(out, "        max_fuel: {},", unit.max_fuel);
        let _ = writeln!(out, "        max_ammo: {},", unit.max_ammo);
        let _ = writeln!(
            out,
            "        fuel_per_turn: FuelPerTurn {{ normal: {}, hidden: {} }},",
            unit.fuel_per_turn.normal,
            render_option(unit.fuel_per_turn.hidden)
        );
        let _ = writeln!(out, "        vision: {},", unit.vision);
        let _ = writeln!(
            out,
            "        indirect_range: {},",
            match &unit.indirect_range {
                Some(range) => format!(
                    "Some(AttackRange {{ minimum: {}, maximum: {} }})",
                    range.min, range.max
                ),
                None => "None".to_owned(),
            }
        );
        let _ = writeln!(
            out,
            "        fire_mode: {},",
            fire_modes.path(&combat.fire_mode)?
        );
        let _ = writeln!(
            out,
            "        weapon_policy: {},",
            weapon_policies.path(&combat.weapon_policy)?
        );
        for slot in &ruleset.weapons.selection.order {
            let field = format!("{slot}_weapon");
            let entry = weapons.and_then(|slots| slots.get(slot));
            match entry {
                Some(entry) => {
                    let _ = writeln!(
                        out,
                        "        {field}: Some(WeaponProfile {{ slot: {}, ammo_cost: {}, damage: &{}_DAMAGE[{}.index()] }}),",
                        weapon_slots.path(slot)?,
                        entry.ammo_cost,
                        slot.to_uppercase(),
                        unit_kinds.path(kind)?
                    );
                }
                None => {
                    let _ = writeln!(out, "        {field}: None,");
                }
            }
        }
        let _ = writeln!(
            out,
            "        can_capture: {},",
            ruleset.capabilities.capture.contains(kind)
        );
        let _ = writeln!(
            out,
            "        elevated_vision: {},",
            ruleset.capabilities.elevated_vision.contains(kind)
        );
        match ruleset.capabilities.transport.get(kind) {
            Some(transport) => {
                let cargo = transport
                    .cargo
                    .iter()
                    .map(|cargo| unit_kinds.path(cargo))
                    .collect::<Result<Vec<_>>>()?;
                let _ = writeln!(
                    out,
                    "        transport: Some(TransportProfile {{ capacity: {}, cargo: UnitKindSet::new(&[{}]) }}),",
                    transport.capacity,
                    cargo.join(", ")
                );
            }
            None => {
                let _ = writeln!(out, "        transport: None,");
            }
        }
        match ruleset.capabilities.supply.get(kind) {
            Some(supply) => {
                let _ = writeln!(out, "        supply: Some(SupplyProfile {{");
                let _ = writeln!(
                    out,
                    "            trigger: {},",
                    supply_triggers.path(&supply.trigger)?
                );
                let _ = writeln!(
                    out,
                    "            relation: {},",
                    relations.path(&supply.relation)?
                );
                let _ = writeln!(
                    out,
                    "            targets: {},",
                    targets.path(&supply.targets)?
                );
                let _ = writeln!(
                    out,
                    "            refill: {},",
                    render_resource_set(&supply.refill, resources)?
                );
                let _ = writeln!(out, "        }}),");
            }
            None => {
                let _ = writeln!(out, "        supply: None,");
            }
        }
        match ruleset.capabilities.repair.get(kind) {
            Some(repair) => {
                let _ = writeln!(out, "        repair: Some(RepairProfile {{");
                let _ = writeln!(
                    out,
                    "            command: {},",
                    commands.path(&repair.command)?
                );
                let _ = writeln!(
                    out,
                    "            relation: {},",
                    relations.path(&repair.relation)?
                );
                let _ = writeln!(
                    out,
                    "            targets: {},",
                    targets.path(&repair.targets)?
                );
                let _ = writeln!(out, "            exact_hp: {},", repair.exact_hp);
                let _ = writeln!(out, "            cost_percent: {},", repair.cost_percent);
                let _ = writeln!(
                    out,
                    "            also_refills: {},",
                    render_resource_set(&repair.also_refills, resources)?
                );
                let _ = writeln!(out, "        }}),");
            }
            None => {
                let _ = writeln!(out, "        repair: None,");
            }
        }
        match ruleset.capabilities.concealment.get(kind) {
            Some(concealment) => {
                let _ = writeln!(
                    out,
                    "        concealment: Some(ConcealmentProfile {{ mode: {}, enter_command: {}, exit_command: {} }}),",
                    concealment_modes.path(&concealment.mode)?,
                    commands.path(&concealment.enter_command)?,
                    commands.path(&concealment.exit_command)?
                );
            }
            None => {
                let _ = writeln!(out, "        concealment: None,");
            }
        }
        let actions = ruleset
            .capabilities
            .special_actions
            .get(kind)
            .map(|actions| {
                actions
                    .iter()
                    .map(|action| commands.path(action))
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();
        let _ = writeln!(out, "        special_actions: &[{}],", actions.join(", "));
        let _ = writeln!(out, "    }},");
    }
    let _ = writeln!(out, "];");
    let _ = writeln!(out);
    Ok(())
}

fn render_resource_set(values: &[String], resources: &Vocabulary) -> Result<String> {
    let rendered = values
        .iter()
        .map(|value| resources.path(value))
        .collect::<Result<Vec<_>>>()?;
    Ok(format!("ResourceSet::new(&[{}])", rendered.join(", ")))
}

fn render_option(value: Option<u64>) -> String {
    match value {
        Some(value) => format!("Some({value})"),
        None => "None".to_owned(),
    }
}

fn render_damage(
    ruleset: &Ruleset,
    unit_kinds: &Vocabulary,
    weapon_slots: &Vocabulary,
    out: &mut String,
) -> Result<()> {
    for slot in &ruleset.weapons.selection.order {
        let table = slot.to_uppercase();
        let _ = writeln!(
            out,
            "/// Base damage of every {slot} weapon against every defender, from `weapons.json`."
        );
        let _ = writeln!(out, "///");
        let _ = writeln!(
            out,
            "/// Indexed `[attacker][defender]` by [`UnitKind::index`]. `None` means the"
        );
        let _ = writeln!(
            out,
            "/// attacker has no {slot} weapon entry against that defender."
        );
        let _ = writeln!(
            out,
            "pub static {table}_DAMAGE: [DamageRow; UnitKind::COUNT] = ["
        );
        for attacker in unit_kinds.values.iter() {
            let entry = ruleset
                .weapons
                .units
                .get(attacker)
                .and_then(|slots| slots.get(slot));
            let _ = writeln!(out, "    // {attacker}");
            let _ = writeln!(out, "    [");
            for defender in unit_kinds.values.iter() {
                let damage = entry.and_then(|entry| entry.damage.get(defender));
                let _ = writeln!(
                    out,
                    "        {}, // vs {defender}",
                    match damage {
                        Some(value) => format!("Some({value})"),
                        None => "None".to_owned(),
                    }
                );
            }
            let _ = writeln!(out, "    ],");
        }
        let _ = writeln!(out, "];");
        let _ = writeln!(out);

        // Referenced from `UNIT_PROFILES`; keep the vocabulary honest.
        weapon_slots.variant(slot)?;
    }
    Ok(())
}

fn render_terrain_profiles(
    ruleset: &Ruleset,
    terrains: &Vocabulary,
    unit_kinds: &Vocabulary,
    property_kinds: &Vocabulary,
    terrain_traits: &Vocabulary,
    out: &mut String,
) -> Result<()> {
    let _ = writeln!(
        out,
        "/// Everything `terrain.json` says about a terrain, keyed by [`Terrain::index`]."
    );
    let _ = writeln!(
        out,
        "pub static TERRAIN_PROFILES: [TerrainProfile; Terrain::COUNT] = ["
    );
    for (name, terrain) in &ruleset.terrain.terrains {
        let traits = terrain
            .traits
            .iter()
            .map(|value| terrain_traits.path(value))
            .collect::<Result<Vec<_>>>()?;
        let _ = writeln!(out, "    TerrainProfile {{");
        let _ = writeln!(out, "        terrain: {},", terrains.path(name)?);
        let _ = writeln!(out, "        defense_stars: {},", terrain.defense_stars);
        let _ = writeln!(
            out,
            "        property_kind: {},",
            property_kinds.optional_path(terrain.property_kind.as_ref())?
        );
        let _ = writeln!(
            out,
            "        traits: TerrainTraits::new(&[{}]),",
            traits.join(", ")
        );
        let _ = writeln!(
            out,
            "        vision_bonus: {},",
            match terrain.vision_bonus {
                Some(value) => format!("Some({value})"),
                None => "None".to_owned(),
            }
        );
        let _ = writeln!(
            out,
            "        vision_limit: {},",
            match terrain.vision_limit {
                Some(value) => format!("Some({value})"),
                None => "None".to_owned(),
            }
        );
        let _ = writeln!(
            out,
            "        elimination_replacement: {},",
            terrains.optional_path(terrain.elimination_replacement.as_ref())?
        );
        let _ = writeln!(
            out,
            "        destructible: {},",
            match &terrain.destructible {
                Some(destructible) => format!(
                    "Some(Destructible {{ maximum_hp: {}, target_kind: {}, destruction_replacement: {} }})",
                    destructible.maximum_hp,
                    unit_kinds.path(&destructible.target_kind)?,
                    terrains.path(&destructible.destruction_replacement)?
                ),
                None => "None".to_owned(),
            }
        );
        let _ = writeln!(out, "    }},");
    }
    let _ = writeln!(out, "];");
    let _ = writeln!(out);
    Ok(())
}

fn render_movement_costs(
    ruleset: &Ruleset,
    terrains: &Vocabulary,
    weather: &Vocabulary,
    movement_classes: &Vocabulary,
    out: &mut String,
) -> Result<()> {
    let _ = writeln!(
        out,
        "/// Movement point cost to enter a terrain, from `movement-costs.json`."
    );
    let _ = writeln!(out, "///");
    let _ = writeln!(
        out,
        "/// Indexed `[terrain][weather][movement class]`. `None` is the specification's"
    );
    let _ = writeln!(out, "/// `-`: impassable.");
    let _ = writeln!(
        out,
        "pub static MOVEMENT_COSTS: [[[Option<u8>; MovementClass::COUNT]; WeatherKind::COUNT];"
    );
    let _ = writeln!(out, "    Terrain::COUNT] = [");
    for terrain in &terrains.values {
        let columns = ruleset
            .movement
            .terrains
            .get(terrain)
            .ok_or_else(|| anyhow!("movement-costs.json is missing {terrain}"))?;
        let _ = writeln!(out, "    // {terrain}");
        let _ = writeln!(out, "    [");
        for condition in &weather.values {
            let costs = columns
                .get(condition)
                .ok_or_else(|| anyhow!("movement-costs.json {terrain} is missing {condition}"))?;
            let cells = movement_classes
                .values
                .iter()
                .map(|class| {
                    let cost = costs.get(class).copied().flatten();
                    match cost {
                        Some(cost) => format!("Some({cost})"),
                        None => "None".to_owned(),
                    }
                })
                .collect::<Vec<_>>();
            let _ = writeln!(out, "        [{}], // {condition}", cells.join(", "));
        }
        let _ = writeln!(out, "    ],");
    }
    let _ = writeln!(out, "];");
    let _ = writeln!(out);
    Ok(())
}

fn render_selection(ruleset: &Ruleset, weapon_slots: &Vocabulary, out: &mut String) -> Result<()> {
    let order = ruleset
        .weapons
        .selection
        .order
        .iter()
        .map(|slot| weapon_slots.path(slot))
        .collect::<Result<Vec<_>>>()?;
    let _ = writeln!(
        out,
        "/// The order `weapons.json` mandates weapon slots be considered in."
    );
    let _ = writeln!(
        out,
        "pub static WEAPON_SELECTION_ORDER: [WeaponSlot; WeaponSlot::COUNT] = [{}];",
        order.join(", ")
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "/// Whether weapon selection requires the slot's ammo cost to be affordable."
    );
    let _ = writeln!(
        out,
        "pub const WEAPON_SELECTION_REQUIRES_AVAILABLE_AMMO: bool = {};",
        ruleset.weapons.selection.requires_available_ammo
    );
    Ok(())
}
