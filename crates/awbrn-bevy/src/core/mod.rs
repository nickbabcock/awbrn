pub mod grid;
pub mod id_index;
pub mod map;
pub mod units;

use awbrn_map::Position;
use bevy::prelude::*;

pub use grid::*;
pub use id_index::*;
pub use map::*;
pub use units::*;

/// Color used for inactive units
pub const INACTIVE_UNIT_COLOR: Color = Color::srgb(0.67, 0.67, 0.67);

#[derive(Component, Copy, Clone)]
pub struct SpriteSize {
    pub width: f32,
    pub height: f32,
    pub z_index: i8,
}

#[derive(Component, Reflect, Clone, Copy, PartialEq, Eq, Debug)]
#[component(immutable)]
#[require(Transform)]
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

/// Compute the world-space offset that centers the map's visual content on the
/// camera origin (0, 0). Sprites use center anchors, so we shift by half a tile
/// on x. Terrain sprites are 32 px tall on 16 px tiles, so the first row's tall
/// portion extends one full tile above the grid – shift by a full tile on y.
pub(crate) fn world_origin_offset(grid: &GridSystem) -> Vec3 {
    let map_pixel_width = grid.map_width * GridSystem::TILE_SIZE;
    let map_pixel_height = grid.map_height * GridSystem::TILE_SIZE;
    Vec3::new(
        -map_pixel_width / 2.0 + GridSystem::TILE_SIZE / 2.0,
        map_pixel_height / 2.0 - GridSystem::TILE_SIZE,
        0.0,
    )
}

pub(crate) fn map_position_to_world_translation(
    sprite_size: &SpriteSize,
    map_position: MapPosition,
    game_map: &GameMap,
) -> Vec3 {
    let grid = GridSystem::new(game_map.width(), game_map.height());
    let offset = world_origin_offset(&grid);
    let grid_pos = grid.sprite_position(map_position.into(), sprite_size);
    let local_pos = grid.grid_to_world(&grid_pos);
    let z_offset = map_position.y() as f32 * 0.001;

    offset + Vec3::new(local_pos.x, -local_pos.y, local_pos.z + z_offset)
}

pub(crate) fn position_to_world_translation(
    sprite_size: &SpriteSize,
    position: Position,
    game_map: &GameMap,
) -> Vec3 {
    map_position_to_world_translation(sprite_size, position.into(), game_map)
}

/// Observer that triggers when MapPosition is inserted
pub(crate) fn on_map_position_insert(
    trigger: On<Insert, MapPosition>,
    mut query: Query<(
        &mut Transform,
        &SpriteSize,
        &MapPosition,
        Has<crate::render::animation::UnitPathAnimation>,
    )>,
    game_map: Res<GameMap>,
) {
    let entity = trigger.entity;

    let Ok((mut transform, sprite_size, map_position, has_path_animation)) = query.get_mut(entity)
    else {
        warn!("Entity {:?} not found in query for MapPosition", entity);
        return;
    };

    if has_path_animation {
        return;
    }

    let final_world_pos =
        map_position_to_world_translation(sprite_size, *map_position, game_map.as_ref());

    transform.translation = final_world_pos;

    info!(
        "Observer: Updated Transform for entity {:?} to position ({}, {}) -> {:?}",
        entity,
        map_position.x(),
        map_position.y(),
        final_world_pos
    );
}

pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameMap>()
            .init_state::<AppState>()
            .init_state::<GameMode>()
            .add_sub_state::<LoadingState>()
            .register_type::<MapPosition>()
            .register_type::<Faction>()
            .register_type::<Unit>()
            .add_observer(on_map_position_insert)
            .add_observer(units::on_unit_destroyed);
    }
}

#[derive(States, Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
pub enum AppState {
    #[default]
    Menu,
    Loading,
    InGame,
}

#[derive(States, Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
pub enum GameMode {
    #[default]
    None,
    Replay,
    Game,
}

#[derive(bevy::state::state::SubStates, Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
#[source(AppState = AppState::Loading)]
pub enum LoadingState {
    #[default]
    LoadingReplay,
    LoadingAssets,
    Complete,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test MapPosition -> Transform observer including updates
    #[test]
    fn test_map_position_observer() {
        let mut app = App::new();

        app.add_observer(on_map_position_insert)
            .init_resource::<GameMap>();

        let terrain_entity = app
            .world_mut()
            .spawn((
                MapPosition::new(5, 3),
                crate::core::map::TerrainTile {
                    terrain: awbrn_core::GraphicalTerrain::Plain,
                    position: Position::new(5, 3),
                },
            ))
            .id();

        let unit_entity = app
            .world_mut()
            .spawn((MapPosition::new(8, 2), Unit(awbrn_core::Unit::Infantry)))
            .id();

        app.update();

        let terrain_transform = *app
            .world()
            .entity(terrain_entity)
            .get::<Transform>()
            .unwrap();
        let unit_transform = *app.world().entity(unit_entity).get::<Transform>().unwrap();

        assert!(
            terrain_transform
                .translation
                .abs_diff_eq(Vec3::new(80.0, -48.0, 0.0), 0.1)
        );
        assert!(
            unit_transform
                .translation
                .abs_diff_eq(Vec3::new(124.5, -36.0, 1.0), 0.1)
        );

        app.world_mut()
            .entity_mut(terrain_entity)
            .insert(MapPosition::new(1, 7));
        app.world_mut()
            .entity_mut(unit_entity)
            .insert(MapPosition::new(9, 1));

        app.update();

        let updated_terrain_transform = *app
            .world()
            .entity(terrain_entity)
            .get::<Transform>()
            .unwrap();
        let updated_unit_transform = *app.world().entity(unit_entity).get::<Transform>().unwrap();

        assert!(
            updated_terrain_transform
                .translation
                .abs_diff_eq(Vec3::new(16.0, -112.0, 0.0), 0.1)
        );
        assert!(
            updated_unit_transform
                .translation
                .abs_diff_eq(Vec3::new(140.5, -20.0, 1.0), 0.1)
        );

        assert_ne!(
            terrain_transform.translation, updated_terrain_transform.translation,
            "Terrain transform should change when MapPosition is updated"
        );
        assert_ne!(
            unit_transform.translation, updated_unit_transform.translation,
            "Unit transform should change when MapPosition is updated"
        );
    }
}
