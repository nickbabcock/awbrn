mod awbrn_plugin;
pub mod core;
pub mod features;
mod json_plugin;
pub mod loading;
pub mod modes;
pub mod projection;
pub mod render;
pub mod replay_archive;
mod ui_atlas;

pub use awbrn_plugin::AwbrnPlugin;
pub use features::event_bus::{
    AttackPreviewChanged, DeleteUnitCommandRequested, EventSink, HoveredCargoUnit, HoveredTile,
    HoveredUnit, MapDimensions, MoveCommandRequested, NewDay, PlayerRosterEntry,
    PlayerRosterSnapshot, PlayerRosterStats, PostMoveAction, ProductionOption,
    ProductionOptionsChanged, ProductionSite, ReplayLoaded, ReplayLoadedPlayer, TileHoverChanged,
    TileSelected, UnitActionOption, UnitActionsChanged, UnitBuilt, UnitMoved, UnitOrder,
    UnloadCommandRequested,
};
pub use json_plugin::*;
pub use loading::{
    LiveMatchPlayer, MapAssetPathResolver, PendingGameStart, PendingLiveMatch,
    PendingLiveTransitions, PendingMatchMap, ReplayToLoad, StaticAssetPathResolver,
};
pub use ui_atlas::*;
