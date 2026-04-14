use awbrn_game::world::StrongIdMap;
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;

/// Unique identifier for a unit within a server game.
/// Assigned by a monotonic counter in [`crate::ServerGameState`].
#[derive(
    Component,
    Reflect,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(transparent)]
#[component(immutable, on_add = on_server_unit_id_add, on_remove = on_server_unit_id_remove)]
#[reflect(Component)]
pub struct ServerUnitId(pub u64);

fn on_server_unit_id_add(mut world: DeferredWorld, context: HookContext) {
    let unit_id = *world.get::<ServerUnitId>(context.entity).unwrap();
    world
        .resource_mut::<StrongIdMap<ServerUnitId>>()
        .insert(unit_id, context.entity);
}

fn on_server_unit_id_remove(mut world: DeferredWorld, context: HookContext) {
    let unit_id = *world.get::<ServerUnitId>(context.entity).unwrap();
    world
        .resource_mut::<StrongIdMap<ServerUnitId>>()
        .remove(unit_id);
}
