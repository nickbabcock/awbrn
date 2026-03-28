use crate::MapPosition;
use crate::world::BoardIndex;
use awbrn_map::{AwbrnMap, Position};
use awbrn_types::GraphicalTerrain;
use bevy::prelude::*;

#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq)]
#[component(immutable)]
#[reflect(Component)]
/// `TerrainTile` must only exist on entities that also have `MapPosition`.
pub struct TerrainTile {
    pub terrain: GraphicalTerrain,
}

/// Terrain HP is used for destructible terrain like pipe seams.
#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[reflect(Component)]
pub struct TerrainHp(pub u8);

impl TerrainHp {
    pub fn value(&self) -> u8 {
        self.0
    }
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

    pub fn set_terrain(
        &mut self,
        position: Position,
        terrain: GraphicalTerrain,
    ) -> Option<GraphicalTerrain> {
        self.0.set_terrain(position, terrain)
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

    let (map_width, map_height, terrain_tiles): (usize, usize, Vec<_>) = {
        let game_map = world.resource::<GameMap>();
        (
            game_map.width(),
            game_map.height(),
            (0..game_map.height())
                .flat_map(|y| {
                    (0..game_map.width()).filter_map(move |x| {
                        let position = Position::new(x, y);
                        game_map
                            .terrain_at(position)
                            .map(|terrain| (position, TerrainTile { terrain }))
                    })
                })
                .collect(),
        )
    };

    world
        .resource_mut::<BoardIndex>()
        .reset(map_width, map_height);

    for (position, terrain_tile) in terrain_tiles {
        world.spawn((MapPosition::from(position), terrain_tile));
    }
}
