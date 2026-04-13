pub(crate) mod board_index;
pub(crate) mod id_index;
pub(crate) mod map;
pub(crate) mod units;
pub(crate) mod weather;

pub mod fog;

pub use board_index::{BoardIndex, BoardIndexError};
pub use fog::{
    FogActive, FogOfWarMap, FogOfWarState, FriendlyFactions, FriendlyUnit, TerrainFogProperties,
    collect_friendly_units, range_modifier_for_weather, rebuild_fog_map,
};
pub use id_index::StrongIdMap;
pub use map::{GameMap, TerrainHp, TerrainTile, initialize_terrain_semantic_world};
pub use units::{
    Ammo, Capturing, Cargo, CarriedBy, Faction, Fuel, GraphicalHp, HasCargo, Hiding, Unit,
    UnitActive, UnitDestroyed, UnitHp, VisionRange,
};
pub use weather::CurrentWeather;
