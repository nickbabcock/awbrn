use std::path::Path;

use awbrn_game::GameWorldPlugin;
use awbrn_game::replay::{apply_observed_transitions, initialize_replay_semantic_world};
use awbrn_game::snapshot::{canonicalize_replay_semantic_snapshot, capture_game_snapshot};
use awbrn_game::world::GameMap;
use awbrn_map::{AwbrnMap, AwbwMap, AwbwMapData};
use awbw_replay::ReplayParser;
use awvm::semantic::{AwbwVisibility, observe_transition};
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
