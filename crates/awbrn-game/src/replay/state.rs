use awbrn_types::{AwbwGamePlayerId, PlayerFaction};
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use std::collections::HashMap;

use crate::world::StrongIdMap;

#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[component(immutable, on_add = on_awbw_unit_id_add, on_remove = on_awbw_unit_id_remove)]
#[reflect(Component)]
pub struct AwbwUnitId(pub awbrn_types::AwbwUnitId);

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

/// Temporary vision range boosts granted by CO powers, per faction.
#[derive(Resource, Debug, Default, Clone)]
pub struct PowerVisionBoosts(pub HashMap<PlayerFaction, i32>);

/// Resource tracking the current state of replay playback.
#[derive(Resource, Reflect, Debug, Clone, Copy, PartialEq, Eq)]
#[reflect(Resource)]
pub struct ReplayState {
    pub next_action_index: u32,
    pub day: u32,
    /// The player whose turn it currently is. Set at bootstrap from turn order
    /// and updated by `apply_end()` from `UpdatedInfo.next_player_id`.
    #[reflect(ignore)]
    pub active_player_id: Option<AwbwGamePlayerId>,
}

impl Default for ReplayState {
    fn default() -> Self {
        Self {
            next_action_index: 0,
            day: 1,
            active_player_id: None,
        }
    }
}
