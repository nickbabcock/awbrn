pub(crate) mod board_index;
pub(crate) mod capture;
pub(crate) mod id_index;
pub(crate) mod map;
pub(crate) mod units;
pub(crate) mod weather;

pub mod visibility;

pub use board_index::{BoardIndex, BoardIndexError};
pub use capture::{
    CaptureAction, CaptureActionError, CaptureActionOutcome, CaptureProgressInput,
    capture_property_at, captured_terrain,
};
pub use id_index::StrongIdMap;
pub use map::{GameMap, TerrainHp, TerrainTile, initialize_terrain_semantic_world};
pub use units::{
    Ammo, CaptureProgress, CaptureResolution, Cargo, CarriedBy, Faction, Fuel, GraphicalHp,
    HasCargo, Hiding, Unit, UnitActive, UnitDestroyed, UnitHp, VisionRange,
};
pub use visibility::{FriendlyFactions, ViewerVisibility};
pub use weather::CurrentWeather;
