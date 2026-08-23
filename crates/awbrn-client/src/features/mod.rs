pub mod camera;
pub mod event_bus;
pub mod input;
pub mod player_display;
pub mod player_roster;
pub mod visibility;
pub mod weather;

pub use awbrn_bevy::world::{CurrentWeather, FriendlyFactions, ViewerVisibility};
pub use camera::CameraScale;
pub use event_bus::{
    DeleteUnitCommandRequested, EventSink, HoveredCargoUnit, HoveredTile, HoveredUnit,
    MapDimensions, MoveCommandRequested, NewDay, PlayerRosterEntry, PlayerRosterSnapshot,
    PlayerRosterStats, PostMoveAction, ProductionOption, ProductionOptionsChanged, ProductionSite,
    ReplayLoaded, ReplayLoadedPlayer, TileHoverChanged, TileSelected, UnitActionOption,
    UnitActionsChanged, UnitBuilt, UnitMoved, UnitOrder, UnloadCommandRequested,
};
pub use input::{SelectedTile, TileCursor};

use bevy::prelude::*;

pub struct FeaturesPlugin;

impl Plugin for FeaturesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            weather::WeatherPlugin,
            camera::CameraPlugin,
            input::InputPlugin,
            visibility::VisibilityPlugin,
            player_display::PlayerDisplayPlugin,
        ));
    }
}
