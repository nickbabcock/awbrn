use std::io::BufWriter;
use std::path::Path;

use awbrn_bevy::GameWorldPlugin;
use awbrn_bevy::replay::{ReplayState, ReplayViewpoint, initialize_replay_semantic_world};
use awbrn_bevy::snapshot::{GameSnapshot, capture_game_snapshot, write_replay_semantic_snapshot};
use awbrn_bevy::world::GameMap;
use awbrn_client::loading::apply_replay_building_overrides;
use awbrn_client::modes::replay::presentation::{
    ReplayAdvanceLock, ReplayFollowupCommand, ReplayTransitionSource, ReplayTurnCommand,
};
use awbrn_map::{AwbrnMap, AwbwMap, AwbwMapData};
use awbw_replay::ReplayParser;
use awvm_awbw::RecordedAdapter;
use bevy::prelude::*;
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

macro_rules! replay_semantic_snapshot {
    ($test_name:ident, $replay_file:literal, $map_file:literal) => {
        #[test]
        fn $test_name() {
            let rows = replay_semantic_snapshot_rows($replay_file, $map_file);
            assert_json_snapshot!(rows);
        }
    };
}

replay_semantic_snapshot!(
    replay_semantic_snapshots_1362397,
    "1362397.zip",
    "162795.json"
);
replay_semantic_snapshot!(
    replay_semantic_snapshots_1391406,
    "1391406.zip",
    "146471.json"
);
replay_semantic_snapshot!(
    replay_semantic_snapshots_1403019,
    "1403019.zip",
    "168602.json"
);
replay_semantic_snapshot!(
    replay_semantic_snapshots_1419680,
    "1419680.zip",
    "73021.json"
);
replay_semantic_snapshot!(
    replay_semantic_snapshots_1468032,
    "1468032_landfall_2025-12-22.zip",
    "108806.json"
);
replay_semantic_snapshot!(
    replay_semantic_snapshots_1563018,
    "1563018.zip",
    "96502.json"
);
replay_semantic_snapshot!(
    replay_semantic_snapshots_1578186,
    "replay_1578186_d-day_2026-01-14.zip",
    "67073.json"
);
replay_semantic_snapshot!(
    replay_semantic_snapshots_1699315,
    "replay_1699315_missle-bomb_2026-07-28.zip",
    "178597.json"
);

fn replay_semantic_snapshot_rows(replay_file: &str, map_file: &str) -> Vec<ReplaySnapshotRow> {
    let replay_bytes = std::fs::read(replay_fixture_path(replay_file)).unwrap();
    let replay = ReplayParser::new().parse(&replay_bytes).unwrap();

    let map_path = map_fixture_path(map_file);
    let map_data: AwbwMapData = serde_json::from_slice(&std::fs::read(map_path).unwrap()).unwrap();
    let mut awbw_map = AwbwMap::try_from(&map_data).unwrap();
    apply_replay_building_overrides(&mut awbw_map, &replay.games.first().unwrap().buildings);

    let mut app = App::new();
    app.add_plugins(GameWorldPlugin);
    app.world_mut()
        .resource_mut::<GameMap>()
        .set(AwbrnMap::from_map(&awbw_map));

    initialize_replay_semantic_world(&replay, app.world_mut());

    let actions = replay.turns.clone();
    let adapter = RecordedAdapter::new(&replay, &map_data).unwrap();
    app.insert_resource(ReplayTransitionSource::new(adapter));
    app.insert_resource(awbrn_client::loading::LoadedReplay(
        awbrn_client::replay_archive::ReplayArchive::Awbw(replay.clone()),
    ));
    app.insert_resource(ReplayAdvanceLock::default());
    app.insert_resource(ReplayViewpoint::Spectator);
    let last_index = actions.len().saturating_sub(1);
    // Digests are checkpointed at turn starts instead of taken per action.
    // World state is cumulative, so a divergence anywhere inside a turn still
    // shows up at the next checkpoint, and capturing plus canonicalizing every
    // unit and terrain tile on all ~13k archived actions dominated this
    // suite's runtime.
    let mut checkpoint = turn_key(app.world());
    let mut rows = Vec::new();
    for (action_index, action) in actions.into_iter().enumerate() {
        let action_kind = action.kind_name();
        ReplayTurnCommand {
            index: action_index,
        }
        .apply(app.world_mut());
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
        app.world_mut()
            .resource_mut::<ReplayState>()
            .next_action_index += 1;

        let post_key = turn_key(app.world());
        if post_key == checkpoint && action_index != last_index {
            continue;
        }
        checkpoint = post_key;

        let snapshot = capture_game_snapshot(app.world_mut()).unwrap_or_else(|error| {
            panic!(
                "{replay_file} could not snapshot action {action_index} ({action_kind}): {error}"
            )
        });
        let day = snapshot.day;
        let type_registry = app.world().resource::<AppTypeRegistry>().read();
        let checksum = checksum(&snapshot, &type_registry).unwrap_or_else(|error| {
            panic!(
                "{replay_file} could not canonicalize action {action_index} \
                 ({action_kind}): {error}"
            )
        });
        rows.push(ReplaySnapshotRow {
            action_index,
            day,
            action_kind,
            checksum,
        });
    }

    rows
}

/// The turn the world is in: digests are checkpointed when this changes.
fn turn_key(world: &World) -> (u32, Option<awbrn_types::AwbwGamePlayerId>) {
    let replay_state = world.resource::<ReplayState>();
    (replay_state.day, replay_state.active_player_id)
}

/// Digest the canonical replay-semantic form without materializing it: the
/// value is only ever hashed, and building the tree first dominated this
/// suite's runtime.
fn checksum(
    snapshot: &GameSnapshot,
    type_registry: &bevy::reflect::TypeRegistry,
) -> Result<String, awbrn_bevy::snapshot::GameSnapshotError> {
    let hasher = highway::HighwayHasher::new(highway::Key::default());
    let mut writer = BufWriter::with_capacity(0x8000, hasher);
    write_replay_semantic_snapshot(snapshot, type_registry, &mut writer)?;
    let hash = writer.into_inner().unwrap().finalize256();
    Ok(format!(
        "0x{:016x}{:016x}{:016x}{:016x}",
        hash[0], hash[1], hash[2], hash[3]
    ))
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
