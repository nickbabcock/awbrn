use std::io::BufWriter;
use std::path::Path;

use awbrn_bevy::core::{CorePlugin, map::GameMap};
use awbrn_bevy::features::CurrentWeather;
use awbrn_bevy::loading::{LoadedReplay, apply_replay_building_overrides};
use awbrn_bevy::modes::replay::ReplayPlugin;
use awbrn_bevy::modes::replay::ReplayState;
use awbrn_bevy::modes::replay::bootstrap::initialize_replay_semantic_world;
use awbrn_bevy::modes::replay::commands::{
    ReplayAdvanceLock, ReplayFollowupCommand, ReplayTurnCommand,
};
use awbrn_bevy::snapshot::{
    CanonicalReplaySnapshot, canonicalize_replay_semantic_snapshot,
    capture_replay_semantic_snapshot,
};
use awbrn_map::{AwbrnMap, AwbwMap, AwbwMapData};
use awbw_replay::ReplayParser;
use bevy::ecs::reflect::AppTypeRegistry;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use highway::HighwayHash;
use insta::assert_json_snapshot;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct ReplaySnapshotRow {
    action_index: usize,
    day: u32,
    action_kind: &'static str,
    checksum: String,
}

#[test]
fn replay_semantic_snapshots_1362397() {
    let replay_bytes = std::fs::read(replay_fixture_path("1362397.zip")).unwrap();
    let replay = ReplayParser::new().parse(&replay_bytes).unwrap();

    let map_path = map_fixture_path("162795.json");
    let map_data: AwbwMapData = serde_json::from_slice(&std::fs::read(map_path).unwrap()).unwrap();
    let mut awbw_map = AwbwMap::try_from(&map_data).unwrap();
    apply_replay_building_overrides(&mut awbw_map, &replay.games.first().unwrap().buildings);

    let mut app = App::new();
    app.add_plugins((
        StatesPlugin,
        CorePlugin,
        ReplayPlugin,
        awbrn_bevy::features::fog::FogPlugin,
    ));
    app.insert_resource(CurrentWeather::default());
    app.insert_resource(LoadedReplay(replay));
    app.world_mut()
        .resource_mut::<GameMap>()
        .set(AwbrnMap::from_map(&awbw_map));

    initialize_replay_semantic_world(app.world_mut());

    let actions = app.world().resource::<LoadedReplay>().0.turns.clone();
    let mut rows = Vec::with_capacity(actions.len());
    for (action_index, action) in actions.into_iter().enumerate() {
        ReplayTurnCommand {
            action: action.clone(),
        }
        .apply(app.world_mut());
        // The replay controls own cursor advancement in the real app before they queue the
        // command. The command itself only mutates semantic world state, so the headless harness
        // mirrors the control-layer cursor update here.
        app.world_mut()
            .resource_mut::<ReplayState>()
            .next_action_index += 1;

        settle_replay_semantics(app.world_mut());

        let snapshot = capture_replay_semantic_snapshot(app.world_mut()).unwrap();
        let type_registry = app.world().resource::<AppTypeRegistry>().read();
        let canonical = canonicalize_replay_semantic_snapshot(&snapshot, &type_registry).unwrap();
        rows.push(ReplaySnapshotRow {
            action_index,
            day: canonical.day,
            action_kind: action.kind_name(),
            checksum: checksum(&canonical),
        });
    }

    assert_json_snapshot!(rows);
}

fn settle_replay_semantics(world: &mut World) {
    loop {
        let active_entity = world.resource::<ReplayAdvanceLock>().active_entity();
        let Some(active_entity) = active_entity else {
            break;
        };

        let deferred_action = {
            let mut replay_lock = world.resource_mut::<ReplayAdvanceLock>();
            replay_lock.release_for(active_entity)
        };

        if let Some(followup) = deferred_action {
            ReplayFollowupCommand {
                action: followup.action,
                recompute_fog: followup.recompute_fog,
            }
            .apply(world);
        }
    }
}

fn checksum(snapshot: &CanonicalReplaySnapshot) -> String {
    let hasher = highway::HighwayHasher::new(highway::Key::default());
    let mut writer = BufWriter::with_capacity(0x8000, hasher);
    serde_json::to_writer(&mut writer, snapshot).unwrap();
    let hash = writer.into_inner().unwrap().finalize256();
    format!(
        "0x{:016x}{:016x}{:016x}{:016x}",
        hash[0], hash[1], hash[2], hash[3]
    )
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
