use crate::core::SpriteSize;
use bevy::prelude::*;

#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[require(SpriteSize { width: 23.0, height: 24.0, z_index: 1 })]
pub struct Unit(pub awbrn_core::Unit);

#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Faction(pub awbrn_core::PlayerFaction);

/// Component to mark a unit that can receive orders this turn.
/// Units without this component have already acted and appear grey/frozen.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitActive;

/// Component to mark an entity as capturing a building
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Capturing;

/// Component to mark an entity as carrying cargo (up to 2 units).
/// Stores entity references directly rather than mode-specific IDs.
#[derive(Component, Reflect, Debug, Clone, PartialEq, Eq, Default)]
pub struct HasCargo {
    pub cargo: [Option<Entity>; 2],
}

impl HasCargo {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_cargo(&mut self, entity: Entity) -> bool {
        if let Some(slot) = self.cargo.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(entity);
            true
        } else {
            false
        }
    }

    pub fn is_empty(&self) -> bool {
        self.cargo.iter().all(|slot| slot.is_none())
    }

    pub fn remove_cargo(&mut self, entity: Entity) -> bool {
        if let Some(slot) = self.cargo.iter_mut().find(|slot| **slot == Some(entity)) {
            *slot = None;
            true
        } else {
            false
        }
    }
}

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
