//! Board membership: which entities go away when the board does.
//!
//! Terrain tiles and units belong to a board root entity through the
//! [`BoardOf`] relationship. The relationship target is declared with
//! `linked_spawn`, so despawning the root despawns every entity that belongs to
//! it. One despawn replaces a sweep for each component type, and it cannot
//! collect the terrain but forget the units.

use awbrn_map::Pos;
use bevy::prelude::*;

use crate::MapPosition;
use crate::world::{BoardIndex, TerrainTile, Unit};

/// The entity that owns the current board.
///
/// Despawning it despawns every terrain tile and unit on the board.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardRoot;

/// Relationship component placed on board entities, pointing to their board.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = BoardEntities)]
pub struct BoardOf(pub Entity);

/// Relationship target on the board root, auto-maintained by Bevy when
/// [`BoardOf`] is added or removed.
#[derive(Component, Debug)]
#[relationship_target(relationship = BoardOf, linked_spawn)]
pub struct BoardEntities(Vec<Entity>);

/// The current board root, when a board is loaded.
pub fn board_root(world: &mut World) -> Option<Entity> {
    let mut query = world.query_filtered::<Entity, With<BoardRoot>>();
    query.iter(world).next()
}

/// Despawns every board root, and with it every entity on that board.
pub fn despawn_board(world: &mut World) {
    let roots: Vec<Entity> = {
        let mut query = world.query_filtered::<Entity, With<BoardRoot>>();
        query.iter(world).collect()
    };
    for root in roots {
        let _ = world.despawn(root);
    }
}

/// Puts the board entities that belong to no board under `root`.
///
/// A terrain tile or a unit can reach the world without a board: spawned before
/// the board existed, or written in by a snapshot restore. This adopts them, so
/// the next [`despawn_board`] takes them too, and puts the units back into
/// [`BoardIndex`], which their spawn hook cannot do when the index is reset
/// after they arrive.
pub fn adopt_unattached_board_entities(world: &mut World, root: Entity) {
    let unattached: Vec<(Entity, Option<Pos>)> = {
        let mut query = world.query_filtered::<
            (Entity, Option<&MapPosition>, Has<Unit>),
            (Or<(With<Unit>, With<TerrainTile>)>, Without<BoardOf>),
        >();
        query
            .iter(world)
            .map(|(entity, map_position, is_unit)| {
                let unit_position = is_unit
                    .then(|| map_position.map(MapPosition::position))
                    .flatten();
                (entity, unit_position)
            })
            .collect()
    };

    for (entity, unit_position) in unattached {
        world.entity_mut(entity).insert(BoardOf(root));

        let Some(position) = unit_position else {
            continue;
        };
        let mut board_index = world.resource_mut::<BoardIndex>();
        if let Err(error) = board_index.set_unit(position, entity) {
            warn!(
                "Failed to index adopted unit entity {:?} at {:?}: {:?}",
                entity, position, error
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{GameMap, initialize_terrain_semantic_world};
    use awbrn_map::{AwbrnMap, Dimensions};
    use awbrn_types::GraphicalTerrain;

    fn board_app() -> App {
        let mut app = App::new();
        let mut game_map = GameMap::default();
        game_map.set(AwbrnMap::new(
            Dimensions::new(2, 2),
            GraphicalTerrain::Plain,
        ));
        app.insert_resource(game_map);
        app.insert_resource(BoardIndex::default());
        app
    }

    #[test]
    fn rebuilding_the_board_takes_its_terrain_and_units_with_it() {
        let mut app = board_app();
        let first_root = initialize_terrain_semantic_world(app.world_mut());
        let unit = app
            .world_mut()
            .spawn((
                MapPosition::from(Pos::new(0, 1)),
                Unit(awbrn_types::Unit::Infantry),
                BoardOf(first_root),
            ))
            .id();
        let terrain: Vec<Entity> = {
            let mut query = app
                .world_mut()
                .query_filtered::<Entity, With<TerrainTile>>();
            query.iter(app.world()).collect()
        };

        let second_root = initialize_terrain_semantic_world(app.world_mut());

        assert_ne!(first_root, second_root);
        assert!(app.world().get_entity(first_root).is_err());
        assert!(app.world().get_entity(unit).is_err());
        for entity in terrain {
            assert!(app.world().get_entity(entity).is_err());
        }
        assert_eq!(
            app.world()
                .resource::<BoardIndex>()
                .unit_entity(Pos::new(0, 1))
                .unwrap(),
            None
        );
    }

    #[test]
    fn the_new_board_owns_the_terrain_it_spawns() {
        let mut app = board_app();
        let root = initialize_terrain_semantic_world(app.world_mut());

        let owned = app.world().entity(root).get::<BoardEntities>().unwrap();
        assert_eq!(owned.0.len(), 4);
    }

    #[test]
    fn a_unit_spawned_before_the_board_joins_it() {
        let mut app = board_app();
        let position = Pos::new(1, 0);
        let unit = app
            .world_mut()
            .spawn((
                MapPosition::from(position),
                Unit(awbrn_types::Unit::Infantry),
            ))
            .id();

        let root = initialize_terrain_semantic_world(app.world_mut());

        assert_eq!(app.world().entity(unit).get::<BoardOf>().unwrap().0, root);
        assert_eq!(
            app.world()
                .resource::<BoardIndex>()
                .unit_entity(position)
                .unwrap(),
            Some(unit)
        );
    }
}
