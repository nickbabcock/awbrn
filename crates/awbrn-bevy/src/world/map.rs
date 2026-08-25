use crate::MapPosition;
use crate::world::BoardIndex;
use crate::world::board::{BoardOf, BoardRoot, adopt_unattached_board_entities, despawn_board};
use awbrn_map::{AwbrnMap, Dimensions, Pos};
use awbrn_types::GraphicalTerrain;
use bevy::prelude::*;
use std::ops::Index;

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
#[derive(Resource, Debug)]
pub struct GameMap(AwbrnMap);

impl Default for GameMap {
    fn default() -> Self {
        let default_terrain = GraphicalTerrain::Plain;
        GameMap(AwbrnMap::new(Dimensions::new(1, 1), default_terrain))
    }
}

impl GameMap {
    /// The shape of this board, and of every map over it.
    pub fn dimensions(&self) -> Dimensions {
        self.0.dimensions()
    }

    pub fn width(&self) -> u8 {
        self.0.width()
    }

    pub fn height(&self) -> u8 {
        self.0.height()
    }

    /// Every tile with its coordinate, row by row.
    pub fn iter(&self) -> impl Iterator<Item = (Pos, GraphicalTerrain)> {
        self.0.iter()
    }

    pub fn set(&mut self, map: AwbrnMap) {
        self.0 = map;
    }

    pub fn terrain_at(&self, position: Pos) -> Option<GraphicalTerrain> {
        self.0.terrain_at(position)
    }

    pub fn set_terrain(
        &mut self,
        position: Pos,
        terrain: GraphicalTerrain,
    ) -> Option<GraphicalTerrain> {
        self.0.set_terrain(position, terrain)
    }
}

impl Index<Pos> for GameMap {
    type Output = GraphicalTerrain;

    fn index(&self, index: Pos) -> &Self::Output {
        &self.0[index]
    }
}

/// Rebuilds the board: the old board goes away, and a fresh board root owns
/// the terrain that `GameMap` describes.
///
/// Returns the new board root, so a caller that spawns units can put them on
/// the board it just made.
pub fn initialize_terrain_semantic_world(world: &mut World) -> Entity {
    despawn_board(world);

    let (dimensions, terrain_tiles): (Dimensions, Vec<_>) = {
        let game_map = world.resource::<GameMap>();
        (
            game_map.dimensions(),
            game_map
                .iter()
                .map(|(position, terrain)| (position, TerrainTile { terrain }))
                .collect(),
        )
    };

    world.resource_mut::<BoardIndex>().reset(dimensions);

    let root = world.spawn(BoardRoot).id();
    for (position, terrain_tile) in terrain_tiles {
        world.spawn((MapPosition::from(position), terrain_tile, BoardOf(root)));
    }

    // Units can predate the board, and the reset above emptied the unit index
    // out from under them. Adopt them, which also puts them back in the index.
    adopt_unattached_board_entities(world, root);

    root
}
