use awbrn_core::{GraphicalTerrain, Weather};
use awbrn_map::{AwbrnMap, Position};
use bevy::prelude::*;

// Resource to track camera scale
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct CameraScale(f32);

impl CameraScale {
    pub fn scale(&self) -> f32 {
        self.0
    }

    pub fn zoom_in(&self) -> Self {
        let current_index = ZOOM_LEVELS
            .iter()
            .position(|&z| (z - self.0).abs() < 0.01)
            .unwrap_or(0);

        let new_scale = ZOOM_LEVELS[current_index.saturating_add(1).min(ZOOM_LEVELS.len() - 1)];

        CameraScale(new_scale)
    }

    pub fn zoom_out(&self) -> Self {
        let current_index = ZOOM_LEVELS
            .iter()
            .position(|&z| (z - self.0).abs() < 0.01)
            .unwrap_or(0);

        let new_scale = ZOOM_LEVELS[current_index.saturating_sub(1)];

        CameraScale(new_scale)
    }
}

impl Default for CameraScale {
    fn default() -> Self {
        CameraScale(2.0)
    }
}

// Available zoom levels
const ZOOM_LEVELS: [f32; 3] = [1.0, 1.5, 2.0];

// Resource to track current weather
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurrentWeather(Weather);

impl Default for CurrentWeather {
    fn default() -> Self {
        CurrentWeather(Weather::Clear)
    }
}

impl CurrentWeather {
    pub fn set(&mut self, weather: Weather) {
        self.0 = weather;
    }

    pub fn weather(&self) -> Weather {
        self.0
    }
}
// Add a resource to store the loaded map
#[derive(Resource)]
pub struct GameMap(AwbrnMap);

impl Default for GameMap {
    fn default() -> Self {
        let default_terrain = GraphicalTerrain::Plain;
        GameMap(AwbrnMap::new(1, 1, default_terrain))
    }
}

impl GameMap {
    pub fn width(&self) -> usize {
        self.0.width()
    }

    pub fn height(&self) -> usize {
        self.0.height()
    }

    pub fn set(&mut self, map: AwbrnMap) {
        self.0 = map;
    }

    pub fn terrain_at(&self, position: Position) -> Option<GraphicalTerrain> {
        self.0.terrain_at(position)
    }
}

// Component to store terrain data for each tile
#[derive(Component)]
pub struct TerrainTile {
    pub terrain: GraphicalTerrain,
    pub position: Position,
}

// Component to mark the currently selected tile
#[derive(Component)]
pub struct SelectedTile;
