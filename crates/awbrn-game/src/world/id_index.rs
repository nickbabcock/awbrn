use bevy::prelude::*;
use std::collections::HashMap;

#[derive(Debug, Resource)]
pub struct StrongIdMap<T> {
    units: HashMap<T, Entity>,
}

impl<T> StrongIdMap<T>
where
    T: Eq + std::hash::Hash,
{
    pub fn insert(&mut self, strong_id: T, entity: Entity) {
        self.units.insert(strong_id, entity);
    }

    pub fn get(&self, strong_id: &T) -> Option<Entity> {
        self.units.get(strong_id).copied()
    }

    pub fn remove(&mut self, strong_id: T) -> Option<Entity> {
        self.units.remove(&strong_id)
    }
}

impl<T> Default for StrongIdMap<T> {
    fn default() -> Self {
        Self {
            units: HashMap::new(),
        }
    }
}
