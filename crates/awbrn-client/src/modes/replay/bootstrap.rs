use awbrn_bevy::MapPosition;
use awbrn_bevy::replay::{
    RecipientObservations, ReplayPlayerRegistry, ReplayState, ReplayTerrainKnowledge,
    refresh_viewer_visibility,
};
use awbrn_bevy::world::{
    Ammo, BoardOf, Faction, FriendlyFactions, Fuel, GameMap, GraphicalHp, TerrainHp, TerrainTile,
    Unit, UnitActive, UnitId, ViewerVisibility, VisionRange, initialize_terrain_semantic_world,
};
use awbrn_map::Pos;
use awbrn_types::PlayerFaction;
use awbw_replay::AwbwReplay;
use bevy::prelude::*;

use crate::features::event_bus::player_id_from_js;
use crate::features::player_display::PlayerDisplayFactionOverrides;
use crate::features::player_roster::{
    PlayerPowerMeters, emit_player_roster_updated, power_meters_from_observation,
};
use crate::loading::LoadedReplay;
use crate::modes::replay::presentation::{ReplayAdvanceLock, ReplayTransitionSource};
use crate::modes::replay::roster::player_roster_seed_from_replay;

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

/// Record the turn every boundary of the archive falls in.
///
/// One replay of the whole archive, paid once when it is loaded, is what makes
/// crossing a game by turns as cheap as reading it by actions. Without it a
/// jump would have to replay the archive to find out where it was going.
fn build_replay_outline(world: &mut World) {
    let boundaries = world
        .try_resource_scope(|world, mut source: Mut<ReplayTransitionSource>| {
            world
                .get_resource::<LoadedReplay>()
                .map(|replay| source.outline(&replay.0))
        })
        .flatten();
    match boundaries {
        Some(Ok(boundaries)) => {
            world.insert_resource(crate::modes::replay::timeline::ReplayOutline(boundaries));
        }
        Some(Err(error)) => {
            error!("Could not read the archive's turns: {error}");
            world.init_resource::<crate::modes::replay::timeline::ReplayOutline>();
        }
        None => {
            world.init_resource::<crate::modes::replay::timeline::ReplayOutline>();
        }
    }
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
    build_replay_outline(world);

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

/// Initialize the ECS world for replay playback from a parsed `AwbwReplay`.
///
/// Sets up terrain HP for pipe seams, spawns unit entities, and installs the
/// presentation-visibility resources. Does NOT insert `ReplayAdvanceLock`;
/// [`initialize_replay_semantic_world_for_client`] does.
pub fn initialize_replay_semantic_world(replay: &AwbwReplay, world: &mut World) {
    let board_root = initialize_terrain_semantic_world(world);

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

    let (replay_units, player_registry, first_player_id) = replay
        .games
        .first()
        .map(|first_game| {
            let mut registry = ReplayPlayerRegistry::default();
            let mut sorted_players = first_game.players.iter().collect::<Vec<_>>();
            sorted_players.sort_by_key(|p| p.order);
            for p in &sorted_players {
                let team = if first_game.team {
                    p.team.as_bytes().first().copied().unwrap_or(0)
                } else {
                    0
                };
                registry.add_player(awvm_awbw::player_id(p.id), p.faction, team);
            }
            let first_player_id = first_game
                .players
                .iter()
                .min_by_key(|p| p.order)
                .map(|p| awvm_awbw::player_id(p.id));

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
                        MapPosition::new(unit.x as u8, unit.y as u8),
                        Faction(faction),
                        UnitId(awvm_awbw::unit_id(unit.id)),
                        Unit(awvm_awbw::unit_kind(unit.name)),
                        Fuel(unit.fuel),
                        Ammo(unit.ammo),
                        GraphicalHp::Visible(awbrn_types::VisualHp::new(initial_graphical_hp(
                            unit.hit_points,
                        ))),
                        VisionRange(unit.vision),
                        UnitActive,
                    )
                })
                .collect::<Vec<_>>();

            (units, registry, first_player_id)
        })
        .unwrap_or_default();

    for replay_unit in replay_units {
        world.spawn((replay_unit, BoardOf(board_root)));
    }

    // Presentation visibility stays omniscient until the first projection
    // arrives; the viewpoint defaults to spectator, which is the same view.
    let terrain_knowledge = {
        let game_map = world.resource::<GameMap>();
        ReplayTerrainKnowledge::from_map_and_registry(game_map, &player_registry)
    };
    world.insert_resource(player_registry);
    world.insert_resource(terrain_knowledge);
    world.insert_resource(RecipientObservations::default());
    world.insert_resource(ViewerVisibility::default());
    world.insert_resource(FriendlyFactions::default());

    world.insert_resource(ReplayState {
        active_player_id: first_player_id,
        ..ReplayState::default()
    });
}

fn initial_graphical_hp(hit_points: f64) -> u8 {
    hit_points.ceil().clamp(0.0, 10.0) as u8
}

fn initial_terrain_hp(
    replay: &AwbwReplay,
    terrain_tile: TerrainTile,
    position: Pos,
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
                building.x == u32::from(position.x) && building.y == u32::from(position.y)
            })
        })
        // AWBW overloads replay building capture progress for pipe seams. We
        // translate that wire-level overload into terrain HP at bootstrap.
        .and_then(|building| u8::try_from(building.capture).ok())
        .unwrap_or(99);

    Some(TerrainHp(hp))
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
            registry.add_player(player_id_from_js(player.player_id), faction, 0);
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
    use awbrn_map::{AwbrnMap, Dimensions};
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
