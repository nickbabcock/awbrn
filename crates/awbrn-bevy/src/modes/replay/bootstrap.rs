use bevy::prelude::*;

use awbrn_core::PlayerFaction;

use crate::core::map::initialize_terrain_semantic_world;
use crate::core::{Faction, MapPosition, Unit, UnitActive};
use crate::loading::LoadedReplay;
use crate::modes::replay::AwbwUnitId;
use crate::modes::replay::commands::ReplayAdvanceLock;
use crate::modes::replay::state::ReplayState;

pub fn initialize_replay_semantic_world(world: &mut World) {
    initialize_terrain_semantic_world(world);

    let replay_units = world
        .get_resource::<LoadedReplay>()
        .and_then(|loaded_replay| loaded_replay.0.games.first())
        .map(|first_game| {
            first_game
                .units
                .iter()
                .map(|unit| {
                    let faction = first_game
                        .players
                        .iter()
                        .find(|player| player.id == unit.players_id)
                        .map(|player| player.faction)
                        .unwrap_or(PlayerFaction::OrangeStar);

                    (unit, faction)
                })
                .map(|(unit, faction)| {
                    (
                        MapPosition::new(unit.x as usize, unit.y as usize),
                        Faction(faction),
                        AwbwUnitId(unit.id),
                        Unit(unit.name),
                        UnitActive,
                    )
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    for replay_unit in replay_units {
        world.spawn(replay_unit);
    }

    world.insert_resource(ReplayState::default());
    world.insert_resource(ReplayAdvanceLock::default());
}
