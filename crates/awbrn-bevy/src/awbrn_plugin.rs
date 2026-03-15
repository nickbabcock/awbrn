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

use crate::replay_turn::{
    PendingCourseArrows, ReplayAdvanceLock, ReplayFollowupCommand, ReplayPathTile, ReplayState,
    ReplayTurnCommand, UnitPathAnimation, action_requires_path_animation, movement_direction,
    scaled_animation_duration,
};
use crate::{
    AwbwUnitId, CameraScale, Capturing, CapturingIndicator, CargoIndicator, CurrentWeather,
    Faction, GameMap, GraphicalHp, GridSystem, HasCargo, HealthIndicator, JsonAssetPlugin,
    MapBackdrop, SelectedTile, SpriteSize, StrongIdMap, TerrainTile, TileCursor, UiAtlasAsset,
    UiAtlasResource, Unit, UnitActive, UnitAtlasResource,
};
use awbrn_core::{GraphicalMovement, GraphicalTerrain, Weather, get_unit_animation_frames};
use awbrn_map::{AwbrnMap, AwbwMap, AwbwMapData, Position};
use awbw_replay::{AwbwReplay, ReplayParser, game_models::AwbwBuilding};
use bevy::ecs::system::SystemParam;
use bevy::input::{ButtonState, keyboard::KeyboardInput};
use bevy::sprite::Anchor;
use bevy::state::state::SubStates;
use bevy::{log, prelude::*};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};

/// Color used for inactive units
const INACTIVE_UNIT_COLOR: Color = Color::srgb(0.67, 0.67, 0.67);
const COURSE_ARROW_LAYER_OFFSET: f32 = 0.5;
const COURSE_ARROW_BASE_SCALE: f32 = 0.8;
const COURSE_ARROW_REVEAL_MS: u64 = 75;
const COURSE_ARROW_LIFETIME_MS: u64 = 250;
const COURSE_ARROW_STAGGER_MS: u64 = 25;
const COURSE_ARROW_SPRITE_SIZE: SpriteSize = SpriteSize {
    width: 16.0,
    height: 16.0,
    z_index: 0,
};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CourseArrowSpriteKind {
    Body,
    Curved,
    Tip,
}

impl CourseArrowSpriteKind {
    fn sprite_name(self) -> &'static str {
        match self {
            Self::Body => "Arrow_Body.png",
            Self::Curved => "Arrow_Curved.png",
            Self::Tip => "Arrow_Tip.png",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CourseArrowSpawn {
    kind: CourseArrowSpriteKind,
    position: Position,
    rotation_degrees: f32,
    start_delay: Duration,
}

#[allow(dead_code)]
#[derive(Component, Debug, Clone, Copy)]
struct CourseArrowPiece {
    owner: Entity,
    kind: CourseArrowSpriteKind,
    rotation_degrees: f32,
    start_delay: Duration,
    reveal_duration: Duration,
    total_duration: Duration,
    elapsed: Duration,
}

#[derive(Component, Reflect, Clone, Copy, PartialEq, Eq, Debug)]
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

#[derive(Default)]
struct ReplayControlState {
    suppress_exhausted_repeat: bool,
}

// Resource to mark that a new game should be started
#[derive(Resource)]
pub struct PendingGameStart(pub Handle<AwbwMapAsset>);

// Resource to hold UI atlas handles during loading
#[derive(Resource)]
struct PendingUiAtlas {
    atlas: Handle<UiAtlasAsset>,
    texture: Handle<Image>,
}

/// Type alias for external events containing game events
pub type ExternalGameEvent = ExternalEvent<GameEvent>;

/// System parameter that bundles UI atlas resource and assets for convenient access.
///
/// This allows systems and observers to access the UI atlas with a single parameter
/// and provides helper methods for creating sprites.
#[derive(SystemParam)]
struct UiAtlas<'w> {
    atlas_res: Res<'w, UiAtlasResource>,
    atlas_assets: Res<'w, Assets<UiAtlasAsset>>,
}

impl<'w> UiAtlas<'w> {
    pub fn cargo_sprite(&self) -> impl Bundle {
        (
            Transform::from_translation(Vec3::new(0.0, -8.0, 1.0)),
            self.sprite_for("HasCargo.png"),
        )
    }

    pub fn capturing_sprite(&self) -> impl Bundle {
        (
            Transform::from_translation(Vec3::new(0.0, -8.0, 1.0)),
            self.sprite_for("Capturing.png"),
        )
    }

    pub fn health_sprite(&self, sprite_name: &str) -> impl Bundle {
        (
            Transform::from_translation(Vec3::new(7.5, -8.0, 1.0)),
            self.sprite_for(sprite_name),
        )
    }

    /// Creates a sprite from the UI atlas for the given sprite name.
    ///
    /// # Panics
    ///
    /// Panics if the UI atlas is not loaded or if the sprite name is not found.
    /// This is acceptable because this should only be called after the UI atlas
    /// is fully loaded during the Complete loading state.
    fn sprite_for(&self, sprite_name: &str) -> Sprite {
        let ui_atlas = self
            .atlas_assets
            .get(&self.atlas_res.handle)
            .expect("UI atlas should be loaded");

        let index_map = ui_atlas.index_map();
        let sprite_index = *index_map
            .get(sprite_name)
            .unwrap_or_else(|| panic!("{} not found in UI atlas", sprite_name));

        Sprite::from_atlas_image(
            self.atlas_res.texture.clone(),
            TextureAtlas {
                layout: self.atlas_res.layout.clone(),
                index: sprite_index,
            },
        )
    }
}

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

/// Observer that triggers when Capturing component is inserted - spawns the indicator
fn on_capturing_insert(trigger: On<Insert, Capturing>, mut commands: Commands, ui_atlas: UiAtlas) {
    let entity = trigger.entity;

    let indicator_entity = commands
        .spawn((ui_atlas.capturing_sprite(), ChildOf(entity)))
        .id();

    commands
        .entity(entity)
        .insert(CapturingIndicator(indicator_entity));

    log::info!("Spawned capturing indicator for entity {:?}", entity);
}

/// Observer that triggers when HasCargo component is inserted - spawns the indicator
fn on_cargo_insert(trigger: On<Insert, HasCargo>, mut commands: Commands, ui_atlas: UiAtlas) {
    let entity = trigger.entity;

    let indicator_entity = commands
        .spawn((ui_atlas.cargo_sprite(), ChildOf(entity)))
        .id();

    commands
        .entity(entity)
        .insert(CargoIndicator(indicator_entity));

    log::info!("Spawned cargo indicator for entity {:?}", entity);
}

/// Observer that triggers when GraphicalHp component is inserted
fn on_health_insert(
    trigger: On<Insert, GraphicalHp>,
    mut commands: Commands,
    ui_atlas: UiAtlas,
    query: Query<&GraphicalHp>,
) {
    let entity = trigger.entity;

    // Get the HP value
    let Ok(hp) = query.get(entity) else {
        log::warn!("GraphicalHp component not found for entity {:?}", entity);
        return;
    };

    // Don't show indicator for full health (10 HP)
    if hp.is_full_health() {
        return;
    }

    // Don't show indicator for destroyed units (0 HP)
    if hp.is_destroyed() {
        log::warn!("Unit {:?} has 0 HP", entity);
        return;
    }

    // Spawn health indicator sprite
    let hp_value = hp.value();
    let sprite_name = format!("Healthv2/{}.png", hp_value);

    let indicator_entity = commands
        .spawn((ui_atlas.health_sprite(&sprite_name), ChildOf(entity)))
        .id();

    commands
        .entity(entity)
        .insert(HealthIndicator(indicator_entity));

    log::info!(
        "Spawned health indicator for entity {:?} with HP {}",
        entity,
        hp_value
    );
}

/// Observer that triggers when GraphicalHp component is removed
fn on_health_remove(
    trigger: On<Remove, GraphicalHp>,
    mut commands: Commands,
    query: Query<&HealthIndicator>,
) {
    let entity = trigger.entity;

    // Get the indicator child entity if it exists
    if let Ok(indicator) = query.get(entity) {
        commands.entity(indicator.0).despawn();
        commands.entity(entity).remove::<HealthIndicator>();
        log::info!("Removed health indicator from entity {:?}", entity);
    }
}

/// Observer that triggers when UnitActive component is removed - applies grey filter and freezes animation
fn on_unit_active_remove(
    trigger: On<Remove, UnitActive>,
    mut commands: Commands,
    mut query: Query<&mut Sprite>,
) {
    let entity = trigger.entity;

    let Ok(mut sprite) = query.get_mut(entity) else {
        return;
    };

    // Apply grey filter and stop animation
    sprite.color = INACTIVE_UNIT_COLOR;
    commands.entity(entity).remove::<Animation>();
}

/// Observer that triggers when UnitActive component is inserted - restores animation and color
fn on_unit_active_insert(
    trigger: On<Insert, UnitActive>,
    mut commands: Commands,
    mut query: Query<(&Unit, &Faction, &mut Sprite)>,
) {
    let entity = trigger.entity;

    let Ok((unit, faction, mut sprite)) = query.get_mut(entity) else {
        return;
    };

    // Restore normal color
    sprite.color = Color::WHITE;

    // Restore idle animation
    let animation_frames =
        get_unit_animation_frames(awbrn_core::GraphicalMovement::Idle, unit.0, faction.0);

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

    commands.entity(entity).insert(animation);
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

/// System to update health indicator when GraphicalHp value changes
fn update_health_indicator(
    mut commands: Commands,
    ui_atlas: UiAtlas,
    query: Query<(Entity, &GraphicalHp, Option<&HealthIndicator>), Changed<GraphicalHp>>,
) {
    for (entity, hp, indicator) in query.iter() {
        // Despawn old indicator if it exists
        if let Some(indicator) = indicator {
            commands.entity(indicator.0).despawn();
        }

        // If full health now, remove HealthIndicator component if present
        if hp.is_full_health() {
            if indicator.is_some() {
                commands.entity(entity).remove::<HealthIndicator>();
                log::info!(
                    "Unit {:?} restored to full health, removing indicator",
                    entity
                );
            }
            continue;
        }

        // If destroyed, remove indicator if present
        if hp.is_destroyed() {
            if indicator.is_some() {
                commands.entity(entity).remove::<HealthIndicator>();
            }
            log::warn!("Unit {:?} destroyed (0 HP)", entity);
            continue;
        }

        // Spawn new indicator with updated HP
        let hp_value = hp.value();
        let sprite_name = format!("Healthv2/{}.png", hp_value);

        let new_indicator = commands
            .spawn((ui_atlas.health_sprite(&sprite_name), ChildOf(entity)))
            .id();

        commands
            .entity(entity)
            .insert(HealthIndicator(new_indicator));

        log::info!(
            "Updated health indicator for entity {:?} to HP {}",
            entity,
            hp_value
        );
    }
}

/// System to update unit sprite when Faction changes
fn update_unit_on_faction_change(
    mut query: Query<(&Unit, &Faction, &mut Sprite, &mut Animation), Changed<Faction>>,
) {
    for (unit, faction, mut sprite, mut animation) in query.iter_mut() {
        // Get new animation frames for the updated faction
        let animation_frames =
            get_unit_animation_frames(awbrn_core::GraphicalMovement::Idle, unit.0, faction.0);

        // Update animation component
        animation.start_index = animation_frames.start_index();
        animation.frame_durations = animation_frames.raw();
        animation.current_frame = 0;
        animation.frame_timer = Timer::new(
            Duration::from_millis(animation_frames.raw()[0] as u64),
            TimerMode::Once,
        );

        // Update sprite to show first frame of new faction
        if let Some(atlas) = &mut sprite.texture_atlas {
            atlas.index = animation_frames.start_index() as usize;
        }

        log::info!(
            "Updated unit {:?} sprite to faction {:?}",
            unit.0,
            faction.0
        );
    }
}

/// System to update unit sprite when Unit type changes
fn update_unit_on_type_change(
    mut query: Query<(&Unit, &Faction, &mut Sprite, &mut Animation), Changed<Unit>>,
) {
    for (unit, faction, mut sprite, mut animation) in query.iter_mut() {
        // Get new animation frames for the updated unit type
        let animation_frames =
            get_unit_animation_frames(awbrn_core::GraphicalMovement::Idle, unit.0, faction.0);

        // Update animation component
        animation.start_index = animation_frames.start_index();
        animation.frame_durations = animation_frames.raw();
        animation.current_frame = 0;
        animation.frame_timer = Timer::new(
            Duration::from_millis(animation_frames.raw()[0] as u64),
            TimerMode::Once,
        );

        // Update sprite to show first frame of new unit type
        if let Some(atlas) = &mut sprite.texture_atlas {
            atlas.index = animation_frames.start_index() as usize;
        }

        log::info!(
            "Updated sprite to unit type {:?} for faction {:?}",
            unit.0,
            faction.0
        );
    }
}

/// Compute the world-space offset that centers the map's visual content on the
/// camera origin (0, 0). Sprites use center anchors, so we shift by half a tile
/// on x. Terrain sprites are 32 px tall on 16 px tiles, so the first row's tall
/// portion extends one full tile above the grid – shift by a full tile on y.
fn world_origin_offset(grid: &GridSystem) -> Vec3 {
    let map_pixel_width = grid.map_width * GridSystem::TILE_SIZE;
    let map_pixel_height = grid.map_height * GridSystem::TILE_SIZE;
    Vec3::new(
        -map_pixel_width / 2.0 + GridSystem::TILE_SIZE / 2.0,
        map_pixel_height / 2.0 - GridSystem::TILE_SIZE,
        0.0,
    )
}

fn map_position_to_world_translation(
    sprite_size: &SpriteSize,
    map_position: MapPosition,
    game_map: &GameMap,
) -> Vec3 {
    let grid = GridSystem::new(game_map.width(), game_map.height());
    let offset = world_origin_offset(&grid);
    let grid_pos = grid.sprite_position(map_position.into(), sprite_size);
    let local_pos = grid.grid_to_world(&grid_pos);
    let z_offset = map_position.y() as f32 * 0.001;

    offset + Vec3::new(local_pos.x, -local_pos.y, local_pos.z + z_offset)
}

fn position_to_world_translation(
    sprite_size: &SpriteSize,
    position: Position,
    game_map: &GameMap,
) -> Vec3 {
    map_position_to_world_translation(sprite_size, position.into(), game_map)
}

fn ease_out_quint(progress: f32) -> f32 {
    1.0 - (1.0 - progress.clamp(0.0, 1.0)).powi(5)
}

fn build_course_arrow_spawns(path: &[ReplayPathTile]) -> Vec<CourseArrowSpawn> {
    if path.len() < 2 {
        return Vec::new();
    }

    let mut spawns = Vec::with_capacity(path.len().saturating_sub(1));

    for i in 1..path.len() - 1 {
        let current = path[i];
        if !current.unit_visible {
            continue;
        }

        let prev = path[i - 1];
        let next = path[i + 1];
        let start_delay = scaled_animation_duration((i as u64 - 1) * COURSE_ARROW_STAGGER_MS);

        let (kind, rotation_degrees) = if !next.unit_visible {
            let head_diff_x = current.position.x as isize - prev.position.x as isize;
            let head_diff_y = current.position.y as isize - prev.position.y as isize;
            let rotation_degrees = if head_diff_x > 0 {
                90.0
            } else if head_diff_x < 0 {
                -90.0
            } else if head_diff_y > 0 {
                0.0
            } else {
                180.0
            };

            (CourseArrowSpriteKind::Tip, rotation_degrees)
        } else {
            let diff_x = next.position.x as isize - prev.position.x as isize;
            let diff_y = next.position.y as isize - prev.position.y as isize;

            if diff_x.abs() >= 2 {
                (
                    CourseArrowSpriteKind::Body,
                    if diff_x > 0 { 90.0 } else { -90.0 },
                )
            } else if diff_y.abs() >= 2 {
                (
                    CourseArrowSpriteKind::Body,
                    if diff_y > 0 { 180.0 } else { 0.0 },
                )
            } else {
                let prev_to_current_x = current.position.x as isize - prev.position.x as isize;
                let prev_to_current_y = current.position.y as isize - prev.position.y as isize;

                let connects_north = prev_to_current_y > 0
                    || current.position.y as isize - next.position.y as isize > 0;
                let connects_east = prev_to_current_x < 0
                    || next.position.x as isize - current.position.x as isize > 0;
                let connects_south = prev_to_current_y < 0
                    || next.position.y as isize - current.position.y as isize > 0;
                let connects_west = prev_to_current_x > 0
                    || current.position.x as isize - next.position.x as isize > 0;

                let rotation_degrees = if connects_west && connects_north {
                    0.0
                } else if connects_north && connects_east {
                    -90.0
                } else if connects_east && connects_south {
                    180.0
                } else if connects_south && connects_west {
                    90.0
                } else {
                    unreachable!("turn piece must connect exactly two orthogonal directions");
                };

                (CourseArrowSpriteKind::Curved, rotation_degrees)
            }
        };

        spawns.push(CourseArrowSpawn {
            kind,
            position: current.position,
            rotation_degrees,
            start_delay,
        });
    }

    let before_head = path[path.len() - 2];
    let head = path[path.len() - 1];
    if head.unit_visible {
        let head_diff_x = head.position.x as isize - before_head.position.x as isize;
        let head_diff_y = head.position.y as isize - before_head.position.y as isize;
        let rotation_degrees = if head_diff_x > 0 {
            90.0
        } else if head_diff_x < 0 {
            -90.0
        } else if head_diff_y > 0 {
            0.0
        } else {
            180.0
        };

        spawns.push(CourseArrowSpawn {
            kind: CourseArrowSpriteKind::Tip,
            position: head.position,
            rotation_degrees,
            start_delay: scaled_animation_duration(
                (path.len() as u64 - 2) * COURSE_ARROW_STAGGER_MS,
            ),
        });
    }

    spawns
}

fn current_segment_and_progress(path_animation: &UnitPathAnimation) -> (usize, f32) {
    let last_segment = path_animation.segment_durations.len().saturating_sub(1);
    if path_animation.elapsed >= path_animation.total_duration {
        return (last_segment, 1.0);
    }

    let mut elapsed = path_animation.elapsed;
    for (index, segment_duration) in path_animation.segment_durations.iter().enumerate() {
        if elapsed < *segment_duration {
            let segment_secs = segment_duration.as_secs_f32().max(f32::EPSILON);
            return (index, elapsed.as_secs_f32() / segment_secs);
        }
        elapsed = elapsed.saturating_sub(*segment_duration);
    }

    (last_segment, 1.0)
}

fn unit_animation_for(
    unit: Unit,
    faction: Faction,
    movement: GraphicalMovement,
) -> (awbrn_core::UnitAnimationFrames, Animation) {
    let animation_frames = get_unit_animation_frames(movement, unit.0, faction.0);
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

    (animation_frames, animation)
}

#[derive(Clone, Copy)]
struct UnitVisualState {
    unit: Unit,
    faction: Faction,
    flip_x: bool,
}

fn flip_x_for_movement(idle_flip_x: bool, movement: GraphicalMovement) -> bool {
    match movement {
        GraphicalMovement::Idle => idle_flip_x,
        GraphicalMovement::Up | GraphicalMovement::Down => false,
        GraphicalMovement::Lateral => idle_flip_x,
    }
}

fn flip_x_for_lateral_direction(moving_right: bool) -> bool {
    !moving_right
}

fn set_unit_pose(
    sprite: &mut Sprite,
    visual_state: UnitVisualState,
    movement: GraphicalMovement,
) -> awbrn_core::UnitAnimationFrames {
    let animation_frames =
        get_unit_animation_frames(movement, visual_state.unit.0, visual_state.faction.0);
    sprite.flip_x = visual_state.flip_x;
    if let Some(atlas) = &mut sprite.texture_atlas {
        atlas.index = animation_frames.start_index() as usize;
    }
    animation_frames
}

fn set_unit_animation_state(
    commands: &mut Commands,
    entity: Entity,
    sprite: &mut Sprite,
    animation: Option<Mut<Animation>>,
    visual_state: UnitVisualState,
    movement: GraphicalMovement,
) {
    set_unit_pose(sprite, visual_state, movement);
    let (_, new_animation) = unit_animation_for(visual_state.unit, visual_state.faction, movement);
    sprite.color = Color::WHITE;

    if let Some(mut animation) = animation {
        animation.start_index = new_animation.start_index;
        animation.frame_durations = new_animation.frame_durations;
        animation.current_frame = 0;
        animation.frame_timer = new_animation.frame_timer;
    } else {
        commands.entity(entity).insert(new_animation);
    }
}

fn restore_unit_visual_state(
    commands: &mut Commands,
    entity: Entity,
    sprite: &mut Sprite,
    animation: Option<Mut<Animation>>,
    visual_state: UnitVisualState,
    has_active: bool,
) {
    set_unit_pose(sprite, visual_state, GraphicalMovement::Idle);
    if has_active {
        set_unit_animation_state(
            commands,
            entity,
            sprite,
            animation,
            visual_state,
            GraphicalMovement::Idle,
        );
    } else {
        sprite.color = INACTIVE_UNIT_COLOR;
        commands.entity(entity).remove::<Animation>();
    }
}

/// System to update Transform when MapPosition changes
type MapPositionTransformQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Transform,
        &'static SpriteSize,
        &'static MapPosition,
    ),
    (Changed<MapPosition>, Without<UnitPathAnimation>),
>;

fn update_transform_on_position_change(
    mut query: MapPositionTransformQuery,
    game_map: Res<GameMap>,
) {
    for (mut transform, sprite_size, map_position) in query.iter_mut() {
        let final_world_pos =
            map_position_to_world_translation(sprite_size, *map_position, game_map.as_ref());
        transform.translation = final_world_pos;

        log::info!(
            "Updated Transform for position ({}, {}) -> {:?}",
            map_position.x(),
            map_position.y(),
            final_world_pos
        );
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
            .init_resource::<ReplayAdvanceLock>()
            .init_state::<AppState>()
            .init_state::<GameMode>()
            .add_sub_state::<LoadingState>()
            .insert_resource(MapPathResolver(self.map_resolver.clone()))
            .add_message::<ExternalGameEvent>()
            .register_type::<AwbwUnitId>()
            .register_type::<HasCargo>()
            .register_type::<MapPosition>()
            .register_type::<Faction>()
            .register_type::<Unit>()
            .add_observer(on_map_position_insert)
            .add_observer(handle_unit_spawn)
            .add_observer(on_capturing_remove)
            .add_observer(on_cargo_remove)
            .add_observer(on_capturing_insert)
            .add_observer(on_cargo_insert)
            .add_observer(on_health_insert)
            .add_observer(on_health_remove)
            .add_observer(on_unit_active_remove)
            .add_observer(on_unit_active_insert)
            .add_systems(Startup, (setup_camera, setup_unit_atlas));

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
                    animate_terrain,
                    cleanup_empty_cargo,
                    update_health_indicator,
                    update_unit_on_faction_change,
                    update_unit_on_type_change,
                    update_transform_on_position_change,
                    update_tile_cursor,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            // Replay-specific systems
            .add_systems(
                Update,
                (
                    spawn_pending_course_arrows.before(animate_course_arrows),
                    animate_course_arrows,
                    animate_unit_paths.before(animate_units),
                    animate_units,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                check_assets_loaded.run_if(in_state(LoadingState::LoadingAssets)),
            )
            // Game mode setup systems
            .add_systems(
                OnEnter(LoadingState::Complete),
                (setup_ui_atlas, emit_map_dimensions),
            )
            .add_systems(
                OnEnter(LoadingState::Complete),
                spawn_tile_cursor.after(setup_ui_atlas),
            )
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

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct MapDimensions {
    pub width: f32,
    pub height: f32,
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
    MapDimensions(MapDimensions),
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

    // Start loading UI atlas
    let ui_atlas_handle = asset_server.load("data/ui_atlas.json");
    let ui_texture_handle = asset_server.load("textures/ui.png");
    commands.insert_resource(PendingUiAtlas {
        atlas: ui_atlas_handle,
        texture: ui_texture_handle,
    });

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
    asset_server: Res<AssetServer>,
) {
    let Some(pending) = pending_game else { return };
    commands.insert_resource(MapAssetHandle(pending.0.clone()));
    commands.remove_resource::<PendingGameStart>();

    // Start loading UI atlas
    let ui_atlas_handle = asset_server.load("data/ui_atlas.json");
    let ui_texture_handle = asset_server.load("textures/ui.png");
    commands.insert_resource(PendingUiAtlas {
        atlas: ui_atlas_handle,
        texture: ui_texture_handle,
    });

    game_mode_state.set(GameMode::Game);
    app_state.set(AppState::Loading);
    loading_state.set(LoadingState::LoadingAssets);
    info!("Started game mode");
}

fn setup_camera(mut commands: Commands, camera_scale: Res<CameraScale>) {
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: bevy::camera::ScalingMode::WindowSize,
            scale: 1.0 / camera_scale.scale(),
            ..OrthographicProjection::default_2d()
        }),
        Msaa::Off, // https://github.com/bevyengine/bevy/discussions/3748#discussioncomment-5565500
    ));
}

fn setup_unit_atlas(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let texture = asset_server.load("textures/units.png");
    let layout = TextureAtlasLayout::from_grid(
        UVec2::new(
            awbrn_core::UNIT_SPRITE_WIDTH,
            awbrn_core::UNIT_SPRITE_HEIGHT,
        ),
        awbrn_core::UNIT_SPRITESHEET_COLUMNS,
        awbrn_core::UNIT_SPRITESHEET_ROWS,
        Some(UVec2::new(
            awbrn_core::UNIT_SPRITESHEET_PADDING_X,
            awbrn_core::UNIT_SPRITESHEET_PADDING_Y,
        )),
        Some(UVec2::new(
            awbrn_core::UNIT_SPRITESHEET_OFFSET_X,
            awbrn_core::UNIT_SPRITESHEET_OFFSET_Y,
        )),
    );
    let layout = texture_atlas_layouts.add(layout);

    commands.insert_resource(UnitAtlasResource { texture, layout });
}

// Resource to hold the map handle
#[derive(Resource)]
struct MapAssetHandle(Handle<AwbwMapAsset>);

// System to check if map and UI atlas assets are loaded and then transition state
fn check_assets_loaded(
    map_handle: Res<MapAssetHandle>,
    pending_ui: Res<PendingUiAtlas>,
    awbw_maps: Res<Assets<AwbwMapAsset>>,
    ui_atlas_assets: Res<Assets<UiAtlasAsset>>,
    loaded_replay: Option<Res<LoadedReplay>>,
    mut game_map: ResMut<GameMap>,
    mut next_state: ResMut<NextState<LoadingState>>,
) {
    // Check map is loaded
    let Some(awbw_map_asset) = awbw_maps.get(&map_handle.0) else {
        return;
    };

    // Check UI atlas is loaded
    if ui_atlas_assets.get(&pending_ui.atlas).is_none() {
        return;
    }

    // Both loaded - process map and transition
    let mut awbw_map = awbw_map_asset.to_awbw_map();
    if let Some(replay) = loaded_replay
        && let Some(first_game) = replay.0.games.first()
    {
        apply_replay_building_overrides(&mut awbw_map, &first_game.buildings);
    }
    let awbrn_map = AwbrnMap::from_map(&awbw_map);

    info!(
        "Map asset processed: {}x{}. UI atlas loaded. Transitioning to Complete state.",
        awbrn_map.width(),
        awbrn_map.height()
    );

    game_map.set(awbrn_map);
    next_state.set(LoadingState::Complete);
}

fn apply_replay_building_overrides(map: &mut AwbwMap, buildings: &[AwbwBuilding]) {
    for building in buildings {
        let position = Position::new(building.x as usize, building.y as usize);
        let Some(terrain) = map.terrain_at_mut(position) else {
            warn!(
                "Skipping replay building override at out-of-bounds position {:?}",
                position
            );
            continue;
        };

        *terrain = building.terrain_id;
    }
}

// System that runs on LoadingState::Complete to setup UI atlas resource
fn setup_ui_atlas(
    mut commands: Commands,
    pending_ui: Res<PendingUiAtlas>,
    ui_atlas_assets: Res<Assets<UiAtlasAsset>>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    // Atlas is guaranteed to be loaded at this point
    let ui_atlas = ui_atlas_assets
        .get(&pending_ui.atlas)
        .expect("UI atlas should be loaded before setup");

    // Create the texture atlas layout
    let layout = ui_atlas.layout();
    let layout_handle = texture_atlas_layouts.add(layout);

    // Store in resource for observers to use
    commands.insert_resource(UiAtlasResource {
        handle: pending_ui.atlas.clone(),
        texture: pending_ui.texture.clone(),
        layout: layout_handle,
    });

    info!("UI atlas resource initialized");
}

fn compute_map_dimensions(game_map: &GameMap, camera_scale: &CameraScale) -> MapDimensions {
    // Extra tile of height accounts for terrain sprites (16x32) overhanging their
    // 16px tile by one full tile above the first row.
    MapDimensions {
        width: game_map.width() as f32 * GridSystem::TILE_SIZE * camera_scale.scale(),
        height: (game_map.height() as f32 + 1.0) * GridSystem::TILE_SIZE * camera_scale.scale(),
    }
}

fn emit_map_dimensions(
    game_map: Res<GameMap>,
    camera_scale: Res<CameraScale>,
    mut event_writer: MessageWriter<ExternalGameEvent>,
) {
    let dims = compute_map_dimensions(&game_map, &camera_scale);
    event_writer.write(ExternalGameEvent {
        payload: GameEvent::MapDimensions(dims),
    });
}

fn handle_camera_scaling(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut camera_scale: ResMut<CameraScale>,
    mut query: Query<&mut Projection, With<Camera>>,
    game_map: Res<GameMap>,
    mut event_writer: MessageWriter<ExternalGameEvent>,
) {
    let new_zoom = if keyboard_input.just_pressed(KeyCode::Equal) {
        camera_scale.zoom_in()
    } else if keyboard_input.just_pressed(KeyCode::Minus) {
        camera_scale.zoom_out()
    } else {
        return;
    };

    *camera_scale = new_zoom;

    // Bevy recommends zooming orthographic cameras via projection scale.
    if let Ok(mut projection) = query.single_mut()
        && let Projection::Orthographic(orthographic) = &mut *projection
    {
        orthographic.scale = 1.0 / camera_scale.scale();
    }

    let dims = compute_map_dimensions(&game_map, &camera_scale);
    event_writer.write(ExternalGameEvent {
        payload: GameEvent::MapDimensions(dims),
    });
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

fn spawn_tile_cursor(mut commands: Commands, ui_atlas: UiAtlas) {
    commands.spawn((
        ui_atlas.sprite_for("Effects/TileCursor.png"),
        Transform::from_translation(Vec3::new(0.0, 0.0, 10.0)),
        Visibility::Hidden,
        TileCursor,
    ));
}

fn update_tile_cursor(
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    game_map: Res<GameMap>,
    mut cursor_q: Query<(&mut Transform, &mut Visibility), With<TileCursor>>,
) {
    let Ok((mut transform, mut visibility)) = cursor_q.single_mut() else {
        return;
    };

    let Ok(window) = windows.single() else {
        *visibility = Visibility::Hidden;
        return;
    };
    let Ok((camera, camera_transform)) = camera_q.single() else {
        *visibility = Visibility::Hidden;
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        *visibility = Visibility::Hidden;
        return;
    };
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) else {
        *visibility = Visibility::Hidden;
        return;
    };
    let world_pos = ray.origin.truncate();

    let map_w = game_map.width() as f32;
    let map_h = game_map.height() as f32;
    let tile = GridSystem::TILE_SIZE;

    // Tile sprites are Bevy-centered at (-W*8 + gx*16, H*8 - gy*16).
    // Add tile/2 so floor() snaps to the nearest tile center rather than corner.
    let gx_f = (world_pos.x + map_w * tile / 2.0 + tile / 2.0) / tile;
    let gy_f = (map_h * tile / 2.0 + tile / 2.0 - world_pos.y) / tile;

    if gx_f < 0.0 || gy_f < 0.0 || gx_f >= map_w || gy_f >= map_h {
        *visibility = Visibility::Hidden;
        return;
    }

    let gx = gx_f.floor() as usize;
    let gy = gy_f.floor() as usize;

    let center_x = -map_w * tile / 2.0 + gx as f32 * tile;
    let center_y = map_h * tile / 2.0 - gy as f32 * tile;

    transform.translation.x = center_x;
    transform.translation.y = center_y;
    *visibility = Visibility::Visible;
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

fn spawn_pending_course_arrows(
    mut commands: Commands,
    ui_atlas: UiAtlas,
    game_map: Res<GameMap>,
    pending: Query<(Entity, &PendingCourseArrows), Added<PendingCourseArrows>>,
    existing_arrows: Query<(Entity, &CourseArrowPiece)>,
) {
    for (owner, pending) in &pending {
        for (entity, arrow) in &existing_arrows {
            if arrow.owner == owner {
                commands.entity(entity).despawn();
            }
        }

        for spawn in build_course_arrow_spawns(&pending.path) {
            let mut transform = Transform::from_translation(
                position_to_world_translation(
                    &COURSE_ARROW_SPRITE_SIZE,
                    spawn.position,
                    game_map.as_ref(),
                ) + Vec3::new(0.0, 0.0, COURSE_ARROW_LAYER_OFFSET),
            );
            transform.rotation = Quat::from_rotation_z(spawn.rotation_degrees.to_radians());
            transform.scale = Vec3::splat(COURSE_ARROW_BASE_SCALE);

            commands.spawn((
                ui_atlas.sprite_for(spawn.kind.sprite_name()),
                transform,
                Visibility::Hidden,
                CourseArrowPiece {
                    owner,
                    kind: spawn.kind,
                    rotation_degrees: spawn.rotation_degrees,
                    start_delay: spawn.start_delay,
                    reveal_duration: scaled_animation_duration(COURSE_ARROW_REVEAL_MS),
                    total_duration: scaled_animation_duration(COURSE_ARROW_LIFETIME_MS),
                    elapsed: Duration::ZERO,
                },
            ));
        }

        commands.entity(owner).remove::<PendingCourseArrows>();
    }
}

fn animate_course_arrows(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(
        Entity,
        &mut CourseArrowPiece,
        &mut Transform,
        &mut Visibility,
    )>,
) {
    for (entity, mut arrow, mut transform, mut visibility) in &mut query {
        arrow.elapsed += time.delta();

        if arrow.elapsed < arrow.start_delay {
            *visibility = Visibility::Hidden;
            continue;
        }

        *visibility = Visibility::Visible;
        let visible_elapsed = arrow.elapsed.saturating_sub(arrow.start_delay);
        let reveal_progress = if arrow.reveal_duration.is_zero() {
            1.0
        } else {
            visible_elapsed.as_secs_f32() / arrow.reveal_duration.as_secs_f32()
        };
        let scale = COURSE_ARROW_BASE_SCALE
            + (1.0 - COURSE_ARROW_BASE_SCALE) * ease_out_quint(reveal_progress);
        transform.scale = Vec3::splat(scale);

        if visible_elapsed >= arrow.total_duration {
            commands.entity(entity).despawn();
        }
    }
}

type UnitPathAnimationQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Transform,
        &'static SpriteSize,
        &'static mut UnitPathAnimation,
        &'static mut Sprite,
        &'static Unit,
        &'static Faction,
        Option<&'static mut Animation>,
        Has<UnitActive>,
    ),
>;

fn animate_unit_paths(
    mut commands: Commands,
    time: Res<Time>,
    game_map: Res<GameMap>,
    mut replay_lock: ResMut<ReplayAdvanceLock>,
    mut query: UnitPathAnimationQuery,
) {
    for (
        entity,
        mut transform,
        sprite_size,
        mut path_animation,
        mut sprite,
        unit,
        faction,
        animation,
        has_active,
    ) in &mut query
    {
        let idle_visual_state = UnitVisualState {
            unit: *unit,
            faction: *faction,
            flip_x: path_animation.idle_flip_x,
        };

        if path_animation.path.len() < 2 {
            commands.entity(entity).remove::<UnitPathAnimation>();
            restore_unit_visual_state(
                &mut commands,
                entity,
                &mut sprite,
                animation,
                idle_visual_state,
                has_active,
            );
            continue;
        }

        let previous_elapsed = path_animation.elapsed;
        path_animation.elapsed =
            (path_animation.elapsed + time.delta()).min(path_animation.total_duration);
        let (segment_index, segment_t) = current_segment_and_progress(&path_animation);

        let moving_right = if segment_index + 1 < path_animation.path.len() {
            path_animation.path[segment_index + 1].x > path_animation.path[segment_index].x
        } else {
            false
        };
        let movement = if segment_index + 1 < path_animation.path.len() {
            movement_direction(
                path_animation.path[segment_index],
                path_animation.path[segment_index + 1],
            )
        } else {
            path_animation.current_movement
        };
        let flip_x = if matches!(movement, GraphicalMovement::Lateral) {
            flip_x_for_lateral_direction(moving_right)
        } else {
            flip_x_for_movement(path_animation.idle_flip_x, movement)
        };
        let moving_visual_state = UnitVisualState {
            unit: *unit,
            faction: *faction,
            flip_x,
        };

        if previous_elapsed.is_zero()
            || segment_index != path_animation.current_segment
            || movement != path_animation.current_movement
        {
            path_animation.current_segment = segment_index;
            path_animation.current_movement = movement;
            set_unit_animation_state(
                &mut commands,
                entity,
                &mut sprite,
                animation,
                moving_visual_state,
                movement,
            );
        }

        let start_world = position_to_world_translation(
            sprite_size,
            path_animation.path[segment_index],
            game_map.as_ref(),
        );
        let end_world = position_to_world_translation(
            sprite_size,
            path_animation.path[segment_index + 1],
            game_map.as_ref(),
        );
        transform.translation = start_world.lerp(end_world, segment_t);

        if path_animation.elapsed >= path_animation.total_duration {
            transform.translation = position_to_world_translation(
                sprite_size,
                *path_animation.path.last().unwrap(),
                game_map.as_ref(),
            );
            commands.entity(entity).remove::<UnitPathAnimation>();
            restore_unit_visual_state(
                &mut commands,
                entity,
                &mut sprite,
                None,
                idle_visual_state,
                has_active,
            );

            if let Some(action) = replay_lock.release_for(entity) {
                commands.queue(ReplayFollowupCommand { action });
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReplayAdvanceResult {
    Advanced,
    AdvancedWithLock,
    Exhausted,
}

fn advance_replay_action(
    commands: &mut Commands,
    replay_state: &mut ReplayState,
    loaded_replay: &LoadedReplay,
) -> ReplayAdvanceResult {
    let Some(action) = loaded_replay
        .0
        .turns
        .get(replay_state.turn as usize)
        .cloned()
    else {
        return ReplayAdvanceResult::Exhausted;
    };

    commands.queue(ReplayTurnCommand { action });
    replay_state.turn += 1;

    if action_requires_path_animation(
        loaded_replay
            .0
            .turns
            .get((replay_state.turn - 1) as usize)
            .expect("queued replay action should still exist"),
    ) {
        ReplayAdvanceResult::AdvancedWithLock
    } else {
        ReplayAdvanceResult::Advanced
    }
}

fn handle_replay_controls(
    mut commands: Commands,
    mut keyboard_input: MessageReader<KeyboardInput>,
    mut replay_control: Local<ReplayControlState>,
    mut replay_state: ResMut<ReplayState>,
    loaded_replay: Res<LoadedReplay>,
    replay_lock: Res<ReplayAdvanceLock>,
) {
    let mut replay_blocked = replay_lock.is_active();

    for event in keyboard_input.read() {
        if event.key_code != KeyCode::ArrowRight {
            continue;
        }

        match event.state {
            ButtonState::Released => {
                replay_control.suppress_exhausted_repeat = false;
            }
            ButtonState::Pressed => {
                if replay_blocked {
                    continue;
                }

                if event.repeat && replay_control.suppress_exhausted_repeat {
                    continue;
                }

                match advance_replay_action(&mut commands, &mut replay_state, &loaded_replay) {
                    ReplayAdvanceResult::Advanced => {
                        replay_control.suppress_exhausted_repeat = false;
                    }
                    ReplayAdvanceResult::AdvancedWithLock => {
                        replay_control.suppress_exhausted_repeat = false;
                        replay_blocked = true;
                    }
                    ReplayAdvanceResult::Exhausted => {
                        info!("Reached the end of the replay turns");
                        replay_control.suppress_exhausted_repeat = true;
                    }
                }
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
            UnitActive,
        ));
    }
}

fn init_replay_state(mut commands: Commands) {
    commands.init_resource::<ReplayState>();
    commands.insert_resource(ReplayAdvanceLock::default());
}

fn handle_unit_spawn(
    trigger: On<Insert, AwbwUnitId>,
    mut commands: Commands,
    unit_atlas: Res<UnitAtlasResource>,
    mut map: ResMut<StrongIdMap<AwbwUnitId>>,
    mut query: Query<(&Unit, &Faction, &AwbwUnitId, Has<UnitActive>)>,
) {
    let entity = trigger.entity;
    let Ok((unit, faction, unit_id, has_active)) = query.get_mut(entity) else {
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

    // Determine visual state based on UnitActive component
    let (color, should_animate) = if has_active {
        (Color::WHITE, true)
    } else {
        (INACTIVE_UNIT_COLOR, false)
    };

    let mut sprite = Sprite::from_atlas_image(
        unit_atlas.texture.clone(),
        TextureAtlas {
            layout: unit_atlas.layout.clone(),
            index: animation_frames.start_index() as usize,
        },
    );
    sprite.color = color;

    let mut entity_commands = commands.entity(entity);
    entity_commands.insert((sprite, Anchor::default()));

    // Only add animation if unit is active
    if should_animate {
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
        entity_commands.insert(animation);
    }
}

fn on_map_position_insert(
    trigger: On<Insert, MapPosition>,
    mut query: Query<(
        &mut Transform,
        &SpriteSize,
        &MapPosition,
        Has<UnitPathAnimation>,
    )>,
    game_map: Res<GameMap>,
) {
    let entity = trigger.entity;

    let Ok((mut transform, sprite_size, map_position, has_path_animation)) = query.get_mut(entity)
    else {
        warn!("Entity {:?} not found in query for MapPosition", entity);
        return;
    };

    if has_path_animation {
        return;
    }

    let final_world_pos =
        map_position_to_world_translation(sprite_size, *map_position, game_map.as_ref());

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
    use awbrn_core::AwbwGamePlayerId;
    use awbrn_core::{
        AwbwTerrain, AwbwUnitId as CoreUnitId, Faction as TerrainFaction, PlayerFaction, Property,
    };
    use awbw_replay::turn_models::{
        Action, AwbwHpDisplay, BuildingInfo, CaptureAction, CombatInfo, CombatInfoVision,
        CombatUnit, CopValueInfo, CopValues, FireAction, MoveAction, PowerAction, TargetedPlayer,
        UnitProperty,
    };
    use awbw_replay::{Hidden, Masked};
    use bevy::input::keyboard::{Key, NativeKey};
    use indexmap::IndexMap;
    use std::time::Duration;

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
                .abs_diff_eq(Vec3::new(80.0, -48.0, 0.0), 0.1)
        );
        assert!(
            unit_transform
                .translation
                .abs_diff_eq(Vec3::new(124.5, -36.0, 1.0), 0.1)
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
                .abs_diff_eq(Vec3::new(16.0, -112.0, 0.0), 0.1)
        );
        assert!(
            updated_unit_transform
                .translation
                .abs_diff_eq(Vec3::new(140.5, -20.0, 1.0), 0.1)
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

    #[test]
    fn test_apply_replay_building_overrides_updates_owned_property_terrain() {
        let mut map = AwbwMap::new(3, 3, AwbwTerrain::Plain);
        *map.terrain_at_mut(Position::new(1, 1)).unwrap() =
            AwbwTerrain::Property(Property::City(TerrainFaction::Neutral));

        apply_replay_building_overrides(
            &mut map,
            &[AwbwBuilding {
                id: 1,
                games_id: 7,
                terrain_id: AwbwTerrain::Property(Property::City(TerrainFaction::Player(
                    PlayerFaction::BlueMoon,
                ))),
                x: 1,
                y: 1,
                capture: 20,
                last_capture: 20,
                last_updated: "2026-03-14".to_string(),
            }],
        );

        assert_eq!(
            map.terrain_at(Position::new(1, 1)),
            Some(AwbwTerrain::Property(Property::City(
                TerrainFaction::Player(PlayerFaction::BlueMoon)
            )))
        );
    }

    #[test]
    fn test_apply_replay_building_overrides_ignores_out_of_bounds_positions() {
        let mut map = AwbwMap::new(2, 2, AwbwTerrain::Plain);

        apply_replay_building_overrides(
            &mut map,
            &[AwbwBuilding {
                id: 1,
                games_id: 7,
                terrain_id: AwbwTerrain::Property(Property::HQ(PlayerFaction::PurpleLightning)),
                x: 99,
                y: 99,
                capture: 20,
                last_capture: 20,
                last_updated: "2026-03-14".to_string(),
            }],
        );

        assert_eq!(
            map.terrain_at(Position::new(0, 0)),
            Some(AwbwTerrain::Plain)
        );
        assert_eq!(
            map.terrain_at(Position::new(1, 1)),
            Some(AwbwTerrain::Plain)
        );
    }

    #[test]
    fn replay_press_advances_immediately() {
        let mut app = replay_controls_test_app(2);

        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Pressed, false);
        app.update();

        assert_eq!(app.world().resource::<ReplayState>().turn, 1);
    }

    #[test]
    fn replay_repeat_presses_advance_one_action_each() {
        let mut app = replay_controls_test_app(3);

        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Pressed, false);
        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Pressed, true);
        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Pressed, true);
        app.update();

        assert_eq!(app.world().resource::<ReplayState>().turn, 3);
    }

    #[test]
    fn replay_ignores_unrelated_and_release_events() {
        let mut app = replay_controls_test_app(2);

        send_key_event(&mut app, KeyCode::Space, ButtonState::Pressed, false);
        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Released, false);
        app.update();

        assert_eq!(app.world().resource::<ReplayState>().turn, 0);
    }

    #[test]
    fn replay_repeat_events_stop_at_end_until_release() {
        let mut app = replay_controls_test_app(1);

        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Pressed, false);
        app.update();
        assert_eq!(app.world().resource::<ReplayState>().turn, 1);

        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Pressed, true);
        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Pressed, true);
        app.update();

        assert_eq!(app.world().resource::<ReplayState>().turn, 1);
    }

    #[test]
    fn replay_release_clears_end_suppression() {
        let mut app = replay_controls_test_app(1);

        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Pressed, false);
        app.update();
        assert_eq!(app.world().resource::<ReplayState>().turn, 1);

        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Pressed, true);
        app.update();
        assert_eq!(app.world().resource::<ReplayState>().turn, 1);

        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Released, false);
        app.update();

        app.world_mut().resource_mut::<ReplayState>().turn = 0;
        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Pressed, false);
        app.update();

        assert_eq!(app.world().resource::<ReplayState>().turn, 1);
    }

    #[test]
    fn replay_move_action_blocks_additional_presses_in_same_frame() {
        let mut app = replay_controls_test_app_with_actions(vec![
            test_move_action(),
            test_replay_action(),
            test_replay_action(),
        ]);

        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Pressed, false);
        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Pressed, true);
        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Pressed, true);
        app.update();

        assert_eq!(app.world().resource::<ReplayState>().turn, 1);
    }

    #[test]
    fn course_arrow_generation_matches_reference_rotations() {
        let straight = build_course_arrow_spawns(&[
            ReplayPathTile {
                position: Position::new(1, 1),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Position::new(2, 1),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Position::new(3, 1),
                unit_visible: true,
            },
        ]);
        assert_eq!(straight.len(), 2);
        assert_eq!(straight[0].kind, CourseArrowSpriteKind::Body);
        assert_eq!(straight[0].rotation_degrees, 90.0);
        assert_eq!(straight[1].kind, CourseArrowSpriteKind::Tip);
        assert_eq!(straight[1].rotation_degrees, 90.0);

        let curved = build_course_arrow_spawns(&[
            ReplayPathTile {
                position: Position::new(3, 3),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Position::new(2, 3),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Position::new(2, 2),
                unit_visible: true,
            },
        ]);
        assert_eq!(curved.len(), 2);
        assert_eq!(curved[0].kind, CourseArrowSpriteKind::Curved);
        assert_eq!(curved[0].rotation_degrees, -90.0);
        assert_eq!(curved[1].kind, CourseArrowSpriteKind::Tip);
        assert_eq!(curved[1].rotation_degrees, 180.0);
    }

    #[test]
    fn course_arrow_generation_skips_hidden_tiles() {
        let spawns = build_course_arrow_spawns(&[
            ReplayPathTile {
                position: Position::new(1, 1),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Position::new(2, 1),
                unit_visible: false,
            },
            ReplayPathTile {
                position: Position::new(3, 1),
                unit_visible: true,
            },
        ]);

        assert_eq!(spawns.len(), 1);
        assert_eq!(spawns[0].kind, CourseArrowSpriteKind::Tip);
        assert_eq!(spawns[0].position, Position::new(3, 1));
    }

    #[test]
    fn hidden_tail_promotes_last_visible_middle_tile_to_tip() {
        let spawns = build_course_arrow_spawns(&[
            ReplayPathTile {
                position: Position::new(1, 1),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Position::new(2, 1),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Position::new(3, 1),
                unit_visible: false,
            },
        ]);

        assert_eq!(spawns.len(), 1);
        assert_eq!(spawns[0].kind, CourseArrowSpriteKind::Tip);
        assert_eq!(spawns[0].position, Position::new(2, 1));
        assert_eq!(spawns[0].start_delay, scaled_animation_duration(0));
        assert_eq!(spawns[0].rotation_degrees, 90.0);
    }

    #[test]
    fn s_curve_generates_complementary_curve_rotations() {
        let spawns = build_course_arrow_spawns(&[
            ReplayPathTile {
                position: Position::new(0, 0),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Position::new(1, 0),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Position::new(1, 1),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Position::new(2, 1),
                unit_visible: true,
            },
        ]);

        assert_eq!(spawns.len(), 3);
        assert_eq!(spawns[0].kind, CourseArrowSpriteKind::Curved);
        assert_eq!(spawns[0].rotation_degrees, 90.0);
        assert_eq!(spawns[1].kind, CourseArrowSpriteKind::Curved);
        assert_eq!(spawns[1].rotation_degrees, -90.0);
        assert_eq!(spawns[2].kind, CourseArrowSpriteKind::Tip);
        assert_eq!(spawns[2].rotation_degrees, 90.0);
    }

    #[test]
    fn leftward_tip_points_left() {
        let spawns = build_course_arrow_spawns(&[
            ReplayPathTile {
                position: Position::new(3, 1),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Position::new(2, 1),
                unit_visible: true,
            },
        ]);

        assert_eq!(spawns.len(), 1);
        assert_eq!(spawns[0].kind, CourseArrowSpriteKind::Tip);
        assert_eq!(spawns[0].rotation_degrees, -90.0);
    }

    #[test]
    fn animated_move_visits_intermediate_tiles_and_releases_lock() {
        let mut app = replay_animation_test_app();
        let unit_entity = spawn_test_unit(
            &mut app,
            Position::new(8, 33),
            CoreUnitId::new(173623341),
            PlayerFaction::GreenEarth,
        );
        app.update();

        let start_translation = app
            .world()
            .entity(unit_entity)
            .get::<Transform>()
            .unwrap()
            .translation;

        ReplayTurnCommand {
            action: test_move_action(),
        }
        .apply(app.world_mut());

        assert!(
            app.world()
                .entity(unit_entity)
                .contains::<UnitPathAnimation>()
        );
        assert_eq!(
            app.world()
                .entity(unit_entity)
                .get::<MapPosition>()
                .unwrap()
                .position(),
            Position::new(7, 32)
        );
        assert_eq!(
            app.world().resource::<ReplayAdvanceLock>().active_entity(),
            Some(unit_entity)
        );
        assert_eq!(
            app.world()
                .entity(unit_entity)
                .get::<Transform>()
                .unwrap()
                .translation,
            start_translation
        );

        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_millis(50));
        app.update();

        let mid_translation = app
            .world()
            .entity(unit_entity)
            .get::<Transform>()
            .unwrap()
            .translation;
        assert_ne!(mid_translation, start_translation);

        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_millis(650));
        app.update();

        let expected_final = position_to_world_translation(
            app.world().entity(unit_entity).get::<SpriteSize>().unwrap(),
            Position::new(7, 32),
            app.world().resource::<GameMap>(),
        );
        let final_translation = app
            .world()
            .entity(unit_entity)
            .get::<Transform>()
            .unwrap()
            .translation;
        assert!(
            final_translation.abs_diff_eq(expected_final, 0.05),
            "unexpected final translation: {final_translation:?}"
        );
        let final_sprite = app.world().entity(unit_entity).get::<Sprite>().unwrap();
        assert!(!final_sprite.flip_x);
        assert_eq!(
            final_sprite.texture_atlas.as_ref().unwrap().index,
            get_unit_animation_frames(
                GraphicalMovement::Idle,
                awbrn_core::Unit::Infantry,
                PlayerFaction::GreenEarth
            )
            .start_index() as usize
        );
        assert!(
            !app.world()
                .entity(unit_entity)
                .contains::<UnitPathAnimation>()
        );
        assert!(!app.world().resource::<ReplayAdvanceLock>().is_active());
    }

    #[test]
    fn move_action_spawns_and_expires_course_arrows_in_world_space() {
        let mut app = replay_animation_test_app();
        let unit_entity = spawn_test_unit(
            &mut app,
            Position::new(8, 33),
            CoreUnitId::new(173623341),
            PlayerFaction::GreenEarth,
        );
        app.update();

        ReplayTurnCommand {
            action: test_move_action(),
        }
        .apply(app.world_mut());
        app.update();

        let arrows = course_arrows(&mut app);
        assert_eq!(arrows.len(), 2);

        let curved = arrows
            .iter()
            .find(|(piece, _, _)| piece.kind == CourseArrowSpriteKind::Curved)
            .expect("curve tile should spawn");
        assert!(matches!(curved.1, Visibility::Visible));
        assert!((curved.2.scale.x - COURSE_ARROW_BASE_SCALE).abs() < 0.001);

        let tip = arrows
            .iter()
            .find(|(piece, _, _)| piece.kind == CourseArrowSpriteKind::Tip)
            .expect("tip tile should spawn");
        assert!(matches!(tip.1, Visibility::Hidden));

        let unit_z = app
            .world()
            .entity(unit_entity)
            .get::<Transform>()
            .unwrap()
            .translation
            .z;
        assert!(curved.2.translation.z > 0.0);
        assert!(curved.2.translation.z < unit_z);

        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_millis(25));
        app.update();

        for (_, visibility, _) in course_arrows(&mut app) {
            assert!(matches!(visibility, Visibility::Visible));
        }

        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_millis(300));
        app.update();

        assert!(course_arrows(&mut app).is_empty());
    }

    #[test]
    fn capture_followup_waits_for_move_completion() {
        let mut app = replay_animation_test_app();
        let unit_entity = spawn_test_unit(
            &mut app,
            Position::new(2, 2),
            CoreUnitId::new(1),
            PlayerFaction::OrangeStar,
        );
        spawn_test_property(&mut app, Position::new(2, 1));
        app.update();

        ReplayTurnCommand {
            action: test_capture_action(),
        }
        .apply(app.world_mut());

        assert!(!app.world().entity(unit_entity).contains::<Capturing>());

        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_millis(400));
        app.update();

        assert!(app.world().entity(unit_entity).contains::<Capturing>());
    }

    #[test]
    fn fire_followup_waits_for_move_completion() {
        let mut app = replay_animation_test_app();
        let attacker = spawn_test_unit(
            &mut app,
            Position::new(4, 4),
            CoreUnitId::new(10),
            PlayerFaction::OrangeStar,
        );
        let defender = spawn_test_unit(
            &mut app,
            Position::new(5, 4),
            CoreUnitId::new(11),
            PlayerFaction::BlueMoon,
        );
        app.update();

        ReplayTurnCommand {
            action: test_fire_action(),
        }
        .apply(app.world_mut());

        assert!(app.world().entity(attacker).get::<GraphicalHp>().is_none());
        assert!(app.world().entity(defender).get::<GraphicalHp>().is_none());

        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_millis(400));
        app.update();

        assert_eq!(
            app.world()
                .entity(attacker)
                .get::<GraphicalHp>()
                .unwrap()
                .value(),
            8
        );
        assert_eq!(
            app.world()
                .entity(defender)
                .get::<GraphicalHp>()
                .unwrap()
                .value(),
            5
        );
    }

    #[test]
    fn lateral_animation_uses_faction_facing_and_restores_idle_pose() {
        let mut app = replay_animation_test_app();
        let unit_entity = spawn_test_unit(
            &mut app,
            Position::new(4, 4),
            CoreUnitId::new(42),
            PlayerFaction::BlueMoon,
        );
        app.update();

        ReplayTurnCommand {
            action: test_move_action_for(CoreUnitId::new(42), 1, 5, 4, &[(4, 4), (5, 4)]),
        }
        .apply(app.world_mut());

        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_millis(200));
        app.update();

        let moving_sprite = app.world().entity(unit_entity).get::<Sprite>().unwrap();
        assert!(!moving_sprite.flip_x);

        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_millis(200));
        app.update();

        let final_sprite = app.world().entity(unit_entity).get::<Sprite>().unwrap();
        assert!(!final_sprite.flip_x);
        assert_eq!(
            final_sprite.texture_atlas.as_ref().unwrap().index,
            get_unit_animation_frames(
                GraphicalMovement::Idle,
                awbrn_core::Unit::Infantry,
                PlayerFaction::BlueMoon
            )
            .start_index() as usize
        );
    }

    fn replay_controls_test_app(action_count: usize) -> App {
        replay_controls_test_app_with_actions(vec![test_replay_action(); action_count])
    }

    fn replay_controls_test_app_with_actions(actions: Vec<Action>) -> App {
        let mut app = App::new();
        app.add_message::<KeyboardInput>();
        app.add_systems(Update, handle_replay_controls);
        app.insert_resource(ReplayState::default());
        app.insert_resource(ReplayAdvanceLock::default());
        app.insert_resource(StrongIdMap::<AwbwUnitId>::default());
        app.insert_resource(LoadedReplay(AwbwReplay {
            games: Vec::new(),
            turns: actions,
        }));
        app
    }

    fn send_key_event(app: &mut App, key_code: KeyCode, state: ButtonState, repeat: bool) {
        app.world_mut().write_message(KeyboardInput {
            key_code,
            logical_key: Key::Unidentified(NativeKey::Unidentified),
            state,
            text: None,
            repeat,
            window: Entity::PLACEHOLDER,
        });
    }

    fn test_replay_action() -> Action {
        Action::Power(PowerAction {
            player_id: AwbwGamePlayerId::new(1),
            co_name: "Test CO".to_string(),
            co_power: "N".to_string(),
            power_name: "Test Power".to_string(),
        })
    }

    fn replay_animation_test_app() -> App {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default());
        app.init_resource::<GameMap>();
        app.init_resource::<StrongIdMap<AwbwUnitId>>();
        app.init_resource::<Assets<UiAtlasAsset>>();
        app.init_resource::<Assets<TextureAtlasLayout>>();
        app.insert_resource(ReplayAdvanceLock::default());
        app.add_observer(on_map_position_insert);
        app.add_observer(on_unit_active_remove);
        app.add_systems(
            Update,
            (
                spawn_pending_course_arrows.before(animate_course_arrows),
                animate_course_arrows,
                animate_unit_paths,
                update_transform_on_position_change,
            ),
        );

        app.world_mut().resource_mut::<GameMap>().set(AwbrnMap::new(
            40,
            40,
            GraphicalTerrain::Plain,
        ));
        insert_test_ui_atlas(&mut app);

        app
    }

    fn insert_test_ui_atlas(app: &mut App) {
        let atlas_handle = {
            let mut assets = app.world_mut().resource_mut::<Assets<UiAtlasAsset>>();
            assets.add(UiAtlasAsset {
                size: crate::UiAtlasSize {
                    width: 48,
                    height: 16,
                },
                sprites: vec![
                    crate::UiAtlasSprite {
                        name: "Arrow_Body.png".to_string(),
                        x: 0,
                        y: 0,
                        width: 16,
                        height: 16,
                    },
                    crate::UiAtlasSprite {
                        name: "Arrow_Curved.png".to_string(),
                        x: 16,
                        y: 0,
                        width: 16,
                        height: 16,
                    },
                    crate::UiAtlasSprite {
                        name: "Arrow_Tip.png".to_string(),
                        x: 32,
                        y: 0,
                        width: 16,
                        height: 16,
                    },
                ],
            })
        };
        let layout_handle = {
            let mut layouts = app.world_mut().resource_mut::<Assets<TextureAtlasLayout>>();
            layouts.add(TextureAtlasLayout::from_grid(
                UVec2::new(16, 16),
                3,
                1,
                None,
                None,
            ))
        };

        app.world_mut().insert_resource(UiAtlasResource {
            handle: atlas_handle,
            texture: Handle::default(),
            layout: layout_handle,
        });
    }

    fn course_arrows(app: &mut App) -> Vec<(CourseArrowPiece, Visibility, Transform)> {
        let mut query = app
            .world_mut()
            .query::<(&CourseArrowPiece, &Visibility, &Transform)>();
        query
            .iter(app.world())
            .map(|(piece, visibility, transform)| (*piece, *visibility, *transform))
            .collect()
    }

    fn spawn_test_unit(
        app: &mut App,
        position: Position,
        unit_id: CoreUnitId,
        faction: PlayerFaction,
    ) -> Entity {
        let entity = app
            .world_mut()
            .spawn((
                MapPosition::from(position),
                Transform::default(),
                Sprite::from_atlas_image(
                    Handle::default(),
                    TextureAtlas {
                        layout: Handle::default(),
                        index: 0,
                    },
                ),
                Unit(awbrn_core::Unit::Infantry),
                Faction(faction),
                AwbwUnitId(unit_id),
                UnitActive,
            ))
            .id();

        app.world_mut()
            .resource_mut::<StrongIdMap<AwbwUnitId>>()
            .insert(AwbwUnitId(unit_id), entity);

        entity
    }

    fn spawn_test_property(app: &mut App, position: Position) {
        app.world_mut().spawn((
            MapPosition::from(position),
            Transform::default(),
            Sprite::from_atlas_image(
                Handle::default(),
                TextureAtlas {
                    layout: Handle::default(),
                    index: 0,
                },
            ),
            TerrainTile {
                terrain: GraphicalTerrain::Property(Property::City(TerrainFaction::Neutral)),
                position,
            },
        ));
    }

    fn test_move_action() -> Action {
        test_move_action_for(
            CoreUnitId::new(173623341),
            3276855,
            7,
            32,
            &[(8, 33), (7, 33), (7, 32)],
        )
    }

    fn test_move_action_for(
        unit_id: CoreUnitId,
        player_id: u32,
        final_x: u32,
        final_y: u32,
        path: &[(u32, u32)],
    ) -> Action {
        Action::Move(MoveAction {
            unit: IndexMap::from([(
                TargetedPlayer::Global,
                Hidden::Visible(test_unit_property(
                    unit_id.as_u32(),
                    player_id,
                    awbrn_core::Unit::Infantry,
                    final_x,
                    final_y,
                )),
            )]),
            paths: IndexMap::from([(
                TargetedPlayer::Global,
                path.iter()
                    .map(|&(x, y)| awbw_replay::turn_models::PathTile {
                        unit_visible: true,
                        x,
                        y,
                    })
                    .collect(),
            )]),
            dist: 3,
            trapped: false,
            discovered: None,
        })
    }

    fn test_capture_action() -> Action {
        Action::Capt {
            move_action: Some(MoveAction {
                unit: IndexMap::from([(
                    TargetedPlayer::Global,
                    Hidden::Visible(test_unit_property(1, 1, awbrn_core::Unit::Infantry, 2, 1)),
                )]),
                paths: IndexMap::from([(
                    TargetedPlayer::Global,
                    vec![
                        awbw_replay::turn_models::PathTile {
                            unit_visible: true,
                            x: 2,
                            y: 2,
                        },
                        awbw_replay::turn_models::PathTile {
                            unit_visible: true,
                            x: 2,
                            y: 1,
                        },
                    ],
                )]),
                dist: 1,
                trapped: false,
                discovered: None,
            }),
            capture_action: CaptureAction {
                building_info: BuildingInfo {
                    buildings_capture: 10,
                    buildings_id: 99,
                    buildings_x: 2,
                    buildings_y: 1,
                    buildings_team: None,
                },
                vision: IndexMap::new(),
                income: None,
            },
        }
    }

    fn test_fire_action() -> Action {
        Action::Fire {
            move_action: Some(MoveAction {
                unit: IndexMap::from([(
                    TargetedPlayer::Global,
                    Hidden::Visible(test_unit_property(10, 1, awbrn_core::Unit::Infantry, 5, 4)),
                )]),
                paths: IndexMap::from([(
                    TargetedPlayer::Global,
                    vec![
                        awbw_replay::turn_models::PathTile {
                            unit_visible: true,
                            x: 4,
                            y: 4,
                        },
                        awbw_replay::turn_models::PathTile {
                            unit_visible: true,
                            x: 5,
                            y: 4,
                        },
                    ],
                )]),
                dist: 1,
                trapped: false,
                discovered: None,
            }),
            fire_action: FireAction {
                combat_info_vision: IndexMap::from([(
                    TargetedPlayer::Global,
                    CombatInfoVision {
                        has_vision: true,
                        combat_info: CombatInfo {
                            attacker: Masked::Visible(CombatUnit {
                                units_ammo: 0,
                                units_hit_points: Some(test_hp(8)),
                                units_id: CoreUnitId::new(10),
                                units_x: 5,
                                units_y: 4,
                            }),
                            defender: Masked::Visible(CombatUnit {
                                units_ammo: 0,
                                units_hit_points: Some(test_hp(5)),
                                units_id: CoreUnitId::new(11),
                                units_x: 5,
                                units_y: 4,
                            }),
                        },
                    },
                )]),
                cop_values: CopValues {
                    attacker: CopValueInfo {
                        player_id: AwbwGamePlayerId::new(1),
                        cop_value: 0,
                        tag_value: None,
                    },
                    defender: CopValueInfo {
                        player_id: AwbwGamePlayerId::new(2),
                        cop_value: 0,
                        tag_value: None,
                    },
                },
            },
        }
    }

    fn test_unit_property(
        unit_id: u32,
        player_id: u32,
        unit_name: awbrn_core::Unit,
        x: u32,
        y: u32,
    ) -> UnitProperty {
        UnitProperty {
            units_id: CoreUnitId::new(unit_id),
            units_games_id: Some(1403019),
            units_players_id: player_id,
            units_name: unit_name,
            units_movement_points: Some(3),
            units_vision: Some(2),
            units_fuel: Some(99),
            units_fuel_per_turn: Some(0),
            units_sub_dive: "N".to_string(),
            units_ammo: Some(0),
            units_short_range: Some(0),
            units_long_range: Some(0),
            units_second_weapon: Some("N".to_string()),
            units_symbol: Some("G".to_string()),
            units_cost: Some(1000),
            units_movement_type: "F".to_string(),
            units_x: Some(x),
            units_y: Some(y),
            units_moved: Some(1),
            units_capture: Some(0),
            units_fired: Some(0),
            units_hit_points: test_hp(10),
            units_cargo1_units_id: Default::default(),
            units_cargo2_units_id: Default::default(),
            units_carried: Some("N".to_string()),
            countries_code: PlayerFaction::OrangeStar,
        }
    }

    fn test_hp(value: u8) -> AwbwHpDisplay {
        serde_json::from_value(serde_json::json!(value)).unwrap()
    }
}
