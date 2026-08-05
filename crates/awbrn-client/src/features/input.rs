use crate::core::coords::{LogicalPx, TILE_SIZE, map_position_to_world_translation};
use crate::core::{RenderLayer, SpriteSize};
use crate::features::event_bus::{EventSink, TileSelected};
use crate::render::UiAtlas;
use awbrn_game::MapPosition;
use awbrn_game::world::{BoardIndex, GameMap, TerrainTile};
use awbrn_map::Position;
use bevy::ecs::system::SystemParam;
use bevy::input::{
    ButtonState,
    mouse::MouseButtonInput,
    touch::{TouchInput, TouchPhase},
};
use bevy::prelude::*;
use bevy::window::CursorMoved;

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

/// How far a pointer may travel and still be a tap.
///
/// One threshold for both pointers. A mouse used to commit on press, which made
/// every pan drag also a click at the tile the drag began on; holding it to the
/// discipline touch already had is what removes that.
const GESTURE_MOVE_THRESHOLD: f32 = 8.0;

/// Which pointer a gesture belongs to. Only one is tracked at a time: a second
/// contact means the player is pinching, and no gesture survives that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PointerId {
    Mouse,
    Touch(u64),
}

/// What the pointer did, once it is known which it was.
///
/// The recognizer reports; it does not decide what a drag means. Whether a drag
/// moves a unit or the camera depends on what the press landed on, and only the
/// play state machine knows that. It claims the drag in [`PointerSet::Claim`]
/// and the camera takes what is left in [`PointerSet::Consume`].
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct PointerGesture {
    pub kind: PointerGestureKind,
    /// Logical viewport position, for the camera and for anchoring a menu.
    pub viewport: Vec2,
    /// Viewport travel: cumulative from `start_viewport` for [`PointerGestureKind::DragStart`],
    /// and since the previous report for [`PointerGestureKind::DragMove`].
    pub delta: Vec2,
    /// The tile under the pointer, when it is over the board.
    pub tile: Option<Position>,
    /// Whether the pointer is a finger rather than a mouse.
    pub coarse: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerGestureKind {
    /// A press that ended where it began.
    Tap,
    /// A press that has begun to travel. `tile` is the tile it began on and
    /// `delta` is cumulative from its starting viewport position.
    DragStart,
    /// A drag report after it starts. `delta` is travel since the prior report.
    DragMove,
    /// A press released after crossing the drag threshold. This may be emitted
    /// without `DragStart`, including Started/Ended input with no Moved events.
    DragEnd,
    /// The gesture was abandoned: a second contact arrived, or the pointer left.
    DragCancel,
}

/// The order the three halves of pointer handling must run in.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointerSet {
    /// Turn raw button, cursor, and touch events into gestures.
    Recognize,
    /// Decide what a drag is for, before anything acts on it.
    Claim,
    /// Act on whatever was not claimed.
    Consume,
}

#[derive(Debug, Clone, Copy)]
struct ActivePointer {
    id: PointerId,
    start_viewport: Vec2,
    start_tile: Option<Position>,
    viewport: Vec2,
    dragging: bool,
    /// A gesture that a second contact interrupted. It is tracked to its
    /// release so the release cannot be read as a tap.
    abandoned: bool,
    coarse: bool,
}

#[derive(Resource, Debug, Default)]
pub(crate) struct PointerState {
    primary: Option<ActivePointer>,
}

fn reset_pointer_state(mut state: ResMut<PointerState>, mut owner: ResMut<DragOwner>) {
    state.primary = None;
    *owner = DragOwner::Camera;
}

/// Who owns the drag currently in flight.
///
/// The camera pans on any drag it is not told to keep off, so the default is
/// [`DragOwner::Camera`]: a claim is something the play machine asserts, never
/// something the camera has to ask permission for.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DragOwner {
    #[default]
    Camera,
    Unit,
}

/// Whether the player is using a finger rather than a mouse.
///
/// Nothing can know this before a pointer is used, so it is observed rather
/// than configured, and it latches: a session that has seen a finger is a touch
/// session even while a trackpad is also attached.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PointerIsCoarse(pub bool);

/// A tap landed on a board drawn too small to commit to.
///
/// The tap selects and the camera comes back up to a size the finger can work
/// at, rather than the tap being silently dropped or, worse, honoured.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReturnToTouchFloor;

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

/// Turning a point on the screen into a point on the board.
///
/// Three queries travel together everywhere this is needed, so they travel as
/// one parameter.
#[derive(SystemParam)]
pub(crate) struct BoardProjection<'w, 's> {
    windows: Query<'w, 's, &'static Window>,
    cameras: Query<'w, 's, (&'static Camera, &'static GlobalTransform)>,
    game_map: Res<'w, GameMap>,
}

impl BoardProjection<'_, '_> {
    /// Where a viewport point lands in the world, when it is over the surface.
    pub(crate) fn world_at(&self, viewport: Vec2) -> Option<Vec2> {
        let window = self.windows.single().ok()?;
        if viewport.x < 0.0
            || viewport.y < 0.0
            || viewport.x > window.width()
            || viewport.y > window.height()
        {
            return None;
        }

        let (camera, transform) = self.cameras.single().ok()?;
        LogicalPx::from_window_position(viewport)
            .to_world(camera, transform)
            .map(|world| world.to_vec2())
    }

    /// The tile under a viewport position, when that position is over the board.
    pub(crate) fn tile_at(&self, viewport: Vec2) -> Option<Position> {
        let world = self.world_at(viewport)?;
        crate::core::coords::WorldPos::from_bevy(world)
            .to_map_position(self.game_map.as_ref())
            .map(|map_position| map_position.position())
    }

    /// The tile under the mouse cursor, when there is one over the board.
    pub(crate) fn cursor_tile(&self) -> Option<Position> {
        let cursor = self.windows.single().ok()?.cursor_position()?;
        self.tile_at(cursor)
    }
}

/// Turn raw pointer events into gestures, for mouse and touch alike.
///
/// Both pointers commit on release and both must travel further than
/// [`GESTURE_MOVE_THRESHOLD`] before a press becomes a drag, so neither can
/// commit something the player was only reaching for.
pub(crate) fn recognize_pointer_gestures(
    projection: BoardProjection<'_, '_>,
    mut button_reader: MessageReader<MouseButtonInput>,
    mut cursor_reader: MessageReader<CursorMoved>,
    mut touch_reader: MessageReader<TouchInput>,
    mut state: ResMut<PointerState>,
    mut coarse: ResMut<PointerIsCoarse>,
    mut gestures: MessageWriter<PointerGesture>,
) {
    let Ok(window) = projection.windows.single() else {
        return;
    };
    let locate = |viewport: Vec2| projection.tile_at(viewport);

    let press = |state: &mut PointerState,
                 gestures: &mut MessageWriter<PointerGesture>,
                 id: PointerId,
                 viewport: Vec2,
                 coarse: bool| {
        // A second contact is a pinch beginning. Whatever the first was doing
        // is abandoned rather than completed, so a two-finger zoom can never
        // leave a move behind it.
        if let Some(active) = state.primary.as_mut() {
            if !active.abandoned {
                active.abandoned = true;
                if active.dragging {
                    gestures.write(PointerGesture {
                        kind: PointerGestureKind::DragCancel,
                        viewport: active.viewport,
                        delta: Vec2::ZERO,
                        tile: None,
                        coarse: active.coarse,
                    });
                }
            }
            return;
        }

        state.primary = Some(ActivePointer {
            id,
            start_viewport: viewport,
            start_tile: locate(viewport),
            viewport,
            dragging: false,
            abandoned: false,
            coarse,
        });
    };

    let travel = |state: &mut PointerState,
                  gestures: &mut MessageWriter<PointerGesture>,
                  id: PointerId,
                  viewport: Vec2| {
        let Some(active) = state.primary.as_mut() else {
            return;
        };
        if active.id != id || active.abandoned {
            return;
        }

        let delta = viewport - active.viewport;
        active.viewport = viewport;
        if !active.dragging {
            if active.start_viewport.distance(viewport) <= GESTURE_MOVE_THRESHOLD {
                return;
            }
            active.dragging = true;
            gestures.write(PointerGesture {
                kind: PointerGestureKind::DragStart,
                viewport,
                delta: viewport - active.start_viewport,
                tile: active.start_tile,
                coarse: active.coarse,
            });
        }

        gestures.write(PointerGesture {
            kind: PointerGestureKind::DragMove,
            viewport,
            delta,
            tile: locate(viewport),
            coarse: active.coarse,
        });
    };

    let release = |state: &mut PointerState,
                   gestures: &mut MessageWriter<PointerGesture>,
                   id: PointerId,
                   viewport: Option<Vec2>,
                   cancelled: bool| {
        let Some(active) = state.primary else {
            return;
        };
        if active.id != id {
            return;
        }
        state.primary = None;

        let viewport = viewport.unwrap_or(active.viewport);
        let travelled =
            active.dragging || active.start_viewport.distance(viewport) > GESTURE_MOVE_THRESHOLD;

        if active.abandoned || cancelled {
            if active.dragging {
                gestures.write(PointerGesture {
                    kind: PointerGestureKind::DragCancel,
                    viewport,
                    delta: Vec2::ZERO,
                    tile: None,
                    coarse: active.coarse,
                });
            }
            return;
        }

        gestures.write(PointerGesture {
            kind: if travelled {
                PointerGestureKind::DragEnd
            } else {
                PointerGestureKind::Tap
            },
            viewport,
            delta: Vec2::ZERO,
            tile: locate(viewport),
            coarse: active.coarse,
        });
    };

    for event in button_reader.read() {
        if event.button != MouseButton::Left {
            continue;
        }
        match event.state {
            ButtonState::Pressed => {
                let Some(viewport) = window.cursor_position() else {
                    continue;
                };
                press(&mut state, &mut gestures, PointerId::Mouse, viewport, false);
            }
            ButtonState::Released => release(
                &mut state,
                &mut gestures,
                PointerId::Mouse,
                window.cursor_position(),
                false,
            ),
        }
    }

    for cursor in cursor_reader.read() {
        travel(&mut state, &mut gestures, PointerId::Mouse, cursor.position);
    }

    for touch in touch_reader.read() {
        match touch.phase {
            TouchPhase::Started => {
                if !coarse.0 {
                    coarse.0 = true;
                }
                press(
                    &mut state,
                    &mut gestures,
                    PointerId::Touch(touch.id),
                    touch.position,
                    true,
                )
            }
            TouchPhase::Moved => travel(
                &mut state,
                &mut gestures,
                PointerId::Touch(touch.id),
                touch.position,
            ),
            TouchPhase::Ended => release(
                &mut state,
                &mut gestures,
                PointerId::Touch(touch.id),
                Some(touch.position),
                false,
            ),
            TouchPhase::Canceled => release(
                &mut state,
                &mut gestures,
                PointerId::Touch(touch.id),
                None,
                true,
            ),
        }
    }
}

/// Replay mode still speaks in clicks, and a tap is what a click was.
pub(crate) fn emit_tile_clicked_from_taps(
    mut gestures: MessageReader<PointerGesture>,
    mut click_writer: MessageWriter<TileClicked>,
) {
    for gesture in gestures.read() {
        if gesture.kind != PointerGestureKind::Tap {
            continue;
        }
        if let Some(position) = gesture.tile {
            click_writer.write(TileClicked { position });
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
        app.init_resource::<PointerState>();
        app.init_resource::<PointerIsCoarse>();
        app.init_resource::<DragOwner>();
        app.add_message::<TileClicked>();
        app.add_message::<PointerGesture>();
        app.add_message::<ReturnToTouchFloor>();
        app.add_observer(on_tile_selected);
        app.configure_sets(
            Update,
            (
                PointerSet::Recognize,
                PointerSet::Claim,
                PointerSet::Consume,
            )
                .chain()
                .run_if(in_state(crate::core::AppState::InGame)),
        );
        app.add_systems(
            Update,
            recognize_pointer_gestures.in_set(PointerSet::Recognize),
        );
        app.add_systems(
            Update,
            update_tile_cursor.run_if(in_state(crate::core::AppState::InGame)),
        );
        app.add_systems(
            Update,
            (emit_tile_clicked_from_taps, handle_tile_clicks)
                .chain()
                .in_set(PointerSet::Consume)
                .run_if(in_state(crate::core::GameMode::Replay)),
        );
        app.add_systems(OnExit(crate::core::AppState::InGame), reset_pointer_state);
        app.add_systems(OnExit(crate::core::GameMode::Game), reset_pointer_state);
        app.add_systems(OnExit(crate::core::GameMode::Replay), reset_pointer_state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::window::WindowResolution;

    #[test]
    fn mouse_release_outside_the_window_clears_the_active_pointer() {
        let mut app = App::new();
        app.init_resource::<GameMap>()
            .init_resource::<PointerState>()
            .init_resource::<PointerIsCoarse>()
            .add_message::<MouseButtonInput>()
            .add_message::<CursorMoved>()
            .add_message::<TouchInput>()
            .add_message::<PointerGesture>()
            .add_systems(Update, recognize_pointer_gestures);

        let mut window = Window {
            resolution: WindowResolution::new(400, 300),
            ..default()
        };
        window.set_cursor_position(Some(Vec2::new(100.0, 100.0)));
        let window_entity = app.world_mut().spawn(window).id();
        app.world_mut()
            .spawn((Camera::default(), GlobalTransform::default()));

        app.world_mut()
            .resource_mut::<Messages<MouseButtonInput>>()
            .write(MouseButtonInput {
                button: MouseButton::Left,
                state: ButtonState::Pressed,
                window: window_entity,
            });
        app.update();
        assert!(app.world().resource::<PointerState>().primary.is_some());

        app.world_mut()
            .entity_mut(window_entity)
            .get_mut::<Window>()
            .unwrap()
            .set_cursor_position(None);
        app.world_mut()
            .resource_mut::<Messages<MouseButtonInput>>()
            .write(MouseButtonInput {
                button: MouseButton::Left,
                state: ButtonState::Released,
                window: window_entity,
            });
        app.update();

        assert!(app.world().resource::<PointerState>().primary.is_none());
    }
}
