use awbrn_core::{GraphicalTerrain, Terrain, Weather};
use awbrn_map::{AwbrnMap, AwbwMap, Position};
use bevy::prelude::*;
use std::fs;
use std::path::Path;

// Resource to track camera scale
#[derive(Resource)]
struct CameraScale(f32);

impl Default for CameraScale {
    fn default() -> Self {
        CameraScale(1.0)
    }
}

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
        .init_resource::<CameraScale>()
        .add_systems(Startup, (setup_camera, setup_map))
        .add_systems(Update, handle_camera_scaling)
        .run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn handle_camera_scaling(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut camera_scale: ResMut<CameraScale>,
    mut query: Query<&mut Transform, With<Camera>>,
) {
    let scale_delta = if keyboard_input.pressed(KeyCode::Equal) {
        0.05 // Scale up when plus is pressed
    } else if keyboard_input.pressed(KeyCode::Minus) {
        -0.05 // Scale down when minus is pressed
    } else {
        0.0 // No change
    };

    if scale_delta != 0.0 {
        // Update the camera scale resource
        camera_scale.0 = (camera_scale.0 + scale_delta).clamp(0.2, 3.0);

        // Apply the scale to the camera transform
        if let Ok(mut transform) = query.single_mut() {
            transform.scale = Vec3::splat(1.0 / camera_scale.0);
        }
    }
}

fn setup_map(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    // Get the workspace directory and asset paths
    let workspace_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let map_path = workspace_dir.join("assets/maps/162795.json");

    // Load and parse the map
    let awbrn_map = if map_path.exists() {
        // Load the map file content
        let map_data = fs::read_to_string(&map_path).expect("Failed to read map file");

        // Parse the JSON content - we need to pass this as a slice of bytes
        let awbw_map =
            AwbwMap::parse_json(map_data.as_bytes()).expect("Failed to parse map JSON data");

        // Convert to AwbrnMap
        AwbrnMap::from_map(&awbw_map)
    } else {
        // Fallback to a default map if file not found
        info!("Map file not found at {:?}, using default map", map_path);
        let default_terrain = GraphicalTerrain::Terrain(Terrain::Plain);
        AwbrnMap::new(10, 10, default_terrain)
    };

    // Log map info
    info!("Loaded map: {}x{}", awbrn_map.width(), awbrn_map.height());

    // Load the tileset
    let texture = asset_server.load("textures/tiles.png");
    let layout = TextureAtlasLayout::from_grid(UVec2::new(16, 32), 64, 27, None, None);
    let texture_atlas_layout = texture_atlas_layouts.add(layout);

    // Calculate grid display parameters
    let tile_size = 16.0; // Assuming 16x16 tiles
    let width = awbrn_map.width() as f32;
    let height = awbrn_map.height() as f32;
    let offset_x = -width * tile_size / 2.0 + tile_size / 2.0;
    let offset_y = height * 32.0 / 2.0 - 32.0 / 2.0;

    // Set a default weather
    let weather = Weather::Clear;

    // Spawn sprites for each map tile
    for y in 0..awbrn_map.height() {
        for x in 0..awbrn_map.width() {
            let pos_x = offset_x + x as f32 * tile_size;
            let pos_y = offset_y - y as f32 * tile_size;

            let position = Position::new(x, y);
            if let Some(terrain) = awbrn_map.terrain_at(position) {
                // Calculate sprite index for this terrain
                let sprite_index = awbrn_core::spritesheet_index(weather, terrain);

                // Spawn terrain sprite
                commands
                    .spawn(Sprite::from_atlas_image(
                        texture.clone(),
                        TextureAtlas {
                            layout: texture_atlas_layout.clone(),
                            index: sprite_index.index() as usize,
                        },
                    ))
                    .insert(Transform::from_xyz(pos_x, pos_y, 0.0));
            }
        }
    }
}
