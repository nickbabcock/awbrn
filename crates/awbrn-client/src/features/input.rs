use crate::core::coords::{LogicalPx, TILE_SIZE, map_position_to_world_translation};
use crate::core::{RenderLayer, SpriteSize};
use crate::features::event_bus::{
    AmmoDisplay, EventSink, HoveredCargoUnit, HoveredTile, HoveredUnit, TileHoverChanged,
    TileSelected,
};
use crate::features::weather::CurrentWeather;
use crate::projection::{
    ClientProjectionSet, ProjectedTerrainRenderState, ProjectedUnitRenderState,
};
use crate::render::UiAtlas;
use awbrn_bevy::MapPosition;
use awbrn_bevy::world::{
    Ammo, BoardIndex, CaptureProgress, Faction, FriendlyFactions, Fuel, GameMap, GraphicalHp,
    HasCargo, TerrainTile, Unit, ViewerVisibility,
};
use awbrn_map::Pos;
use awbrn_types::{GraphicalTerrain, UnitExt};
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
    pub position: Pos,
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
    pub tile: Option<Pos>,
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
    start_tile: Option<Pos>,
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

fn reset_pointer_state(
    mut state: ResMut<PointerState>,
    mut owner: ResMut<DragOwner>,
    mut inspected: ResMut<InspectedTile>,
    mut hovered: ResMut<HoveredTileState>,
) {
    state.primary = None;
    *owner = DragOwner::Camera;
    inspected.0 = None;
    *hovered = HoveredTileState::default();
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

type HoveredUnitQueryItem<'a> = (
    &'a ProjectedUnitRenderState,
    &'a Faction,
    Option<&'a GraphicalHp>,
    Option<&'a Ammo>,
    Option<&'a Fuel>,
    Option<&'a CaptureProgress>,
);

#[derive(SystemParam)]
struct HoverInfo<'w, 's> {
    board_index: Res<'w, BoardIndex>,
    weather: Res<'w, CurrentWeather>,
    visibility: Res<'w, ViewerVisibility>,
    friendly_factions: Res<'w, FriendlyFactions>,
    terrain: Query<'w, 's, &'static ProjectedTerrainRenderState>,
    units: Query<'w, 's, HoveredUnitQueryItem<'static>>,
    transports: Query<'w, 's, &'static HasCargo>,
}

/// One unit as the readout compares it.
///
/// Every field is `Copy`, so a frame that changes nothing costs a comparison
/// instead of the strings and vector the payload needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HoveredUnitKey {
    unit: Unit,
    faction: Faction,
    health: Option<u8>,
    /// `None` where the unit has no such component to read.
    ammo: Option<u32>,
    fuel: Option<u32>,
    /// Points already put into taking the property under it, when it is taking
    /// one.
    capture: Option<u8>,
}

/// The tile under the mouse, as the readout compares it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HoveredTileKey {
    position: Pos,
    terrain: GraphicalTerrain,
    terrain_sprite_index: u16,
    unit: Option<HoveredUnitKey>,
}

/// What the presentation was last told, in comparable form.
///
/// The cargo of the hovered unit lives beside the tile rather than inside it so
/// that its buffer survives from one frame to the next, which is what keeps the
/// per-frame read free of allocation.
#[derive(Debug, Default, PartialEq, Eq)]
struct HoverKey {
    /// `None` when the pointer is not over the board.
    tile: Option<HoveredTileKey>,
    cargo: Vec<HoveredUnitKey>,
}

/// The tile a finger is reading.
///
/// A finger cannot hover, so the readout follows what the player last pointed
/// at: a tap holds a tile until the next tap, and a drag reads the tile under
/// the finger the whole way, which is how the destination of a move can be read
/// before the finger lifts. A mouse needs none of this and keeps its cursor.
#[derive(Resource, Debug, Default)]
struct InspectedTile(Option<Pos>);

/// Follow the finger, for the readout only. This commits nothing.
fn track_inspected_tile(
    mut gestures: MessageReader<PointerGesture>,
    mut inspected: ResMut<InspectedTile>,
) {
    for gesture in gestures.read() {
        if !gesture.coarse {
            continue;
        }
        match gesture.kind {
            PointerGestureKind::Tap
            | PointerGestureKind::DragStart
            | PointerGestureKind::DragMove
            | PointerGestureKind::DragEnd => {
                // A pointer off the board reports no tile. The tile it last
                // stood on is still the last thing the player asked about, so
                // the readout holds there rather than emptying.
                if let Some(tile) = gesture.tile {
                    inspected.0 = Some(tile);
                }
            }
            PointerGestureKind::DragCancel => {}
        }
    }
}

#[derive(Resource, Debug, Default)]
struct HoveredTileState {
    /// What the presentation was last told.
    current: HoverKey,
    /// This frame's read, swapped in when it differs from `current`.
    scratch: HoverKey,
}

fn unit_key(
    state: &ProjectedUnitRenderState,
    health: Option<&GraphicalHp>,
    ammo: Option<&Ammo>,
    fuel: Option<&Fuel>,
    capture: Option<&CaptureProgress>,
) -> HoveredUnitKey {
    HoveredUnitKey {
        capture: capture.map(|progress| progress.value()),
        unit: state.unit,
        faction: state.faction,
        health: health
            .and_then(|hp| hp.visible())
            .map(awbrn_types::VisualHp::get),
        ammo: ammo.map(Ammo::value),
        fuel: fuel.map(Fuel::value),
    }
}

fn cargo_details(key: HoveredUnitKey) -> HoveredCargoUnit {
    HoveredCargoUnit {
        unit: key.unit.0,
        name: key.unit.0.name().to_string(),
        faction_code: key.faction.0.country_code().to_string(),
        health: key.health,
        ammo: key.ammo,
        max_ammo: key.unit.0.max_ammo(),
        fuel: key.fuel,
        max_fuel: key.unit.0.max_fuel(),
    }
}

/// Read one tile into `key`, reusing its cargo buffer.
fn read_hovered_tile(info: &HoverInfo<'_, '_>, position: Option<Pos>, key: &mut HoverKey) {
    key.tile = None;
    key.cargo.clear();

    let Some(position) = position else {
        return;
    };
    let Ok(terrain_entity) = info.board_index.terrain_entity(position) else {
        return;
    };
    let Ok(terrain) = info.terrain.get(terrain_entity) else {
        return;
    };
    let terrain = terrain.0;

    let unit = info
        .board_index
        .unit_entity(position)
        .ok()
        .flatten()
        .and_then(|unit_entity| {
            let (state, actual_faction, health, ammo, fuel, capture) =
                info.units.get(unit_entity).ok()?;
            if !state.visible {
                return None;
            }

            // Ammunition, fuel and health of a unit the viewer can see are all
            // part of their observation, whoever owns it. Cargo is the one part
            // fog withholds from an enemy transport, per `spec/semantics/fog.md`.
            let disclose_cargo = !info.visibility.fog_active()
                || info.friendly_factions.0.contains(&actual_faction.0);
            if disclose_cargo {
                let carried = info
                    .transports
                    .get(unit_entity)
                    .into_iter()
                    .flat_map(HasCargo::iter);
                for cargo_entity in carried {
                    // A carried unit captures nothing; it is not on the ground.
                    if let Ok((state, _, health, ammo, fuel, _)) = info.units.get(cargo_entity) {
                        key.cargo.push(unit_key(state, health, ammo, fuel, None));
                    }
                }
            }

            Some(unit_key(state, health, ammo, fuel, capture))
        });

    key.tile = Some(HoveredTileKey {
        position,
        terrain,
        terrain_sprite_index: awbrn_content::spritesheet_index(info.weather.weather(), terrain)
            .index(),
        unit,
    });
}

fn hover_payload(key: &HoverKey) -> Option<HoveredTile> {
    let tile = key.tile?;
    let terrain = tile.terrain.as_terrain();

    Some(HoveredTile {
        x: tile.position.x,
        y: tile.position.y,
        terrain_name: terrain.type_name().to_string(),
        terrain_owner: terrain.owner().map(|faction| faction.name().to_string()),
        terrain_sprite_index: tile.terrain_sprite_index,
        defense_stars: tile.terrain.defense_stars(),
        // What the property still owes, rather than what has been paid into it.
        // A capture is finished by the number that reaches zero.
        capture_remaining: tile
            .unit
            .and_then(|unit| unit.capture)
            .map(|progress| CaptureProgress::REQUIRED.saturating_sub(progress)),
        unit: tile.unit.map(|unit| HoveredUnit {
            unit: unit.unit.0,
            name: unit.unit.0.name().to_string(),
            faction_code: unit.faction.0.country_code().to_string(),
            health: unit.health,
            ammo: unit.ammo,
            max_ammo: unit.unit.0.max_ammo(),
            ammo_display: AmmoDisplay::for_unit(unit.unit.0),
            fuel: unit.fuel,
            max_fuel: unit.unit.0.max_fuel(),
            loaded_units: key.cargo.iter().copied().map(cargo_details).collect(),
        }),
    })
}

/// Report the tile under the mouse when its visible information changes.
fn emit_tile_hover_changed(
    projection: BoardProjection<'_, '_>,
    info: HoverInfo<'_, '_>,
    coarse: Res<PointerIsCoarse>,
    inspected: Res<InspectedTile>,
    mut hovered: ResMut<HoveredTileState>,
    sink: If<Res<EventSink<TileHoverChanged>>>,
) {
    // A touch session has no cursor to read, and the one it reports is wherever
    // the last finger happened to leave it.
    let position = if coarse.0 {
        inspected.0
    } else {
        projection.cursor_tile()
    };

    // The scratch buffer is written every frame and nothing observes this
    // resource, so its change flag would report a change that is not one.
    let hovered = hovered.bypass_change_detection();
    read_hovered_tile(&info, position, &mut hovered.scratch);

    if hovered.scratch == hovered.current {
        return;
    }

    std::mem::swap(&mut hovered.current, &mut hovered.scratch);
    sink.emit(TileHoverChanged {
        tile: hover_payload(&hovered.current),
    });
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
    pub(crate) fn tile_at(&self, viewport: Vec2) -> Option<Pos> {
        let world = self.world_at(viewport)?;
        crate::core::coords::WorldPos::from_bevy(world)
            .to_map_position(self.game_map.as_ref())
            .map(|map_position| map_position.position())
    }

    /// The tile under the mouse cursor, when there is one over the board.
    pub(crate) fn cursor_tile(&self) -> Option<Pos> {
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
        app.init_resource::<HoveredTileState>();
        app.init_resource::<InspectedTile>();
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
            (recognize_pointer_gestures, track_inspected_tile)
                .chain()
                .in_set(PointerSet::Recognize),
        );
        app.add_systems(
            Update,
            update_tile_cursor.run_if(in_state(crate::core::AppState::InGame)),
        );
        app.add_systems(
            Update,
            emit_tile_hover_changed
                .after(ClientProjectionSet::DerivePresentation)
                .after(PointerSet::Recognize)
                .run_if(in_state(crate::core::AppState::InGame)),
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
    use crate::projection::ProjectedUnitOverlayFlags;
    use awbrn_bevy::world::{CarriedBy, Hiding};
    use awbrn_map::Dimensions;
    use awbrn_types::{GraphicalTerrain, PlayerFaction};
    use bevy::ecs::system::RunSystemOnce;
    use bevy::window::WindowResolution;

    fn hover_app() -> App {
        let mut app = App::new();
        app.insert_resource(BoardIndex::new(Dimensions::new(2, 1)));
        app.init_resource::<GameMap>();
        app.init_resource::<CurrentWeather>();
        app.init_resource::<ViewerVisibility>();
        app.init_resource::<FriendlyFactions>();

        for x in 0..2 {
            let position = Pos::new(x, 0);
            let terrain = app
                .world_mut()
                .spawn(ProjectedTerrainRenderState(GraphicalTerrain::Plain))
                .id();
            app.world_mut()
                .resource_mut::<BoardIndex>()
                .set_terrain(position, terrain)
                .unwrap();
        }

        app
    }

    /// A unit the viewer can see, placed on the board at `position`.
    fn spawn_unit(
        app: &mut App,
        position: Pos,
        unit: awbrn_types::Unit,
        faction: PlayerFaction,
    ) -> Entity {
        let entity = spawn_carried_unit(app, unit, faction);
        app.world_mut()
            .resource_mut::<BoardIndex>()
            .set_unit(position, entity)
            .unwrap();
        entity
    }

    /// A unit off the board, which is what a carried unit is.
    fn spawn_carried_unit(
        app: &mut App,
        unit: awbrn_types::Unit,
        faction: PlayerFaction,
    ) -> Entity {
        app.world_mut()
            .spawn((
                ProjectedUnitRenderState {
                    unit: Unit(unit),
                    faction: Faction(faction),
                    visible: true,
                    active: true,
                    overlays: ProjectedUnitOverlayFlags::default(),
                },
                Faction(faction),
                GraphicalHp::from(awvm::semantic::ObservedUnitHp::Exact(70)),
                Ammo(4),
                Fuel(50),
            ))
            .id()
    }

    /// What the readout would report for `position`, without a camera.
    fn hover_at(app: &mut App, position: Pos) -> Option<HoveredTile> {
        app.world_mut()
            .run_system_once_with(
                |In(position): In<Pos>, info: HoverInfo| {
                    let mut key = HoverKey::default();
                    read_hovered_tile(&info, Some(position), &mut key);
                    hover_payload(&key)
                },
                position,
            )
            .unwrap()
    }

    #[test]
    fn unchanged_coarse_hover_emits_only_once() {
        let mut app = hover_app();
        app.init_resource::<PointerIsCoarse>()
            .init_resource::<InspectedTile>()
            .init_resource::<HoveredTileState>()
            .add_systems(Update, emit_tile_hover_changed);
        app.world_mut().resource_mut::<PointerIsCoarse>().0 = true;
        app.world_mut().resource_mut::<InspectedTile>().0 = Some(Pos::new(0, 0));

        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorded = events.clone();
        app.insert_resource(EventSink::<TileHoverChanged>::new(move |event| {
            recorded.lock().unwrap().push(event);
        }));

        app.update();
        let first = {
            let mut events = events.lock().unwrap();
            assert_eq!(events.len(), 1);
            events.pop().unwrap()
        };
        assert_eq!(first.tile.unwrap().x, 0);

        app.update();
        assert!(events.lock().unwrap().is_empty());
    }

    #[test]
    fn a_visible_enemy_discloses_its_ammunition_and_fuel_but_not_its_cargo() {
        let mut app = hover_app();
        app.world_mut()
            .resource_mut::<ViewerVisibility>()
            .reset(true, awbrn_map::Dimensions::new(2, 1));
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);

        let transport = spawn_unit(
            &mut app,
            Pos::new(0, 0),
            awbrn_types::Unit::Apc,
            PlayerFaction::BlueMoon,
        );
        let cargo = spawn_carried_unit(
            &mut app,
            awbrn_types::Unit::Infantry,
            PlayerFaction::BlueMoon,
        );
        app.world_mut()
            .entity_mut(cargo)
            .insert(CarriedBy(transport));

        let unit = hover_at(&mut app, Pos::new(0, 0)).unwrap().unit.unwrap();

        assert_eq!(unit.ammo, Some(4));
        assert_eq!(unit.fuel, Some(50));
        assert_eq!(unit.health, Some(7));
        assert!(unit.loaded_units.is_empty(), "fog withholds enemy cargo");
    }

    #[test]
    fn cargo_is_reported_in_the_order_it_was_loaded() {
        let mut app = hover_app();

        let transport = spawn_unit(
            &mut app,
            Pos::new(0, 0),
            awbrn_types::Unit::Lander,
            PlayerFaction::OrangeStar,
        );
        let mut loaded = Vec::new();
        for carried in [awbrn_types::Unit::Mech, awbrn_types::Unit::Infantry] {
            let entity = spawn_carried_unit(&mut app, carried, PlayerFaction::OrangeStar);
            app.world_mut()
                .entity_mut(entity)
                .insert(CarriedBy(transport));
            loaded.push(entity);
        }
        // Put the two carried units in separate archetypes, so a readout that
        // scans the world instead of asking the transport reports them in the
        // order the archetypes happen to sit in rather than the order they
        // were loaded in.
        app.world_mut().entity_mut(loaded[0]).insert(Hiding);

        let unit = hover_at(&mut app, Pos::new(0, 0)).unwrap().unit.unwrap();

        let names: Vec<_> = unit
            .loaded_units
            .iter()
            .map(|cargo| cargo.name.as_str())
            .collect();
        assert_eq!(names, ["Mech", "Infantry"]);
    }

    #[test]
    fn a_weapon_that_never_runs_out_is_not_reported_as_a_count() {
        let mut app = hover_app();
        spawn_unit(
            &mut app,
            Pos::new(0, 0),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
        );

        let unit = hover_at(&mut app, Pos::new(0, 0)).unwrap().unit.unwrap();

        assert_eq!(unit.ammo_display, AmmoDisplay::Unlimited);
        assert_eq!(
            AmmoDisplay::for_unit(awbrn_types::Unit::Apc),
            AmmoDisplay::None
        );
        assert_eq!(
            AmmoDisplay::for_unit(awbrn_types::Unit::Tank),
            AmmoDisplay::Counted
        );
    }

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
