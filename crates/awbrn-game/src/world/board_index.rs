use awbrn_map::Position;
use bevy::ecs::world::DeferredWorld;
use bevy::log::warn;
use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardIndexError {
    OutOfBounds {
        position: Position,
        width: usize,
        height: usize,
    },
    MissingTerrain {
        position: Position,
    },
}

#[derive(Debug, Resource)]
pub struct BoardIndex {
    width: usize,
    height: usize,
    terrain_by_tile: Vec<Option<Entity>>,
    unit_by_tile: Vec<Option<Entity>>,
}

impl Default for BoardIndex {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

impl BoardIndex {
    pub fn new(width: usize, height: usize) -> Self {
        let tile_count = width.saturating_mul(height);

        Self {
            width,
            height,
            terrain_by_tile: vec![None; tile_count],
            unit_by_tile: vec![None; tile_count],
        }
    }

    pub fn reset(&mut self, width: usize, height: usize) {
        *self = Self::new(width, height);
    }

    pub fn terrain_entity(&self, position: Position) -> Result<Entity, BoardIndexError> {
        let index = self.tile_index(position)?;
        self.terrain_by_tile[index].ok_or(BoardIndexError::MissingTerrain { position })
    }

    pub fn unit_entity(&self, position: Position) -> Result<Option<Entity>, BoardIndexError> {
        let index = self.tile_index(position)?;
        Ok(self.unit_by_tile[index])
    }

    pub fn set_terrain(
        &mut self,
        position: Position,
        entity: Entity,
    ) -> Result<(), BoardIndexError> {
        let index = self.tile_index(position)?;
        self.terrain_by_tile[index] = Some(entity);
        Ok(())
    }

    pub fn remove_terrain(&mut self, position: Position) -> Result<(), BoardIndexError> {
        let index = self.tile_index(position)?;
        self.terrain_by_tile[index] = None;
        Ok(())
    }

    pub fn set_unit(&mut self, position: Position, entity: Entity) -> Result<(), BoardIndexError> {
        let index = self.tile_index(position)?;
        match self.unit_by_tile[index] {
            Some(existing) if existing != entity => {
                warn!(
                    "BoardIndex unit collision at {:?}: replacing {:?} with {:?}",
                    position, existing, entity
                );
            }
            _ => {}
        }
        self.unit_by_tile[index] = Some(entity);
        Ok(())
    }

    pub fn remove_unit(
        &mut self,
        position: Position,
        entity: Entity,
    ) -> Result<(), BoardIndexError> {
        let index = self.tile_index(position)?;
        if self.unit_by_tile[index] == Some(entity) {
            self.unit_by_tile[index] = None;
        }
        Ok(())
    }

    fn tile_index(&self, position: Position) -> Result<usize, BoardIndexError> {
        if position.x >= self.width || position.y >= self.height {
            return Err(BoardIndexError::OutOfBounds {
                position,
                width: self.width,
                height: self.height,
            });
        }

        Ok(position.y * self.width + position.x)
    }
}

pub fn add_terrain_to_board_index(mut world: DeferredWorld, entity: Entity, position: Position) {
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

pub fn remove_terrain_from_board_index(
    mut world: DeferredWorld,
    entity: Entity,
    position: Position,
) {
    let Some(mut index) = world.get_resource_mut::<BoardIndex>() else {
        return;
    };

    if let Err(error) = index.remove_terrain(position) {
        warn!(
            "Failed to remove terrain entity {:?} at {:?} from BoardIndex: {:?}",
            entity, position, error
        );
    }
}

pub fn add_unit_to_board_index(mut world: DeferredWorld, entity: Entity, position: Position) {
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

pub fn remove_unit_from_board_index(mut world: DeferredWorld, entity: Entity, position: Position) {
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
        game_map.set(AwbrnMap::new(2, 2, GraphicalTerrain::Plain));
        app.insert_resource(game_map);
        app.insert_resource(BoardIndex::default());

        initialize_terrain_semantic_world(app.world_mut());

        let board_index = app.world().resource::<BoardIndex>();
        assert!(board_index.terrain_entity(Position::new(0, 0)).is_ok());
        assert!(board_index.terrain_entity(Position::new(1, 0)).is_ok());
        assert!(board_index.terrain_entity(Position::new(0, 1)).is_ok());
        assert!(board_index.terrain_entity(Position::new(1, 1)).is_ok());
    }

    #[test]
    fn board_index_updates_when_unit_map_position_changes_or_is_removed() {
        let mut app = App::new();
        app.insert_resource(BoardIndex::new(8, 8));

        let start = Position::new(1, 1);
        let end = Position::new(4, 5);
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
        app.insert_resource(BoardIndex::new(4, 4));

        let position = Position::new(2, 3);
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
        let board_index = BoardIndex::new(2, 2);

        assert!(matches!(
            board_index.terrain_entity(Position::new(5, 0)),
            Err(BoardIndexError::OutOfBounds { .. })
        ));
        assert!(matches!(
            board_index.unit_entity(Position::new(0, 5)),
            Err(BoardIndexError::OutOfBounds { .. })
        ));
    }

    #[test]
    fn second_unit_overwrites_existing_slot() {
        let mut board_index = BoardIndex::new(2, 2);
        let position = Position::new(1, 1);
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
        game_map.set(AwbrnMap::new(1, 1, GraphicalTerrain::Plain));
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
                .terrain_entity(Position::new(0, 0))
                .unwrap(),
            terrain_entity
        );
    }

    #[test]
    fn removing_or_despawning_terrain_clears_its_slot() {
        let mut app = App::new();
        app.insert_resource(BoardIndex::new(2, 2));

        let position = Position::new(1, 1);
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
