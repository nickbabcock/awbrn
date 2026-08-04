mod awbrn_plugin;
pub mod core;
pub mod features;
mod json_plugin;
pub mod loading;
pub mod modes;
pub mod projection;
pub mod render;
mod ui_atlas;

pub use awbrn_plugin::AwbrnPlugin;
pub use features::event_bus::{
    EventSink, MapDimensions, NewDay, PlayerRosterEntry, PlayerRosterSnapshot, PlayerRosterStats,
    ProductionOption, ProductionOptionsChanged, ProductionSite, ReplayLoaded, ReplayLoadedPlayer,
    TileSelected, UnitBuilt, UnitMoved,
};
pub use json_plugin::*;
pub use loading::{
    LiveMatchPlayer, MapAssetPathResolver, PendingGameStart, PendingLiveMatch,
    PendingLiveTransitions, PendingMatchMap, ReplayToLoad, StaticAssetPathResolver,
};
pub use ui_atlas::*;
