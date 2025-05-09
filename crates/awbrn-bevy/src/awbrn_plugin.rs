use std::time::Duration;

use crate::{
    AwbwReplayAsset, CameraScale, CurrentWeather, GameMap, GridSystem, JsonAssetPlugin,
    ReplayAssetPlugin, SelectedTile, TerrainTile,
};
use awbrn_core::{PlayerFaction, Unit, Weather, unit_spritesheet_index};
use awbrn_map::{AwbrnMap, AwbwMap, AwbwMapData, Position};
use bevy::prelude::*;
use serde::Deserialize;

// Define AwbwMap as an Asset
#[derive(Asset, TypePath, Deserialize)]
#[serde(transparent)]
struct AwbwMapAsset(AwbwMapData);

impl AwbwMapAsset {
    // Convert to AwbwMap
    fn to_awbw_map(&self) -> AwbwMap {
        AwbwMap::try_from(&self.0).unwrap()
    }
}

// Components for animation
#[derive(Component)]
struct Animation {
    start_index: usize,
    frames_count: usize,
    #[expect(dead_code)]
    frame_time: Duration,
    timer: Timer,
    current_frame: usize,
}

#[derive(Component)]
struct AnimatedUnit;

#[derive(States, Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
pub enum AppState {
    #[default]
    Idle,
    LoadingReplay,
    LoadingAssets,
    MapLoaded,
}

pub struct AwbrnPlugin;

impl Plugin for AwbrnPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((JsonAssetPlugin::<AwbwMapAsset>::new(), ReplayAssetPlugin))
            .init_resource::<CameraScale>()
            .init_resource::<CurrentWeather>()
            .init_resource::<GameMap>()
            .init_state::<AppState>()
            .add_systems(Startup, setup_camera)
            .add_systems(
                Update,
                (
                    handle_camera_scaling,
                    handle_weather_toggle,
                    update_sprites_on_weather_change,
                    handle_tile_clicks,
                    animate_units,
                    check_map_asset_loaded.run_if(in_state(AppState::LoadingAssets)),
                    check_replay_loaded.run_if(in_state(AppState::LoadingReplay)),
                ),
            )
            .add_systems(
                OnEnter(AppState::MapLoaded),
                (setup_map_visuals, spawn_animated_unit),
            );
    }
}

fn setup_camera(mut commands: Commands, camera_scale: Res<CameraScale>) {
    commands.spawn((
        Camera2d,
        Transform::from_scale(Vec3::splat(1.0 / camera_scale.scale())),
        Msaa::Off, // https://github.com/bevyengine/bevy/discussions/3748#discussioncomment-5565500
    ));
}

// Resource to track the handle of the loading replay
#[derive(Resource)]
pub struct ReplayAssetHandle(pub Handle<AwbwReplayAsset>);

// System to check if the replay is loaded and process it
fn check_replay_loaded(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    replay_handle: Res<ReplayAssetHandle>,
    replay_assets: Res<Assets<AwbwReplayAsset>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    // Check if the replay asset has loaded
    if let Some(replay_asset) = replay_assets.get(&replay_handle.0) {
        // Get the parsed replay from the asset
        let replay = &replay_asset.0;

        // Check if we have at least one game
        if let Some(first_game) = replay.games.first() {
            let map_id = first_game.maps_id;
            info!("Found map ID: {:?} in replay", map_id);

            // Load the map using the asset system with the correct file name
            let map_name = format!("maps/{}.json", map_id.as_u32());
            let map_handle: Handle<AwbwMapAsset> = asset_server.load(&map_name);

            // Store the handle in a resource
            commands.insert_resource(MapAssetHandle(map_handle));

            // Transition to asset loading state
            next_state.set(AppState::LoadingAssets);
        } else {
            error!("No games found in replay");
            // Fall back to default map for now
            let map_handle: Handle<AwbwMapAsset> = asset_server.load("maps/162795.json");
            commands.insert_resource(MapAssetHandle(map_handle));
            next_state.set(AppState::LoadingAssets);
        }
    }
}

// Resource to hold the map handle
#[derive(Resource)]
struct MapAssetHandle(Handle<AwbwMapAsset>);

// System to check if map asset is loaded and then transition state
fn check_map_asset_loaded(
    map_handle: Res<MapAssetHandle>,
    awbw_maps: Res<Assets<AwbwMapAsset>>,
    mut game_map: ResMut<GameMap>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let Some(awbw_map_asset) = awbw_maps.get(&map_handle.0) else {
        return;
    };

    let awbw_map = awbw_map_asset.to_awbw_map();
    let awbrn_map = AwbrnMap::from_map(&awbw_map);

    info!(
        "Map asset processed: {}x{}. Transitioning to MapLoaded state.",
        awbrn_map.width(),
        awbrn_map.height()
    );

    game_map.set(awbrn_map);
    next_state.set(AppState::MapLoaded);
}

fn handle_camera_scaling(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut camera_scale: ResMut<CameraScale>,
    mut query: Query<&mut Transform, With<Camera>>,
) {
    let new_zoom = if keyboard_input.just_pressed(KeyCode::Equal) {
        camera_scale.zoom_in()
    } else if keyboard_input.just_pressed(KeyCode::Minus) {
        camera_scale.zoom_out()
    } else {
        return;
    };

    *camera_scale = new_zoom;

    // Apply the scale to the camera transform
    if let Ok(mut transform) = query.single_mut() {
        transform.scale = Vec3::splat(1.0 / camera_scale.scale());
    }
}

fn handle_weather_toggle(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut current_weather: ResMut<CurrentWeather>,
) {
    if keyboard_input.just_pressed(KeyCode::Space) {
        let new_weather = match current_weather.weather() {
            Weather::Clear => Weather::Snow,
            Weather::Snow => Weather::Rain,
            Weather::Rain => Weather::Clear,
        };

        current_weather.set(new_weather);
        info!("Weather changed to: {:?}", current_weather.weather());
    }
}

fn update_sprites_on_weather_change(
    current_weather: Res<CurrentWeather>,
    mut query: Query<(&mut Sprite, &TerrainTile)>,
) {
    if current_weather.is_changed() {
        for (mut sprite, terrain_tile) in query.iter_mut() {
            let sprite_index =
                awbrn_core::spritesheet_index(current_weather.weather(), terrain_tile.terrain);
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

// Extracted the map setup into a separate function for reuse
fn setup_map_visuals(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    current_weather: Res<CurrentWeather>,
    game_map: Res<GameMap>,
) {
    // Load the tileset
    let texture = asset_server.load("textures/tiles.png");
    let layout = TextureAtlasLayout::from_grid(UVec2::new(16, 32), 64, 27, None, None);
    let texture_atlas_layout = texture_atlas_layouts.add(layout);

    // Create a grid system for positioning using the retrieved AwbrnMap reference
    let grid = GridSystem::new(game_map.width(), game_map.height());

    // Calculate the offset needed to center the map in Bevy's world coordinates
    let map_pixel_width = grid.map_width * GridSystem::TILE_SIZE;
    let map_pixel_height = grid.map_height * GridSystem::TILE_SIZE;
    // Bevy's origin is center, Y increases upwards.
    // Our local grid origin is top-left, Y increases downwards.
    // We want the center of our grid to align with Bevy's center (0,0).
    // The top-left corner of our grid in Bevy coordinates should be:
    let world_origin_offset = Vec3::new(-map_pixel_width / 2.0, map_pixel_height / 2.0, 0.0);

    // Spawn sprites for each map tile
    for y in 0..game_map.height() {
        for x in 0..game_map.width() {
            let position = Position::new(x, y);
            if let Some(terrain) = game_map.terrain_at(position) {
                // Calculate sprite index for this terrain
                let sprite_index =
                    awbrn_core::spritesheet_index(current_weather.weather(), terrain);

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
    let grid = GridSystem::new(game_map.width(), game_map.height());

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
