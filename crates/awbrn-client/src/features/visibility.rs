pub use awbrn_game::world::{FriendlyFactions, ViewerVisibility};
use bevy::prelude::*;

pub struct VisibilityPlugin;

impl Plugin for VisibilityPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ViewerVisibility>()
            .init_resource::<FriendlyFactions>();
    }
}
