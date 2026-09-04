pub mod bootstrap;
pub mod controls;
pub mod navigation;
pub mod presentation;
pub(crate) mod state;
pub mod timeline;

use crate::core::{AppState, GameMode};
use crate::features::event_bus::{EventSink, ReplayPositionChanged, ReplayViewpointChanged};
use awbrn_bevy::replay::{ReplayViewpoint, refresh_viewer_visibility};
use bevy::prelude::*;

#[derive(Debug)]
pub struct ReplayPlugin;

impl Plugin for ReplayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<presentation::ReplayAdvanceLock>()
            .init_resource::<timeline::ReplayOutline>()
            .add_message::<timeline::ReplayNavigate>()
            .add_plugins(navigation::NavigationPlugin)
            .add_observer(presentation::on_carried_by_add)
            .add_observer(presentation::on_carried_by_remove)
            .add_observer(presentation::on_new_day)
            .add_systems(
                Update,
                (
                    controls::handle_replay_controls,
                    timeline::handle_replay_navigation,
                )
                    .chain()
                    .run_if(in_state(GameMode::Replay).and_then(in_state(AppState::InGame))),
            )
            // The controls redraw from the position, so the position is
            // reported after everything that could have moved it.
            .add_systems(
                Update,
                timeline::emit_replay_position
                    .run_if(resource_exists::<EventSink<ReplayPositionChanged>>)
                    .run_if(
                        resource_exists_and_changed::<awbrn_bevy::replay::ReplayState>
                            .or_else(resource_exists_and_changed::<timeline::ReplayOutline>),
                    )
                    .run_if(in_state(GameMode::Replay).and_then(in_state(AppState::InGame))),
            )
            // Switching viewpoint re-selects a recipient projection the ECS
            // was already reconciled from; nothing recomputes vision.
            .add_systems(
                Update,
                refresh_viewer_visibility
                    .run_if(resource_changed::<ReplayViewpoint>)
                    .run_if(in_state(GameMode::Replay).and_then(in_state(AppState::InGame))),
            )
            // A followed viewpoint changes seat when the turn does, so the
            // report is owed to a new turn as much as to a new viewpoint.
            .add_systems(
                Update,
                controls::emit_replay_viewpoint
                    .run_if(resource_exists::<EventSink<ReplayViewpointChanged>>)
                    .run_if(
                        resource_changed::<ReplayViewpoint>.or_else(
                            resource_exists_and_changed::<awbrn_bevy::replay::ReplayState>,
                        ),
                    )
                    .run_if(in_state(GameMode::Replay).and_then(in_state(AppState::InGame))),
            );
    }
}
