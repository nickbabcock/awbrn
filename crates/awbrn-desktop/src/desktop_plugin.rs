use crate::web_asset_plugin::WebMapAssetPathResolver;
use awbrn_client::{
    AwbrnPlugin, EventSink, MapDimensions, NewDay, PlayerRosterSnapshot, ReplayLoaded,
    TileSelected, UnitBuilt, UnitMoved,
};
use bevy::{asset::AssetMetaCheck, prelude::*};
use std::{fs, sync::Arc};

pub struct AwbrnDesktopPlugin;

impl Plugin for AwbrnDesktopPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(
            DefaultPlugins
                .set(ImagePlugin::default_nearest())
                .set(AssetPlugin {
                    file_path: String::from("../../assets"),
                    meta_check: AssetMetaCheck::Never,
                    ..AssetPlugin::default()
                }),
        )
        .add_plugins(AwbrnPlugin::new(Arc::new(WebMapAssetPathResolver)))
        .insert_resource(EventSink::<NewDay>::new(|e| {
            info!("New Day: Day {}", e.day);
        }))
        .insert_resource(EventSink::<UnitMoved>::new(|e| {
            info!(
                "Unit {} moved from ({}, {}) to ({}, {})",
                e.unit_id, e.from_x, e.from_y, e.to_x, e.to_y
            );
        }))
        .insert_resource(EventSink::<UnitBuilt>::new(|e| {
            info!(
                "Unit {} ({}) built at ({}, {}) by player {}",
                e.unit_id, e.unit_type, e.x, e.y, e.player_id
            );
        }))
        .insert_resource(EventSink::<TileSelected>::new(|e| {
            info!(
                "Tile selected at ({}, {}) - terrain: {}",
                e.x, e.y, e.terrain_type
            );
        }))
        .insert_resource(EventSink::<MapDimensions>::new(|e| {
            info!("Map dimensions: {}x{}", e.width, e.height);
        }))
        .insert_resource(EventSink::<ReplayLoaded>::new(|e| {
            info!(
                "Replay loaded: game {} map {} with {} players",
                e.game_id,
                e.map_id,
                e.players.len()
            );
        }))
        .insert_resource(EventSink::<PlayerRosterSnapshot>::new(|_| {}))
        .add_systems(Update, handle_file_drop);
    }
}

fn handle_file_drop(mut commands: Commands, mut file_drop_events: MessageReader<FileDragAndDrop>) {
    for event in file_drop_events.read() {
        let FileDragAndDrop::DroppedFile { path_buf, .. } = event else {
            continue;
        };

        let data = match fs::read(path_buf) {
            Ok(data) => data,
            Err(e) => {
                error!("Failed to read file: {:?}", e);
                continue;
            }
        };

        commands.insert_resource(awbrn_client::ReplayToLoad(data));
    }
}
