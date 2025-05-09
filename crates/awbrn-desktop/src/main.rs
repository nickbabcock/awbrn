use awbrn_bevy::{AppState, AwbrnPlugin, AwbwReplayAsset, ReplayAssetHandle};
use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(ImagePlugin::default_nearest())
                .set(AssetPlugin {
                    file_path: String::from("../../assets"),
                    ..AssetPlugin::default()
                }),
        )
        .add_plugins(AwbrnPlugin)
        .add_systems(Startup, load_replay_system)
        .run();
}

fn load_replay_system(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    info!("Loading replay file...");

    // Read the replay file from assets (hardcoded for now)
    let replay_path = "replays/1362397.zip";

    // Use the asset server to load the replay file as an AwbwReplayAsset
    let replay_handle: Handle<AwbwReplayAsset> = asset_server.load(replay_path);

    // Store the handle to check when it's loaded
    commands.insert_resource(ReplayAssetHandle(replay_handle));

    next_state.set(AppState::LoadingReplay);
}
