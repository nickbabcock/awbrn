use std::io::BufWriter;
use std::path::Path;

use awbrn_client::core::CorePlugin;
use awbrn_client::features::CurrentWeather;
use awbrn_client::loading::{LoadedReplay, apply_replay_building_overrides};
use awbrn_client::modes::replay::ReplayPlugin;
use awbrn_client::modes::replay::bootstrap::initialize_replay_semantic_world_for_client as initialize_replay_semantic_world;
use awbrn_client::modes::replay::commands::{
    ReplayAdvanceLock, ReplayFollowupCommand, ReplayTurnCommand,
};
use awbrn_client::render::UiAtlasResource;
use awbrn_client::snapshot::{CanonicalReplaySnapshot, canonicalize_replay_semantic_snapshot};
use awbrn_game::replay::ReplayState;
use awbrn_game::snapshot::capture_game_snapshot;
use awbrn_game::world::GameMap;
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
        awbrn_client::features::fog::FogPlugin,
    ));
    app.insert_resource(CurrentWeather::default());
    app.insert_resource(LoadedReplay(replay));
    insert_test_ui_atlas(&mut app);
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

        let snapshot = capture_game_snapshot(app.world_mut()).unwrap();
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

fn insert_test_ui_atlas(app: &mut App) {
    if !app
        .world()
        .contains_resource::<Assets<awbrn_client::UiAtlasAsset>>()
    {
        app.insert_resource(Assets::<awbrn_client::UiAtlasAsset>::default());
    }
    if !app
        .world()
        .contains_resource::<Assets<TextureAtlasLayout>>()
    {
        app.insert_resource(Assets::<TextureAtlasLayout>::default());
    }

    let atlas_handle = {
        let mut assets = app
            .world_mut()
            .resource_mut::<Assets<awbrn_client::UiAtlasAsset>>();
        assets.add(awbrn_client::UiAtlasAsset {
            size: awbrn_client::UiAtlasSize {
                width: 48,
                height: 16,
            },
            sprites: vec![
                awbrn_client::UiAtlasSprite {
                    name: "Arrow_Body.png".to_string(),
                    x: 0,
                    y: 0,
                    width: 16,
                    height: 16,
                },
                awbrn_client::UiAtlasSprite {
                    name: "Arrow_Curved.png".to_string(),
                    x: 16,
                    y: 0,
                    width: 16,
                    height: 16,
                },
                awbrn_client::UiAtlasSprite {
                    name: "Arrow_Tip.png".to_string(),
                    x: 32,
                    y: 0,
                    width: 16,
                    height: 16,
                },
            ],
        })
    };
    let layout_handle = {
        let mut layouts = app.world_mut().resource_mut::<Assets<TextureAtlasLayout>>();
        layouts.add(TextureAtlasLayout::from_grid(
            UVec2::new(16, 16),
            3,
            1,
            None,
            None,
        ))
    };

    app.world_mut().insert_resource(UiAtlasResource {
        handle: atlas_handle,
        texture: Handle::default(),
        layout: layout_handle,
    });
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
