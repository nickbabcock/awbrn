use bevy::prelude::*;

use crate::core::map::{TerrainHp, TerrainTile, initialize_terrain_semantic_world};
use crate::core::units::VisionRange;
use crate::core::{Ammo, Faction, Fuel, GameMap, MapPosition, Unit, UnitActive};
use crate::features::fog::{FogActive, FogOfWarMap, FriendlyFactions};
use crate::loading::LoadedReplay;
use crate::modes::replay::AwbwUnitId;
use crate::modes::replay::commands::ReplayAdvanceLock;
use crate::modes::replay::fog::{ReplayFogEnabled, ReplayPlayerRegistry, ReplayTerrainKnowledge};
use crate::modes::replay::state::ReplayState;
use awbrn_core::PlayerFaction;
use awbrn_map::Position;

pub fn initialize_replay_semantic_world(world: &mut World) {
    initialize_terrain_semantic_world(world);

    let terrain_entities: Vec<_> = {
        let mut query = world.query::<(Entity, &TerrainTile, &MapPosition)>();
        query
            .iter(world)
            .map(|(entity, terrain_tile, map_pos)| (entity, *terrain_tile, map_pos.position()))
            .collect()
    };
    for (entity, terrain_tile, position) in terrain_entities {
        if let Some(terrain_hp) = initial_terrain_hp(world, terrain_tile, position) {
            world.entity_mut(entity).insert(terrain_hp);
        }
    }

    let (replay_units, fog_enabled, player_registry, first_player_id) = world
        .get_resource::<LoadedReplay>()
        .and_then(|loaded_replay| loaded_replay.0.games.first())
        .map(|first_game| {
            let fog_enabled = first_game.fog;
            let registry = ReplayPlayerRegistry::from_players(&first_game.players, first_game.team);
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
    world.insert_resource(ReplayAdvanceLock::default());
}

fn initial_terrain_hp(
    world: &World,
    terrain_tile: TerrainTile,
    position: Position,
) -> Option<TerrainHp> {
    if !matches!(
        terrain_tile.terrain,
        awbrn_core::GraphicalTerrain::PipeSeam(_)
    ) {
        return None;
    }

    let hp = world
        .get_resource::<LoadedReplay>()
        .and_then(|loaded_replay| loaded_replay.0.games.first())
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
    use crate::core::{CorePlugin, GameMap};
    use crate::modes::replay::ReplayPlugin;
    use awbrn_core::GraphicalTerrain;
    use awbrn_map::{AwbrnMap, Position};
    use awbw_replay::ReplayParser;
    use bevy::state::app::StatesPlugin;
    use std::path::Path;

    #[test]
    fn seam_tiles_default_to_99_hp_without_replay_building_data() {
        let mut app = bootstrap_test_app();
        app.world_mut().resource_mut::<GameMap>().set(AwbrnMap::new(
            1,
            1,
            GraphicalTerrain::PipeSeam(awbrn_core::PipeSeamType::Vertical),
        ));

        initialize_replay_semantic_world(app.world_mut());

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
            GraphicalTerrain::PipeSeam(awbrn_core::PipeSeamType::Vertical),
        );
        app.world_mut().resource_mut::<GameMap>().set(map);
        app.world_mut().insert_resource(LoadedReplay(replay));

        initialize_replay_semantic_world(app.world_mut());

        let mut query = app.world_mut().query::<(&MapPosition, &TerrainHp)>();
        let (_, terrain_hp) = query
            .iter(app.world())
            .find(|(map_pos, _)| map_pos.position() == Position::new(16, 10))
            .unwrap();
        assert_eq!(terrain_hp.value(), expected_hp);
    }

    fn bootstrap_test_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            StatesPlugin,
            CorePlugin,
            ReplayPlugin,
            crate::features::fog::FogPlugin,
        ));
        app
    }

    fn replay_fixture_path(file_name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/replays")
            .join(file_name)
    }
}
