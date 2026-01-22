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

use crate::replay_turn::{ReplayState, ReplayTurnCommand};
use crate::{
    AwbwUnitId, CameraScale, Capturing, CapturingIndicator, CargoIndicator, CurrentWeather,
    Faction, GameMap, GridSystem, HasCargo, JsonAssetPlugin, MapBackdrop, SelectedTile, SpriteSize,
    StrongIdMap, TerrainTile, UiAtlasAsset, UiAtlasResource, Unit,
};
use awbrn_core::{GraphicalTerrain, Weather, get_unit_animation_frames};
use awbrn_map::{AwbrnMap, AwbwMap, AwbwMapData, Position};
use awbw_replay::{AwbwReplay, ReplayParser};
use bevy::sprite::Anchor;
use bevy::state::state::SubStates;
use bevy::{log, prelude::*};
use serde::{Deserialize, Serialize};
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

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
#[require(Transform)]
pub struct MapPosition(pub Position);

impl MapPosition {
    pub fn new(x: usize, y: usize) -> Self {
        Self(Position::new(x, y))
    }

    pub fn x(&self) -> usize {
        self.0.x
    }

    pub fn y(&self) -> usize {
        self.0.y
    }

    pub fn position(&self) -> Position {
        self.0
    }
}

impl From<Position> for MapPosition {
    fn from(position: Position) -> Self {
        Self(position)
    }
}

impl From<MapPosition> for Position {
    fn from(position: MapPosition) -> Self {
        position.0
    }
}

impl AsRef<Position> for MapPosition {
    fn as_ref(&self) -> &Position {
        &self.0
    }
}

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

/// Type alias for external events containing game events
pub type ExternalGameEvent = ExternalEvent<GameEvent>;

/// Observer that triggers when Capturing component is removed - cleans up the indicator
fn on_capturing_remove(
    trigger: On<Remove, Capturing>,
    mut commands: Commands,
    query: Query<&CapturingIndicator>,
) {
    let entity = trigger.entity;

    // Get the indicator child entity if it exists
    if let Ok(indicator) = query.get(entity) {
        commands.entity(indicator.0).despawn();
        log::info!("Removed capturing indicator from entity {:?}", entity);
    }
}

/// Observer that triggers when HasCargo component is removed - cleans up the indicator
fn on_cargo_remove(
    trigger: On<Remove, HasCargo>,
    mut commands: Commands,
    query: Query<&CargoIndicator>,
) {
    let entity = trigger.entity;

    if let Ok(indicator) = query.get(entity) {
        commands.entity(indicator.0).despawn();
    }
}

/// System to automatically remove HasCargo component when it becomes empty.
/// Uses change detection to only check when HasCargo is modified.
fn cleanup_empty_cargo(
    mut commands: Commands,
    query: Query<(Entity, &HasCargo), Changed<HasCargo>>,
) {
    for (entity, has_cargo) in query.iter() {
        if has_cargo.is_empty() {
            commands.entity(entity).remove::<HasCargo>();
            log::info!(
                "Transport entity {:?} cargo is empty, removing HasCargo component",
                entity
            );
        }
    }
}

/// System to spawn capturing indicators when UI atlas is loaded
fn spawn_deferred_capturing_indicators(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    ui_atlas_assets: Res<Assets<UiAtlasAsset>>,
    ui_atlas_resource: Option<Res<UiAtlasResource>>,
    query: Query<Entity, (With<Capturing>, Without<CapturingIndicator>)>,
) {
    // Only process if we have entities waiting for indicators
    if query.is_empty() {
        return;
    }

    // Initialize or get resource
    let (atlas_handle, texture_handle, layout_handle) = if let Some(res) = &ui_atlas_resource {
        (res.handle.clone(), res.texture.clone(), res.layout.clone())
    } else {
        // First time - load assets
        let atlas: Handle<UiAtlasAsset> = asset_server.load("data/ui_atlas.json");
        let texture = asset_server.load("textures/ui.png");

        commands.insert_resource(UiAtlasResource {
            handle: atlas.clone(),
            texture: texture.clone(),
            layout: Handle::default(),
        });
        return; // Wait for next frame
    };

    // Wait for UI atlas to load
    let Some(ui_atlas) = ui_atlas_assets.get(&atlas_handle) else {
        return;
    };

    // Create layout if needed
    let layout = if layout_handle.id() == Handle::default().id() {
        let new_layout = ui_atlas.layout();
        let new_handle = texture_atlas_layouts.add(new_layout);

        // Update resource with new layout handle
        commands.insert_resource(UiAtlasResource {
            handle: atlas_handle.clone(),
            texture: texture_handle.clone(),
            layout: new_handle.clone(),
        });
        new_handle
    } else {
        layout_handle
    };

    // Get sprite index
    let index_map = ui_atlas.index_map();
    let Some(&sprite_index) = index_map.get("Capturing.png") else {
        log::error!("Capturing.png not found in UI atlas");
        return;
    };

    // Spawn indicators for all waiting entities
    for entity in query.iter() {
        let offset = Vec3::new(0.0, -8.0, 1.0);

        let indicator_entity = commands
            .spawn((
                Sprite::from_atlas_image(
                    texture_handle.clone(),
                    TextureAtlas {
                        layout: layout.clone(),
                        index: sprite_index,
                    },
                ),
                Transform::from_translation(offset),
                ChildOf(entity),
            ))
            .id();

        commands
            .entity(entity)
            .insert(CapturingIndicator(indicator_entity));

        log::info!("Spawned capturing indicator for entity {:?}", entity);
    }
}

/// System to spawn cargo indicators when UI atlas is loaded
fn spawn_deferred_cargo_indicators(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    ui_atlas_assets: Res<Assets<UiAtlasAsset>>,
    ui_atlas_resource: Option<Res<UiAtlasResource>>,
    query: Query<Entity, (With<HasCargo>, Without<CargoIndicator>)>,
) {
    // Only process if we have entities waiting for indicators
    if query.is_empty() {
        return;
    }

    // Initialize or get resource
    let (atlas_handle, texture_handle, layout_handle) = if let Some(res) = &ui_atlas_resource {
        (res.handle.clone(), res.texture.clone(), res.layout.clone())
    } else {
        // First time - load assets
        let atlas: Handle<UiAtlasAsset> = asset_server.load("data/ui_atlas.json");
        let texture = asset_server.load("textures/ui.png");

        commands.insert_resource(UiAtlasResource {
            handle: atlas.clone(),
            texture: texture.clone(),
            layout: Handle::default(),
        });
        return; // Wait for next frame
    };

    // Wait for UI atlas to load
    let Some(ui_atlas) = ui_atlas_assets.get(&atlas_handle) else {
        return;
    };

    // Create layout if needed
    let layout = if layout_handle.id() == Handle::default().id() {
        let new_layout = ui_atlas.layout();
        let new_handle = texture_atlas_layouts.add(new_layout);

        // Update resource with new layout handle
        commands.insert_resource(UiAtlasResource {
            handle: atlas_handle.clone(),
            texture: texture_handle.clone(),
            layout: new_handle.clone(),
        });
        new_handle
    } else {
        layout_handle
    };

    // Get sprite index
    let index_map = ui_atlas.index_map();
    let Some(&sprite_index) = index_map.get("HasCargo.png") else {
        log::error!("HasCargo.png not found in UI atlas");
        return;
    };

    // Spawn indicators for all waiting entities
    for entity in query.iter() {
        let offset = Vec3::new(0.0, -8.0, 1.0);

        let indicator_entity = commands
            .spawn((
                Sprite::from_atlas_image(
                    texture_handle.clone(),
                    TextureAtlas {
                        layout: layout.clone(),
                        index: sprite_index,
                    },
                ),
                Transform::from_translation(offset),
                ChildOf(entity),
            ))
            .id();

        commands
            .entity(entity)
            .insert(CargoIndicator(indicator_entity));

        log::info!("Spawned cargo indicator for entity {:?}", entity);
    }
}

pub struct AwbrnPlugin {
    map_resolver: Arc<dyn MapAssetPathResolver>,
    event_bus: Option<Arc<dyn EventBus<GameEvent>>>,
}

impl AwbrnPlugin {
    pub fn new(map_resolver: Arc<dyn MapAssetPathResolver>) -> Self {
        Self {
            map_resolver,
            event_bus: None,
        }
    }

    pub fn with_event_bus(mut self, event_bus: Arc<dyn EventBus<GameEvent>>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }
}

impl Plugin for AwbrnPlugin {
    fn build(&self, app: &mut App) {
        let mut app_builder = app
            .add_plugins(JsonAssetPlugin::<AwbwMapAsset>::new())
            .add_plugins(JsonAssetPlugin::<UiAtlasAsset>::new())
            .init_resource::<CameraScale>()
            .init_resource::<CurrentWeather>()
            .init_resource::<GameMap>()
            .init_resource::<StrongIdMap<AwbwUnitId>>()
            .init_state::<AppState>()
            .init_state::<GameMode>()
            .add_sub_state::<LoadingState>()
            .insert_resource(MapPathResolver(self.map_resolver.clone()))
            .add_message::<ExternalGameEvent>()
            .add_observer(on_map_position_insert)
            .add_observer(handle_unit_spawn)
            .add_observer(on_capturing_remove)
            .add_observer(on_cargo_remove)
            .add_systems(Startup, setup_camera);

        // Only add event bus if provided
        if let Some(ref event_bus) = self.event_bus {
            app_builder = app_builder
                .insert_resource(EventBusResource(event_bus.clone()))
                .add_systems(Update, event_forwarder::<GameEvent>);
        }

        app_builder
            // Shared systems (run in any game mode)
            .add_systems(
                Update,
                (
                    handle_camera_scaling,
                    handle_weather_toggle,
                    update_backdrop_on_weather_change,
                    update_static_terrain_on_weather_change,
                    update_animated_terrain_on_weather_change,
                    handle_tile_clicks,
                    animate_units,
                    animate_terrain,
                    spawn_deferred_capturing_indicators,
                    spawn_deferred_cargo_indicators,
                    cleanup_empty_cargo,
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
                (setup_map_visuals, spawn_animated_unit, init_replay_state)
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

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct NewDay {
    pub day: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct UnitMoved {
    pub unit_id: u32,
    pub from_x: usize,
    pub from_y: usize,
    pub to_x: usize,
    pub to_y: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct UnitBuilt {
    pub unit_id: u32,
    pub unit_type: String,
    pub x: usize,
    pub y: usize,
    pub player_id: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct TileSelected {
    pub x: usize,
    pub y: usize,
    pub terrain_type: String,
}

/// Union type for all game events that can be sent to external systems
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(tag = "type")]
pub enum GameEvent {
    NewDay(NewDay),
    UnitMoved(UnitMoved),
    UnitBuilt(UnitBuilt),
    TileSelected(TileSelected),
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

fn update_backdrop_on_weather_change(
    current_weather: Res<CurrentWeather>,
    mut backdrop_query: Query<&mut Sprite, (With<MapBackdrop>, Without<TerrainTile>)>,
) {
    if !current_weather.is_changed() {
        return;
    }

    let plain_index =
        awbrn_core::spritesheet_index(current_weather.weather(), GraphicalTerrain::Plain);

    for mut sprite in backdrop_query.iter_mut() {
        if let Some(atlas) = &mut sprite.texture_atlas {
            atlas.index = plain_index.index() as usize;
        }
    }
}

type StaticTerrainQuery<'w, 's> = Query<
    'w,
    's,
    (&'static mut Sprite, &'static TerrainTile),
    (Without<TerrainAnimation>, Without<MapBackdrop>),
>;

fn update_static_terrain_on_weather_change(
    current_weather: Res<CurrentWeather>,
    mut static_query: StaticTerrainQuery,
) {
    if !current_weather.is_changed() {
        return;
    }

    for (mut sprite, terrain_tile) in static_query.iter_mut() {
        let sprite_index =
            awbrn_core::spritesheet_index(current_weather.weather(), terrain_tile.terrain);
        if let Some(atlas) = &mut sprite.texture_atlas {
            atlas.index = sprite_index.index() as usize;
        }
    }
}

type AnimatedTerrainQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Sprite,
        &'static TerrainTile,
        &'static mut TerrainAnimation,
    ),
    (With<TerrainAnimation>, Without<MapBackdrop>),
>;

fn update_animated_terrain_on_weather_change(
    current_weather: Res<CurrentWeather>,
    mut animated_query: AnimatedTerrainQuery,
) {
    if !current_weather.is_changed() {
        return;
    }

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

// Handling sprite picking using direct mouse input
fn handle_tile_clicks(
    mouse_button_input: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    tiles: Query<(Entity, &Transform, &TerrainTile)>,
    mut commands: Commands,
    selected: Query<Entity, With<SelectedTile>>,
    mut event_writer: MessageWriter<ExternalGameEvent>,
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
    if let Some((entity, tile)) = closest_entity
        && closest_distance < 16.0
    {
        // Assuming the tile size is approximately 16 pixels
        commands.entity(entity).insert(SelectedTile);
        info!(
            "Selected terrain at {:?}: {:?}",
            tile.position, tile.terrain
        );

        // Send tile selected event
        event_writer.write(ExternalGameEvent {
            payload: GameEvent::TileSelected(TileSelected {
                x: tile.position.x,
                y: tile.position.y,
                terrain_type: format!("{:?}", tile.terrain),
            }),
        });
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

fn handle_replay_controls(
    mut commands: Commands,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut replay_state: ResMut<ReplayState>,
    loaded_replay: Res<LoadedReplay>,
) {
    if !keyboard_input.just_pressed(KeyCode::ArrowRight) {
        return;
    }

    let Some(action) = loaded_replay
        .0
        .turns
        .get(replay_state.turn as usize)
        .cloned()
    else {
        info!("Reached the end of the replay turns");
        return;
    };

    commands.queue(ReplayTurnCommand { action });
    replay_state.turn += 1;
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
    let layout = TextureAtlasLayout::from_grid(
        UVec2::new(16, 32),
        awbrn_core::TILESHEET_COLUMNS,
        awbrn_core::TILESHEET_ROWS,
        None,
        None,
    );
    let texture_atlas_layout = texture_atlas_layouts.add(layout);
    let plain_index =
        awbrn_core::spritesheet_index(current_weather.weather(), GraphicalTerrain::Plain);

    // Spawn sprites for each map tile
    for y in 0..game_map.height() {
        for x in 0..game_map.width() {
            let position = Position::new(x, y);
            if let Some(terrain) = game_map.terrain_at(position) {
                commands.spawn((
                    Sprite::from_atlas_image(
                        texture.clone(),
                        TextureAtlas {
                            layout: texture_atlas_layout.clone(),
                            index: plain_index.index() as usize,
                        },
                    ),
                    Anchor::default(),
                    MapPosition::new(x, y),
                    MapBackdrop,
                ));

                // Calculate sprite index for this terrain
                let sprite_index =
                    awbrn_core::spritesheet_index(current_weather.weather(), terrain);

                // Create the terrain entity with MapPosition - Transform is automatically required and will be updated by sync system
                let mut entity_commands = commands.spawn((
                    Sprite::from_atlas_image(
                        texture.clone(),
                        TextureAtlas {
                            layout: texture_atlas_layout.clone(),
                            index: sprite_index.index() as usize,
                        },
                    ),
                    Anchor::default(),
                    MapPosition::new(x, y), // Transform automatically included
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

fn spawn_animated_unit(mut commands: Commands, loaded_replay: Res<LoadedReplay>) {
    // Get units from replay data
    let (players, replay_units) = if let Some(first_game) = loaded_replay.0.games.first() {
        info!("Found {} units in replay", first_game.units.len());
        (&first_game.players, &first_game.units)
    } else {
        info!("No games found in replay, not spawning units");
        return;
    };

    // Create spawn requests for all units from replay data
    for unit in replay_units {
        let faction = players
            .iter()
            .find(|p| p.id == unit.players_id)
            .unwrap()
            .faction;

        commands.spawn((
            MapPosition::new(unit.x as usize, unit.y as usize),
            Faction(faction),
            AwbwUnitId(unit.id),
            Unit(unit.name),
        ));
    }
}

fn init_replay_state(mut commands: Commands) {
    commands.init_resource::<ReplayState>();
}

fn handle_unit_spawn(
    trigger: On<Insert, AwbwUnitId>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut map: ResMut<StrongIdMap<AwbwUnitId>>,
    mut query: Query<(&Unit, &Faction, &AwbwUnitId)>,
) {
    // Load the units texture
    let texture = asset_server.load("textures/units.png");

    // Create the texture atlas layout for units
    let layout = TextureAtlasLayout::from_grid(UVec2::new(23, 24), 64, 86, None, Some(uvec2(1, 0)));
    let texture_atlas_layout = texture_atlas_layouts.add(layout);

    let entity = trigger.entity;
    let Ok((unit, faction, unit_id)) = query.get_mut(entity) else {
        warn!("Unit entity {:?} not found in query", entity);
        return;
    };

    log::info!(
        "Spawning unit {:?} of type {:?} for faction {:?} at entity {:?}",
        unit_id,
        unit.0,
        faction.0,
        entity
    );

    map.insert(*unit_id, entity);

    // Get animation data
    let animation_frames =
        get_unit_animation_frames(awbrn_core::GraphicalMovement::Idle, unit.0, faction.0);

    // Create the animation component
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

    commands.entity(entity).insert((
        Sprite::from_atlas_image(
            texture,
            TextureAtlas {
                layout: texture_atlas_layout,
                index: animation_frames.start_index() as usize,
            },
        ),
        Anchor::default(),
        animation,
    ));
}

fn on_map_position_insert(
    trigger: On<Insert, MapPosition>,
    mut query: Query<(&mut Transform, &SpriteSize, &MapPosition)>,
    game_map: Res<GameMap>,
) {
    let entity = trigger.entity;

    let Ok((mut transform, sprite_size, map_position)) = query.get_mut(entity) else {
        warn!("Entity {:?} not found in query for MapPosition", entity);
        return;
    };

    // Create grid system for position calculations
    let grid = GridSystem::new(game_map.width(), game_map.height());
    let map_pixel_width = grid.map_width * GridSystem::TILE_SIZE;
    let map_pixel_height = grid.map_height * GridSystem::TILE_SIZE;
    let world_origin_offset = Vec3::new(-map_pixel_width / 2.0, map_pixel_height / 2.0, 0.0);

    // Use the grid system's sprite_position method
    let grid_pos = grid.sprite_position((*map_position).into(), sprite_size);

    let local_pos = grid.grid_to_world(&grid_pos);
    let z_offset = map_position.y() as f32 * 0.001;
    let final_world_pos =
        world_origin_offset + Vec3::new(local_pos.x, -local_pos.y, local_pos.z + z_offset);

    transform.translation = final_world_pos;

    info!(
        "Observer: Updated Transform for entity {:?} to position ({}, {}) -> {:?}",
        entity,
        map_position.x(),
        map_position.y(),
        final_world_pos
    );
}

pub trait EventBus<T: Serialize + Send + Sync + 'static>: Send + Sync {
    /// Publish an event to the bus
    fn publish_event(&self, payload: &ExternalEvent<T>);
}

#[derive(Resource)]
pub struct EventBusResource<T>(pub Arc<dyn EventBus<T>>);

impl<T> EventBusResource<T> {
    pub fn new(bus: Arc<dyn EventBus<T>>) -> Self {
        Self(bus)
    }
}

#[derive(Message, Debug, Clone)]
pub struct ExternalEvent<T: Serialize + Send + Sync + 'static> {
    pub payload: T,
}

pub fn event_forwarder<T: Serialize + Send + Sync + 'static>(
    mut events: MessageReader<ExternalEvent<T>>,
    bus: Option<Res<EventBusResource<T>>>,
) {
    let Some(bus) = bus else { return };

    for event in events.read() {
        bus.0.publish_event(event);
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
            event_bus: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test MapPosition -> Transform observer including updates
    #[test]
    fn test_map_position_observer() {
        let mut app = App::new();

        // Add required systems and resources
        app.add_observer(on_map_position_insert)
            .init_resource::<GameMap>();

        // Test Case 1: Initial spawn - observer should trigger immediately
        let terrain_entity = app
            .world_mut()
            .spawn((
                MapPosition::new(5, 3),
                TerrainTile {
                    terrain: awbrn_core::GraphicalTerrain::Plain,
                    position: Position::new(5, 3),
                },
            ))
            .id();

        let unit_entity = app
            .world_mut()
            .spawn((MapPosition::new(8, 2), Unit(awbrn_core::Unit::Infantry)))
            .id();

        // Run one update to process any events from observer
        app.update();

        // Verify initial positioning with snapshots
        let terrain_transform = *app
            .world()
            .entity(terrain_entity)
            .get::<Transform>()
            .unwrap();
        let unit_transform = *app.world().entity(unit_entity).get::<Transform>().unwrap();

        assert!(
            terrain_transform
                .translation
                .abs_diff_eq(Vec3::new(72.0, -32.0, 0.0), 0.1)
        );
        assert!(
            unit_transform
                .translation
                .abs_diff_eq(Vec3::new(116.5, -20.0, 1.0), 0.1)
        );

        // Test Case 2: Update MapPosition - observer should trigger on component replacement
        app.world_mut()
            .entity_mut(terrain_entity)
            .insert(MapPosition::new(1, 7));
        app.world_mut()
            .entity_mut(unit_entity)
            .insert(MapPosition::new(9, 1));

        app.update();

        // Verify updated positioning with snapshots
        let updated_terrain_transform = *app
            .world()
            .entity(terrain_entity)
            .get::<Transform>()
            .unwrap();
        let updated_unit_transform = *app.world().entity(unit_entity).get::<Transform>().unwrap();

        assert!(
            updated_terrain_transform
                .translation
                .abs_diff_eq(Vec3::new(8.0, -96.0, 0.0), 0.1)
        );
        assert!(
            updated_unit_transform
                .translation
                .abs_diff_eq(Vec3::new(132.5, -4.0, 1.0), 0.1)
        );

        // Verify positions actually changed
        assert_ne!(
            terrain_transform.translation, updated_terrain_transform.translation,
            "Terrain transform should change when MapPosition is updated"
        );
        assert_ne!(
            unit_transform.translation, updated_unit_transform.translation,
            "Unit transform should change when MapPosition is updated"
        );
    }
}
