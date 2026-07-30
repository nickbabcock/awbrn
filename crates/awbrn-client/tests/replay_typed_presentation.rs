use std::path::Path;

use awbrn_client::loading::apply_replay_building_overrides;
use awbrn_client::modes::replay::navigation::PendingCourseArrows;
use awbrn_client::modes::replay::presentation::{
    ReplayAdvanceLock, ReplayFollowupCommand, ReplayTransitionSource, ReplayTurnCommand,
};
use awbrn_client::render::animation::UnitPathAnimation;
use awbrn_game::GameWorldPlugin;
use awbrn_game::replay::{ReplayViewpoint, initialize_replay_semantic_world};
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
    let adapter = RecordedAdapter::new(&replay, &map_data).unwrap();

    let mut graphical_map = AwbwMap::try_from(&map_data).unwrap();
    apply_replay_building_overrides(&mut graphical_map, &replay.games.first().unwrap().buildings);

    let mut app = App::new();
    app.add_plugins(GameWorldPlugin);
    app.world_mut()
        .resource_mut::<GameMap>()
        .set(AwbrnMap::from_map(&graphical_map));
    initialize_replay_semantic_world(&replay, app.world_mut());
    app.insert_resource(ReplayTransitionSource::new(adapter));
    app.insert_resource(ReplayAdvanceLock::default());
    app.insert_resource(ReplayViewpoint::Spectator);

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

/// Power charge is an authoritative AWVM fact projected to every player, so the
/// presentation cache must track it rather than derive it from unit costs.
#[test]
fn typed_transitions_record_public_power_charge() {
    use awbrn_client::features::player_roster::PlayerPowerCharges;

    let replay_bytes = std::fs::read(replay_fixture_path("1362397.zip")).unwrap();
    let replay = ReplayParser::new().parse(&replay_bytes).unwrap();
    let map_data: AwbwMapData =
        serde_json::from_slice(&std::fs::read(map_fixture_path("162795.json")).unwrap()).unwrap();
    let adapter = RecordedAdapter::new(&replay, &map_data).unwrap();

    let mut graphical_map = AwbwMap::try_from(&map_data).unwrap();
    apply_replay_building_overrides(&mut graphical_map, &replay.games.first().unwrap().buildings);

    let mut app = App::new();
    app.add_plugins(GameWorldPlugin);
    app.world_mut()
        .resource_mut::<GameMap>()
        .set(AwbrnMap::from_map(&graphical_map));
    initialize_replay_semantic_world(&replay, app.world_mut());
    app.insert_resource(ReplayTransitionSource::new(adapter));
    app.insert_resource(ReplayAdvanceLock::default());
    app.insert_resource(ReplayViewpoint::Spectator);
    app.insert_resource(PlayerPowerCharges::default());

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

        let charges = app.world().resource::<PlayerPowerCharges>();
        assert_eq!(
            charges.0.len(),
            replay.games.first().unwrap().players.len(),
            "every applied transition reports a charge for each player"
        );
        if charges.0.values().any(|charge| *charge > 0) {
            charged = true;
            break;
        }
    }

    assert!(
        charged,
        "combat in the fixture should raise at least one player's power charge"
    );
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
