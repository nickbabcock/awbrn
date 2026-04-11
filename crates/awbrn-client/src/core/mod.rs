pub mod coords;

use awbrn_game::world::{GameMap, TerrainTile, Unit};
use awbrn_game::{GameWorldPlugin, MapPosition};
use bevy::prelude::*;

/// Color used for inactive units
pub const INACTIVE_UNIT_COLOR: Color = Color::srgb(0.67, 0.67, 0.67);

/// Z-layer ordering for rendering. Higher values render on top.
/// Within each layer, a small y-based offset (0.001 per row) provides depth sorting.
pub struct RenderLayer;

impl RenderLayer {
    pub const BACKDROP: i8 = 0;
    pub const TERRAIN: i8 = 1;
    pub const FOG_OVERLAY: i8 = 2;
    pub const UNIT: i8 = 3;
    pub const COURSE_ARROW: i8 = 4;
    pub const CURSOR: i8 = 10;
}

#[derive(Component, Copy, Clone)]
pub struct SpriteSize {
    pub width: f32,
    pub height: f32,
    pub z_index: i8,
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
        coords::map_position_to_world_translation(sprite_size, *map_position, game_map.as_ref());

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
        app.add_plugins(GameWorldPlugin)
            .init_state::<AppState>()
            .init_state::<GameMode>()
            .add_sub_state::<LoadingState>()
            .add_observer(on_map_position_insert);

        // Register visual required components for game types defined in awbrn-game
        app.world_mut()
            .register_required_components_with::<Unit, SpriteSize>(|| SpriteSize {
                width: 23.0,
                height: 24.0,
                z_index: RenderLayer::UNIT,
            });
        app.world_mut()
            .register_required_components::<Unit, Visibility>();
        app.world_mut()
            .register_required_components_with::<TerrainTile, SpriteSize>(|| SpriteSize {
                width: 16.0,
                height: 32.0,
                z_index: RenderLayer::TERRAIN,
            });
        app.world_mut()
            .register_required_components::<MapPosition, Transform>();
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

        app.add_plugins(GameWorldPlugin)
            .add_observer(on_map_position_insert);

        // Register visual required components needed for the observer
        app.world_mut()
            .register_required_components_with::<Unit, SpriteSize>(|| SpriteSize {
                width: 23.0,
                height: 24.0,
                z_index: RenderLayer::UNIT,
            });
        app.world_mut()
            .register_required_components_with::<TerrainTile, SpriteSize>(|| SpriteSize {
                width: 16.0,
                height: 32.0,
                z_index: RenderLayer::TERRAIN,
            });
        app.world_mut()
            .register_required_components::<MapPosition, Transform>();

        let terrain_entity = app
            .world_mut()
            .spawn((
                MapPosition::new(5, 3),
                TerrainTile {
                    terrain: awbrn_types::GraphicalTerrain::Plain,
                },
            ))
            .id();

        let unit_entity = app
            .world_mut()
            .spawn((MapPosition::new(8, 2), Unit(awbrn_types::Unit::Infantry)))
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
                .abs_diff_eq(Vec3::new(80.0, -48.0, 1.0), 0.1)
        );
        assert!(
            unit_transform
                .translation
                .abs_diff_eq(Vec3::new(124.5, -36.0, 3.0), 0.1)
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
                .abs_diff_eq(Vec3::new(16.0, -112.0, 1.0), 0.1)
        );
        assert!(
            updated_unit_transform
                .translation
                .abs_diff_eq(Vec3::new(140.5, -20.0, 3.0), 0.1)
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
