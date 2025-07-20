//! Bevy plugin for AWBRN with support for multiple game modes.
//!
//! ```mermaid
//! stateDiagram-v2
//!     [*] --> Menu
//!
//!     state AppState {
//!         Menu --> Loading : ReplayToLoad resource<br/>or PendingGameStart resource
//!         Loading --> InGame : LoadingState Complete
//!         InGame --> Menu : User action
//!
//!         state Loading {
//!             [*] --> LoadingReplay : Replay mode
//!             [*] --> LoadingAssets : Game mode or<br/>after replay parsed
//!             LoadingReplay --> LoadingAssets : Replay parsed<br/>map loading starts
//!             LoadingAssets --> Complete : Map loaded
//!             Complete --> [*] : Transition to InGame
//!         }
//!     }
//!
//!     state GameMode {
//!         None --> Replay : ReplayToLoad resource
//!         None --> Game : PendingGameStart resource
//!         Replay --> None : Reset
//!         Game --> None : Reset
//!     }
//!
//!     note right of GameMode : Independent state<br/>determines active systems<br/>in InGame
//! ```

use crate::{
    CameraScale, CurrentWeather, GameMap, GridSystem, JsonAssetPlugin, SelectedTile, TerrainTile,
};
use awbrn_core::{Weather, get_unit_animation_frames};
use awbrn_map::{AwbrnMap, AwbwMap, AwbwMapData, Position};
use awbw_replay::{AwbwReplay, ReplayParser};
use bevy::prelude::*;
use bevy::state::state::SubStates;
use serde::Deserialize;
use std::{sync::Arc, time::Duration};

/// Trait for resolving map asset paths from map IDs
pub trait MapAssetPathResolver: Send + Sync + 'static {
    fn resolve_path(&self, map_id: u32) -> String;
}

// Define AwbwMap as an Asset
#[derive(Asset, TypePath, Deserialize)]
#[serde(transparent)]
pub struct AwbwMapAsset(AwbwMapData);

impl AwbwMapAsset {
    // Convert to AwbwMap
    fn to_awbw_map(&self) -> AwbwMap {
        AwbwMap::try_from(&self.0).unwrap()
    }
}

// Components for animation
#[derive(Component)]
struct Animation {
    start_index: u16,
    frame_durations: [u16; 4], // Duration in milliseconds for each frame
    current_frame: u8,
    frame_timer: Timer,
}

#[derive(Component)]
struct TerrainAnimation {
    start_index: u16,
    frame_count: u8,
    current_frame: u8,
    frame_timer: Timer,
}

#[derive(Component)]
struct AnimatedUnit;

#[derive(Component)]
struct AnimatedTerrain;

#[derive(States, Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
pub enum AppState {
    #[default]
    Menu,
    Loading,
    InGame,
}

#[derive(States, Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
pub enum GameMode {
    #[default]
    None,
    Replay,
    Game,
}

#[derive(SubStates, Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
#[source(AppState = AppState::Loading)]
pub enum LoadingState {
    #[default]
    LoadingReplay,
    LoadingAssets,
    Complete,
}

// Resource containing the raw replay data to parse and load
#[derive(Resource)]
pub struct ReplayToLoad(pub Vec<u8>);

// Resource containing the loaded replay data
#[derive(Resource)]
pub struct LoadedReplay(pub AwbwReplay);

// Resource to mark that a new game should be started
#[derive(Resource)]
pub struct PendingGameStart(pub Handle<AwbwMapAsset>);

pub struct AwbrnPlugin {
    map_resolver: Arc<dyn MapAssetPathResolver>,
}

impl AwbrnPlugin {
    pub fn new(map_resolver: Arc<dyn MapAssetPathResolver>) -> Self {
        Self { map_resolver }
    }
}

impl Plugin for AwbrnPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(JsonAssetPlugin::<AwbwMapAsset>::new())
            .init_resource::<CameraScale>()
            .init_resource::<CurrentWeather>()
            .init_resource::<GameMap>()
            .init_state::<AppState>()
            .init_state::<GameMode>()
            .add_sub_state::<LoadingState>()
            .insert_resource(MapPathResolver(self.map_resolver.clone()))
            .add_systems(Startup, setup_camera)
            // Shared systems (run in any game mode)
            .add_systems(
                Update,
                (
                    handle_camera_scaling,
                    handle_weather_toggle,
                    update_sprites_on_weather_change,
                    handle_tile_clicks,
                    animate_units,
                    animate_terrain,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            // Replay-specific systems
            .add_systems(
                Update,
                check_map_asset_loaded.run_if(in_state(LoadingState::LoadingAssets)),
            )
            // Game mode setup systems
            .add_systems(
                OnEnter(LoadingState::Complete),
                (setup_map_visuals, spawn_animated_unit)
                    .chain()
                    .run_if(in_state(GameMode::Replay)),
            )
            .add_systems(
                OnEnter(LoadingState::Complete),
                setup_map_visuals.run_if(in_state(GameMode::Game)),
            )
            // Game-specific systems (would handle unit movement, turn logic, etc.)
            .add_systems(
                Update,
                (
                    // Placeholder for game-specific systems
                    handle_game_input,
                )
                    .run_if(in_state(GameMode::Game).and(in_state(AppState::InGame))),
            )
            // Replay-specific systems (would handle replay controls, playback, etc.)
            .add_systems(
                Update,
                (
                    // Placeholder for replay-specific systems
                    handle_replay_controls,
                )
                    .run_if(in_state(GameMode::Replay).and(in_state(AppState::InGame))),
            )
            // Resource-based detection systems
            .add_systems(Update, (detect_replay_to_load, detect_pending_game_start))
            // State transition systems
            .add_systems(OnEnter(LoadingState::Complete), transition_to_in_game);
    }
}

// Resource to store the map resolver
#[derive(Resource, Clone)]
struct MapPathResolver(Arc<dyn MapAssetPathResolver>);

// System to transition from loading complete to in-game
fn transition_to_in_game(mut next_app_state: ResMut<NextState<AppState>>) {
    next_app_state.set(AppState::InGame);
}

// Resource-based detection systems for managing game modes
fn detect_replay_to_load(
    mut commands: Commands,
    replay_to_load: Option<Res<ReplayToLoad>>,
    mut app_state: ResMut<NextState<AppState>>,
    mut game_mode_state: ResMut<NextState<GameMode>>,
    mut loading_state: ResMut<NextState<LoadingState>>,
    map_resolver: Res<MapPathResolver>,
    asset_server: Res<AssetServer>,
) {
    let Some(replay_res) = replay_to_load else {
        return;
    };
    commands.remove_resource::<ReplayToLoad>();

    let parser = ReplayParser::new();
    let replay = match parser.parse(&replay_res.0) {
        Ok(replay) => replay,
        Err(e) => {
            error!("Failed to parse replay: {:?}", e);
            return;
        }
    };

    // Start loading the map for the first game
    if let Some(first_game) = replay.games.first() {
        let map_id = first_game.maps_id;
        info!("Found map ID: {:?} in replay", map_id);

        let asset_path = map_resolver.0.resolve_path(map_id.as_u32());
        let map_handle: Handle<AwbwMapAsset> = asset_server.load(asset_path);
        commands.insert_resource(MapAssetHandle(map_handle));
    } else {
        error!("No games found in replay");
        let asset_path = map_resolver.0.resolve_path(162795);
        let map_handle: Handle<AwbwMapAsset> = asset_server.load(asset_path);
        commands.insert_resource(MapAssetHandle(map_handle));
    }

    // Store the parsed replay data directly as a resource
    commands.insert_resource(LoadedReplay(replay));
    game_mode_state.set(GameMode::Replay);
    app_state.set(AppState::Loading);
    loading_state.set(LoadingState::LoadingAssets);
    info!("Started loading replay mode");
}

fn detect_pending_game_start(
    mut commands: Commands,
    pending_game: Option<Res<PendingGameStart>>,
    mut app_state: ResMut<NextState<AppState>>,
    mut game_mode_state: ResMut<NextState<GameMode>>,
    mut loading_state: ResMut<NextState<LoadingState>>,
) {
    let Some(pending) = pending_game else { return };
    commands.insert_resource(MapAssetHandle(pending.0.clone()));
    commands.remove_resource::<PendingGameStart>();
    game_mode_state.set(GameMode::Game);
    app_state.set(AppState::Loading);
    loading_state.set(LoadingState::LoadingAssets);
    info!("Started game mode");
}

fn setup_camera(mut commands: Commands, camera_scale: Res<CameraScale>) {
    commands.spawn((
        Camera2d,
        Transform::from_scale(Vec3::splat(1.0 / camera_scale.scale())),
        Msaa::Off, // https://github.com/bevyengine/bevy/discussions/3748#discussioncomment-5565500
    ));
}

// Resource to hold the map handle
#[derive(Resource)]
struct MapAssetHandle(Handle<AwbwMapAsset>);

// System to check if map asset is loaded and then transition state
fn check_map_asset_loaded(
    map_handle: Res<MapAssetHandle>,
    awbw_maps: Res<Assets<AwbwMapAsset>>,
    mut game_map: ResMut<GameMap>,
    mut next_state: ResMut<NextState<LoadingState>>,
) {
    let Some(awbw_map_asset) = awbw_maps.get(&map_handle.0) else {
        return;
    };

    let awbw_map = awbw_map_asset.to_awbw_map();
    let awbrn_map = AwbrnMap::from_map(&awbw_map);

    info!(
        "Map asset processed: {}x{}. Transitioning to Complete state.",
        awbrn_map.width(),
        awbrn_map.height()
    );

    game_map.set(awbrn_map);
    next_state.set(LoadingState::Complete);
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
    mut static_query: Query<(&mut Sprite, &TerrainTile), Without<TerrainAnimation>>,
    mut animated_query: Query<
        (&mut Sprite, &TerrainTile, &mut TerrainAnimation),
        With<TerrainAnimation>,
    >,
) {
    if current_weather.is_changed() {
        // Update static terrain sprites
        for (mut sprite, terrain_tile) in static_query.iter_mut() {
            let sprite_index =
                awbrn_core::spritesheet_index(current_weather.weather(), terrain_tile.terrain);
            if let Some(atlas) = &mut sprite.texture_atlas {
                atlas.index = sprite_index.index() as usize;
            }
        }

        // Update animated terrain sprites and their animation data
        for (mut sprite, terrain_tile, mut animation) in animated_query.iter_mut() {
            let sprite_index =
                awbrn_core::spritesheet_index(current_weather.weather(), terrain_tile.terrain);

            // Update animation parameters
            animation.start_index = sprite_index.index();
            animation.frame_count = sprite_index.animation_frames();
            animation.current_frame = 0;

            // Update sprite to show first frame of new weather
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
        animation.frame_timer.tick(time.delta());

        // Check if we need to advance to the next frame
        if animation.frame_timer.just_finished() {
            // Find the next frame with non-zero duration
            let start_frame = animation.current_frame;
            loop {
                animation.current_frame =
                    (animation.current_frame + 1) % animation.frame_durations.len() as u8;

                // If we've cycled back to start, break to avoid infinite loop
                if animation.current_frame == start_frame {
                    break;
                }

                // If we found a frame with non-zero duration, use it
                if animation.frame_durations[animation.current_frame as usize] > 0 {
                    break;
                }
            }

            // Update the sprite's texture atlas index
            if let Some(atlas) = &mut sprite.texture_atlas {
                atlas.index = animation.start_index as usize + animation.current_frame as usize;
            }

            // Set the timer for the next frame duration
            let next_duration = animation.frame_durations[animation.current_frame as usize];
            if next_duration > 0 {
                animation.frame_timer =
                    Timer::new(Duration::from_millis(next_duration as u64), TimerMode::Once);
            }
        }
    }
}

fn animate_terrain(time: Res<Time>, mut query: Query<(&mut TerrainAnimation, &mut Sprite)>) {
    for (mut animation, mut sprite) in query.iter_mut() {
        animation.frame_timer.tick(time.delta());

        // Check if we need to advance to the next frame
        if animation.frame_timer.just_finished() {
            // Move to the next frame, cycling back to 0 when we reach the end
            animation.current_frame = (animation.current_frame + 1) % animation.frame_count;

            // Update the sprite's texture atlas index
            if let Some(atlas) = &mut sprite.texture_atlas {
                atlas.index = animation.start_index as usize + animation.current_frame as usize;
            }

            animation.frame_timer = Timer::new(Duration::from_millis(300), TimerMode::Once);
        }
    }
}

// Placeholder system for game-specific input handling
fn handle_game_input() {
    // TODO: Implement game-specific input handling
    // This would handle things like:
    // - Unit selection and movement
    // - Turn management
    // - Game actions (attack, wait, etc.)
}

// Placeholder system for replay-specific controls
fn handle_replay_controls() {
    // TODO: Implement replay-specific controls
    // This would handle things like:
    // - Play/pause replay
    // - Step forward/backward through turns
    // - Replay speed control
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

                // Check if terrain has multiple animation frames
                let mut entity_commands = commands.spawn((
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

                // Add animation component if terrain has multiple frames
                if sprite_index.animation_frames() > 1 {
                    entity_commands.insert((
                        TerrainAnimation {
                            start_index: sprite_index.index(),
                            frame_count: sprite_index.animation_frames(),
                            current_frame: 0,
                            frame_timer: Timer::new(Duration::from_millis(300), TimerMode::Once),
                        },
                        AnimatedTerrain,
                    ));
                }
            }
        }
    }
}

fn spawn_animated_unit(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    game_map: Res<GameMap>,
    loaded_replay: Res<LoadedReplay>,
) {
    // Load the units texture
    let texture = asset_server.load("textures/units.png");

    // Create the texture atlas layout for units
    // The sprites are 23x24 with 86 rows and 64 columns
    let layout = TextureAtlasLayout::from_grid(UVec2::new(23, 24), 64, 86, None, Some(uvec2(1, 0)));
    let texture_atlas_layout = texture_atlas_layouts.add(layout);

    // Create a grid system
    let grid = GridSystem::new(game_map.width(), game_map.height());

    // Calculate the centering offset (same logic as in setup_map)
    let map_pixel_width = grid.map_width * GridSystem::TILE_SIZE;
    let map_pixel_height = grid.map_height * GridSystem::TILE_SIZE;
    let world_origin_offset = Vec3::new(-map_pixel_width / 2.0, map_pixel_height / 2.0, 0.0);

    // Get units from replay data
    let (players, replay_units) = if let Some(first_game) = loaded_replay.0.games.first() {
        // Collect all units
        info!("Found {} units in replay", first_game.units.len());
        (&first_game.players, &first_game.units)
    } else {
        info!("No games found in replay, not spawning units");
        return;
    };

    // Spawn units from replay data
    for unit in replay_units {
        // Get the unit position
        let x = unit.x as usize;
        let y = unit.y as usize;

        // Get player faction
        let faction = players
            .iter()
            .find(|x| x.id == unit.players_id)
            .unwrap()
            .faction;

        // Get animation data using the new system
        let animation_frames =
            get_unit_animation_frames(awbrn_core::GraphicalMovement::Idle, unit.name, faction);

        // Create a grid position for the unit (with southeast gravity)
        let grid_pos = grid.unit_position(x, y);

        // Convert to local world position (relative to top-left 0,0, Y down)
        let local_pos = grid.grid_to_world(&grid_pos);

        // Adjust local position to Bevy world coordinates
        let final_world_pos =
            world_origin_offset + Vec3::new(local_pos.x, -local_pos.y, local_pos.z);

        info!(
            "Spawning {} unit at grid position ({}, {}), HP: {}, frame count: {}",
            unit.name.name(),
            x,
            y,
            unit.hit_points,
            animation_frames.frame_count()
        );

        // Create the animation component with variable frame timing
        let frame_durations = animation_frames.raw();

        let animation = Animation {
            start_index: animation_frames.start_index(),
            frame_durations,
            current_frame: 0,
            frame_timer: Timer::new(
                Duration::from_millis(frame_durations[0] as u64),
                TimerMode::Once,
            ),
        };

        // Spawn the animated unit
        commands.spawn((
            Sprite::from_atlas_image(
                texture.clone(),
                TextureAtlas {
                    layout: texture_atlas_layout.clone(),
                    index: animation_frames.start_index() as usize,
                },
            ),
            // Use the calculated final world position
            Transform::from_translation(final_world_pos),
            animation,
            AnimatedUnit,
        ));
    }
}

/// Default implementation of MapAssetPathResolver that formats paths as "maps/{map_id}.json"
pub struct DefaultMapAssetPathResolver;

impl MapAssetPathResolver for DefaultMapAssetPathResolver {
    fn resolve_path(&self, map_id: u32) -> String {
        format!("maps/{}.json", map_id)
    }
}

impl Default for AwbrnPlugin {
    fn default() -> Self {
        Self {
            map_resolver: Arc::new(DefaultMapAssetPathResolver),
        }
    }
}
