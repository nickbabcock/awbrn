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
        .add_systems(Startup, (setup_camera, setup_test_sprite))
        .run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn setup_test_sprite(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let texture = asset_server.load("textures/tiles.png");
    let layout = TextureAtlasLayout::from_grid(UVec2::new(16, 32), 64, 27, None, None);
    let texture_atlas_layout = texture_atlas_layouts.add(layout);
    commands.spawn(Sprite::from_atlas_image(
        texture,
        TextureAtlas {
            layout: texture_atlas_layout,
            index: 1,
        },
    ));
}
