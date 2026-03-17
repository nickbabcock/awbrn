pub mod bootstrap;
pub mod commands;
pub(crate) mod controls;
pub(crate) mod state;

use crate::core::{AppState, GameMode, StrongIdMap};
use crate::snapshot::ReplaySnapshotEntity;
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;

pub use state::ReplayState;

#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[component(immutable, on_add = on_awbw_unit_id_add, on_remove = on_awbw_unit_id_remove)]
#[reflect(Component)]
#[require(ReplaySnapshotEntity)]
pub struct AwbwUnitId(pub awbrn_core::AwbwUnitId);

fn on_awbw_unit_id_add(mut world: DeferredWorld, context: HookContext) {
    let unit_id = *world.get::<AwbwUnitId>(context.entity).unwrap();
    world
        .resource_mut::<StrongIdMap<AwbwUnitId>>()
        .insert(unit_id, context.entity);
}

fn on_awbw_unit_id_remove(mut world: DeferredWorld, context: HookContext) {
    let unit_id = *world.get::<AwbwUnitId>(context.entity).unwrap();
    world
        .resource_mut::<StrongIdMap<AwbwUnitId>>()
        .remove(unit_id);
}

pub struct ReplayPlugin;

impl Plugin for ReplayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StrongIdMap<AwbwUnitId>>()
            .init_resource::<commands::ReplayAdvanceLock>()
            .add_plugins(crate::snapshot::ReplaySnapshotPlugin)
            .register_type::<AwbwUnitId>()
            .add_systems(
                Update,
                controls::handle_replay_controls
                    .run_if(in_state(GameMode::Replay).and(in_state(AppState::InGame))),
            );
    }
}
