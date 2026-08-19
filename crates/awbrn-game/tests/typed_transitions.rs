use std::path::Path;

use awbrn_game::GameWorldPlugin;
use awbrn_game::replay::{
    AwbwUnitId, ReplayPlayerRegistry, ReplayTerrainKnowledge, ReplayViewpoint,
    apply_observed_transitions, initialize_replay_semantic_world, refresh_viewer_visibility,
};
use awbrn_game::snapshot::{canonicalize_replay_semantic_snapshot, capture_game_snapshot};
use awbrn_game::world::{GameMap, GraphicalHp, ViewerVisibility};
use awbrn_map::{AwbrnMap, AwbwMap, AwbwMapData};
use awbrn_types::VisualHp;
use awbw_replay::ReplayParser;
use awvm::semantic::{
    AwbwVisibility, ObservedTransition, PlayerId, State, observe, observe_transition,
};
use awvm::transition::{ExecuteOutcome, execute};
use awvm_awbw::{RecordedAdapter, diagnostic_command};
use bevy::ecs::reflect::AppTypeRegistry;
use bevy::prelude::*;

#[test]
fn native_and_recorded_sources_drive_the_same_headless_boundary() {
    let replay = ReplayParser::new()
        .parse(&std::fs::read(asset("replays/replay_1699315_missle-bomb_2026-07-28.zip")).unwrap())
        .unwrap();
    let map_data: AwbwMapData =
        serde_json::from_slice(&std::fs::read(asset("maps/178597.json")).unwrap()).unwrap();
    let action = &replay.turns[0];

    let mut recorded = RecordedAdapter::new(&replay, &map_data).unwrap();
    let initial = recorded.state().clone();
    let player = initial.turn.active_player.clone();
    let command = diagnostic_command(player, action).unwrap();
    let execution = match execute(&initial, command, &[]).unwrap() {
        ExecuteOutcome::Accepted(execution) => execution,
        ExecuteOutcome::Rejected(violation) => panic!("launch was rejected: {violation:?}"),
    };
    let native = initial
        .players
        .iter()
        .map(|player| {
            observe_transition(
                &AwbwVisibility,
                &initial,
                &execution.state,
                &execution.events,
                &player.id,
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    let recorded = recorded.advance(action).unwrap();
    let archived = recorded
        .post_state()
        .players
        .iter()
        .map(|player| recorded.observe(&player.id))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    let mut native_app = presentation_app(&replay, &map_data);
    let mut archived_app = presentation_app(&replay, &map_data);
    apply_observed_transitions(native_app.world_mut(), &native).unwrap();
    apply_observed_transitions(archived_app.world_mut(), &archived).unwrap();

    let native_snapshot = canonical_snapshot(&mut native_app);
    let archived_snapshot = canonical_snapshot(&mut archived_app);
    assert_eq!(native_snapshot.day, 1);
    assert_eq!(
        native_snapshot, archived_snapshot,
        "native execution and recorded outcomes must reconcile to identical state"
    );

    let awbw_replay::turn_models::Action::Launch { launch_action, .. } = action else {
        panic!("fixture action is not a launch");
    };
    let silo =
        awbrn_map::Position::new(launch_action.silo_x as usize, launch_action.silo_y as usize);
    for app in [&native_app, &archived_app] {
        assert_eq!(
            app.world().resource::<GameMap>().terrain_at(silo),
            Some(awbrn_types::GraphicalTerrain::MissileSilo(
                awbrn_types::MissileSiloStatus::Unloaded
            ))
        );
    }
}

/// The viewpoint selects a recipient projection; it does not recompute vision.
///
/// A fogged archive is reconciled once from every player's projection, so the
/// ECS holds both rosters. Each viewpoint then sees exactly what its own
/// projection listed, and switching between them touches no rules code.
#[test]
fn a_viewpoint_sees_what_its_own_projection_listed() {
    let replay = ReplayParser::new()
        .parse(&std::fs::read(asset("replays/1391406.zip")).unwrap())
        .unwrap();
    let map_data: AwbwMapData =
        serde_json::from_slice(&std::fs::read(asset("maps/146471.json")).unwrap()).unwrap();
    assert!(
        replay.games[0].fog,
        "this assertion is about fog; the fixture must be a fogged game"
    );

    let mut recorded = RecordedAdapter::new(&replay, &map_data).unwrap();
    let players = recorded
        .state()
        .players
        .iter()
        .map(|player| player.id.clone())
        .collect::<Vec<_>>();
    let transition = recorded.advance(&replay.turns[0]).unwrap();
    let projections = players
        .iter()
        .map(|player| transition.observe(player))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    let mut app = presentation_app(&replay, &map_data);
    apply_observed_transitions(app.world_mut(), &projections).unwrap();

    // Spectator has no projection to select and is shown everything.
    assert!(
        !app.world().resource::<ViewerVisibility>().fog_active(),
        "a spectator is not looking through anyone's fog"
    );

    let owners = projections
        .iter()
        .map(|projection| awbw_player(&projection.post.recipient))
        .collect::<Vec<_>>();
    let mut seen_by = Vec::new();
    for owner in &owners {
        app.world_mut()
            .insert_resource(ReplayViewpoint::Player(*owner));
        refresh_viewer_visibility(app.world_mut());
        let visibility = app.world().resource::<ViewerVisibility>();
        assert!(
            visibility.fog_active(),
            "a player of a fogged match looks through fog"
        );
        assert!(
            visibility.player_disclosed(*owner),
            "a player's own funds are disclosed to itself"
        );
        for other in &owners {
            if other != owner {
                assert!(
                    !visibility.player_disclosed(*other),
                    "an opponent's funds stay hidden under fog"
                );
            }
        }
        seen_by.push(visible_units(app.world()));
    }

    assert_eq!(seen_by.len(), 2, "the fixture is a two-player game");
    assert_ne!(
        seen_by[0], seen_by[1],
        "two players of a fogged match cannot see the same units"
    );
    for (index, seen) in seen_by.iter().enumerate() {
        assert!(
            !seen.is_empty(),
            "a player sees at least its own units (player {index})"
        );
    }
}

#[test]
fn viewpoint_selects_visible_or_hidden_graphical_hp() {
    let case: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../spec/fixtures/fog/sonja-hidden-hp-noninterference.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let mut state: State = serde_json::from_value(case["left"]["initial_state"].clone()).unwrap();
    let sonja = PlayerId::from("1");
    let opponent = PlayerId::from("2");
    state.players[0].id = sonja.clone();
    state.players[1].id = opponent.clone();
    state.units[0].owner = state.player_index(&sonja).unwrap();
    state.units[1].owner = state.player_index(&opponent).unwrap();
    state.turn.active_player = sonja.clone();
    state.turn.order = vec![sonja.clone(), opponent.clone()];

    let transitions = [&sonja, &opponent]
        .into_iter()
        .map(|recipient| ObservedTransition {
            post: observe(&AwbwVisibility, &state, recipient).unwrap(),
            events: Vec::new(),
        })
        .collect::<Vec<_>>();

    let replay = ReplayParser::new()
        .parse(&std::fs::read(asset("replays/1391406.zip")).unwrap())
        .unwrap();
    let map_data: AwbwMapData =
        serde_json::from_slice(&std::fs::read(asset("maps/146471.json")).unwrap()).unwrap();
    let mut app = presentation_app(&replay, &map_data);
    let mut registry = ReplayPlayerRegistry::default();
    registry.add_player(
        awbrn_types::AwbwGamePlayerId::new(1),
        awbrn_types::PlayerFaction::OrangeStar,
        0,
    );
    registry.add_player(
        awbrn_types::AwbwGamePlayerId::new(2),
        awbrn_types::PlayerFaction::BlueMoon,
        0,
    );
    app.world_mut().insert_resource(registry);
    apply_observed_transitions(app.world_mut(), &transitions).unwrap();

    let unit = app
        .world()
        .resource::<awbrn_game::world::StrongIdMap<AwbwUnitId>>()
        .get(&AwbwUnitId(awbrn_types::AwbwUnitId::new(0)))
        .unwrap();
    assert_eq!(
        app.world()
            .get::<GraphicalHp>(unit)
            .and_then(|hp| hp.visible())
            .map(VisualHp::get),
        Some(8)
    );

    app.world_mut()
        .insert_resource(ReplayViewpoint::Player(awbrn_types::AwbwGamePlayerId::new(
            2,
        )));
    refresh_viewer_visibility(app.world_mut());
    assert_eq!(
        app.world().get::<GraphicalHp>(unit),
        Some(&GraphicalHp::Hidden)
    );

    app.world_mut().insert_resource(ReplayViewpoint::Spectator);
    refresh_viewer_visibility(app.world_mut());
    assert_eq!(
        app.world()
            .get::<GraphicalHp>(unit)
            .and_then(|hp| hp.visible())
            .map(VisualHp::get),
        Some(8)
    );

    app.world_mut()
        .insert_resource(ReplayViewpoint::Player(awbrn_types::AwbwGamePlayerId::new(
            1,
        )));
    refresh_viewer_visibility(app.world_mut());
    assert_eq!(
        app.world()
            .get::<GraphicalHp>(unit)
            .and_then(|hp| hp.visible())
            .map(VisualHp::get),
        Some(8)
    );
}

fn visible_units(world: &World) -> Vec<u32> {
    let visibility = world.resource::<ViewerVisibility>();
    let mut visible = world
        .iter_entities()
        .filter_map(|entity| entity.get::<AwbwUnitId>())
        .filter(|id| visibility.unit_visible(id.0))
        .map(|id| id.0.as_u32())
        .collect::<Vec<_>>();
    visible.sort_unstable();
    visible
}

fn awbw_player(id: &awvm::semantic::PlayerId) -> awbrn_types::AwbwGamePlayerId {
    awbrn_types::AwbwGamePlayerId::new(id.as_str().parse().unwrap())
}

/// Terrain memory records what the selected viewpoint can currently see.
///
/// A projection reports a fogged tile's terrain but never its owner, so the
/// property sprite a viewer remembers is presentation state. It must follow
/// the projection's own visibility, not a second vision calculation.
#[test]
fn terrain_memory_follows_the_projections_visibility() {
    let replay = ReplayParser::new()
        .parse(&std::fs::read(asset("replays/1391406.zip")).unwrap())
        .unwrap();
    let map_data: AwbwMapData =
        serde_json::from_slice(&std::fs::read(asset("maps/146471.json")).unwrap()).unwrap();

    let mut recorded = RecordedAdapter::new(&replay, &map_data).unwrap();
    let players = recorded
        .state()
        .players
        .iter()
        .map(|player| player.id.clone())
        .collect::<Vec<_>>();
    let transition = recorded.advance(&replay.turns[0]).unwrap();
    let projections = players
        .iter()
        .map(|player| transition.observe(player))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    let mut app = presentation_app(&replay, &map_data);
    apply_observed_transitions(app.world_mut(), &projections).unwrap();
    let viewer = awbw_player(&projections[0].post.recipient);
    app.world_mut()
        .insert_resource(ReplayViewpoint::Player(viewer));
    refresh_viewer_visibility(app.world_mut());

    let key = app
        .world()
        .resource::<ReplayPlayerRegistry>()
        .knowledge_key_for_player(viewer)
        .unwrap();
    let (visible, fogged) = {
        let visibility = app.world().resource::<ViewerVisibility>();
        let game_map = app.world().resource::<GameMap>();
        let mut positions = (0..game_map.height())
            .flat_map(|y| (0..game_map.width()).map(move |x| awbrn_map::Position::new(x, y)));
        let visible = positions
            .clone()
            .find(|position| !visibility.is_fogged(*position))
            .expect("a viewer sees at least the tiles under its own units");
        let fogged = positions
            .find(|position| visibility.is_fogged(*position))
            .expect("a fogged match hides at least one tile from each player");
        (visible, fogged)
    };

    // Repaint both tiles, then re-select the same viewpoint. Only the one the
    // projection calls visible may be re-learned.
    let repainted = awbrn_types::GraphicalTerrain::Property(awbrn_types::Property::City(
        awbrn_types::Faction::Neutral,
    ));
    let remembered_before = app.world().resource::<ReplayTerrainKnowledge>().by_view[&key][&fogged];
    {
        let mut game_map = app.world_mut().resource_mut::<GameMap>();
        game_map.set_terrain(visible, repainted);
        game_map.set_terrain(fogged, repainted);
    }
    refresh_viewer_visibility(app.world_mut());

    let knowledge = &app.world().resource::<ReplayTerrainKnowledge>().by_view[&key];
    assert_eq!(
        knowledge[&visible], repainted,
        "a visible tile is re-learned"
    );
    assert_eq!(
        knowledge[&fogged], remembered_before,
        "a fogged tile keeps what the viewer last saw"
    );
    assert_ne!(
        remembered_before, repainted,
        "the fixture must actually change the fogged tile, or this proves nothing"
    );
}

fn presentation_app(replay: &awbw_replay::AwbwReplay, map_data: &AwbwMapData) -> App {
    let map = AwbwMap::try_from(map_data).unwrap();
    let mut app = App::new();
    app.add_plugins(GameWorldPlugin);
    app.world_mut()
        .resource_mut::<GameMap>()
        .set(AwbrnMap::from_map(&map));
    initialize_replay_semantic_world(replay, app.world_mut());
    app
}

fn canonical_snapshot(app: &mut App) -> awbrn_game::snapshot::CanonicalReplaySnapshot {
    let snapshot = capture_game_snapshot(app.world_mut()).unwrap();
    let registry = app.world().resource::<AppTypeRegistry>().read();
    canonicalize_replay_semantic_snapshot(&snapshot, &registry).unwrap()
}

fn asset(relative: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets")
        .join(relative)
}
