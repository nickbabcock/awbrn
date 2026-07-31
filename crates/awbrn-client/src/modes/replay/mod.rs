pub mod bootstrap;
pub(crate) mod controls;
pub mod navigation;
pub mod presentation;
pub(crate) mod state;

use crate::core::{AppState, GameMode};
use awbrn_game::replay::{ReplayViewpoint, refresh_viewer_visibility};
use bevy::prelude::*;

pub struct ReplayPlugin;

impl Plugin for ReplayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<presentation::ReplayAdvanceLock>()
            .add_plugins(navigation::NavigationPlugin)
            .add_observer(presentation::on_carried_by_add)
            .add_observer(presentation::on_carried_by_remove)
            .add_observer(presentation::on_new_day)
            .add_systems(
                Update,
                controls::handle_replay_controls
                    .run_if(in_state(GameMode::Replay).and_then(in_state(AppState::InGame))),
            )
            // Switching viewpoint re-selects a recipient projection the ECS
            // was already reconciled from; nothing recomputes vision.
            .add_systems(
                Update,
                refresh_viewer_visibility
                    .run_if(resource_changed::<ReplayViewpoint>)
                    .run_if(in_state(GameMode::Replay).and_then(in_state(AppState::InGame))),
            );
    }
}
