//! The battle calculator's wire edge.
//!
//! Two exports, both free functions rather than methods on [`crate::BevyApp`]:
//! the calculator answers a hypothetical, so it needs no board, no canvas and
//! no running game. That is what lets the same worker answer while a replay is
//! paused, while a live match is mid-turn, or before either has loaded.
//!
//! Nothing here computes anything. [`battle_forecast`] hands the request to
//! `awvm::calculator` and renames its fields for JavaScript; [`battle_catalog`]
//! reads the ruleset tables so the interface's pickers cannot offer a unit,
//! terrain or commander the rules do not have, and cannot disagree with the
//! ruleset about what a Mega Tank costs or how many stars a mountain is worth.

use awbrn_types::{
    BridgeType, Faction, GraphicalTerrain, MissileSiloStatus, PipeSeamType, PipeType,
    PlayerFaction, Property, PropertyKind, RiverType, RoadType, SeaDirection, ShoalDirection,
    UnitExt,
};
use awvm::calculator::{
    self, BattleRequest, CalculatorError, Fighter, FundsRange, NetFunds, SideContext, Unscorable,
};
use awvm::combat::{DamageRange, Weapon};
use awvm::commander::PowerLevel;
use awvm::ruleset::{self, CommanderKind, Domain, Terrain, UnitKind, WeatherKind};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

/// Damage at both ends of its luck, in percentage points of a whole unit.
///
/// Uncapped, exactly as the board's own forecast reports it: 101 and 160
/// against the same target are a bare kill and an overkill, and the difference
/// is the reason to send something cheaper at one of them.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct BattleBracket {
    pub low: u16,
    pub high: u16,
}

impl From<DamageRange> for BattleBracket {
    fn from(range: DamageRange) -> Self {
        Self {
            low: range.low,
            high: range.high,
        }
    }
}

/// Funds at both ends of the damage they were priced from.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct FundsBracket {
    pub low: u64,
    pub high: u64,
}

impl From<FundsRange> for FundsBracket {
    fn from(range: FundsRange) -> Self {
        Self {
            low: range.low,
            high: range.high,
        }
    }
}

/// What the exchange moves, from the attacking player's seat.
///
/// `low` is the attacker's worst case — its weakest roll against the strongest
/// reply — and `high` its best. The two ends are not independently reachable
/// and must not be recombined.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct NetFundsBracket {
    pub low: i64,
    pub high: i64,
}

impl From<NetFunds> for NetFundsBracket {
    fn from(net: NetFunds) -> Self {
        Self {
            low: net.low,
            high: net.high,
        }
    }
}

/// Everything one army brings to the exchange that is not the unit.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct BattleSide {
    /// `null` fights under the ruleset's neutral commander, which is what
    /// "no CO" means to the combat algebra.
    #[tsify(optional)]
    pub commander: Option<CommanderKind>,
    /// `null` is day-to-day.
    #[tsify(optional)]
    pub power: Option<PowerLevel>,
    pub funds: u64,
    /// Every capturable tile the army holds, com towers included.
    pub properties: u64,
    pub com_towers: u64,
}

/// One unit in the exchange, and the ground under it.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct BattleFighter {
    pub unit: UnitKind,
    /// Health in points on the 0-100 scale, not the 1-10 the board draws.
    pub health: u8,
    /// `null` takes the unit's full magazine.
    #[tsify(optional)]
    pub ammo: Option<u64>,
    pub terrain: Terrain,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct BattleRequestWire {
    pub weather: WeatherKind,
    pub attacker: BattleSide,
    pub attacking_unit: BattleFighter,
    pub defender: BattleSide,
    pub defending_units: Vec<BattleFighter>,
}

/// Why a pairing has no numbers.
///
/// Reported rather than hidden: a player who asked what an Anti-Air does to a
/// Battleship is owed the answer "nothing it holds can reach it", and a row
/// that silently vanished would look like the calculator had failed.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "kebab-case")]
pub enum BattleImpossible {
    /// Nothing this attacker holds has a damage entry against the target.
    NoWeapon,
    /// The attacker has no weapon at all.
    Unarmed,
}

impl From<Unscorable> for BattleImpossible {
    fn from(reason: Unscorable) -> Self {
        match reason {
            Unscorable::NoWeapon => Self::NoWeapon,
            Unscorable::Unarmed => Self::Unarmed,
        }
    }
}

/// One target, and what the exchange with it costs both sides.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct BattleRow {
    pub target: BattleFighter,
    pub name: String,
    /// Present exactly when `impossible` is absent.
    #[tsify(optional)]
    pub result: Option<BattleResult>,
    #[tsify(optional)]
    pub impossible: Option<BattleImpossible>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct BattleResult {
    /// Which weapon fires. A unit out of shells falls back to its secondary,
    /// and the drop in damage is otherwise unexplained.
    pub weapon: BattleWeapon,
    pub damage: BattleBracket,
    /// Absent when nothing replies at all, which is a different fact from a
    /// reply that lands nothing.
    #[tsify(optional)]
    pub counter: Option<BattleBracket>,
    pub counter_first: bool,
    pub destroys: bool,
    pub may_destroy: bool,
    pub value_dealt: FundsBracket,
    #[tsify(optional)]
    pub value_taken: Option<FundsBracket>,
    /// The reply again, one rung per health the target may be left standing
    /// in, so a reader can tell the health spread from the luck. Empty when
    /// there is no spread: a reply that lands first is scored at full health,
    /// and a target that never survives never answers.
    pub counter_steps: Vec<BattleCounterStep>,
    /// What the target is worth whole, at the health it is standing at.
    pub target_value: u64,
    pub net: NetFundsBracket,
}

/// What the target answers with from one of the healths it may be left in.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct BattleCounterStep {
    /// The health in points the target is left standing at, at the top of the
    /// bar the board would draw it in.
    pub target_health: u8,
    pub counter: BattleBracket,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "kebab-case")]
pub enum BattleWeapon {
    /// The magazine weapon: shells, missiles, torpedoes.
    Ammo,
    /// The weapon that never runs out: the machine gun, the bazooka.
    Unlimited,
}

impl From<Weapon> for BattleWeapon {
    fn from(weapon: Weapon) -> Self {
        match weapon {
            Weapon::Ammo => Self::Ammo,
            Weapon::Unlimited => Self::Unlimited,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct BattleReportWire {
    /// What the attacker is worth at the health it is fighting at.
    pub attacker_value: u64,
    pub rows: Vec<BattleRow>,
}

/// The calculator error category a caller can handle without parsing text.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "kebab-case")]
pub enum BattleCalculatorErrorKind {
    Health,
    Properties,
    ComTowers,
    Layout,
}

/// A calculator failure with a stable category and a readable explanation.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct BattleCalculatorError {
    pub kind: BattleCalculatorErrorKind,
    pub message: String,
}

impl From<CalculatorError> for BattleCalculatorError {
    fn from(error: CalculatorError) -> Self {
        let kind = match &error {
            CalculatorError::Health(_) => BattleCalculatorErrorKind::Health,
            CalculatorError::Properties(_) => BattleCalculatorErrorKind::Properties,
            CalculatorError::ComTowers { .. } => BattleCalculatorErrorKind::ComTowers,
            CalculatorError::Layout(_) => BattleCalculatorErrorKind::Layout,
        };
        Self {
            kind,
            message: error.to_string(),
        }
    }
}

/// Score one attacker against every target it is being weighed against.
///
/// Every number comes back from `awvm::calculator`, which lowers the request
/// into a state and puts it to the same reducer a real order goes through.
#[wasm_bindgen]
pub fn battle_forecast(
    request: BattleRequestWire,
) -> Result<BattleReportWire, BattleCalculatorError> {
    let report = calculator::forecast(&BattleRequest {
        weather: request.weather,
        attacker: side(request.attacker),
        attacking_unit: fighter(request.attacking_unit),
        defender: side(request.defender),
        defending_units: request
            .defending_units
            .iter()
            .copied()
            .map(fighter)
            .collect(),
    })
    .map_err(BattleCalculatorError::from)?;

    Ok(BattleReportWire {
        attacker_value: report.attacker_value,
        rows: report
            .outcomes
            .into_iter()
            .map(|outcome| BattleRow {
                target: BattleFighter {
                    unit: outcome.target.unit,
                    health: outcome.target.hp,
                    ammo: outcome.target.ammo,
                    terrain: outcome.target.terrain,
                },
                name: outcome.target.unit.name().to_string(),
                impossible: outcome.engagement.as_ref().err().copied().map(Into::into),
                result: outcome.engagement.ok().map(|engagement| BattleResult {
                    weapon: engagement.weapon.into(),
                    damage: engagement.damage.into(),
                    counter: engagement.counter.map(Into::into),
                    counter_first: engagement.counter_first,
                    destroys: engagement.destroys,
                    may_destroy: engagement.may_destroy,
                    value_dealt: engagement.value_dealt.into(),
                    value_taken: engagement.value_taken.map(Into::into),
                    counter_steps: engagement
                        .counter_steps
                        .into_iter()
                        .map(|step| BattleCounterStep {
                            target_health: step.target_hp,
                            counter: step.counter.into(),
                        })
                        .collect(),
                    target_value: engagement.target_value,
                    net: engagement.net.into(),
                }),
            })
            .collect(),
    })
}

fn side(wire: BattleSide) -> SideContext {
    SideContext {
        commander: wire.commander,
        power: wire.power,
        funds: wire.funds,
        properties: wire.properties,
        com_towers: wire.com_towers,
    }
}

fn fighter(wire: BattleFighter) -> Fighter {
    Fighter {
        unit: wire.unit,
        hp: wire.health,
        ammo: wire.ammo,
        terrain: wire.terrain,
    }
}

/// One unit kind, as a picker needs it.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct CatalogUnit {
    pub unit: UnitKind,
    pub name: String,
    pub cost: u64,
    pub domain: CatalogDomain,
    pub max_ammo: u64,
    /// Whether the unit fires from beyond one tile, which is also why it draws
    /// no reply.
    pub is_indirect: bool,
}

/// Where a unit travels, which is the only thing a picker needs the domain for:
/// it decides what ground to open the unit on.
///
/// Mirrored rather than re-exported because the ruleset's own domain type is
/// internal to the rules and carries no TypeScript declaration.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "kebab-case")]
pub enum CatalogDomain {
    Ground,
    Air,
    Sea,
}

impl From<Domain> for CatalogDomain {
    fn from(domain: Domain) -> Self {
        match domain {
            Domain::Ground => Self::Ground,
            Domain::Air => Self::Air,
            Domain::Sea => Self::Sea,
        }
    }
}

/// One terrain, with the defense it grants and the tile the board draws for it.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct CatalogTerrain {
    pub terrain: Terrain,
    pub name: String,
    pub stars: u8,
    /// The cell of the terrain sheet a picker draws for this ground, so the
    /// choice is made from the tile the player already knows off the map
    /// rather than from its name.
    pub sprite_index: u16,
}

/// One commander, keyed the way the portrait sheet keys its art.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct CatalogCommander {
    pub commander: CommanderKind,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct BattleCatalog {
    pub units: Vec<CatalogUnit>,
    pub terrains: Vec<CatalogTerrain>,
    pub commanders: Vec<CatalogCommander>,
}

/// Everything the calculator's pickers may offer, read off the ruleset.
///
/// It is read once and held, so a picker never asks the worker again. The
/// alternative was three hand-written tables in TypeScript that would silently
/// disagree with the ruleset the first time a cost changed.
#[wasm_bindgen]
pub fn battle_catalog() -> BattleCatalog {
    BattleCatalog {
        units: UnitKind::ALL
            .into_iter()
            .map(|unit| {
                let profile = ruleset::profile(unit);
                CatalogUnit {
                    unit,
                    name: unit.name().to_string(),
                    cost: profile.cost,
                    domain: profile.domain.into(),
                    max_ammo: profile.max_ammo,
                    is_indirect: profile.indirect_range.is_some(),
                }
            })
            .collect(),
        terrains: Terrain::ALL
            .into_iter()
            .filter(|terrain| stands_on(*terrain))
            .map(|terrain| CatalogTerrain {
                terrain,
                name: terrain_name(terrain).to_string(),
                stars: ruleset::defense_stars(terrain),
                sprite_index: awbrn_content::spritesheet_index(
                    WeatherKind::Clear,
                    tile_art(terrain),
                )
                .index(),
            })
            .collect(),
        commanders: CommanderKind::ALL
            .into_iter()
            // The neutral commander is what an empty picker already means, so
            // offering it by name would be the same choice written twice.
            .filter(|commander| *commander != CommanderKind::Neutral)
            .map(|commander| CatalogCommander {
                commander,
                name: commander_name(commander).to_string(),
            })
            .collect(),
    }
}

/// Whether a unit can be standing on this terrain when it is shot at.
///
/// A pipe and its seam are scenery, and a teleporter is a doorway. A combatant
/// does not occupy these terrain types, so their defense values do not apply.
fn stands_on(terrain: Terrain) -> bool {
    !matches!(
        terrain,
        Terrain::Pipe | Terrain::PipeSeam | Terrain::Teleporter
    )
}

/// The ruleset spells terrain in its own identifiers; a player reads names.
///
/// Properties borrow the names the map editor already uses for them, so the
/// picker and the board agree on what a building is called.
fn terrain_name(terrain: Terrain) -> &'static str {
    match terrain {
        Terrain::Airport => PropertyKind::Airport.name(),
        Terrain::Base => PropertyKind::Base.name(),
        Terrain::City => PropertyKind::City.name(),
        Terrain::ComTower => PropertyKind::ComTower.name(),
        Terrain::Hq => PropertyKind::HQ.name(),
        Terrain::Lab => PropertyKind::Lab.name(),
        Terrain::Port => PropertyKind::Port.name(),
        Terrain::Bridge => "Bridge",
        Terrain::MissileSilo => "Missile Silo",
        Terrain::Mountain => "Mountain",
        Terrain::Pipe => "Pipe",
        Terrain::PipeSeam => "Pipe Seam",
        Terrain::Plain => "Plain",
        Terrain::Reef => "Reef",
        Terrain::River => "River",
        Terrain::Road => "Road",
        Terrain::Sea => "Sea",
        Terrain::Shoal => "Shoal",
        Terrain::Teleporter => "Teleporter",
        Terrain::Wood => "Wood",
    }
}

/// The tile the picker draws for one terrain kind.
///
/// Most terrain the board draws is shaped by its neighbours: a road bends, a
/// river forks, a sea tile takes its coastline from four directions at once. A
/// picker has no neighbours, so each kind is shown in its standalone form —
/// the straight run, the open water, the unowned building. It is the same art
/// the map uses, which is the point: a player picks the ground by recognising
/// it rather than by reading its name.
///
/// A property is drawn unowned. The army holding a building changes only its
/// colours, and the defense it grants is the same for whoever stands on it.
fn tile_art(terrain: Terrain) -> GraphicalTerrain {
    match terrain {
        Terrain::Airport => GraphicalTerrain::Property(Property::Airport(Faction::Neutral)),
        Terrain::Base => GraphicalTerrain::Property(Property::Base(Faction::Neutral)),
        Terrain::Bridge => GraphicalTerrain::Bridge(BridgeType::Horizontal),
        Terrain::City => GraphicalTerrain::Property(Property::City(Faction::Neutral)),
        Terrain::ComTower => GraphicalTerrain::Property(Property::ComTower(Faction::Neutral)),
        // An HQ is the one building that is never unowned, so the picker shows
        // the first army's, the way an empty map does.
        Terrain::Hq => GraphicalTerrain::Property(Property::HQ(PlayerFaction::OrangeStar)),
        Terrain::Lab => GraphicalTerrain::Property(Property::Lab(Faction::Neutral)),
        // A silo that has fired is a different tile with the same defense. The
        // loaded one is what a player is picturing when they pick it.
        Terrain::MissileSilo => GraphicalTerrain::MissileSilo(MissileSiloStatus::Loaded),
        Terrain::Mountain => GraphicalTerrain::Mountain,
        Terrain::Pipe => GraphicalTerrain::Pipe(PipeType::Horizontal),
        Terrain::PipeSeam => GraphicalTerrain::PipeSeam(PipeSeamType::Horizontal),
        Terrain::Plain => GraphicalTerrain::Plain,
        Terrain::Port => GraphicalTerrain::Property(Property::Port(Faction::Neutral)),
        Terrain::Reef => GraphicalTerrain::Reef,
        Terrain::River => GraphicalTerrain::River(RiverType::Horizontal),
        Terrain::Road => GraphicalTerrain::Road(RoadType::Horizontal),
        Terrain::Sea => GraphicalTerrain::Sea(SeaDirection::Sea),
        Terrain::Shoal => GraphicalTerrain::Shoal(ShoalDirection::S),
        Terrain::Teleporter => GraphicalTerrain::Teleporter,
        Terrain::Wood => GraphicalTerrain::Wood,
    }
}

/// Commander display names, taken from the same table that names the portraits.
///
/// Keeping one source means the picker cannot say "Von Bolt" beside a portrait
/// labelled "von-bolt".
fn commander_name(commander: CommanderKind) -> &'static str {
    awbrn_content::co_portraits()
        .iter()
        .find(|portrait| portrait.key() == commander.as_str())
        .map_or_else(|| commander.as_str(), |portrait| portrait.display_name())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculator_errors_keep_their_kind_and_message() {
        let errors = [
            (
                CalculatorError::Health(0),
                BattleCalculatorErrorKind::Health,
            ),
            (
                CalculatorError::Properties(201),
                BattleCalculatorErrorKind::Properties,
            ),
            (
                CalculatorError::ComTowers {
                    towers: 2,
                    properties: 1,
                },
                BattleCalculatorErrorKind::ComTowers,
            ),
            (
                CalculatorError::Layout("missing unit".into()),
                BattleCalculatorErrorKind::Layout,
            ),
        ];

        for (error, kind) in errors {
            let message = error.to_string();
            let wire = BattleCalculatorError::from(error);
            assert_eq!(wire.kind, kind);
            assert_eq!(wire.message, message);
        }
    }

    #[test]
    fn every_commander_has_a_display_name() {
        for commander in CommanderKind::ALL {
            if commander == CommanderKind::Neutral {
                continue;
            }
            assert_ne!(
                commander_name(commander),
                commander.as_str(),
                "{commander} has no display name"
            );
        }
    }

    #[test]
    fn terrain_picker_excludes_scenery() {
        assert!(!stands_on(Terrain::Pipe));
        assert!(!stands_on(Terrain::PipeSeam));
        assert!(!stands_on(Terrain::Teleporter));
        assert!(stands_on(Terrain::Plain));
    }
}
