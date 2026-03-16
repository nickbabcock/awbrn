use crate::core::SpriteSize;
use bevy::ecs::entity::MapEntities;
use bevy::ecs::relationship::RelationshipSourceCollection;
use bevy::prelude::*;

#[derive(EntityEvent)]
pub struct UnitDestroyed {
    pub entity: Entity,
}

pub(crate) fn on_unit_destroyed(trigger: On<UnitDestroyed>, mut commands: Commands) {
    commands.entity(trigger.entity).despawn();
}

#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[component(immutable)]
#[require(SpriteSize { width: 23.0, height: 24.0, z_index: 1 })]
pub struct Unit(pub awbrn_core::Unit);

#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[component(immutable)]
pub struct Faction(pub awbrn_core::PlayerFaction);

/// Component to mark a unit that can receive orders this turn.
/// Units without this component have already acted and appear grey/frozen.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[component(storage = "SparseSet")]
pub struct UnitActive;

/// Component to mark an entity as capturing a building
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[component(storage = "SparseSet")]
pub struct Capturing;

/// Relationship component placed on carried units, pointing to their transport.
#[derive(Component)]
#[relationship(relationship_target = HasCargo)]
pub struct CarriedBy(pub Entity);

/// Fixed-capacity collection for up to 2 cargo entities.
#[derive(Debug, Clone)]
pub struct Cargo(pub [Option<Entity>; 2]);

impl RelationshipSourceCollection for Cargo {
    type SourceIter<'a> =
        core::iter::Copied<core::iter::Flatten<core::slice::Iter<'a, Option<Entity>>>>;

    fn new() -> Self {
        Cargo([None; 2])
    }

    fn with_capacity(_capacity: usize) -> Self {
        Self::new()
    }

    fn reserve(&mut self, _additional: usize) {}

    fn add(&mut self, entity: Entity) -> bool {
        if let Some(slot) = self.0.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(entity);
            true
        } else {
            false
        }
    }

    fn remove(&mut self, entity: Entity) -> bool {
        if let Some(slot) = self.0.iter_mut().find(|slot| **slot == Some(entity)) {
            *slot = None;
            true
        } else {
            false
        }
    }

    fn iter(&self) -> Self::SourceIter<'_> {
        self.0.iter().flatten().copied()
    }

    fn len(&self) -> usize {
        self.0.iter().filter(|slot| slot.is_some()).count()
    }

    fn clear(&mut self) {
        self.0 = [None; 2];
    }

    fn shrink_to_fit(&mut self) {}

    fn extend_from_iter(&mut self, entities: impl IntoIterator<Item = Entity>) {
        for entity in entities {
            if !self.add(entity) {
                break;
            }
        }
    }
}

impl MapEntities for Cargo {
    fn map_entities<M: EntityMapper>(&mut self, entity_mapper: &mut M) {
        for entity in self.0.iter_mut().flatten() {
            *entity = entity_mapper.get_mapped(*entity);
        }
    }
}

/// Relationship target on transports, auto-maintained by Bevy when `CarriedBy` is added/removed.
#[derive(Component)]
#[relationship_target(relationship = CarriedBy)]
pub struct HasCargo(Cargo);

#[derive(Debug, Component, Clone, Copy, PartialEq, Eq, Hash)]
#[component(immutable)]
pub struct GraphicalHp(pub u8);

impl GraphicalHp {
    pub fn value(&self) -> u8 {
        self.0
    }

    pub fn is_full_health(&self) -> bool {
        self.0 >= 10
    }

    pub fn is_destroyed(&self) -> bool {
        self.0 == 0
    }
}
