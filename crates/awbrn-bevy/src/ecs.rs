use std::collections::HashMap;

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

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AwbwUnitId(pub awbrn_core::AwbwUnitId);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[require(SpriteSize { width: 23.0, height: 24.0, z_index: 1 })]
pub struct Unit(pub awbrn_core::Unit);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Faction(pub awbrn_core::PlayerFaction);

#[derive(Component, Copy, Clone)]
pub struct SpriteSize {
    pub width: f32,
    pub height: f32,
    pub z_index: u8,
}

#[derive(Component)]
#[require(SpriteSize { width: 16.0, height: 32.0, z_index: 0 })]
pub struct TerrainTile {
    pub terrain: GraphicalTerrain,
    pub position: Position,
}

// Component to mark the currently selected tile
#[derive(Component)]
pub struct SelectedTile;

#[derive(Debug, Resource)]
pub struct StrongIdMap<T> {
    units: HashMap<T, Entity>,
}

impl<T> StrongIdMap<T>
where
    T: Eq + std::hash::Hash,
{
    pub fn insert(&mut self, strong_id: T, entity: Entity) {
        self.units.insert(strong_id, entity);
    }

    pub fn get(&self, strong_id: &T) -> Option<Entity> {
        self.units.get(strong_id).copied()
    }

    pub fn remove(&mut self, strong_id: T) -> Option<Entity> {
        self.units.remove(&strong_id)
    }
}

impl<T> Default for StrongIdMap<T> {
    fn default() -> Self {
        Self {
            units: HashMap::new(),
        }
    }
}
