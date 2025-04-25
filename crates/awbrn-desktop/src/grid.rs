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

    /// Create a grid position for a terrain tile
    pub fn terrain_position(&self, x: usize, y: usize) -> GridPosition {
        GridPosition {
            x,
            y,
            width: 16.0,
            height: 32.0,
            z_index: 0.0,
            h_align: HorizontalAlign::Center,
            v_align: VerticalAlign::Center,
        }
    }

    /// Create a grid position for a unit
    pub fn unit_position(&self, x: usize, y: usize) -> GridPosition {
        let unit_width = 23.0;
        let unit_height = 24.0;

        GridPosition {
            x,
            y,
            width: unit_width,
            height: unit_height,
            z_index: 1.0,
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
    fn test_terrain_position() {
        let grid = GridSystem::new(10, 10);
        let pos = grid.terrain_position(3, 4);

        assert_eq!(pos.x, 3);
        assert_eq!(pos.y, 4);
        assert_eq!(pos.width, 16.0);
        assert_eq!(pos.height, 32.0);
        assert_eq!(pos.z_index, 0.0);
        assert_eq!(pos.h_align, HorizontalAlign::Center);
        assert_eq!(pos.v_align, VerticalAlign::Center);
    }

    #[test]
    fn test_unit_position() {
        let grid = GridSystem::new(10, 10);
        let pos = grid.unit_position(3, 4);

        assert_eq!(pos.x, 3);
        assert_eq!(pos.y, 4);
        assert_eq!(pos.width, 23.0);
        assert_eq!(pos.height, 24.0);
        assert_eq!(pos.z_index, 1.0);
        assert_eq!(pos.h_align, HorizontalAlign::Center);
        assert_eq!(pos.v_align, VerticalAlign::Center);
    }

    #[test]
    fn test_grid_to_world_terrain_origin() {
        let grid = GridSystem::new(10, 10);
        let pos = grid.terrain_position(0, 0);
        let local_pos = grid.grid_to_world(&pos);
        assert_relative_eq!(local_pos.x, 0.0, epsilon = 0.001);
        assert_relative_eq!(local_pos.y, -8.0, epsilon = 0.001);
        assert_relative_eq!(local_pos.z, 0.0, epsilon = 0.001);
    }

    #[test]
    fn test_grid_to_world_unit_origin() {
        let grid = GridSystem::new(10, 10);
        let pos = grid.unit_position(0, 0);
        let local_pos = grid.grid_to_world(&pos);
        assert_relative_eq!(local_pos.x, -3.5, epsilon = 0.001);
        assert_relative_eq!(local_pos.y, -4.0, epsilon = 0.001);
        assert_relative_eq!(local_pos.z, 1.0, epsilon = 0.001);
    }

    #[test]
    fn test_grid_to_world_terrain() {
        let grid = GridSystem::new(10, 10);
        let pos = grid.terrain_position(3, 4);
        let local_pos = grid.grid_to_world(&pos);
        assert_relative_eq!(local_pos.x, 48.0, epsilon = 0.001);
        assert_relative_eq!(local_pos.y, 56.0, epsilon = 0.001);
        assert_relative_eq!(local_pos.z, 0.0, epsilon = 0.001);
    }

    #[test]
    fn test_grid_to_world_unit() {
        let grid = GridSystem::new(10, 10);
        let pos = grid.unit_position(3, 4);
        let local_pos = grid.grid_to_world(&pos);
        assert_relative_eq!(local_pos.x, 44.5, epsilon = 0.001);
        assert_relative_eq!(local_pos.y, 60.0, epsilon = 0.001);
        assert_relative_eq!(local_pos.z, 1.0, epsilon = 0.001);
    }
}
