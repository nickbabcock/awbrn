use crate::core::SpriteSize;
use awbrn_core::GraphicalTerrain;
use awbrn_map::{AwbrnMap, Position};
use bevy::prelude::*;

#[derive(Component)]
#[require(SpriteSize { width: 16.0, height: 32.0, z_index: 0 })]
pub struct TerrainTile {
    pub terrain: GraphicalTerrain,
    pub position: Position,
}

/// Add a resource to store the loaded map
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
