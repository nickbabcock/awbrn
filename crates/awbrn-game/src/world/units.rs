use bevy::ecs::entity::MapEntities;
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::reflect::ReflectMapEntities;
use bevy::ecs::relationship::RelationshipSourceCollection;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;

#[derive(EntityEvent)]
pub struct UnitDestroyed {
    pub entity: Entity,
}

pub fn on_unit_destroyed(trigger: On<UnitDestroyed>, mut commands: Commands) {
    commands.entity(trigger.entity).despawn();
}

#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[component(immutable)]
#[reflect(Component)]
/// `Unit` must only exist on entities that also have `MapPosition`.
pub struct Unit(pub awbrn_types::Unit);

#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[component(immutable)]
#[reflect(Component)]
pub struct Faction(pub awbrn_types::PlayerFaction);

/// Component to mark a unit that can receive orders this turn.
/// Units without this component have already acted and appear grey/frozen.
#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct UnitActive;

/// Component to mark an entity as capturing a building
#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct Capturing;

/// Marker for a unit that is hiding (submarine dive or stealth activation).
#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct Hiding;

/// Relationship component placed on carried units, pointing to their transport.
#[derive(Component, Reflect, MapEntities, Debug, Clone, Copy, PartialEq, Eq)]
#[reflect(Component, MapEntities)]
#[relationship(relationship_target = HasCargo)]
pub struct CarriedBy(#[entities] pub Entity);

const CARGO_CAPACITY: usize = 2;

/// Fixed-capacity collection for cargo entities.
#[derive(Reflect, Debug, Clone, PartialEq, Eq)]
pub struct Cargo(pub [Option<Entity>; CARGO_CAPACITY]);

impl RelationshipSourceCollection for Cargo {
    type SourceIter<'a> =
        core::iter::Copied<core::iter::Flatten<core::slice::Iter<'a, Option<Entity>>>>;

    fn new() -> Self {
        Cargo([None; CARGO_CAPACITY])
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
        self.0 = [None; CARGO_CAPACITY];
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
#[derive(Component, Reflect, MapEntities, Debug, Clone, PartialEq, Eq)]
#[reflect(Component, MapEntities)]
#[relationship_target(relationship = CarriedBy)]
pub struct HasCargo(Cargo);

#[derive(Debug, Component, Reflect, Clone, Copy, PartialEq, Eq, Hash)]
#[component(immutable)]
#[reflect(Component)]
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

fn sync_graphical_hp(mut world: DeferredWorld, HookContext { entity, .. }: HookContext) {
    let Some(exact) = world.entity(entity).get::<UnitHp>().copied() else {
        return;
    };

    world
        .commands()
        .entity(entity)
        .insert(GraphicalHp(exact.0.visual().get()));
}

#[derive(Debug, Component, Reflect, Clone, Copy, PartialEq, Eq, Hash)]
#[component(immutable, on_insert = sync_graphical_hp)]
#[reflect(Component)]
pub struct UnitHp(pub awbrn_types::ExactHp);

#[derive(Debug, Component, Reflect, Clone, Copy, PartialEq, Eq, Hash)]
#[component(immutable)]
#[reflect(Component)]
pub struct Fuel(pub u32);

impl Fuel {
    pub fn value(&self) -> u32 {
        self.0
    }
}

#[derive(Debug, Component, Reflect, Clone, Copy, PartialEq, Eq, Hash)]
#[component(immutable)]
#[reflect(Component)]
pub struct Ammo(pub u32);

impl Ammo {
    pub fn value(&self) -> u32 {
        self.0
    }
}

#[derive(Debug, Component, Reflect, Clone, Copy, PartialEq, Eq, Hash)]
#[component(immutable)]
#[reflect(Component)]
pub struct VisionRange(pub u32);
