pub mod replay;
pub mod snapshot;
pub mod world;

use awbrn_map::Position;
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;

use world::board_index;

#[derive(Component, Reflect, Clone, Copy, PartialEq, Eq, Debug)]
#[component(
    immutable,
    on_insert = on_map_position_insert_into_board_index,
    on_replace = on_map_position_replace_in_board_index
)]
#[reflect(Component)]
pub struct MapPosition(pub Position);

impl MapPosition {
    pub fn new(x: usize, y: usize) -> Self {
        Self(Position::new(x, y))
    }

    pub fn x(&self) -> usize {
        self.0.x
    }

    pub fn y(&self) -> usize {
        self.0.y
    }

    pub fn position(&self) -> Position {
        self.0
    }
}

impl From<Position> for MapPosition {
    fn from(position: Position) -> Self {
        Self(position)
    }
}

impl From<MapPosition> for Position {
    fn from(position: MapPosition) -> Self {
        position.0
    }
}

impl AsRef<Position> for MapPosition {
    fn as_ref(&self) -> &Position {
        &self.0
    }
}

fn on_map_position_insert_into_board_index(
    mut world: DeferredWorld,
    HookContext { entity, .. }: HookContext,
) {
    let Some(position) = world.get::<MapPosition>(entity).copied() else {
        return;
    };

    let has_unit = world.get::<world::Unit>(entity).is_some();
    let has_terrain = world.get::<world::TerrainTile>(entity).is_some();

    if has_unit && has_terrain {
        warn!(
            "Entity {:?} has both Unit and TerrainTile at {:?}; indexing both is invalid ECS state",
            entity,
            position.position()
        );
    }

    if has_unit {
        board_index::add_unit_to_board_index(world.reborrow(), entity, position.position());
    }
    if has_terrain {
        board_index::add_terrain_to_board_index(world, entity, position.position());
    }
}

fn on_map_position_replace_in_board_index(
    mut world: DeferredWorld,
    HookContext { entity, .. }: HookContext,
) {
    let Some(position) = world.get::<MapPosition>(entity).copied() else {
        return;
    };

    let has_unit = world.get::<world::Unit>(entity).is_some();
    let has_terrain = world.get::<world::TerrainTile>(entity).is_some();

    if has_unit && has_terrain {
        warn!(
            "Entity {:?} has both Unit and TerrainTile at {:?}; removing both from BoardIndex",
            entity,
            position.position()
        );
    }

    if has_unit {
        board_index::remove_unit_from_board_index(world.reborrow(), entity, position.position());
    }
    if has_terrain {
        board_index::remove_terrain_from_board_index(world, entity, position.position());
    }
}

/// Initializes the headless semantic ECS world used by clients and tests.
pub struct GameWorldPlugin;

impl Plugin for GameWorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(snapshot::GameSnapshotPlugin)
            .init_resource::<world::GameMap>()
            .init_resource::<world::BoardIndex>()
            .init_resource::<world::FogOfWarMap>()
            .init_resource::<world::FogActive>()
            .init_resource::<world::FriendlyFactions>()
            .init_resource::<world::CurrentWeather>()
            .init_resource::<replay::PowerVisionBoosts>()
            .init_resource::<replay::ReplayFogEnabled>()
            .init_resource::<replay::ReplayTerrainKnowledge>()
            .init_resource::<replay::ReplayViewpoint>()
            .init_resource::<replay::ReplayPlayerRegistry>()
            .init_resource::<world::StrongIdMap<replay::AwbwUnitId>>()
            .register_type::<MapPosition>()
            .register_type::<world::Faction>()
            .register_type::<world::Unit>()
            .register_type::<world::Fuel>()
            .register_type::<world::Ammo>()
            .register_type::<world::VisionRange>()
            .register_type::<replay::AwbwUnitId>()
            .register_type::<replay::ReplayState>()
            .add_observer(world::units::on_unit_destroyed);
    }
}
