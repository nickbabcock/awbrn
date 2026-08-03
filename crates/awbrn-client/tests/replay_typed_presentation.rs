use std::path::Path;

use awbrn_client::features::player_roster::PlayerPowerMeters;
use awbrn_client::loading::LoadedReplay;
use awbrn_client::loading::apply_replay_building_overrides;
use awbrn_client::modes::replay::bootstrap::initialize_replay_semantic_world_for_client;
use awbrn_client::modes::replay::navigation::PendingCourseArrows;
use awbrn_client::modes::replay::presentation::{
    ReplayAdvanceLock, ReplayFollowupCommand, ReplayRewindCommand, ReplayTransitionFailed,
    ReplayTransitionSource, ReplayTurnCommand,
};
use awbrn_client::render::animation::UnitPathAnimation;
use awbrn_game::GameWorldPlugin;
use awbrn_game::replay::{NewDay, ReplayPlayerRegistry, ReplayState};
use awbrn_game::snapshot::{canonicalize_replay_semantic_snapshot, capture_game_snapshot};
use awbrn_game::world::GameMap;
use awbrn_map::{AwbrnMap, AwbwMap, AwbwMapData};
use awbw_replay::ReplayParser;
use awvm_awbw::RecordedAdapter;
use bevy::prelude::*;

#[test]
fn archived_movement_animates_and_applies_only_typed_transitions() {
    let replay_bytes = std::fs::read(replay_fixture_path("1362397.zip")).unwrap();
    let replay = ReplayParser::new().parse(&replay_bytes).unwrap();
    let map_data: AwbwMapData =
        serde_json::from_slice(&std::fs::read(map_fixture_path("162795.json")).unwrap()).unwrap();
    let mut app = replay_test_app(&replay, &map_data);

    for action in replay.turns.iter().cloned() {
        ReplayTurnCommand { action }.apply(app.world_mut());
        let Some(entity) = app.world().resource::<ReplayAdvanceLock>().active_entity() else {
            continue;
        };

        let path = app
            .world()
            .entity(entity)
            .get::<UnitPathAnimation>()
            .expect("typed unit-moved event should install path animation")
            .path
            .clone();
        let destination = *path.last().unwrap();

        let arrows = app
            .world()
            .entity(entity)
            .get::<PendingCourseArrows>()
            .expect("an animated move should queue course arrows");
        assert_eq!(
            arrows
                .path
                .iter()
                .map(|tile| tile.position)
                .collect::<Vec<_>>(),
            path,
            "course arrows must span the animated movement path"
        );
        // Without the fog feature installed there is nothing to mask, so every
        // tile stays visible; the masking itself is exercised by the client.
        assert!(
            arrows.path.iter().all(|tile| tile.unit_visible),
            "unmasked spectator movement should keep every tile visible"
        );
        let followup = app
            .world_mut()
            .resource_mut::<ReplayAdvanceLock>()
            .release_for(entity)
            .unwrap();
        assert!(
            followup.transitions.is_some(),
            "animation completion must carry the typed transition set"
        );
        ReplayFollowupCommand {
            transitions: followup.transitions,
        }
        .apply(app.world_mut());

        assert_eq!(
            app.world()
                .entity(entity)
                .get::<awbrn_game::MapPosition>()
                .expect("the first archived mover should remain on the board")
                .position(),
            destination
        );
        return;
    }

    panic!("fixture did not produce an observable typed movement");
}

/// Power charge and use count are public AWVM facts. The presentation cache
/// combines them with canonical COP/SCOP cost queries for a complete meter.
#[test]
fn typed_transitions_record_public_power_meter() {
    let replay_bytes = std::fs::read(replay_fixture_path("1362397.zip")).unwrap();
    let replay = ReplayParser::new().parse(&replay_bytes).unwrap();
    let map_data: AwbwMapData =
        serde_json::from_slice(&std::fs::read(map_fixture_path("162795.json")).unwrap()).unwrap();
    let mut app = replay_test_app(&replay, &map_data);

    let mut charged = false;
    for action in replay.turns.iter().cloned() {
        ReplayTurnCommand { action }.apply(app.world_mut());
        if let Some(entity) = app.world().resource::<ReplayAdvanceLock>().active_entity() {
            let followup = app
                .world_mut()
                .resource_mut::<ReplayAdvanceLock>()
                .release_for(entity)
                .unwrap();
            ReplayFollowupCommand {
                transitions: followup.transitions,
            }
            .apply(app.world_mut());
        }

        let meters = app.world().resource::<PlayerPowerMeters>();
        assert_eq!(
            meters.0.len(),
            replay.games.first().unwrap().players.len(),
            "every applied transition reports a charge for each player"
        );
        assert!(
            meters
                .0
                .values()
                .all(|meter| meter.cop_cost.is_some() || meter.scop_cost.is_some()),
            "every CO should expose at least one power threshold"
        );
        if meters.0.values().any(|meter| meter.charge > 0) {
            charged = true;
            break;
        }
    }

    assert!(
        charged,
        "combat in the fixture should raise at least one player's power charge"
    );
}

#[test]
fn initial_observations_stay_at_the_opening_state_after_advancing() {
    let replay_bytes = std::fs::read(replay_fixture_path("1362397.zip")).unwrap();
    let replay = ReplayParser::new().parse(&replay_bytes).unwrap();
    let map_data: AwbwMapData =
        serde_json::from_slice(&std::fs::read(map_fixture_path("162795.json")).unwrap()).unwrap();
    let mut app = replay_test_app(&replay, &map_data);
    let initial = app
        .world()
        .resource::<ReplayTransitionSource>()
        .initial_observations()
        .unwrap();

    apply_settled_action(&mut app, &replay, 0);

    assert_eq!(
        app.world()
            .resource::<ReplayTransitionSource>()
            .initial_observations()
            .unwrap(),
        initial
    );
}

#[test]
fn rewind_to_start_restores_every_players_opening_power_meter() {
    let replay_bytes = std::fs::read(replay_fixture_path("1362397.zip")).unwrap();
    let replay = ReplayParser::new().parse(&replay_bytes).unwrap();
    let map_data: AwbwMapData =
        serde_json::from_slice(&std::fs::read(map_fixture_path("162795.json")).unwrap()).unwrap();
    let mut app = replay_test_app(&replay, &map_data);
    let opening_meters = app.world().resource::<PlayerPowerMeters>().clone();

    apply_settled_action(&mut app, &replay, 0);
    ReplayRewindCommand { target_index: 0 }.apply(app.world_mut());

    let restored_meters = app.world().resource::<PlayerPowerMeters>();
    for player in &replay.games.first().unwrap().players {
        let opening = opening_meters
            .get(player.id)
            .expect("every player should have an opening power meter");
        assert_eq!(restored_meters.get(player.id), Some(opening));
    }
}

#[derive(Resource, Default)]
struct ObservedDays(Vec<u32>);

fn record_new_day(trigger: On<NewDay>, mut observed: ResMut<ObservedDays>) {
    observed.0.push(trigger.day);
}

#[test]
fn rewind_emits_one_new_day_only_when_the_day_changes() {
    let replay_bytes = std::fs::read(replay_fixture_path("1362397.zip")).unwrap();
    let replay = ReplayParser::new().parse(&replay_bytes).unwrap();
    let map_data: AwbwMapData =
        serde_json::from_slice(&std::fs::read(map_fixture_path("162795.json")).unwrap()).unwrap();
    let mut app = replay_test_app(&replay, &map_data);
    app.init_resource::<ObservedDays>();
    app.add_observer(record_new_day);

    for index in 0..replay.turns.len() {
        apply_settled_action(&mut app, &replay, index);
        if app.world().resource::<ReplayState>().day > 1 {
            break;
        }
    }
    assert!(app.world().resource::<ReplayState>().day > 1);
    app.world_mut().resource_mut::<ObservedDays>().0.clear();

    ReplayRewindCommand { target_index: 0 }.apply(app.world_mut());

    assert_eq!(app.world().resource::<ObservedDays>().0, vec![1]);
    app.world_mut().resource_mut::<ObservedDays>().0.clear();
    ReplayRewindCommand { target_index: 0 }.apply(app.world_mut());
    assert!(app.world().resource::<ObservedDays>().0.is_empty());
}

#[test]
fn rewind_apply_failure_does_not_commit_the_cursor_or_clear_failure() {
    let replay_bytes = std::fs::read(replay_fixture_path("1362397.zip")).unwrap();
    let replay = ReplayParser::new().parse(&replay_bytes).unwrap();
    let map_data: AwbwMapData =
        serde_json::from_slice(&std::fs::read(map_fixture_path("162795.json")).unwrap()).unwrap();
    let mut app = replay_test_app(&replay, &map_data);
    for index in 0..5 {
        apply_settled_action(&mut app, &replay, index);
    }
    let cursor = app.world().resource::<ReplayState>().next_action_index;
    app.insert_resource(ReplayPlayerRegistry::default());
    app.insert_resource(ReplayTransitionFailed);

    ReplayRewindCommand { target_index: 1 }.apply(app.world_mut());

    assert_eq!(
        app.world().resource::<ReplayState>().next_action_index,
        cursor
    );
    assert!(app.world().contains_resource::<ReplayTransitionFailed>());
}

#[test]
fn rewind_atomically_matches_direct_playback_and_keeps_the_adapter_aligned() {
    const TARGET: usize = 5;
    const FURTHEST: usize = 9;

    let replay_bytes = std::fs::read(replay_fixture_path("1362397.zip")).unwrap();
    let replay = ReplayParser::new().parse(&replay_bytes).unwrap();
    let map_data: AwbwMapData =
        serde_json::from_slice(&std::fs::read(map_fixture_path("162795.json")).unwrap()).unwrap();

    let mut direct = replay_test_app(&replay, &map_data);
    for index in 0..TARGET {
        apply_settled_action(&mut direct, &replay, index);
    }

    let mut rewound = replay_test_app(&replay, &map_data);
    for index in 0..FURTHEST {
        apply_settled_action(&mut rewound, &replay, index);
    }
    ReplayRewindCommand {
        target_index: TARGET as u32,
    }
    .apply(rewound.world_mut());

    assert_eq!(
        rewound.world().resource::<ReplayState>().next_action_index,
        TARGET as u32
    );
    assert!(!rewound.world().resource::<ReplayAdvanceLock>().is_active());
    assert_eq!(
        rewound
            .world_mut()
            .query::<&UnitPathAnimation>()
            .iter(rewound.world())
            .count(),
        0,
        "rebuild actions must never enter the animation pipeline"
    );
    assert_eq!(
        semantic_snapshot(&mut rewound),
        semantic_snapshot(&mut direct)
    );

    // Matching now is not enough: advancing both worlds once more proves the
    // rewind also restored the hidden adapter cursor, not only the visible ECS.
    apply_settled_action(&mut rewound, &replay, TARGET);
    apply_settled_action(&mut direct, &replay, TARGET);
    assert_eq!(
        semantic_snapshot(&mut rewound),
        semantic_snapshot(&mut direct)
    );

    // Boundary zero takes a projection of the initial adapter rather than the
    // post-state of an action, so exercise that separate path too.
    let mut initial = replay_test_app(&replay, &map_data);
    ReplayRewindCommand { target_index: 0 }.apply(rewound.world_mut());
    assert_eq!(
        semantic_snapshot(&mut rewound),
        semantic_snapshot(&mut initial)
    );
    apply_settled_action(&mut rewound, &replay, 0);
    apply_settled_action(&mut initial, &replay, 0);
    assert_eq!(
        semantic_snapshot(&mut rewound),
        semantic_snapshot(&mut initial)
    );
}

fn replay_test_app(replay: &awbw_replay::AwbwReplay, map_data: &AwbwMapData) -> App {
    let adapter = RecordedAdapter::new(replay, map_data).unwrap();
    let mut graphical_map = AwbwMap::try_from(map_data).unwrap();
    apply_replay_building_overrides(&mut graphical_map, &replay.games.first().unwrap().buildings);

    let mut app = App::new();
    app.add_plugins(GameWorldPlugin);
    app.world_mut()
        .resource_mut::<GameMap>()
        .set(AwbrnMap::from_map(&graphical_map));
    app.insert_resource(LoadedReplay(replay.clone()));
    app.insert_resource(ReplayTransitionSource::new(adapter));
    initialize_replay_semantic_world_for_client(app.world_mut());
    app
}

fn apply_settled_action(app: &mut App, replay: &awbw_replay::AwbwReplay, index: usize) {
    ReplayTurnCommand {
        action: replay.turns[index].clone(),
    }
    .apply(app.world_mut());
    if let Some(entity) = app.world().resource::<ReplayAdvanceLock>().active_entity() {
        let followup = app
            .world_mut()
            .resource_mut::<ReplayAdvanceLock>()
            .release_for(entity)
            .unwrap();
        app.world_mut()
            .entity_mut(entity)
            .remove::<UnitPathAnimation>();
        ReplayFollowupCommand {
            transitions: followup.transitions,
        }
        .apply(app.world_mut());
    }
    app.world_mut()
        .resource_mut::<ReplayState>()
        .next_action_index = (index + 1) as u32;
}

fn semantic_snapshot(app: &mut App) -> awbrn_game::snapshot::CanonicalReplaySnapshot {
    let snapshot = capture_game_snapshot(app.world_mut()).unwrap();
    let registry = app
        .world()
        .resource::<bevy::ecs::reflect::AppTypeRegistry>();
    canonicalize_replay_semantic_snapshot(&snapshot, &registry.read()).unwrap()
}

fn replay_fixture_path(file_name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/replays")
        .join(file_name)
}

fn map_fixture_path(file_name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/maps")
        .join(file_name)
}
