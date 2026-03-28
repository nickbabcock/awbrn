//! Bootstrap logic for initializing the ECS world from an AWBW replay.
//!
//! This module contains the pure game-logic bootstrap that sets up terrain,
//! units, fog resources, and replay state. It does not insert any client-only
//! resources (such as `ReplayAdvanceLock`).

use awbrn_map::Position;
use awbrn_types::PlayerFaction;
use awbw_replay::AwbwReplay;
use bevy::prelude::*;

use crate::MapPosition;
use crate::replay::{
    AwbwUnitId, ReplayFogEnabled, ReplayPlayerRegistry, ReplayState, ReplayTerrainKnowledge,
};
use crate::world::{
    Ammo, Faction, FogActive, FogOfWarMap, FriendlyFactions, Fuel, GameMap, TerrainHp, TerrainTile,
    Unit, UnitActive, VisionRange, initialize_terrain_semantic_world,
};

/// Initialize the ECS world for replay playback from a parsed `AwbwReplay`.
///
/// Sets up terrain HP for pipe seams, spawns unit entities, and configures
/// fog-of-war resources. Does NOT insert `ReplayAdvanceLock` — the client
/// layer is responsible for that.
pub fn initialize_replay_semantic_world(replay: &AwbwReplay, world: &mut World) {
    initialize_terrain_semantic_world(world);

    let terrain_entities: Vec<_> = {
        let mut query = world.query::<(Entity, &TerrainTile, &MapPosition)>();
        query
            .iter(world)
            .map(|(entity, terrain_tile, map_pos)| (entity, *terrain_tile, map_pos.position()))
            .collect()
    };
    for (entity, terrain_tile, position) in terrain_entities {
        if let Some(terrain_hp) = initial_terrain_hp(replay, terrain_tile, position) {
            world.entity_mut(entity).insert(terrain_hp);
        }
    }

    let (replay_units, fog_enabled, player_registry, first_player_id) = replay
        .games
        .first()
        .map(|first_game| {
            let fog_enabled = first_game.fog;
            let mut registry = ReplayPlayerRegistry::default();
            let mut sorted_players = first_game.players.iter().collect::<Vec<_>>();
            sorted_players.sort_by_key(|p| p.order);
            for p in &sorted_players {
                let team = if first_game.team {
                    p.team.as_bytes().first().copied().unwrap_or(0)
                } else {
                    0
                };
                registry.add_player(p.id, p.faction, team);
            }
            let first_player_id = first_game
                .players
                .iter()
                .min_by_key(|p| p.order)
                .map(|p| p.id);

            let units = first_game
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
                        Fuel(unit.fuel),
                        Ammo(unit.ammo),
                        VisionRange(unit.vision),
                        UnitActive,
                    )
                })
                .collect::<Vec<_>>();

            (units, fog_enabled, registry, first_player_id)
        })
        .unwrap_or_default();

    for replay_unit in replay_units {
        world.spawn(replay_unit);
    }

    // Initialize fog resources
    let (map_width, map_height, terrain_knowledge) = {
        let game_map = world.resource::<GameMap>();
        (
            game_map.width(),
            game_map.height(),
            ReplayTerrainKnowledge::from_map_and_registry(game_map, &player_registry),
        )
    };
    world
        .resource_mut::<FogOfWarMap>()
        .reset(map_width, map_height);
    world.insert_resource(ReplayFogEnabled(fog_enabled));
    world.insert_resource(player_registry);
    world.insert_resource(terrain_knowledge);
    world.insert_resource(FogActive(false)); // Spectator by default
    world.insert_resource(FriendlyFactions::default());

    world.insert_resource(ReplayState {
        active_player_id: first_player_id,
        ..ReplayState::default()
    });
}

fn initial_terrain_hp(
    replay: &AwbwReplay,
    terrain_tile: TerrainTile,
    position: Position,
) -> Option<TerrainHp> {
    if !matches!(
        terrain_tile.terrain,
        awbrn_types::GraphicalTerrain::PipeSeam(_)
    ) {
        return None;
    }

    let hp = replay
        .games
        .first()
        .and_then(|game| {
            game.buildings.iter().find(|building| {
                building.x as usize == position.x && building.y as usize == position.y
            })
        })
        // AWBW overloads replay building capture progress for pipe seams. We
        // translate that wire-level overload into terrain HP at bootstrap.
        .and_then(|building| u8::try_from(building.capture).ok())
        .unwrap_or(99);

    Some(TerrainHp(hp))
}

#[cfg(test)]
mod tests {
    use super::*;
    use awbrn_map::AwbrnMap;
    use awbrn_types::GraphicalTerrain;
    use awbw_replay::ReplayParser;
    use bevy::app::App;
    use std::path::Path;

    use crate::GameWorldPlugin;

    fn bootstrap_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(GameWorldPlugin);
        app
    }

    fn replay_fixture_path(file_name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/replays")
            .join(file_name)
    }

    #[test]
    fn seam_tiles_default_to_99_hp_without_replay_building_data() {
        let mut app = bootstrap_test_app();
        app.world_mut().resource_mut::<GameMap>().set(AwbrnMap::new(
            1,
            1,
            GraphicalTerrain::PipeSeam(awbrn_types::PipeSeamType::Vertical),
        ));

        let empty_replay = awbw_replay::AwbwReplay {
            games: Vec::new(),
            turns: Vec::new(),
        };
        initialize_replay_semantic_world(&empty_replay, app.world_mut());

        let mut query = app.world_mut().query::<(&TerrainTile, &TerrainHp)>();
        let (_, terrain_hp) = query.single(app.world()).unwrap();
        assert_eq!(terrain_hp.value(), 99);
    }

    #[test]
    fn seam_tiles_use_replay_building_capture_as_initial_hp() {
        let replay_bytes = std::fs::read(replay_fixture_path("1362397.zip")).unwrap();
        let replay = ReplayParser::new().parse(&replay_bytes).unwrap();

        let expected_hp = replay
            .games
            .first()
            .unwrap()
            .buildings
            .iter()
            .find(|b| b.x == 16 && b.y == 10)
            .map(|b| u8::try_from(b.capture).unwrap())
            .expect("fixture should have a building at the pipe seam position");

        let mut app = bootstrap_test_app();
        let mut map = AwbrnMap::new(17, 11, GraphicalTerrain::Plain);
        map.set_terrain(
            Position::new(16, 10),
            GraphicalTerrain::PipeSeam(awbrn_types::PipeSeamType::Vertical),
        );
        app.world_mut().resource_mut::<GameMap>().set(map);

        initialize_replay_semantic_world(&replay, app.world_mut());

        let mut query = app.world_mut().query::<(&MapPosition, &TerrainHp)>();
        let (_, terrain_hp) = query
            .iter(app.world())
            .find(|(map_pos, _)| map_pos.position() == Position::new(16, 10))
            .unwrap();
        assert_eq!(terrain_hp.value(), expected_hp);
    }
}
