pub mod camera;
pub mod event_bus;
pub mod fog;
pub mod input;
pub mod navigation;
pub mod weather;

pub use camera::CameraScale;
pub use event_bus::{EventBus, EventBusResource, ExternalEvent, ExternalGameEvent, GameEvent};
pub use fog::{FogActive, FogOfWarMap, FriendlyFactions};
pub use input::{SelectedTile, TileCursor};
pub use weather::CurrentWeather;

use bevy::prelude::*;

pub struct FeaturesPlugin;

impl Plugin for FeaturesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            navigation::NavigationPlugin,
            weather::WeatherPlugin,
            camera::CameraPlugin,
            input::InputPlugin,
            fog::FogPlugin,
        ));
        app.add_message::<ExternalGameEvent>();
    }
}
