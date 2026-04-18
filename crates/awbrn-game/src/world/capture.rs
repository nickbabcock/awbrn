use bevy::ecs::query::QueryData;
use bevy::prelude::*;

use crate::MapPosition;
use crate::world::{
    BoardIndex, CaptureProgress, CaptureResolution, Faction, GameMap, GraphicalHp, TerrainHp,
    TerrainTile, Unit, UnitActive,
};
use awbrn_map::Position;
use awbrn_types::{Faction as TerrainFaction, GraphicalTerrain, PlayerFaction, Property};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureProgressInput {
    AddCurrentVisualHp,
    SetPostActionProgress(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureAction {
    pub unit_entity: Entity,
    pub progress_input: CaptureProgressInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureActionOutcome {
    Continued {
        entity: Entity,
        tile: Position,
        faction: PlayerFaction,
        progress: CaptureProgress,
    },
    Completed {
        entity: Entity,
        tile: Position,
        new_faction: PlayerFaction,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureActionError {
    MissingUnit(Entity),
    MissingTerrain(Position),
    NonPropertyTerrain(Position),
    NonCapturingUnit {
        entity: Entity,
        unit: awbrn_types::Unit,
    },
    OwnProperty {
        tile: Position,
        faction: PlayerFaction,
    },
}

impl std::fmt::Display for CaptureActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingUnit(entity) => write!(f, "missing capture actor {entity:?}"),
            Self::MissingTerrain(position) => write!(f, "missing terrain at {position:?}"),
            Self::NonPropertyTerrain(position) => {
                write!(f, "terrain at {position:?} is not a property")
            }
            Self::NonCapturingUnit { entity, unit } => {
                write!(f, "unit {entity:?} of type {unit:?} cannot capture")
            }
            Self::OwnProperty { tile, faction } => {
                write!(f, "property at {tile:?} is already owned by {faction:?}")
            }
        }
    }
}

impl std::error::Error for CaptureActionError {}

#[derive(QueryData)]
struct CaptureActor {
    position: &'static MapPosition,
    unit: &'static Unit,
    faction: &'static Faction,
    hp: &'static GraphicalHp,
    progress: Option<&'static CaptureProgress>,
}

struct CaptureActorSnapshot {
    tile: Position,
    faction: PlayerFaction,
    visual_hp: u8,
    progress: CaptureProgress,
}

impl CaptureActorSnapshot {
    fn read(world: &mut World, entity: Entity) -> Result<Self, CaptureActionError> {
        register_capture_actor_components(world);

        let entity_ref = world
            .get_entity(entity)
            .map_err(|_| CaptureActionError::MissingUnit(entity))?;
        let actor = entity_ref
            .get_components::<CaptureActor>()
            .map_err(|_| CaptureActionError::MissingUnit(entity))?;

        if !matches!(
            actor.unit.0,
            awbrn_types::Unit::Infantry | awbrn_types::Unit::Mech
        ) {
            return Err(CaptureActionError::NonCapturingUnit {
                entity,
                unit: actor.unit.0,
            });
        }

        Ok(Self {
            tile: actor.position.position(),
            faction: actor.faction.0,
            visual_hp: actor.hp.0,
            progress: actor
                .progress
                .copied()
                .unwrap_or_else(CaptureProgress::empty),
        })
    }
}

fn register_capture_actor_components(world: &mut World) {
    // EntityRef::get_components reads QueryData without building a QueryState,
    // so optional components like CaptureProgress must be registered up front.
    world.register_component::<MapPosition>();
    world.register_component::<Unit>();
    world.register_component::<Faction>();
    world.register_component::<GraphicalHp>();
    world.register_component::<CaptureProgress>();
}

impl CaptureAction {
    pub fn apply(self, world: &mut World) -> Result<CaptureActionOutcome, CaptureActionError> {
        let actor = CaptureActorSnapshot::read(world, self.unit_entity)?;
        validate_capture_target(world, actor.tile, actor.faction)?;

        let resolution = match self.progress_input {
            CaptureProgressInput::AddCurrentVisualHp => {
                actor.progress.advance_by_visual_hp(actor.visual_hp)
            }
            CaptureProgressInput::SetPostActionProgress(points) => {
                CaptureProgress::from_post_action_points(points)
            }
        };

        world.entity_mut(self.unit_entity).remove::<UnitActive>();

        match resolution {
            CaptureResolution::Continued(progress) => {
                world.entity_mut(self.unit_entity).insert(progress);
                Ok(CaptureActionOutcome::Continued {
                    entity: self.unit_entity,
                    tile: actor.tile,
                    faction: actor.faction,
                    progress,
                })
            }
            CaptureResolution::Completed => {
                world
                    .entity_mut(self.unit_entity)
                    .remove::<CaptureProgress>();
                capture_property_at(world, actor.tile, actor.faction)?;
                Ok(CaptureActionOutcome::Completed {
                    entity: self.unit_entity,
                    tile: actor.tile,
                    new_faction: actor.faction,
                })
            }
        }
    }
}

fn validate_capture_target(
    world: &World,
    tile: Position,
    faction: PlayerFaction,
) -> Result<(), CaptureActionError> {
    let terrain = world
        .resource::<GameMap>()
        .terrain_at(tile)
        .ok_or(CaptureActionError::MissingTerrain(tile))?;
    let GraphicalTerrain::Property(property) = terrain else {
        return Err(CaptureActionError::NonPropertyTerrain(tile));
    };
    if property.faction() == TerrainFaction::Player(faction) {
        return Err(CaptureActionError::OwnProperty { tile, faction });
    }

    Ok(())
}

pub fn capture_property_at(
    world: &mut World,
    tile: Position,
    faction: PlayerFaction,
) -> Result<GraphicalTerrain, CaptureActionError> {
    let terrain = world
        .resource::<GameMap>()
        .terrain_at(tile)
        .ok_or(CaptureActionError::MissingTerrain(tile))?;
    let new_terrain =
        captured_terrain(terrain, faction).ok_or(CaptureActionError::NonPropertyTerrain(tile))?;
    let terrain_entity = world
        .resource::<BoardIndex>()
        .terrain_entity(tile)
        .map_err(|_| CaptureActionError::MissingTerrain(tile))?;

    let updated = world
        .resource_mut::<GameMap>()
        .set_terrain(tile, new_terrain)
        .is_some();
    if !updated {
        return Err(CaptureActionError::MissingTerrain(tile));
    }

    let mut terrain_entity = world.entity_mut(terrain_entity);
    terrain_entity.insert(TerrainTile {
        terrain: new_terrain,
    });
    terrain_entity.remove::<TerrainHp>();

    Ok(new_terrain)
}

pub fn captured_terrain(
    terrain: GraphicalTerrain,
    faction: PlayerFaction,
) -> Option<GraphicalTerrain> {
    let property = match terrain {
        GraphicalTerrain::Property(property) => property,
        _ => return None,
    };

    let owner = TerrainFaction::Player(faction);
    let captured = match property {
        Property::City(_) => Property::City(owner),
        Property::Base(_) => Property::Base(owner),
        Property::Airport(_) => Property::Airport(owner),
        Property::Port(_) => Property::Port(owner),
        Property::ComTower(_) => Property::ComTower(owner),
        Property::Lab(_) => Property::Lab(owner),
        Property::HQ(_) => Property::HQ(faction),
    };

    Some(GraphicalTerrain::Property(captured))
}

#[cfg(test)]
mod tests {
    use super::*;
    use awbrn_map::AwbrnMap;
    use awbrn_types::Unit as UnitKind;

    fn capture_world(unit: UnitKind, terrain: GraphicalTerrain) -> (World, Entity, Position) {
        let tile = Position::new(0, 0);
        let mut world = World::new();
        world.insert_resource(BoardIndex::new(1, 1));

        let mut game_map = GameMap::default();
        game_map.set(AwbrnMap::new(1, 1, terrain));
        world.insert_resource(game_map);

        world.spawn((MapPosition::from(tile), TerrainTile { terrain }));
        let unit_entity = world
            .spawn((
                MapPosition::from(tile),
                Unit(unit),
                Faction(PlayerFaction::OrangeStar),
                GraphicalHp(10),
                UnitActive,
            ))
            .id();

        (world, unit_entity, tile)
    }

    #[test]
    fn capture_action_rejects_non_capturing_unit() {
        let (mut world, unit_entity, _) = capture_world(
            UnitKind::Tank,
            GraphicalTerrain::Property(Property::City(TerrainFaction::Neutral)),
        );

        let err = CaptureAction {
            unit_entity,
            progress_input: CaptureProgressInput::AddCurrentVisualHp,
        }
        .apply(&mut world)
        .unwrap_err();

        assert!(matches!(
            err,
            CaptureActionError::NonCapturingUnit {
                entity,
                unit: UnitKind::Tank
            } if entity == unit_entity
        ));
    }

    #[test]
    fn capture_action_rejects_own_property() {
        let (mut world, unit_entity, tile) = capture_world(
            UnitKind::Infantry,
            GraphicalTerrain::Property(Property::City(TerrainFaction::Player(
                PlayerFaction::OrangeStar,
            ))),
        );

        let err = CaptureAction {
            unit_entity,
            progress_input: CaptureProgressInput::AddCurrentVisualHp,
        }
        .apply(&mut world)
        .unwrap_err();

        assert!(matches!(
            err,
            CaptureActionError::OwnProperty {
                tile: err_tile,
                faction: PlayerFaction::OrangeStar
            } if err_tile == tile
        ));
    }
}
