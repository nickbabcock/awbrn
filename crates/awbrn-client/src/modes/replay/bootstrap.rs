use awbw_replay::AwbwReplay;
use bevy::prelude::*;

use crate::features::player_roster::{emit_player_roster_updated, player_roster_seed_from_replay};
use crate::loading::LoadedReplay;
use crate::modes::replay::commands::ReplayAdvanceLock;
use awbrn_game::replay::initialize_replay_semantic_world;

pub fn initialize_replay_semantic_world_for_client(world: &mut World) {
    let replay = world
        .get_resource::<LoadedReplay>()
        .map(|r| r.0.clone())
        .unwrap_or_else(|| AwbwReplay {
            games: Vec::new(),
            turns: Vec::new(),
        });

    initialize_replay_semantic_world(&replay, world);

    if let Some((config, funds, unit_costs)) = player_roster_seed_from_replay(&replay) {
        world.insert_resource(config);
        world.insert_resource(funds);
        world.insert_resource(unit_costs);
    }

    world.insert_resource(ReplayAdvanceLock::default());
    emit_player_roster_updated(world);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::CorePlugin;
    use crate::features::fog::FogPlugin;
    use crate::modes::replay::ReplayPlugin;
    use awbrn_game::MapPosition;
    use awbrn_game::world::{GameMap, TerrainHp, TerrainTile};
    use awbrn_map::{AwbrnMap, Position};
    use awbrn_types::GraphicalTerrain;
    use awbw_replay::ReplayParser;
    use bevy::state::app::StatesPlugin;
    use std::path::Path;

    #[test]
    fn seam_tiles_default_to_99_hp_without_replay_building_data() {
        let mut app = bootstrap_test_app();
        app.world_mut().resource_mut::<GameMap>().set(AwbrnMap::new(
            1,
            1,
            GraphicalTerrain::PipeSeam(awbrn_types::PipeSeamType::Vertical),
        ));

        initialize_replay_semantic_world_for_client(app.world_mut());

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
        app.world_mut()
            .insert_resource(crate::loading::LoadedReplay(replay));

        initialize_replay_semantic_world_for_client(app.world_mut());

        let mut query = app.world_mut().query::<(&MapPosition, &TerrainHp)>();
        let (_, terrain_hp) = query
            .iter(app.world())
            .find(|(map_pos, _)| map_pos.position() == Position::new(16, 10))
            .unwrap();
        assert_eq!(terrain_hp.value(), expected_hp);
    }

    fn bootstrap_test_app() -> App {
        let mut app = App::new();
        app.add_plugins((StatesPlugin, CorePlugin, ReplayPlugin, FogPlugin));
        app
    }

    fn replay_fixture_path(file_name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/replays")
            .join(file_name)
    }
}
