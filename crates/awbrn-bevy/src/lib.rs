pub mod replay;
pub mod snapshot;
pub mod world;

use awbrn_map::Pos;
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use bevy::reflect::reflect_remote;

use world::board_index;

/// Reflection for the VM's coordinate, which the VM itself cannot derive.
///
/// `awvm` is deliberately free of Bevy (see its crate docs), so the board
/// coordinate carries no `Reflect`. Bevy's remote derive supplies one from
/// this side of the boundary, which is what lets the ECS hold the same
/// coordinate the VM does rather than a second copy of it.
#[reflect_remote(Pos)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PosRemote {
    pub x: u8,
    pub y: u8,
}

#[derive(Component, Reflect, Clone, Copy, PartialEq, Eq, Debug)]
#[component(
    immutable,
    on_insert = on_map_position_insert_into_board_index,
    on_discard = on_map_position_replace_in_board_index
)]
#[reflect(Component)]
/// ECS component wrapper for [`awbrn_map::Pos`].
///
/// Grid semantics are identical to [`Pos`]: top-left origin, `x` right,
/// `y` down. This type exists so ECS lifecycle hooks can keep `BoardIndex`
/// synchronized when map entities move or leave the board.
pub struct MapPosition(#[reflect(remote = PosRemote)] pub Pos);

impl MapPosition {
    pub fn new(x: u8, y: u8) -> Self {
        Self(Pos::new(x, y))
    }

    pub fn x(&self) -> u8 {
        self.0.x
    }

    pub fn y(&self) -> u8 {
        self.0.y
    }

    pub fn position(&self) -> Pos {
        self.0
    }
}

impl From<Pos> for MapPosition {
    fn from(position: Pos) -> Self {
        Self(position)
    }
}

impl From<MapPosition> for Pos {
    fn from(position: MapPosition) -> Self {
        position.0
    }
}

impl AsRef<Pos> for MapPosition {
    fn as_ref(&self) -> &Pos {
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
#[derive(Debug)]
pub struct GameWorldPlugin;

impl Plugin for GameWorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(snapshot::GameSnapshotPlugin)
            .init_resource::<world::GameMap>()
            .init_resource::<world::BoardIndex>()
            .init_resource::<world::ViewerVisibility>()
            .init_resource::<world::FriendlyFactions>()
            .init_resource::<world::CurrentWeather>()
            .init_resource::<replay::RecipientObservations>()
            .init_resource::<replay::ReplayTerrainKnowledge>()
            .init_resource::<replay::ReplayViewpoint>()
            .init_resource::<replay::ReplayPlayerRegistry>()
            .init_resource::<world::StrongIdMap<replay::AwbwUnitId>>()
            .register_type::<MapPosition>()
            .register_type::<world::Faction>()
            .register_type::<world::Unit>()
            .register_type::<world::UnitHp>()
            .register_type::<world::CaptureProgress>()
            .register_type::<world::Fuel>()
            .register_type::<world::Ammo>()
            .register_type::<world::VisionRange>()
            .register_type::<replay::AwbwUnitId>()
            .register_type::<replay::ReplayState>()
            .add_observer(world::units::on_unit_destroyed);
    }
}
