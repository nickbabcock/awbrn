use awbrn_map::{Dimensions, Grid, Pos};
use bevy::ecs::world::DeferredWorld;
use bevy::log::warn;
use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BoardIndexError {
    #[error("position {position:?} is outside the {width}x{height} board")]
    OutOfBounds {
        position: Pos,
        width: u8,
        height: u8,
    },
    #[error("no terrain entity indexed at {position:?}")]
    MissingTerrain { position: Pos },
}

/// Which entity holds the terrain, and which the unit, for each tile.
///
/// Two maps over one board shape, so a coordinate that reads one reads the
/// other. The shape does the bounds checking; this holds no arithmetic of its
/// own.
#[derive(Debug, Resource)]
pub struct BoardIndex {
    terrain_by_tile: Grid<Option<Entity>>,
    unit_by_tile: Grid<Option<Entity>>,
}

impl Default for BoardIndex {
    fn default() -> Self {
        // A board with no tiles: every coordinate is out of bounds, which is
        // what an index consulted before a map loads should say.
        Self::new(Dimensions::new(0, 0))
    }
}

impl BoardIndex {
    pub fn new(dimensions: Dimensions) -> Self {
        Self {
            terrain_by_tile: Grid::filled(dimensions, None),
            unit_by_tile: Grid::filled(dimensions, None),
        }
    }

    pub fn reset(&mut self, dimensions: Dimensions) {
        self.terrain_by_tile.refill(dimensions, None);
        self.unit_by_tile.refill(dimensions, None);
    }

    pub fn terrain_entity(&self, position: Pos) -> Result<Entity, BoardIndexError> {
        self.cell(position)?;
        self.terrain_by_tile[position].ok_or(BoardIndexError::MissingTerrain { position })
    }

    pub fn unit_entity(&self, position: Pos) -> Result<Option<Entity>, BoardIndexError> {
        self.cell(position)?;
        Ok(self.unit_by_tile[position])
    }

    pub fn set_terrain(&mut self, position: Pos, entity: Entity) -> Result<(), BoardIndexError> {
        self.cell(position)?;
        self.terrain_by_tile[position] = Some(entity);
        Ok(())
    }

    pub fn remove_terrain(&mut self, position: Pos, entity: Entity) -> Result<(), BoardIndexError> {
        self.cell(position)?;
        if self.terrain_by_tile[position] == Some(entity) {
            self.terrain_by_tile[position] = None;
        }
        Ok(())
    }

    pub fn set_unit(&mut self, position: Pos, entity: Entity) -> Result<(), BoardIndexError> {
        self.cell(position)?;
        match self.unit_by_tile[position] {
            Some(existing) if existing != entity => {
                warn!(
                    "BoardIndex unit collision at {:?}: replacing {:?} with {:?}",
                    position, existing, entity
                );
            }
            _ => {}
        }
        self.unit_by_tile[position] = Some(entity);
        Ok(())
    }

    pub fn remove_unit(&mut self, position: Pos, entity: Entity) -> Result<(), BoardIndexError> {
        self.cell(position)?;
        if self.unit_by_tile[position] == Some(entity) {
            self.unit_by_tile[position] = None;
        }
        Ok(())
    }

    /// Checks `position` against the board once, so the indexing below cannot
    /// panic.
    fn cell(&self, position: Pos) -> Result<(), BoardIndexError> {
        let dimensions = self.terrain_by_tile.dimensions();
        if dimensions.contains(position) {
            Ok(())
        } else {
            Err(BoardIndexError::OutOfBounds {
                position,
                width: dimensions.width(),
                height: dimensions.height(),
            })
        }
    }
}

pub fn add_terrain_to_board_index(mut world: DeferredWorld, entity: Entity, position: Pos) {
    let Some(mut index) = world.get_resource_mut::<BoardIndex>() else {
        return;
    };

    if let Err(error) = index.set_terrain(position, entity) {
        warn!(
            "Failed to add terrain entity {:?} at {:?} to BoardIndex: {:?}",
            entity, position, error
        );
    }
}

pub fn remove_terrain_from_board_index(mut world: DeferredWorld, entity: Entity, position: Pos) {
    let Some(mut index) = world.get_resource_mut::<BoardIndex>() else {
        return;
    };

    if let Err(error) = index.remove_terrain(position, entity) {
        warn!(
            "Failed to remove terrain entity {:?} at {:?} from BoardIndex: {:?}",
            entity, position, error
        );
    }
}

pub fn add_unit_to_board_index(mut world: DeferredWorld, entity: Entity, position: Pos) {
    let Some(mut index) = world.get_resource_mut::<BoardIndex>() else {
        return;
    };

    if let Err(error) = index.set_unit(position, entity) {
        warn!(
            "Failed to add unit entity {:?} at {:?} to BoardIndex: {:?}",
            entity, position, error
        );
    }
}

pub fn remove_unit_from_board_index(mut world: DeferredWorld, entity: Entity, position: Pos) {
    let Some(mut index) = world.get_resource_mut::<BoardIndex>() else {
        return;
    };

    if let Err(error) = index.remove_unit(position, entity) {
        warn!(
            "Failed to remove unit entity {:?} at {:?} from BoardIndex: {:?}",
            entity, position, error
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MapPosition;
    use crate::world::{GameMap, TerrainTile, Unit, initialize_terrain_semantic_world};
    use awbrn_map::AwbrnMap;
    use awbrn_types::GraphicalTerrain;

    #[test]
    fn terrain_bootstrap_populates_every_in_bounds_slot() {
        let mut app = App::new();
        let mut game_map = GameMap::default();
        game_map.set(AwbrnMap::new(
            Dimensions::new(2, 2),
            GraphicalTerrain::Plain,
        ));
        app.insert_resource(game_map);
        app.insert_resource(BoardIndex::default());

        initialize_terrain_semantic_world(app.world_mut());

        let board_index = app.world().resource::<BoardIndex>();
        board_index.terrain_entity(Pos::new(0, 0)).unwrap();
        board_index.terrain_entity(Pos::new(1, 0)).unwrap();
        board_index.terrain_entity(Pos::new(0, 1)).unwrap();
        board_index.terrain_entity(Pos::new(1, 1)).unwrap();
    }

    #[test]
    fn terrain_bootstrap_keeps_units_spawned_before_it() {
        let mut app = App::new();
        let mut game_map = GameMap::default();
        game_map.set(AwbrnMap::new(
            Dimensions::new(2, 2),
            GraphicalTerrain::Plain,
        ));
        app.insert_resource(game_map);
        app.insert_resource(BoardIndex::default());

        let position = Pos::new(1, 0);
        let entity = app
            .world_mut()
            .spawn((
                MapPosition::from(position),
                Unit(awbrn_types::Unit::Infantry),
            ))
            .id();

        initialize_terrain_semantic_world(app.world_mut());

        assert_eq!(
            app.world()
                .resource::<BoardIndex>()
                .unit_entity(position)
                .unwrap(),
            Some(entity)
        );
    }

    #[test]
    fn board_index_updates_when_unit_map_position_changes_or_is_removed() {
        let mut app = App::new();
        app.insert_resource(BoardIndex::new(Dimensions::new(8, 8)));

        let start = Pos::new(1, 1);
        let end = Pos::new(4, 5);
        let entity = app
            .world_mut()
            .spawn((MapPosition::from(start), Unit(awbrn_types::Unit::Infantry)))
            .id();

        assert_eq!(
            app.world()
                .resource::<BoardIndex>()
                .unit_entity(start)
                .unwrap(),
            Some(entity)
        );

        app.world_mut()
            .entity_mut(entity)
            .insert(MapPosition::from(end));

        let board_index = app.world().resource::<BoardIndex>();
        assert_eq!(board_index.unit_entity(start).unwrap(), None);
        assert_eq!(board_index.unit_entity(end).unwrap(), Some(entity));
        let _ = board_index;

        app.world_mut().entity_mut(entity).remove::<MapPosition>();

        assert_eq!(
            app.world()
                .resource::<BoardIndex>()
                .unit_entity(end)
                .unwrap(),
            None
        );
    }

    #[test]
    fn despawning_unit_clears_its_unit_slot() {
        let mut app = App::new();
        app.insert_resource(BoardIndex::new(Dimensions::new(4, 4)));

        let position = Pos::new(2, 3);
        let entity = app
            .world_mut()
            .spawn((
                MapPosition::from(position),
                Unit(awbrn_types::Unit::Infantry),
            ))
            .id();

        app.world_mut().despawn(entity);

        assert_eq!(
            app.world()
                .resource::<BoardIndex>()
                .unit_entity(position)
                .unwrap(),
            None
        );
    }

    #[test]
    fn terrain_entity_returns_out_of_bounds_errors() {
        let board_index = BoardIndex::new(Dimensions::new(2, 2));

        assert!(matches!(
            board_index.terrain_entity(Pos::new(5, 0)),
            Err(BoardIndexError::OutOfBounds { .. })
        ));
        assert!(matches!(
            board_index.unit_entity(Pos::new(0, 5)),
            Err(BoardIndexError::OutOfBounds { .. })
        ));
    }

    #[test]
    fn second_unit_overwrites_existing_slot() {
        let mut board_index = BoardIndex::new(Dimensions::new(2, 2));
        let position = Pos::new(1, 1);
        let first = Entity::from_raw_u32(1).unwrap();
        let second = Entity::from_raw_u32(2).unwrap();

        board_index.set_unit(position, first).unwrap();
        board_index.set_unit(position, second).unwrap();

        assert_eq!(board_index.unit_entity(position).unwrap(), Some(second));
    }

    #[test]
    fn terrain_bootstrap_registers_spawned_terrain_entities() {
        let mut app = App::new();
        let mut game_map = GameMap::default();
        game_map.set(AwbrnMap::new(
            Dimensions::new(1, 1),
            GraphicalTerrain::Plain,
        ));
        app.insert_resource(game_map);
        app.insert_resource(BoardIndex::default());

        initialize_terrain_semantic_world(app.world_mut());

        let terrain_entity = {
            let mut query = app
                .world_mut()
                .query_filtered::<Entity, With<TerrainTile>>();
            query.single(app.world()).unwrap()
        };

        assert_eq!(
            app.world()
                .resource::<BoardIndex>()
                .terrain_entity(Pos::new(0, 0))
                .unwrap(),
            terrain_entity
        );
    }

    #[test]
    fn removing_or_despawning_terrain_clears_its_slot() {
        let mut app = App::new();
        app.insert_resource(BoardIndex::new(Dimensions::new(2, 2)));

        let position = Pos::new(1, 1);
        let entity = app
            .world_mut()
            .spawn((
                MapPosition::from(position),
                TerrainTile {
                    terrain: GraphicalTerrain::Plain,
                },
            ))
            .id();

        assert_eq!(
            app.world()
                .resource::<BoardIndex>()
                .terrain_entity(position)
                .unwrap(),
            entity
        );

        app.world_mut().entity_mut(entity).remove::<MapPosition>();
        assert!(matches!(
            app.world()
                .resource::<BoardIndex>()
                .terrain_entity(position),
            Err(BoardIndexError::MissingTerrain { .. })
        ));

        let entity = app
            .world_mut()
            .spawn((
                MapPosition::from(position),
                TerrainTile {
                    terrain: GraphicalTerrain::Plain,
                },
            ))
            .id();
        app.world_mut().despawn(entity);

        assert!(matches!(
            app.world()
                .resource::<BoardIndex>()
                .terrain_entity(position),
            Err(BoardIndexError::MissingTerrain { .. })
        ));
    }
}
