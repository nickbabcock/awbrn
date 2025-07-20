use crate::SpriteSize;
use awbrn_map::Position;
use bevy::prelude::*;

// Horizontal alignment options
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HorizontalAlign {
    Left,
    Center,
    Right,
}

// Vertical alignment options
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VerticalAlign {
    Top,
    Center,
    Bottom,
}

// Grid position abstraction to handle positioning on 16x16 grid
#[derive(Debug, Clone)]
pub struct GridPosition {
    pub x: usize,
    pub y: usize,
    pub width: f32,
    pub height: f32,
    pub z_index: f32,
    pub h_align: HorizontalAlign,
    pub v_align: VerticalAlign,
}

// Grid system to handle conversions between grid and world coordinates
// Coordinates calculated by this system are relative to a (0,0) top-left origin.
#[derive(Debug)]
pub struct GridSystem {
    pub map_width: f32,
    pub map_height: f32,
}

impl GridSystem {
    /// The size (in pixels) of a single grid tile.
    pub const TILE_SIZE: f32 = 16.0;

    // Create a new grid system with the given map dimensions
    pub fn new(map_width: usize, map_height: usize) -> Self {
        Self {
            map_width: map_width as f32,
            map_height: map_height as f32,
        }
    }

    // Calculate local world position from grid position, relative to (0,0) top-left.
    // Y increases downwards in this local system.
    pub fn grid_to_world(&self, grid_pos: &GridPosition) -> Vec3 {
        // Base tile position (top-left corner relative to grid origin 0,0)
        let base_x = grid_pos.x as f32 * Self::TILE_SIZE;
        let base_y = grid_pos.y as f32 * Self::TILE_SIZE; // Y increases downwards

        // Apply horizontal alignment within the tile
        let x_align_offset = match grid_pos.h_align {
            HorizontalAlign::Left => 0.0,
            HorizontalAlign::Center => (Self::TILE_SIZE - grid_pos.width) / 2.0,
            HorizontalAlign::Right => Self::TILE_SIZE - grid_pos.width,
        };

        // Apply vertical alignment within the tile (relative to top-left)
        // Y increases downwards
        let y_align_offset = match grid_pos.v_align {
            VerticalAlign::Top => 0.0,
            VerticalAlign::Center => (Self::TILE_SIZE - grid_pos.height) / 2.0,
            VerticalAlign::Bottom => Self::TILE_SIZE - grid_pos.height,
        };

        // Final local position relative to grid's (0,0) top-left
        Vec3::new(
            base_x + x_align_offset,
            base_y + y_align_offset, // Y increases downwards
            grid_pos.z_index,
        )
    }

    /// Create a grid position based on sprite size with flexible positioning
    pub fn sprite_position(&self, position: Position, sprite_size: &SpriteSize) -> GridPosition {
        GridPosition {
            x: position.x,
            y: position.y,
            width: sprite_size.width,
            height: sprite_size.height,
            z_index: sprite_size.z_index as f32,
            h_align: HorizontalAlign::Center,
            v_align: VerticalAlign::Center,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_grid_to_world_terrain_origin() {
        let grid = GridSystem::new(10, 10);
        let terrain_sprite = SpriteSize {
            width: 16.0,
            height: 32.0,
            z_index: 0,
        };
        let pos = grid.sprite_position(Position::new(0, 0), &terrain_sprite);
        let local_pos = grid.grid_to_world(&pos);
        assert_relative_eq!(local_pos.x, 0.0, epsilon = 0.001);
        assert_relative_eq!(local_pos.y, -8.0, epsilon = 0.001);
        assert_relative_eq!(local_pos.z, 0.0, epsilon = 0.001);
    }

    #[test]
    fn test_grid_to_world_unit_origin() {
        let grid = GridSystem::new(10, 10);
        let unit_sprite = SpriteSize {
            width: 23.0,
            height: 24.0,
            z_index: 1,
        };
        let pos = grid.sprite_position(Position::new(0, 0), &unit_sprite);
        let local_pos = grid.grid_to_world(&pos);
        assert_relative_eq!(local_pos.x, -3.5, epsilon = 0.001);
        assert_relative_eq!(local_pos.y, -4.0, epsilon = 0.001);
        assert_relative_eq!(local_pos.z, 1.0, epsilon = 0.001);
    }

    #[test]
    fn test_grid_to_world_terrain() {
        let grid = GridSystem::new(10, 10);
        let terrain_sprite = SpriteSize {
            width: 16.0,
            height: 32.0,
            z_index: 0,
        };
        let pos = grid.sprite_position(Position::new(3, 4), &terrain_sprite);
        let local_pos = grid.grid_to_world(&pos);
        assert_relative_eq!(local_pos.x, 48.0, epsilon = 0.001);
        assert_relative_eq!(local_pos.y, 56.0, epsilon = 0.001);
        assert_relative_eq!(local_pos.z, 0.0, epsilon = 0.001);
    }

    #[test]
    fn test_grid_to_world_unit() {
        let grid = GridSystem::new(10, 10);
        let unit_sprite = SpriteSize {
            width: 23.0,
            height: 24.0,
            z_index: 1,
        };
        let pos = grid.sprite_position(Position::new(3, 4), &unit_sprite);
        let local_pos = grid.grid_to_world(&pos);
        assert_relative_eq!(local_pos.x, 44.5, epsilon = 0.001);
        assert_relative_eq!(local_pos.y, 60.0, epsilon = 0.001);
        assert_relative_eq!(local_pos.z, 1.0, epsilon = 0.001);
    }

    #[test]
    fn test_sprite_position() {
        let grid = GridSystem::new(10, 10);

        // Test terrain-like sprite (z_index: 0)
        let terrain_sprite = SpriteSize {
            width: 16.0,
            height: 32.0,
            z_index: 0,
        };
        let terrain_pos = grid.sprite_position(Position::new(2, 3), &terrain_sprite);
        assert_eq!(terrain_pos.x, 2);
        assert_eq!(terrain_pos.y, 3);
        assert_eq!(terrain_pos.width, 16.0);
        assert_eq!(terrain_pos.height, 32.0);
        assert_eq!(terrain_pos.z_index, 0.0);

        // Test unit-like sprite (z_index: 1)
        let unit_sprite = SpriteSize {
            width: 23.0,
            height: 24.0,
            z_index: 1,
        };
        let unit_pos = grid.sprite_position(Position::new(5, 7), &unit_sprite);
        assert_eq!(unit_pos.x, 5);
        assert_eq!(unit_pos.y, 7);
        assert_eq!(unit_pos.width, 23.0);
        assert_eq!(unit_pos.height, 24.0);
        assert_eq!(unit_pos.z_index, 1.0);

        // Test world positioning with terrain sprite
        let local_pos = grid.grid_to_world(&terrain_pos);
        assert_relative_eq!(local_pos.x, 32.0, epsilon = 0.001); // 2 * 16 + 0 (centered)
        assert_relative_eq!(local_pos.y, 40.0, epsilon = 0.001); // 3 * 16 - 8 (centered vertically)
        assert_relative_eq!(local_pos.z, 0.0, epsilon = 0.001);

        // Test world positioning with unit sprite
        let unit_local_pos = grid.grid_to_world(&unit_pos);
        assert_relative_eq!(unit_local_pos.x, 76.5, epsilon = 0.001); // 5 * 16 - 3.5 (centered)
        assert_relative_eq!(unit_local_pos.y, 108.0, epsilon = 0.001); // 7 * 16 - 4 (centered)
        assert_relative_eq!(unit_local_pos.z, 1.0, epsilon = 0.001);
    }
}
