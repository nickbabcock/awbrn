use awbrn_bevy::AwbrnPlugin;
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
        .run();
}
