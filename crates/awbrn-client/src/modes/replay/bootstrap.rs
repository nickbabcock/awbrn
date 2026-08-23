use bevy::prelude::*;

use crate::features::player_display::PlayerDisplayFactionOverrides;
use crate::features::player_roster::{
    PlayerPowerMeters, emit_player_roster_updated, player_roster_seed_from_replay,
    power_meters_from_observation,
};
use crate::loading::LoadedReplay;
use crate::modes::replay::presentation::{ReplayAdvanceLock, ReplayTransitionSource};
use awbrn_bevy::replay::{
    RecipientObservations, ReplayTerrainKnowledge, initialize_replay_semantic_world,
    refresh_viewer_visibility,
};

/// Hand the presentation the projections of the archive's opening state.
///
/// Without them a viewpoint switched before the first action has nothing to
/// select and falls back to the omniscient spectator view.
fn seed_initial_observations(world: &mut World) {
    let Some(source) = world.get_resource::<ReplayTransitionSource>() else {
        return;
    };
    match source.initial_observations() {
        Ok(observations) => {
            if let Some(observation) = observations.first() {
                world.insert_resource(power_meters_from_observation(observation));
            }
            world
                .resource_mut::<RecipientObservations>()
                .set(observations);
        }
        Err(error) => error!("{error}"),
    }
    refresh_viewer_visibility(world);
}

pub fn initialize_replay_semantic_world_for_client(world: &mut World) {
    let is_awbrn = world
        .get_resource::<LoadedReplay>()
        .is_some_and(|replay| replay.0.awbw().is_none());
    if is_awbrn {
        initialize_awbrn_replay_world(world);
    } else {
        let empty = awbw_replay::AwbwReplay {
            games: Vec::new(),
            turns: Vec::new(),
        };
        let replay = world
            .get_resource::<LoadedReplay>()
            .and_then(|replay| replay.0.awbw())
            .cloned()
            .unwrap_or(empty);
        initialize_replay_semantic_world(&replay, world);
    }
    let initial_terrain_knowledge = world.resource::<ReplayTerrainKnowledge>().clone();
    if let Some(mut source) = world.get_resource_mut::<ReplayTransitionSource>() {
        source.set_initial_terrain_knowledge(initial_terrain_knowledge);
    }
    seed_initial_observations(world);

    if let Some(replay) = world
        .get_resource::<LoadedReplay>()
        .and_then(|replay| replay.0.awbw())
        && let Some((config, funds, unit_costs)) = player_roster_seed_from_replay(replay)
    {
        world.insert_resource(config);
        world.insert_resource(funds);
        world.insert_resource(unit_costs);
    }

    // An empty replay has no opening observation from which to seed meters.
    world.init_resource::<PlayerPowerMeters>();

    world.init_resource::<PlayerDisplayFactionOverrides>();
    world
        .resource_mut::<PlayerDisplayFactionOverrides>()
        .0
        .clear();

    world.insert_resource(ReplayAdvanceLock::default());
    emit_player_roster_updated(world);
}

fn initialize_awbrn_replay_world(world: &mut World) {
    awbrn_bevy::world::initialize_terrain_semantic_world(world);
    let players = {
        let Some(LoadedReplay(crate::replay_archive::ReplayArchive::Awbrn(replay))) =
            world.get_resource::<LoadedReplay>()
        else {
            return;
        };
        replay
            .setup
            .players
            .iter()
            .enumerate()
            .map(|(index, player)| crate::loading::LiveMatchPlayer {
                player_id: index as u32,
                faction_id: player.faction_id,
            })
            .collect::<Vec<_>>()
    };
    let mut registry = awbrn_bevy::replay::ReplayPlayerRegistry::default();
    for player in &players {
        if let Some(faction) = awbrn_types::PlayerFaction::from_id(player.faction_id) {
            registry.add_player(
                awbrn_types::AwbwGamePlayerId::new(player.player_id),
                faction,
                0,
            );
        }
    }
    let knowledge = ReplayTerrainKnowledge::from_map_and_registry(
        world.resource::<awbrn_bevy::world::GameMap>(),
        &registry,
    );
    world.insert_resource(registry);
    world.insert_resource(knowledge);
    world.insert_resource(awbrn_bevy::replay::ReplayState::default());
    world.insert_resource(awbrn_bevy::replay::RecipientObservations::default());
    world.insert_resource(awbrn_bevy::world::ViewerVisibility::default());
    world.insert_resource(awbrn_bevy::world::FriendlyFactions::default());
    let observations = match world
        .get_resource::<ReplayTransitionSource>()
        .map(ReplayTransitionSource::initial_observations)
    {
        Some(Ok(observations)) => observations,
        Some(Err(error)) => {
            error!("Could not observe the opening AWBRN replay state: {error}");
            return;
        }
        None => return,
    };
    let Some(first) = observations.first() else {
        return;
    };
    let (config, funds, unit_costs, power_meters) =
        crate::features::player_roster::player_roster_seed_from_live_match(&players, first);
    world.insert_resource(config);
    world.insert_resource(funds);
    world.insert_resource(unit_costs);
    world.insert_resource(power_meters);
    let transitions = observations
        .into_iter()
        .map(|post| awvm::semantic::ObservedTransition {
            post,
            events: Vec::new(),
        })
        .collect::<Vec<_>>();
    if let Err(error) = awbrn_bevy::replay::apply_observed_transitions(world, &transitions) {
        error!("Could not initialize AWBRN replay presentation: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::CorePlugin;
    use crate::features::visibility::VisibilityPlugin;
    use crate::modes::replay::ReplayPlugin;
    use awbrn_bevy::MapPosition;
    use awbrn_bevy::world::{GameMap, TerrainHp, TerrainTile};
    use awbrn_map::{AwbrnMap, Dimensions, Pos};
    use awbrn_types::GraphicalTerrain;
    use awbw_replay::ReplayParser;
    use bevy::state::app::StatesPlugin;
    use std::path::Path;

    #[test]
    fn seam_tiles_default_to_99_hp_without_replay_building_data() {
        let mut app = bootstrap_test_app();
        app.world_mut().resource_mut::<GameMap>().set(AwbrnMap::new(
            Dimensions::new(1, 1),
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
        let mut map = AwbrnMap::new(Dimensions::new(17, 11), GraphicalTerrain::Plain);
        map.set_terrain(
            Pos::new(16, 10),
            GraphicalTerrain::PipeSeam(awbrn_types::PipeSeamType::Vertical),
        );
        app.world_mut().resource_mut::<GameMap>().set(map);
        app.world_mut()
            .insert_resource(crate::loading::LoadedReplay(
                crate::replay_archive::ReplayArchive::Awbw(replay),
            ));

        initialize_replay_semantic_world_for_client(app.world_mut());

        let mut query = app.world_mut().query::<(&MapPosition, &TerrainHp)>();
        let (_, terrain_hp) = query
            .iter(app.world())
            .find(|(map_pos, _)| map_pos.position() == Pos::new(16, 10))
            .unwrap();
        assert_eq!(terrain_hp.value(), expected_hp);
    }

    fn bootstrap_test_app() -> App {
        let mut app = App::new();
        app.add_plugins((StatesPlugin, CorePlugin, ReplayPlugin, VisibilityPlugin));
        app
    }

    fn replay_fixture_path(file_name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/replays")
            .join(file_name)
    }
}
