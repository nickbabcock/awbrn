use crate::core::grid::GridSystem;
use crate::core::{RenderLayer, SpriteSize, map_position_to_world_translation};
use crate::features::event_bus::{ExternalGameEvent, GameEvent, TileSelected};
use crate::render::UiAtlas;
use awbrn_game::MapPosition;
use awbrn_game::world::{BoardIndex, GameMap, TerrainTile};
use awbrn_map::Position;
use bevy::prelude::*;

/// Component to mark the currently selected tile
#[derive(Component)]
pub struct SelectedTile;

/// Marker component for the tile hover cursor sprite entity.
#[derive(Component)]
pub struct TileCursor;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapPositionClicked {
    pub position: Position,
}

pub(crate) const TILE_CORE_SPRITE_SIZE: SpriteSize = SpriteSize {
    width: GridSystem::TILE_SIZE,
    height: GridSystem::TILE_SIZE,
    z_index: RenderLayer::CURSOR,
};

fn tile_cursor_bundle(ui_atlas: UiAtlas) -> impl Bundle {
    (
        ui_atlas.sprite_for("Effects/TileCursor.png"),
        Transform::from_translation(Vec3::new(0.0, 0.0, TILE_CORE_SPRITE_SIZE.z_index as f32)),
        Visibility::Hidden,
        TileCursor,
    )
}

pub(crate) fn world_to_map_position(world_pos: Vec2, game_map: &GameMap) -> Option<MapPosition> {
    let map_w = game_map.width() as f32;
    let map_h = game_map.height() as f32;
    let tile = GridSystem::TILE_SIZE;

    let origin_x = -map_w * tile / 2.0;
    let origin_y = map_h * tile / 2.0 - tile / 2.0;

    let gx_f = (world_pos.x - origin_x) / tile;
    let gy_f = (origin_y - world_pos.y) / tile;

    if gx_f < 0.0 || gy_f < 0.0 || gx_f >= map_w || gy_f >= map_h {
        return None;
    }

    Some(MapPosition::new(
        gx_f.floor() as usize,
        gy_f.floor() as usize,
    ))
}

pub(crate) fn spawn_tile_cursor(mut commands: Commands, ui_atlas: UiAtlas) {
    commands.spawn(tile_cursor_bundle(ui_atlas));
}

pub(crate) fn update_tile_cursor(
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    game_map: Res<GameMap>,
    mut cursor_q: Query<(&mut Transform, &mut Visibility), With<TileCursor>>,
) {
    let Ok((mut transform, mut visibility)) = cursor_q.single_mut() else {
        return;
    };

    let Ok(window) = windows.single() else {
        *visibility = Visibility::Hidden;
        return;
    };

    let Ok((camera, camera_transform)) = camera_q.single() else {
        *visibility = Visibility::Hidden;
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        *visibility = Visibility::Hidden;
        return;
    };
    let Ok(world_pos) = camera.viewport_to_world_2d(camera_transform, cursor_pos) else {
        *visibility = Visibility::Hidden;
        return;
    };
    let Some(map_position) = world_to_map_position(world_pos, game_map.as_ref()) else {
        *visibility = Visibility::Hidden;
        return;
    };
    let center =
        map_position_to_world_translation(&TILE_CORE_SPRITE_SIZE, map_position, game_map.as_ref());

    transform.translation.x = center.x;
    transform.translation.y = center.y;
    transform.translation.z = TILE_CORE_SPRITE_SIZE.z_index as f32;
    *visibility = Visibility::Visible;
}

pub(crate) fn detect_map_clicks(
    mouse_button_input: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    game_map: Res<GameMap>,
    mut click_writer: MessageWriter<MapPositionClicked>,
) {
    if !mouse_button_input.just_pressed(MouseButton::Left) {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };

    let Some(cursor_position) = window.cursor_position() else {
        return;
    };

    let Ok((camera, camera_transform)) = camera_q.single() else {
        return;
    };

    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_position) else {
        return;
    };

    let world_position = ray.origin.truncate();
    let Some(map_position) = world_to_map_position(world_position, game_map.as_ref()) else {
        return;
    };

    click_writer.write(MapPositionClicked {
        position: map_position.position(),
    });
}

pub(crate) fn handle_tile_clicks(
    board_index: Res<BoardIndex>,
    tiles: Query<&TerrainTile>,
    mut commands: Commands,
    selected: Query<Entity, With<SelectedTile>>,
    mut click_reader: MessageReader<MapPositionClicked>,
    mut event_writer: MessageWriter<ExternalGameEvent>,
) {
    let Some(MapPositionClicked { position }) = click_reader.read().last().copied() else {
        return;
    };

    for entity in selected.iter() {
        commands.entity(entity).remove::<SelectedTile>();
    }

    let Ok(terrain_entity) = board_index.terrain_entity(position) else {
        return;
    };
    let Ok(tile) = tiles.get(terrain_entity) else {
        return;
    };

    commands.entity(terrain_entity).insert(SelectedTile);
    info!("Selected terrain at {:?}: {:?}", position, tile.terrain);

    event_writer.write(ExternalGameEvent {
        payload: GameEvent::TileSelected(TileSelected {
            x: position.x,
            y: position.y,
            terrain_type: format!("{:?}", tile.terrain),
        }),
    });
}

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<MapPositionClicked>();
        app.add_systems(
            Update,
            (
                (detect_map_clicks, handle_tile_clicks).chain(),
                update_tile_cursor,
            )
                .run_if(in_state(crate::core::AppState::InGame)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use awbrn_map::AwbrnMap;

    #[test]
    fn test_tile_cursor_math_matches_tile_centers() {
        let mut game_map = GameMap::default();
        game_map.set(AwbrnMap::new(3, 2, awbrn_types::GraphicalTerrain::Plain));

        for pos in [
            MapPosition::new(0, 0),
            MapPosition::new(1, 0),
            MapPosition::new(0, 1),
            MapPosition::new(2, 1),
        ] {
            let world_pos =
                map_position_to_world_translation(&TILE_CORE_SPRITE_SIZE, pos, &game_map)
                    .truncate();

            assert_eq!(world_to_map_position(world_pos, &game_map), Some(pos));
            assert_eq!(
                world_to_map_position(world_pos + Vec2::new(-3.0, 3.0), &game_map),
                Some(pos)
            );
            assert_eq!(
                world_to_map_position(world_pos + Vec2::new(3.0, -3.0), &game_map),
                Some(pos)
            );
        }

        assert!(
            map_position_to_world_translation(
                &TILE_CORE_SPRITE_SIZE,
                MapPosition::new(0, 0),
                &game_map,
            )
            .truncate()
            .abs_diff_eq(Vec2::new(-16.0, 0.0), 0.001)
        );
        assert!(
            map_position_to_world_translation(
                &TILE_CORE_SPRITE_SIZE,
                MapPosition::new(2, 1),
                &game_map,
            )
            .truncate()
            .abs_diff_eq(Vec2::new(16.0, -16.0), 0.001)
        );
    }
}
