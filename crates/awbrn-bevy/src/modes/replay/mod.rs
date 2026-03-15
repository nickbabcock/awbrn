pub mod commands;
pub(crate) mod controls;
pub(crate) mod state;

use crate::core::{AppState, GameMode, StrongIdMap};
use bevy::{log, prelude::*};

#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AwbwUnitId(pub awbrn_core::AwbwUnitId);

/// Observer that triggers when AwbwUnitId is inserted - adds to the replay-specific entity index
fn on_awbw_unit_id_insert(
    trigger: On<Insert, AwbwUnitId>,
    mut map: ResMut<StrongIdMap<AwbwUnitId>>,
    query: Query<&AwbwUnitId>,
) {
    let entity = trigger.entity;
    let Ok(unit_id) = query.get(entity) else {
        warn!("AwbwUnitId component not found for entity {:?}", entity);
        return;
    };

    log::info!("Indexing unit {:?} at entity {:?}", unit_id, entity);

    map.insert(*unit_id, entity);
}

/// Observer that triggers when AwbwUnitId is removed - cleans up the replay-specific entity index
fn on_awbw_unit_id_remove(
    trigger: On<Remove, AwbwUnitId>,
    mut map: ResMut<StrongIdMap<AwbwUnitId>>,
    query: Query<&AwbwUnitId>,
) {
    let entity = trigger.entity;
    let Ok(unit_id) = query.get(entity) else {
        warn!("AwbwUnitId component not found for entity {:?}", entity);
        return;
    };
    map.remove(*unit_id);
}

pub struct ReplayPlugin;

impl Plugin for ReplayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StrongIdMap<AwbwUnitId>>()
            .init_resource::<commands::ReplayAdvanceLock>()
            .register_type::<AwbwUnitId>()
            .add_observer(on_awbw_unit_id_insert)
            .add_observer(on_awbw_unit_id_remove)
            .add_systems(
                Update,
                controls::handle_replay_controls
                    .run_if(in_state(GameMode::Replay).and(in_state(AppState::InGame))),
            );
    }
}
