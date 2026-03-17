use crate::core::MapPosition;
use crate::core::{RenderLayer, SpriteSize};
use crate::snapshot::ReplaySnapshotEntity;
use awbrn_core::GraphicalTerrain;
use awbrn_map::{AwbrnMap, Position};
use bevy::prelude::*;

#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq)]
#[reflect(Component)]
#[require(SpriteSize { width: 16.0, height: 32.0, z_index: RenderLayer::TERRAIN })]
#[require(ReplaySnapshotEntity)]
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

pub fn initialize_terrain_semantic_world(world: &mut World) {
    let existing_terrain_entities: Vec<Entity> = {
        let mut query = world.query_filtered::<Entity, With<TerrainTile>>();
        query.iter(world).collect()
    };
    for entity in existing_terrain_entities {
        let _ = world.despawn(entity);
    }

    let terrain_tiles: Vec<_> = {
        let game_map = world.resource::<GameMap>();
        (0..game_map.height())
            .flat_map(|y| {
                (0..game_map.width()).filter_map(move |x| {
                    let position = Position::new(x, y);
                    game_map
                        .terrain_at(position)
                        .map(|terrain| TerrainTile { terrain, position })
                })
            })
            .collect()
    };

    for terrain_tile in terrain_tiles {
        world.spawn((MapPosition::from(terrain_tile.position), terrain_tile));
    }
}
