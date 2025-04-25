#![allow(dead_code)]
use awbrn_core::PlayerFaction;
use awbrn_core::Unit;
use awbrn_core::{GraphicalTerrain, Terrain, Weather, unit_spritesheet_index};
use awbrn_map::{AwbrnMap, AwbwMap, Position};
use bevy::prelude::*;
use std::fs;
use std::path::Path;
use std::time::Duration;

mod grid;
use grid::GridSystem;

// Resource to track camera scale
#[derive(Resource)]
struct CameraScale(f32);

impl Default for CameraScale {
    fn default() -> Self {
        CameraScale(1.0) // Default to 1x zoom
    }
}

// Available zoom levels
const ZOOM_LEVELS: [f32; 3] = [1.0, 1.5, 2.0];

// Resource to track current weather
#[derive(Resource)]
struct CurrentWeather(Weather);

impl Default for CurrentWeather {
    fn default() -> Self {
        CurrentWeather(Weather::Clear)
    }
}

// Add a resource to store the loaded map
#[derive(Resource)]
struct GameMap(AwbrnMap);

impl Default for GameMap {
    fn default() -> Self {
        // This default is just a placeholder, it won't be used
        let default_terrain = GraphicalTerrain::Terrain(Terrain::Plain);
        GameMap(AwbrnMap::new(1, 1, default_terrain))
    }
}

// Component to store terrain data for each tile
#[derive(Component)]
struct TerrainTile {
    terrain: GraphicalTerrain,
    position: Position,
}

// Component to mark the currently selected tile
#[derive(Component)]
struct SelectedTile;

// Components for animation
#[derive(Component)]
struct Animation {
    start_index: usize,
    frames_count: usize,
    frame_time: Duration,
    timer: Timer,
    current_frame: usize,
}

#[derive(Component)]
struct AnimatedUnit;

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
        .init_resource::<CurrentWeather>()
        .init_resource::<GameMap>()
        .add_systems(Startup, (setup_camera, setup_map, spawn_animated_unit))
        .add_systems(
            Update,
            (
                handle_camera_scaling,
                handle_weather_toggle,
                update_sprites_on_weather_change,
                handle_tile_clicks,
                animate_units,
            ),
        )
        .run();
}

fn setup_camera(mut commands: Commands, camera_scale: Res<CameraScale>) {
    commands.spawn((
        Camera2d,
        Transform::from_scale(Vec3::splat(1.0 / camera_scale.0)),
    ));
}

fn handle_camera_scaling(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut camera_scale: ResMut<CameraScale>,
    mut query: Query<&mut Transform, With<Camera>>,
) {
    // Check if we should change zoom level
    let zoom_change = if keyboard_input.just_pressed(KeyCode::Equal) {
        // Zoom in (move to next higher zoom level)
        1
    } else if keyboard_input.just_pressed(KeyCode::Minus) {
        // Zoom out (move to next lower zoom level)
        -1
    } else {
        0 // No change
    };

    if zoom_change != 0 {
        // Find current zoom level index
        let current_zoom = camera_scale.0;
        let mut current_index = ZOOM_LEVELS
            .iter()
            .position(|&z| (z - current_zoom).abs() < 0.01)
            .unwrap_or(0);

        // Move to next/previous zoom level
        if zoom_change > 0 {
            // Zoom in - move to next higher zoom level
            current_index = (current_index + 1).min(ZOOM_LEVELS.len() - 1);
        } else {
            // Zoom out - move to next lower zoom level
            current_index = current_index.saturating_sub(1);
        }

        // Get the new zoom level
        let new_zoom = ZOOM_LEVELS[current_index];

        // Update the camera scale resource
        camera_scale.0 = new_zoom;

        info!("Camera zoom level changed to {:.1}x", new_zoom);

        // Apply the scale to the camera transform
        if let Ok(mut transform) = query.single_mut() {
            transform.scale = Vec3::splat(1.0 / new_zoom);
        }
    }
}

fn handle_weather_toggle(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut current_weather: ResMut<CurrentWeather>,
) {
    if keyboard_input.just_pressed(KeyCode::Space) {
        // Toggle between Clear and Snow weather
        current_weather.0 = match current_weather.0 {
            Weather::Clear => Weather::Snow,
            Weather::Snow => Weather::Rain,
            Weather::Rain => Weather::Clear,
        };
        info!("Weather changed to: {:?}", current_weather.0);
    }
}

fn update_sprites_on_weather_change(
    current_weather: Res<CurrentWeather>,
    mut query: Query<(&mut Sprite, &TerrainTile)>,
) {
    if current_weather.is_changed() {
        for (mut sprite, terrain_tile) in query.iter_mut() {
            let sprite_index =
                awbrn_core::spritesheet_index(current_weather.0, terrain_tile.terrain);
            if let Some(atlas) = &mut sprite.texture_atlas {
                atlas.index = sprite_index.index() as usize;
            }
        }
    }
}

// Handling sprite picking using direct mouse input
fn handle_tile_clicks(
    mouse_button_input: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    tiles: Query<(Entity, &Transform, &TerrainTile)>,
    mut commands: Commands,
    selected: Query<Entity, With<SelectedTile>>,
) {
    // Only process on mouse click
    if !mouse_button_input.just_pressed(MouseButton::Left) {
        return;
    }

    // Get the primary window
    let Ok(window) = windows.single() else {
        return;
    };

    // Get the cursor position in the window
    let Some(cursor_position) = window.cursor_position() else {
        return;
    };

    // Get camera transform
    let Ok((camera, camera_transform)) = camera_q.single() else {
        return;
    };

    // Convert cursor position to world coordinates
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_position) else {
        return;
    };

    // Since we're in 2D, we can just use the ray's origin as our world position
    let world_position = ray.origin.truncate();

    // Remove the selection component from any previously selected tile
    for entity in selected.iter() {
        commands.entity(entity).remove::<SelectedTile>();
    }

    // Find the tile closest to the click position
    let mut closest_distance = f32::MAX;
    let mut closest_entity = None;

    for (entity, transform, tile) in tiles.iter() {
        let tile_pos = transform.translation.truncate();
        let distance = world_position.distance(tile_pos);

        // We consider this a hit if it's the closest one so far
        if distance < closest_distance {
            closest_distance = distance;
            closest_entity = Some((entity, tile));
        }
    }

    // If we found a tile and it's within a reasonable distance, mark it as selected
    if let Some((entity, tile)) = closest_entity {
        if closest_distance < 16.0 {
            // Assuming the tile size is approximately 16 pixels
            commands.entity(entity).insert(SelectedTile);
            info!(
                "Selected terrain at {:?}: {:?}",
                tile.position, tile.terrain
            );
        }
    }
}

fn animate_units(time: Res<Time>, mut query: Query<(&mut Animation, &mut Sprite)>) {
    for (mut animation, mut sprite) in query.iter_mut() {
        // Update the timer
        animation.timer.tick(time.delta());

        // Check if we need to advance to the next frame
        if animation.timer.just_finished() {
            // Move to next frame
            animation.current_frame = (animation.current_frame + 1) % animation.frames_count;

            // Update the sprite's texture atlas index
            if let Some(atlas) = &mut sprite.texture_atlas {
                atlas.index = animation.start_index + animation.current_frame;
            }
        }
    }
}

fn setup_map(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    current_weather: Res<CurrentWeather>,
    mut game_map: ResMut<GameMap>,
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

    // Store the map in the resource
    game_map.0 = awbrn_map.clone();

    // Load the tileset
    let texture = asset_server.load("textures/tiles.png");
    let layout = TextureAtlasLayout::from_grid(UVec2::new(16, 32), 64, 27, None, None);
    let texture_atlas_layout = texture_atlas_layouts.add(layout);

    // Create a grid system for positioning (now returns local coords)
    let grid = GridSystem::new(awbrn_map.width(), awbrn_map.height());

    // Calculate the offset needed to center the map in Bevy's world coordinates
    let map_pixel_width = grid.map_width * GridSystem::TILE_SIZE;
    let map_pixel_height = grid.map_height * GridSystem::TILE_SIZE;
    // Bevy's origin is center, Y increases upwards.
    // Our local grid origin is top-left, Y increases downwards.
    // We want the center of our grid to align with Bevy's center (0,0).
    // The top-left corner of our grid in Bevy coordinates should be:
    let world_origin_offset = Vec3::new(-map_pixel_width / 2.0, map_pixel_height / 2.0, 0.0);

    // Spawn sprites for each map tile
    for y in 0..awbrn_map.height() {
        for x in 0..awbrn_map.width() {
            let position = Position::new(x, y);
            if let Some(terrain) = awbrn_map.terrain_at(position) {
                // Calculate sprite index for this terrain
                let sprite_index = awbrn_core::spritesheet_index(current_weather.0, terrain);

                // Create a grid position for this terrain tile
                let grid_pos = grid.terrain_position(x, y);

                // Convert to local world position (relative to top-left 0,0, Y down)
                let local_pos = grid.grid_to_world(&grid_pos);

                // Adjust local position to Bevy world coordinates:
                // 1. Flip the Y coordinate (local Y down -> Bevy Y up)
                // 2. Add the centering offset
                let final_world_pos =
                    world_origin_offset + Vec3::new(local_pos.x, -local_pos.y, local_pos.z);

                // Spawn terrain sprite with position information
                commands.spawn((
                    Sprite::from_atlas_image(
                        texture.clone(),
                        TextureAtlas {
                            layout: texture_atlas_layout.clone(),
                            index: sprite_index.index() as usize,
                        },
                    ),
                    // Use the calculated final world position
                    Transform::from_translation(final_world_pos),
                    TerrainTile {
                        terrain,
                        position: Position::new(x, y),
                    },
                ));
            }
        }
    }
}

fn spawn_animated_unit(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    game_map: Res<GameMap>,
) {
    // Load the units texture
    let texture = asset_server.load("textures/units.png");

    // Create the texture atlas layout for units
    // The sprites are 23x24 with 86 rows and 64 columns
    let layout = TextureAtlasLayout::from_grid(UVec2::new(23, 24), 64, 86, None, None);
    let texture_atlas_layout = texture_atlas_layouts.add(layout);

    // Calculate the start index for the animation
    let start_index = unit_spritesheet_index(
        awbrn_core::GraphicalMovement::None,
        Unit::Infantry,
        PlayerFaction::BlackHole,
    );

    // Define the grid position for the unit
    let x = 13;
    let y = 9;

    // Create a grid system
    let grid = GridSystem::new(game_map.0.width(), game_map.0.height());

    // Calculate the centering offset (same logic as in setup_map)
    let map_pixel_width = grid.map_width * GridSystem::TILE_SIZE;
    let map_pixel_height = grid.map_height * GridSystem::TILE_SIZE;
    let world_origin_offset = Vec3::new(-map_pixel_width / 2.0, map_pixel_height / 2.0, 0.0);

    // Create a grid position for the unit (with southeast gravity)
    let grid_pos = grid.unit_position(x, y);

    // Convert to local world position (relative to top-left 0,0, Y down)
    let local_pos = grid.grid_to_world(&grid_pos);

    // Adjust local position to Bevy world coordinates
    let final_world_pos = world_origin_offset + Vec3::new(local_pos.x, -local_pos.y, local_pos.z);

    info!(
        "Spawning unit at grid position ({}, {}), local pos: {:?}, final world pos: {:?}",
        x, y, local_pos, final_world_pos
    );

    // Create the animation component
    let animation = Animation {
        start_index: start_index.index() as usize,
        frames_count: start_index.animation_frames() as usize,
        frame_time: Duration::from_millis(750 / start_index.animation_frames() as u64),
        timer: Timer::new(
            Duration::from_millis(750 / start_index.animation_frames() as u64),
            TimerMode::Repeating,
        ),
        current_frame: 0,
    };

    // Spawn the animated unit
    commands.spawn((
        Sprite::from_atlas_image(
            texture,
            TextureAtlas {
                layout: texture_atlas_layout,
                index: start_index.index() as usize,
            },
        ),
        // Use the calculated final world position
        Transform::from_translation(final_world_pos),
        animation,
        AnimatedUnit,
    ));
}
