pub mod camera;
pub mod event_bus;
pub mod fog;
pub mod input;
pub mod weather;

pub use awbrn_game::world::{CurrentWeather, FogActive, FogOfWarMap, FriendlyFactions};
pub use camera::CameraScale;
pub use event_bus::{EventBus, EventBusResource, ExternalEvent, ExternalGameEvent, GameEvent};
pub use input::{SelectedTile, TileCursor};

use bevy::prelude::*;

pub struct FeaturesPlugin;

impl Plugin for FeaturesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            weather::WeatherPlugin,
            camera::CameraPlugin,
            input::InputPlugin,
            fog::FogPlugin,
        ));
        app.add_message::<ExternalGameEvent>();
    }
}
