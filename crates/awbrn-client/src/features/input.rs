use crate::core::coords::{LogicalPx, TILE_SIZE, map_position_to_world_translation};
use crate::core::{RenderLayer, SpriteSize};
use crate::features::event_bus::{EventSink, TileSelected};
use crate::render::UiAtlas;
use awbrn_game::MapPosition;
use awbrn_game::world::{BoardIndex, GameMap, TerrainTile};
use awbrn_map::Position;
use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;
use std::collections::BTreeMap;

/// Component to mark the currently selected tile
#[derive(Component)]
pub struct SelectedTile;

/// Marker component for the tile hover cursor sprite entity.
#[derive(Component)]
pub struct TileCursor;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileClicked {
    pub position: Position,
}

pub(crate) const TILE_CORE_SPRITE_SIZE: SpriteSize = SpriteSize {
    width: TILE_SIZE,
    height: TILE_SIZE,
    z_index: RenderLayer::CURSOR,
};

const TOUCH_TAP_MOVE_THRESHOLD: f32 = 8.0;

#[derive(Debug, Clone, Copy)]
struct TouchTapContact {
    start_position: Vec2,
    position: Vec2,
    moved: bool,
    multi_touch: bool,
}

#[derive(Resource, Debug, Default)]
pub(crate) struct TouchTapState {
    active: BTreeMap<u64, TouchTapContact>,
}

fn tile_cursor_bundle(ui_atlas: UiAtlas) -> impl Bundle {
    (
        ui_atlas.sprite_for("Effects/TileCursor.png"),
        Transform::from_translation(Vec3::new(0.0, 0.0, TILE_CORE_SPRITE_SIZE.z_index as f32)),
        Visibility::Hidden,
        TileCursor,
    )
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
    let Some(world_pos) =
        LogicalPx::from_window_position(cursor_pos).to_world(camera, camera_transform)
    else {
        *visibility = Visibility::Hidden;
        return;
    };
    let Some(map_position) = world_pos.to_map_position(game_map.as_ref()) else {
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
    mut click_writer: MessageWriter<TileClicked>,
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

    let Some(world_position) =
        LogicalPx::from_window_position(cursor_position).to_world(camera, camera_transform)
    else {
        return;
    };
    let Some(map_position) = world_position.to_map_position(game_map.as_ref()) else {
        return;
    };

    click_writer.write(TileClicked {
        position: map_position.position(),
    });
}

pub(crate) fn detect_touch_taps(
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    game_map: Res<GameMap>,
    mut touch_reader: MessageReader<TouchInput>,
    mut tap_state: ResMut<TouchTapState>,
    mut click_writer: MessageWriter<TileClicked>,
) {
    for touch in touch_reader.read() {
        match touch.phase {
            TouchPhase::Started => {
                let already_touching = !tap_state.active.is_empty();
                for contact in tap_state.active.values_mut() {
                    contact.multi_touch = true;
                }

                tap_state.active.insert(
                    touch.id,
                    TouchTapContact {
                        start_position: touch.position,
                        position: touch.position,
                        moved: false,
                        multi_touch: already_touching,
                    },
                );
            }
            TouchPhase::Moved => {
                if let Some(contact) = tap_state.active.get_mut(&touch.id) {
                    contact.position = touch.position;
                    contact.moved |=
                        contact.start_position.distance(touch.position) > TOUCH_TAP_MOVE_THRESHOLD;
                }
            }
            TouchPhase::Ended => {
                let active_count = tap_state.active.len();
                let Some(mut contact) = tap_state.active.remove(&touch.id) else {
                    continue;
                };

                contact.position = touch.position;
                contact.moved |=
                    contact.start_position.distance(touch.position) > TOUCH_TAP_MOVE_THRESHOLD;

                if contact.moved || contact.multi_touch || active_count > 1 {
                    continue;
                }

                let Ok(window) = windows.single() else {
                    continue;
                };
                let Ok((camera, camera_transform)) = camera_q.single() else {
                    continue;
                };
                if touch.position.x < 0.0
                    || touch.position.y < 0.0
                    || touch.position.x > window.width()
                    || touch.position.y > window.height()
                {
                    continue;
                }

                let Some(world_pos) = LogicalPx::from_window_position(touch.position)
                    .to_world(camera, camera_transform)
                else {
                    continue;
                };
                let Some(map_position) = world_pos.to_map_position(game_map.as_ref()) else {
                    continue;
                };

                click_writer.write(TileClicked {
                    position: map_position.position(),
                });
            }
            TouchPhase::Canceled => {
                tap_state.active.remove(&touch.id);
            }
        }
    }
}

pub(crate) fn handle_tile_clicks(
    board_index: Res<BoardIndex>,
    tiles: Query<&TerrainTile>,
    mut commands: Commands,
    selected: Query<Entity, With<SelectedTile>>,
    mut click_reader: MessageReader<TileClicked>,
) {
    let Some(TileClicked { position }) = click_reader.read().last().copied() else {
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
}

pub(crate) fn on_tile_selected(
    trigger: On<Insert, SelectedTile>,
    tiles: Query<(&MapPosition, &TerrainTile)>,
    sink: If<Res<EventSink<TileSelected>>>,
) {
    let Ok((map_pos, tile)) = tiles.get(trigger.event_target()) else {
        return;
    };
    let pos = map_pos.position();
    sink.emit(TileSelected {
        x: pos.x,
        y: pos.y,
        terrain_type: format!("{:?}", tile.terrain),
    });
}

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TouchTapState>();
        app.add_message::<TileClicked>();
        app.add_observer(on_tile_selected);
        app.add_systems(
            Update,
            (
                (detect_map_clicks, detect_touch_taps, handle_tile_clicks).chain(),
                update_tile_cursor,
            )
                .run_if(in_state(crate::core::AppState::InGame)),
        );
    }
}
