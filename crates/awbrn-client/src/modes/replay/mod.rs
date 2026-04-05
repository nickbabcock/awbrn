pub mod bootstrap;
pub mod commands;
pub(crate) mod controls;
pub mod fog;
pub mod navigation;
pub(crate) mod state;

use crate::core::{AppState, GameMode};
use awbrn_game::replay::{
    ReplayViewpoint, sync_viewpoint, trigger_fog_recompute_on_weather_change,
};
use awbrn_game::world::CurrentWeather;
use bevy::prelude::*;

pub struct ReplayPlugin;

impl Plugin for ReplayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<commands::ReplayAdvanceLock>()
            .add_plugins(navigation::NavigationPlugin)
            .add_observer(fog::on_replay_fog_dirty)
            .add_observer(commands::on_carried_by_add)
            .add_observer(commands::on_carried_by_remove)
            .add_observer(commands::on_new_day)
            .add_systems(
                Update,
                controls::handle_replay_controls
                    .run_if(in_state(GameMode::Replay).and(in_state(AppState::InGame))),
            )
            .add_systems(
                Update,
                sync_viewpoint
                    .run_if(resource_changed::<ReplayViewpoint>)
                    .run_if(in_state(GameMode::Replay).and(in_state(AppState::InGame))),
            )
            .add_systems(
                Update,
                trigger_fog_recompute_on_weather_change
                    .run_if(resource_changed::<CurrentWeather>)
                    .run_if(in_state(GameMode::Replay).and(in_state(AppState::InGame))),
            );
    }
}
