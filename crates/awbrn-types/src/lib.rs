mod co;
mod hp;
mod terrain;
mod units;
mod weather;

// The board vocabulary awbrn draws with. AWBW's wire ids are not part of it:
// code that reads AWBW records imports them from `awbw_types` directly.
pub use awbw_types::{
    AwbwTerrain, BridgeType, Faction, FactionCode, MissileSiloStatus, PLAYER_FACTION_METADATA,
    PipeRubbleType, PipeSeamType, PipeType, PlayerFaction, PlayerFactionMetadata, Property,
    PropertyKind, RiverType, RoadType, ShoalType,
};
pub use co::*;
pub use hp::*;
pub use terrain::*;
pub use units::*;
pub use weather::*;
