//! Typed coordinate spaces and the conversions between them.
//!
//! ## Spaces
//!
//! | Type          | Origin         | Y axis | Pixel basis                              |
//! |---------------|----------------|--------|------------------------------------------|
//! | [`LogicalPx`] | Canvas top-left | Down  | CSS px (device-pixel-ratio-independent)  |
//! | [`WorldPos`]  | Map center     | Up     | 1 unit = 1 logical px at camera scale 1.0 |
//! | Tile grid     | Map top-left   | Down   | [`TILE_SIZE`] logical px per cell        |
//!
//! Tile-grid positions use [`awbrn_map::Position`] / [`awbrn_game::MapPosition`] (already
//! strong types) and are not folded into the f32 system here.
//!
//! ## Conversions
//!
//! All cross-space conversions live in this module. No other module should
//! compute map origins or perform Y-flips inline.
//!
//! JS pointer events (`offsetX` / `offsetY`) arrive as [`LogicalPx`].
//! `CanvasSize` physical dimensions passed to `resize()` are device pixels —
//! do **not** wrap those in [`LogicalPx`].

use std::ops::{Add, Sub};

use awbrn_game::{MapPosition, world::GameMap};
use awbrn_map::Position;
use bevy::prelude::*;

use crate::core::SpriteSize;

/// The size in logical pixels of one tile cell.
pub const TILE_SIZE: f32 = 16.0;

/// A position in CSS / device-independent pixel space.
///
/// - Origin: canvas top-left
/// - Y axis: down
///
/// This is the space of `Window::cursor_position()`, Bevy's `CursorMoved.position`,
/// and JS pointer events (`offsetX` / `offsetY`).
#[repr(transparent)]
#[derive(Copy, Clone, PartialEq, Default)]
pub struct LogicalPx(Vec2);

impl LogicalPx {
    pub const fn new(x: f32, y: f32) -> Self {
        Self(Vec2::new(x, y))
    }

    /// Wrap a `Vec2` obtained from `Window::cursor_position()` or a Bevy
    /// cursor/touch event (both already in logical-pixel space).
    pub fn from_window_position(v: Vec2) -> Self {
        Self(v)
    }

    pub fn x(self) -> f32 {
        self.0.x
    }

    pub fn y(self) -> f32 {
        self.0.y
    }

    /// Unwrap for Bevy APIs that take logical pixels
    /// (e.g., `Camera::viewport_to_world_2d`).
    pub fn to_vec2(self) -> Vec2 {
        self.0
    }

    /// Project into Bevy world space via the given camera.
    ///
    /// Returns `None` if the position is outside the camera's viewport.
    pub fn to_world(self, camera: &Camera, camera_transform: &GlobalTransform) -> Option<WorldPos> {
        camera
            .viewport_to_world_2d(camera_transform, self.0)
            .ok()
            .map(WorldPos::from_bevy)
    }
}

impl std::fmt::Debug for LogicalPx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LogicalPx({}, {})", self.0.x, self.0.y)
    }
}

impl From<LogicalPx> for Vec2 {
    fn from(p: LogicalPx) -> Vec2 {
        p.0
    }
}

/// `LogicalPx + Vec2` — shift a position by a displacement in the same space.
impl Add<Vec2> for LogicalPx {
    type Output = Self;
    fn add(self, rhs: Vec2) -> Self {
        Self(self.0 + rhs)
    }
}

/// `LogicalPx - Vec2` — shift a position by a displacement in the same space.
impl Sub<Vec2> for LogicalPx {
    type Output = Self;
    fn sub(self, rhs: Vec2) -> Self {
        Self(self.0 - rhs)
    }
}

/// `LogicalPx - LogicalPx` — displacement between two positions.
impl Sub for LogicalPx {
    type Output = Vec2;
    fn sub(self, rhs: Self) -> Vec2 {
        self.0 - rhs.0
    }
}

/// A position in Bevy 2D world space.
///
/// - Origin: center of the map (camera centering applied)
/// - Y axis: up (Bevy convention)
/// - Scale: 1 unit = 1 logical pixel at camera scale 1.0
#[repr(transparent)]
#[derive(Copy, Clone, PartialEq, Default)]
pub struct WorldPos(Vec2);

impl WorldPos {
    pub const fn new(x: f32, y: f32) -> Self {
        Self(Vec2::new(x, y))
    }

    /// Wrap a raw `Vec2` from a Bevy world-space API
    /// (e.g., after `Camera::viewport_to_world_2d` or `Transform::translation.truncate()`).
    pub fn from_bevy(v: Vec2) -> Self {
        Self(v)
    }

    pub fn x(self) -> f32 {
        self.0.x
    }

    pub fn y(self) -> f32 {
        self.0.y
    }

    /// Unwrap for Bevy APIs that take raw `Vec2`.
    pub fn to_vec2(self) -> Vec2 {
        self.0
    }

    /// Convert to a tile-grid position, or `None` if outside the map boundary.
    pub fn to_map_position(self, map: &GameMap) -> Option<MapPosition> {
        world_to_map_position(self, map)
    }
}

impl std::fmt::Debug for WorldPos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WorldPos({}, {})", self.0.x, self.0.y)
    }
}

impl From<WorldPos> for Vec2 {
    fn from(p: WorldPos) -> Vec2 {
        p.0
    }
}

/// `WorldPos + Vec2` — shift a position by a displacement in the same space.
impl Add<Vec2> for WorldPos {
    type Output = Self;
    fn add(self, rhs: Vec2) -> Self {
        Self(self.0 + rhs)
    }
}

/// `WorldPos - Vec2` — shift a position by a displacement in the same space.
impl Sub<Vec2> for WorldPos {
    type Output = Self;
    fn sub(self, rhs: Vec2) -> Self {
        Self(self.0 - rhs)
    }
}

/// `WorldPos - WorldPos` — displacement between two positions.
impl Sub for WorldPos {
    type Output = Vec2;
    fn sub(self, rhs: Self) -> Vec2 {
        self.0 - rhs.0
    }
}

/// World-space size of the map's tile cells, excluding sprite overhang.
pub fn map_tile_world_size(map: &GameMap) -> Vec2 {
    Vec2::new(
        map.width() as f32 * TILE_SIZE,
        map.height() as f32 * TILE_SIZE,
    )
}

/// World-space size of the rendered map, including one extra tile row for
/// terrain sprite overhang below the logical tile grid.
pub fn map_visual_world_size(map: &GameMap) -> Vec2 {
    Vec2::new(
        map.width() as f32 * TILE_SIZE,
        (map.height() as f32 + 1.0) * TILE_SIZE,
    )
}

/// Top edge of the rendered map in world space.
pub fn map_visual_top_world_y(map: &GameMap) -> f32 {
    map.height() as f32 * TILE_SIZE * 0.5
}

/// World-space center of the repeated plain-tile backdrop mesh.
pub fn backdrop_world_translation(z: f32) -> Vec3 {
    Vec3::new(0.0, -TILE_SIZE / 2.0, z)
}

/// World-space position of the **top-left corner** of tile (0, 0).
///
/// Use this as the reference origin for **hit-testing** (world → tile).
/// In world space: X increases right, Y increases up.
fn tile_grid_top_left_world(map: &GameMap) -> WorldPos {
    WorldPos::new(
        -(map.width() as f32) * TILE_SIZE / 2.0,
        (map.height() as f32) * TILE_SIZE / 2.0 - TILE_SIZE / 2.0,
    )
}

/// World-space **center** of tile cell `pos`.
///
/// Use this for sprites that exactly fill one tile (tile cursor, fog overlay).
/// For differently-sized sprites, use [`map_position_to_world_translation`],
/// which adds the alignment offset on top of the tile center.
fn tile_center_world(pos: MapPosition, map: &GameMap) -> WorldPos {
    let tl = tile_grid_top_left_world(map);
    WorldPos::new(
        tl.x() + (pos.x() as f32 + 0.5) * TILE_SIZE,
        tl.y() - (pos.y() as f32 + 0.5) * TILE_SIZE,
    )
}

/// Convert a world-space position to a tile-grid position.
///
/// Returns `None` if `world` lies outside the map boundary.
fn world_to_map_position(world: WorldPos, map: &GameMap) -> Option<MapPosition> {
    let map_w = map.width() as f32;
    let map_h = map.height() as f32;
    let tl = tile_grid_top_left_world(map);

    let gx_f = (world.x() - tl.x()) / TILE_SIZE;
    let gy_f = (tl.y() - world.y()) / TILE_SIZE;

    if gx_f < 0.0 || gy_f < 0.0 || gx_f >= map_w || gy_f >= map_h {
        return None;
    }

    Some(MapPosition::new(
        gx_f.floor() as usize,
        gy_f.floor() as usize,
    ))
}

/// Compute `Transform::translation` for a sprite at `pos`.
///
/// All sprites use center-based anchoring (Bevy default). The alignment
/// offsets center the sprite within its tile cell regardless of sprite
/// dimensions — works for 16×16 cursors, 16×32 terrain, 23×24 units, etc.
///
/// The returned `Vec3::z` is a **render-sort key**, not a spatial depth:
/// `z_index` sets the layer; a small per-row bias (`y * 0.001`) breaks ties
/// within the same layer.
pub fn map_position_to_world_translation(
    sprite_size: &SpriteSize,
    map_position: MapPosition,
    game_map: &GameMap,
) -> Vec3 {
    let center = tile_center_world(map_position, game_map);

    // Center the sprite within the tile cell. Offsets are computed in
    // grid-local space (Y-down); the Y offset is negated for world space (Y-up).
    let x_align = (TILE_SIZE - sprite_size.width) / 2.0;
    let y_align = (TILE_SIZE - sprite_size.height) / 2.0;

    let z_offset = map_position.y() as f32 * 0.001;

    Vec3::new(
        center.x() + x_align,
        center.y() - y_align,
        sprite_size.z_index as f32 + z_offset,
    )
}

/// Like [`map_position_to_world_translation`] but takes a raw [`Position`].
///
/// Kept for call sites (e.g., fog overlay) that work with [`Position`] directly.
pub fn position_to_world_translation(
    sprite_size: &SpriteSize,
    position: Position,
    game_map: &GameMap,
) -> Vec3 {
    map_position_to_world_translation(sprite_size, position.into(), game_map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use awbrn_map::AwbrnMap;

    fn map_3x2() -> GameMap {
        let mut m = GameMap::default();
        m.set(AwbrnMap::new(3, 2, awbrn_types::GraphicalTerrain::Plain));
        m
    }

    /// A 16×16 sprite that exactly fills one tile cell (cursor / fog overlay size).
    const TILE_SPRITE: SpriteSize = SpriteSize {
        width: TILE_SIZE,
        height: TILE_SIZE,
        z_index: 0,
    };

    #[test]
    fn round_trip_tile_to_world_to_tile() {
        let game_map = map_3x2();

        for pos in [
            MapPosition::new(0, 0),
            MapPosition::new(1, 0),
            MapPosition::new(0, 1),
            MapPosition::new(2, 1),
        ] {
            let world = WorldPos::from_bevy(
                map_position_to_world_translation(&TILE_SPRITE, pos, &game_map).truncate(),
            );

            assert_eq!(world.to_map_position(&game_map), Some(pos));

            // Clicks within the tile cell should still resolve to the same tile.
            assert_eq!(
                (world + Vec2::new(-3.0, 3.0)).to_map_position(&game_map),
                Some(pos)
            );
            assert_eq!(
                (world + Vec2::new(3.0, -3.0)).to_map_position(&game_map),
                Some(pos)
            );
        }
    }

    #[test]
    fn tile_center_positions_3x2_map() {
        let game_map = map_3x2();

        // Top-left tile
        assert!(
            map_position_to_world_translation(&TILE_SPRITE, MapPosition::new(0, 0), &game_map)
                .truncate()
                .abs_diff_eq(Vec2::new(-16.0, 0.0), 0.001),
        );

        // Bottom-right tile
        assert!(
            map_position_to_world_translation(&TILE_SPRITE, MapPosition::new(2, 1), &game_map)
                .truncate()
                .abs_diff_eq(Vec2::new(16.0, -16.0), 0.001),
        );
    }

    #[test]
    fn world_outside_map_returns_none() {
        let game_map = map_3x2();

        // Far outside
        assert!(
            WorldPos::new(9999.0, 9999.0)
                .to_map_position(&game_map)
                .is_none()
        );
        assert!(
            WorldPos::new(-9999.0, -9999.0)
                .to_map_position(&game_map)
                .is_none()
        );
    }
}
