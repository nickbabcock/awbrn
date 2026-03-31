use bevy::prelude::*;

/// Unique identifier for a unit within a server game.
/// Assigned by a monotonic counter in [`crate::ServerGameState`].
#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[component(immutable)]
#[reflect(Component)]
pub struct ServerUnitId(pub u64);
