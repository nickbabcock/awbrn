//! Ruleset tables, lowered from `spec/rulesets/` to dense static data.
//!
//! The specification is the normative source; `crates/xtask-ruleset` compiles
//! it into [`generated`](self) tables that this module wraps in accessors. The
//! reducer indexes those tables instead of parsing JSON, which is what makes a
//! command cost microseconds rather than milliseconds.
//!
//! Two rules keep the lowering honest:
//!
//! * The vocabulary enums are generated from the documents, so a specification
//!   change that adds or removes an identifier changes the enum, and every
//!   exhaustive `match` on it stops compiling until it is revisited.
//! * `cargo xtask-ruleset --check` runs in CI, so the checked-in tables cannot
//!   drift from the documents they claim to implement.
//!
//! Only `awbw/2026-07-10` is lowered. [`supports`] is the gate: a state naming
//! any other ruleset is rejected rather than silently evaluated against these
//! tables.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::semantic::RulesetRef;

/// Base damage of one weapon against every unit kind.
pub type DamageRow = [Option<u8>; UnitKind::COUNT];

/// Everything the ruleset says about a unit kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitProfile {
    pub kind: UnitKind,
    /// External AWBW unit-type ID. Only adapters at the wire edge need it.
    pub awbw_id: u32,
    pub domain: Domain,
    pub cost: u64,
    /// Base movement points, before commander effects.
    pub movement: u64,
    pub movement_class: MovementClass,
    pub max_fuel: u64,
    pub max_ammo: u64,
    pub fuel_per_turn: FuelPerTurn,
    pub vision: i64,
    /// Indirect fire range. `None` for units that fire at range one.
    pub indirect_range: Option<AttackRange>,
    pub fire_mode: FireMode,
    pub weapon_policy: WeaponPolicy,
    pub ammo_weapon: Option<WeaponProfile>,
    pub unlimited_weapon: Option<WeaponProfile>,
    pub can_capture: bool,
    pub elevated_vision: bool,
    pub transport: Option<TransportProfile>,
    pub supply: Option<SupplyProfile>,
    pub repair: Option<RepairProfile>,
    pub concealment: Option<ConcealmentProfile>,
    pub special_actions: &'static [Command],
}

impl UnitProfile {
    /// The weapon in `slot`, if this unit kind has one.
    pub const fn weapon(&self, slot: WeaponSlot) -> Option<&WeaponProfile> {
        match slot {
            WeaponSlot::Ammo => self.ammo_weapon.as_ref(),
            WeaponSlot::Unlimited => self.unlimited_weapon.as_ref(),
        }
    }
}

/// Fuel drawn at the start of each turn, in normal and hidden modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FuelPerTurn {
    pub normal: u64,
    /// `None` where the unit kind cannot conceal itself.
    pub hidden: Option<u64>,
}

/// An inclusive attack range in tiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttackRange {
    pub minimum: u64,
    pub maximum: u64,
}

/// One weapon: what it costs to fire and what it does to each defender.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeaponProfile {
    pub slot: WeaponSlot,
    pub ammo_cost: u64,
    pub damage: &'static DamageRow,
}

impl WeaponProfile {
    /// Base damage against `defender`, or `None` when this weapon cannot
    /// target it at all.
    pub const fn damage(&self, defender: UnitKind) -> Option<u8> {
        self.damage[defender.index()]
    }
}

/// Cargo capacity and the kinds that may be carried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransportProfile {
    pub capacity: usize,
    pub cargo: UnitKindSet,
}

/// A start-of-turn resupply operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SupplyProfile {
    pub trigger: SupplyTrigger,
    pub relation: Relation,
    pub targets: TargetSet,
    pub refill: ResourceSet,
}

/// A commanded repair operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RepairProfile {
    pub command: Command,
    pub relation: Relation,
    pub targets: TargetSet,
    pub exact_hp: u8,
    pub cost_percent: u64,
    pub also_refills: ResourceSet,
}

/// How a unit kind conceals itself, and the commands that toggle it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConcealmentProfile {
    pub mode: ConcealmentMode,
    pub enter_command: Command,
    pub exit_command: Command,
}

/// Everything the ruleset says about a terrain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainProfile {
    pub terrain: Terrain,
    pub defense_stars: u8,
    pub property_kind: Option<PropertyKind>,
    pub traits: TerrainTraits,
    /// Extra vision granted to units with elevated vision standing here.
    pub vision_bonus: Option<i64>,
    /// Distance beyond which this tile conceals its occupants in fog.
    pub vision_limit: Option<usize>,
    /// Terrain this becomes when its owner is eliminated.
    pub elimination_replacement: Option<Terrain>,
    pub destructible: Option<Destructible>,
}

impl TerrainProfile {
    /// Whether the terrain carries `value`.
    pub const fn has(&self, value: TerrainTrait) -> bool {
        self.traits.contains(value)
    }

    /// Whether the terrain is a production facility for units of any domain.
    pub fn produces_any(&self) -> bool {
        Domain::ALL.iter().any(|domain| self.has(domain.produces()))
    }
}

impl Domain {
    /// The trait a terrain must carry to produce units of this domain.
    ///
    /// The specification spells these as `produces-<domain>`; matching
    /// exhaustively means a new domain cannot be added without revisiting the
    /// terrains that would have to serve it.
    pub const fn produces(self) -> TerrainTrait {
        match self {
            Self::Air => TerrainTrait::ProducesAir,
            Self::Ground => TerrainTrait::ProducesGround,
            Self::Sea => TerrainTrait::ProducesSea,
        }
    }

    /// The trait a terrain must carry to repair units of this domain.
    pub const fn repairs(self) -> TerrainTrait {
        match self {
            Self::Air => TerrainTrait::RepairsAir,
            Self::Ground => TerrainTrait::RepairsGround,
            Self::Sea => TerrainTrait::RepairsSea,
        }
    }
}

/// A terrain that can be attacked and destroyed, such as a pipe seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Destructible {
    pub maximum_hp: u64,
    /// The unit kind whose damage row the attack is resolved against.
    pub target_kind: UnitKind,
    pub destruction_replacement: Terrain,
}

/// A set of unit kinds, held as a bitmask over [`UnitKind::index`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UnitKindSet(u32);

impl UnitKindSet {
    pub const fn new(kinds: &[UnitKind]) -> Self {
        let mut bits = 0;
        let mut index = 0;
        while index < kinds.len() {
            bits |= 1 << kinds[index].index();
            index += 1;
        }
        Self(bits)
    }

    pub const fn contains(self, kind: UnitKind) -> bool {
        self.0 & (1 << kind.index()) != 0
    }
}

/// A set of terrain traits, held as a bitmask over [`TerrainTrait::index`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerrainTraits(u32);

impl TerrainTraits {
    pub const fn new(traits: &[TerrainTrait]) -> Self {
        let mut bits = 0;
        let mut index = 0;
        while index < traits.len() {
            bits |= 1 << traits[index].index();
            index += 1;
        }
        Self(bits)
    }

    pub const fn contains(self, value: TerrainTrait) -> bool {
        self.0 & (1 << value.index()) != 0
    }
}

/// A set of consumable resources, held as a bitmask over [`Resource::index`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResourceSet(u32);

impl ResourceSet {
    pub const fn new(resources: &[Resource]) -> Self {
        let mut bits = 0;
        let mut index = 0;
        while index < resources.len() {
            bits |= 1 << resources[index].index();
            index += 1;
        }
        Self(bits)
    }

    pub const fn contains(self, resource: Resource) -> bool {
        self.0 & (1 << resource.index()) != 0
    }
}

include!("generated/ruleset.rs");

/// Whether these tables implement the ruleset a state names.
///
/// Only one revision is lowered. Anything else must be refused rather than
/// evaluated against the wrong numbers.
pub fn supports(ruleset: &RulesetRef) -> bool {
    ruleset.id == RULESET_ID && ruleset.revision == RULESET_REVISION
}

/// Everything the ruleset says about a unit kind.
///
/// Infallible. The vocabulary is generated from the tables, so a `UnitKind`
/// value cannot name a kind the tables lack — that is checked once, when the
/// identifier is decoded.
pub fn profile(kind: UnitKind) -> &'static UnitProfile {
    &UNIT_PROFILES[kind.index()]
}

/// Everything the ruleset says about a terrain. Infallible, as [`profile`] is.
pub fn terrain(terrain: Terrain) -> &'static TerrainProfile {
    &TERRAIN_PROFILES[terrain.index()]
}

/// Whether a terrain carries a trait.
pub fn terrain_has(kind: Terrain, value: TerrainTrait) -> bool {
    terrain(kind).has(value)
}

/// Defense stars a terrain contributes to a unit standing on it.
pub fn defense_stars(kind: Terrain) -> u8 {
    terrain(kind).defense_stars
}

/// Movement points needed to enter `kind`. `None` is the specification's `-`:
/// impassable.
pub fn movement_cost(kind: Terrain, weather: WeatherKind, class: MovementClass) -> Option<u64> {
    MOVEMENT_COSTS[kind.index()][weather.index()][class.index()].map(u64::from)
}
