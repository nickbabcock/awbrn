use crate::web_asset_plugin::{WebAssetPlugin, WebMapAssetPathResolver};
use awbrn_bevy::{AwbrnPlugin, ReplayToLoad};
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

fn handle_file_drop(mut commands: Commands, mut file_drop_events: EventReader<FileDragAndDrop>) {
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

        // Signal that a new replay should be loaded (parsing will happen in Bevy)
        commands.insert_resource(ReplayToLoad(data));
    }
}
