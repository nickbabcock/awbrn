use crate::features::camera::{CameraScale, compute_map_dimensions};
use awbrn_game::world::GameMap;
use awbrn_map::Position;
pub use awbrn_protocol::PostMoveAction;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// A resource that receives events of type T.
///
/// Its presence in the world signals that someone is listening for T events.
/// Systems dedicated solely to emitting T can use
/// `run_if(resource_exists::<EventSink<T>>)` to skip work when no listener
/// is registered.
#[derive(Resource)]
pub struct EventSink<T: Send + Sync + 'static>(Arc<dyn Fn(T) + Send + Sync + 'static>);

impl<T: Send + Sync + 'static> EventSink<T> {
    pub fn new(f: impl Fn(T) + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }

    pub fn emit(&self, payload: T) {
        (self.0)(payload);
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct NewDay {
    pub day: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct UnitMoved {
    pub unit_id: u32,
    pub from_x: usize,
    pub from_y: usize,
    pub to_x: usize,
    pub to_y: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct UnitBuilt {
    pub unit_id: u32,
    pub unit_type: String,
    pub x: usize,
    pub y: usize,
    pub player_id: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct TileSelected {
    pub x: usize,
    pub y: usize,
    pub terrain_type: String,
}

/// A unit carried by the unit on the hovered tile.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct HoveredCargoUnit {
    pub unit: awvm::ruleset::UnitKind,
    pub name: String,
    pub faction_code: String,
    pub health: Option<u8>,
    pub ammo: Option<u32>,
    pub max_ammo: u32,
    pub fuel: Option<u32>,
    pub max_fuel: u32,
}

/// How the ammunition of a unit reads on the tile readout.
///
/// The three cases say different things and must not collapse into one number:
/// a transport carries no weapon, an infantry rifle never runs out, and every
/// other weapon spends rounds that can run out.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub enum AmmoDisplay {
    /// The unit has no weapon, so the readout shows no ammunition at all.
    None,
    /// Every weapon of this unit fires without ammunition.
    Unlimited,
    /// The unit spends ammunition: `ammo` and `max_ammo` apply.
    Counted,
}

impl AmmoDisplay {
    /// What the ruleset says about the weapons of one unit kind.
    pub fn for_unit(unit: awvm::ruleset::UnitKind) -> Self {
        match awvm::ruleset::profile(unit).weapon_policy {
            awvm::ruleset::WeaponPolicy::None => Self::None,
            awvm::ruleset::WeaponPolicy::Unlimited => Self::Unlimited,
            awvm::ruleset::WeaponPolicy::Ammo | awvm::ruleset::WeaponPolicy::AmmoWithUnlimited => {
                Self::Counted
            }
        }
    }
}

/// The visible unit on the hovered tile.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct HoveredUnit {
    pub unit: awvm::ruleset::UnitKind,
    pub name: String,
    pub faction_code: String,
    pub health: Option<u8>,
    /// `None` when the unit carries the resource but its amount is unknown.
    pub ammo: Option<u32>,
    pub max_ammo: u32,
    pub ammo_display: AmmoDisplay,
    pub fuel: Option<u32>,
    pub max_fuel: u32,
    pub loaded_units: Vec<HoveredCargoUnit>,
}

/// Information the presentation can show for one hovered board tile.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct HoveredTile {
    pub x: usize,
    pub y: usize,
    /// The kind of terrain alone: `Shoal`, `HQ`. The tile art carries the shape
    /// it is drawn in and the colour of the army that holds it.
    pub terrain_name: String,
    /// The army holding this tile, for tiles an army can hold. The readout
    /// shows the owner in the sprite rather than in the name, so this is what
    /// its accessible description says instead.
    pub terrain_owner: Option<String>,
    /// Capture points this property still owes before it changes hands, when a
    /// visible unit is taking it. `None` when nothing is being captured here.
    pub capture_remaining: Option<u8>,
    pub terrain_sprite_index: u16,
    pub defense_stars: u8,
    pub unit: Option<HoveredUnit>,
}

/// The current hovered tile. `tile: None` clears the presentation readout.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct TileHoverChanged {
    pub tile: Option<HoveredTile>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct ProductionSite {
    pub x: usize,
    pub y: usize,
    pub facility: awvm::ruleset::Terrain,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct ProductionOption {
    pub unit: awvm::ruleset::UnitKind,
    pub name: String,
    pub cost: u32,
    /// Whether the player's funds reach `cost`. An unaffordable unit is still
    /// listed, priced, and struck through, the way the source game lists it.
    pub affordable: bool,
}

/// The production menu implied by the current board selection.
/// `site: None` tells presentation clients to close any open menu.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct ProductionOptionsChanged {
    pub site: Option<ProductionSite>,
    pub options: Vec<ProductionOption>,
}

/// One order on the destination menu.
///
/// Every entry here was accepted by the AWVM reducer against the recipient's
/// own observation. The interface never decides what a unit may do.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct UnitActionOption {
    /// The label the source game uses for this order.
    pub name: String,
    pub action: UnitOrder,
    /// What firing this order would cost both sides. Present only on attacks,
    /// and absent even there when AWVM could not answer — a row with no number
    /// is the honest fallback, and a wrong number is not.
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub forecast: Option<AttackForecast>,
}

impl UnitActionOption {
    /// An order with nothing to forecast, which is every order but an attack.
    pub fn plain(name: impl Into<String>, action: UnitOrder) -> Self {
        Self {
            name: name.into(),
            action,
            forecast: None,
        }
    }
}

/// The damage one strike lands, from its unluckiest roll to its luckiest, in
/// percentage points of a unit at full health.
///
/// Percentages rather than HP because that is the number AWVM's combat
/// arithmetic works in and the vocabulary AWBW's own calculator uses. The
/// figure is not limited by what the target still has, for the same reason
/// AWBW does not limit it: 101 and 160 against the same unit are a bare kill
/// and an overkill, and a player picks a different attacker for each. `low`
/// equals `high` whenever no commander in the exchange grants luck.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct DamageBracket {
    pub low: u16,
    pub high: u16,
}

/// What one attack would cost both sides, before any dice.
///
/// The two brackets are not independent: a counter is scored from what the
/// strike left standing, so `damage.high` pairs with `counter.low` and
/// `damage.low` pairs with `counter.high`. Presentation must read them that
/// way rather than as two separate rolls.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct AttackForecast {
    pub target: ForecastTarget,
    pub damage: DamageBracket,
    /// What comes back, when anything can. `None` is not a counter of zero:
    /// no reply and a reply that happens to do nothing are different facts and
    /// a player acts differently on each.
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub counter: Option<DamageBracket>,
    /// Whether the defender's commander answers before the shot that provoked
    /// it, which is what makes this attacker's own damage depend on the reply.
    pub counter_first: bool,
    /// Whether even the weakest roll finishes the target.
    pub destroys: bool,
    /// Whether the strongest roll finishes it, when the weakest does not.
    pub may_destroy: bool,
}

/// A unit as the menu draws and names it.
///
/// Enough to put a sprite, an army and a health beside a number, and no more:
/// this is identity for a readout, not the tile inspector.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct UnitBadge {
    pub unit: awvm::ruleset::UnitKind,
    pub name: String,
    pub faction_code: String,
    /// Health as the game shows it, or `None` when it is hidden.
    pub health: Option<u8>,
}

/// What an attack is aimed at, as the order names it.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ForecastTarget {
    Unit {
        unit: awvm::ruleset::UnitKind,
        name: String,
        faction_code: String,
        /// Health as the game shows it, or `None` when it is hidden.
        health: Option<u8>,
    },
    /// A destructible tile: a pipe seam. It has no army and does not answer.
    Tile { name: String },
}

/// The command represented by one entry in the unit order menu.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum UnitOrder {
    Move {
        action: PostMoveAction,
    },
    Unload {
        cargo_id: u32,
        #[cfg_attr(target_family = "wasm", tsify(type = "{ x: number; y: number }"))]
        position: Position,
    },
    Delete,
}

/// The destination menu implied by the current proposal.
/// `destination: None` tells presentation clients to close any open menu.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct UnitActionsChanged {
    #[cfg_attr(
        target_family = "wasm",
        tsify(optional, type = "{ x: number; y: number }")
    )]
    pub destination: Option<Position>,
    pub options: Vec<UnitActionOption>,
    /// Which order to highlight first. A drag released on an enemy is explicit
    /// attack intent, and the menu opens saying so.
    pub preselected: Option<usize>,
    /// The unit being commanded. It is the same for every order, so a menu that
    /// forecasts names it once at the head rather than on each row.
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub attacker: Option<UnitBadge>,
}

/// An atomic move-and-act intent chosen on the live board. The browser forwards
/// this as the server's `moveUnit` command; the outcome remains entirely
/// authoritative on the server.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct MoveCommandRequested {
    pub unit_id: u32,
    #[cfg_attr(target_family = "wasm", tsify(type = "{ x: number; y: number }[]"))]
    pub path: Vec<Position>,
    pub action: PostMoveAction,
}

/// A standalone free-unload intent chosen on the live board.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct UnloadCommandRequested {
    pub transport_id: u32,
    pub cargo_id: u32,
    #[cfg_attr(target_family = "wasm", tsify(type = "{ x: number; y: number }"))]
    pub position: Position,
}

/// A voluntary unit-removal intent chosen on the live board.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct DeleteUnitCommandRequested {
    pub unit_id: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct MapDimensions {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct ReplayLoadedPlayer {
    pub player_id: u32,
    pub user_id: u32,
    pub order: u32,
    pub team: Option<String>,
    pub eliminated: bool,
    pub faction_code: String,
    pub faction_name: String,
    pub co_key: Option<String>,
    pub co_name: Option<String>,
    pub tag_co_key: Option<String>,
    pub tag_co_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct ReplayLoaded {
    pub game_id: u32,
    pub map_id: u32,
    pub day: u32,
    pub fog: bool,
    pub team_game: bool,
    pub players: Vec<ReplayLoadedPlayer>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct PlayerRosterStats {
    pub funds: Option<u32>,
    pub unit_count: Option<u32>,
    pub unit_value: Option<u32>,
    pub income: Option<u32>,
    /// Every capturable tile the army holds, com towers included. It is the
    /// figure `income` is derived from, reported alongside it because a
    /// commander who reads the property count reads this and not the money.
    pub properties: Option<u32>,
    /// How many of those properties are com towers, which is the one property
    /// kind that changes what a unit deals and takes.
    pub com_towers: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct PlayerRosterEntry {
    pub player_id: u32,
    pub user_id: u32,
    pub turn_order: u32,
    pub team: Option<String>,
    pub eliminated: bool,
    pub actual_faction_code: String,
    pub actual_faction_name: String,
    pub display_faction_code: String,
    pub display_faction_name: String,
    pub faction_code: String,
    pub faction_name: String,
    pub co_key: Option<String>,
    pub co_name: Option<String>,
    pub tag_co_key: Option<String>,
    pub tag_co_name: Option<String>,
    /// Public CO power charge. `None` when no observation has reported one.
    pub power_charge: Option<u32>,
    /// Current charge required to activate this CO's normal power.
    pub cop_cost: Option<u32>,
    /// Current charge required to activate this CO's super power.
    pub scop_cost: Option<u32>,
    /// Charge one power star is worth, so a meter can be drawn in segments.
    pub power_star_charge: Option<u32>,
    /// The power that this player has active.
    pub active_power: Option<awvm::commander::PowerLevel>,
    pub stats: PlayerRosterStats,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct PlayerRosterSnapshot {
    pub match_id: u32,
    pub map_id: u32,
    pub day: u32,
    pub active_player_id: Option<u32>,
    pub players: Vec<PlayerRosterEntry>,
}

pub(crate) fn emit_map_dimensions(
    game_map: Res<GameMap>,
    camera_scale: Res<CameraScale>,
    sink: Res<EventSink<MapDimensions>>,
) {
    sink.emit(compute_map_dimensions(&game_map, &camera_scale));
}
