use crate::web_asset_plugin::{WebAssetPlugin, WebMapAssetPathResolver};
use awbrn_bevy::{AppState, AwbrnPlugin, AwbwReplayAsset, ReplayAssetHandle};
use awbw_replay::ReplayParser;
use bevy::{asset::AssetMetaCheck, prelude::*};
use std::{fs, sync::Arc};

pub struct AwbrnDesktopPlugin;

impl Plugin for AwbrnDesktopPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(WebAssetPlugin)
            .add_plugins(
                DefaultPlugins
                    .set(ImagePlugin::default_nearest())
                    .set(AssetPlugin {
                        file_path: String::from("../../assets"),
                        meta_check: AssetMetaCheck::Never,
                        ..AssetPlugin::default()
                    }),
            )
            .add_plugins(AwbrnPlugin::new(Arc::new(WebMapAssetPathResolver)))
            .add_systems(Update, handle_file_drop);
    }
}

fn handle_file_drop(
    mut commands: Commands,
    mut file_drop_events: EventReader<FileDragAndDrop>,
    mut assets: ResMut<Assets<AwbwReplayAsset>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
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

        let parser = ReplayParser::new();
        let replay = match parser.parse(&data) {
            Ok(replay) => replay,
            Err(e) => {
                error!("Failed to parse replay file: {:?}", e);
                continue;
            }
        };

        // Create a replay asset for Bevy's asset system
        let replay_asset = AwbwReplayAsset(replay);

        // Get a handle to the asset by adding it to the assets collection
        let handle = assets.add(replay_asset);

        commands.insert_resource(ReplayAssetHandle(handle));
        next_state.set(AppState::LoadingReplay);
    }
}
