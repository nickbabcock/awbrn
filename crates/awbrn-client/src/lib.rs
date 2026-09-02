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
    AttackPreviewChanged, DeleteUnitCommandRequested, EndTurnRequested, EventSink,
    HoveredCargoUnit, HoveredTile, HoveredUnit, InspectedUnitReadout, MapDimensions,
    MoveCommandRequested, NewDay, PlayerRosterEntry, PlayerRosterSnapshot, PlayerRosterStats,
    PostMoveAction, ProductionOption, ProductionOptionsChanged, ProductionSite, ReplayLoaded,
    ReplayLoadedPlayer, ScreenPoint, TileHoverChanged, TileSelected, TurnReadinessChanged,
    UnitActionOption, UnitActionsChanged, UnitBuilt, UnitInspectionChanged, UnitMoved, UnitOrder,
    UnloadCommandRequested,
};
pub use json_plugin::*;
pub use loading::{
    LiveMatchPlayer, MapAssetPathResolver, PendingGameStart, PendingLiveMatch,
    PendingLiveTransitions, PendingMatchMap, ReplayToLoad, StaticAssetPathResolver,
};
pub use ui_atlas::*;
