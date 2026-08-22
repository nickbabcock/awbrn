use crate::core::coords::{TILE_SIZE, map_visual_world_size};
use crate::features::event_bus::{EventSink, MapDimensions};
use crate::features::input::{
    DragOwner, PointerGesture, PointerGestureKind, PointerIsCoarse, PointerSet, ReturnToTouchFloor,
};
use crate::loading::ClientAssetLoader;
use crate::render::UnitAtlasResource;
use awbrn_game::world::GameMap;
use bevy::input::{
    mouse::{MouseScrollUnit, MouseWheel},
    touch::{TouchInput, TouchPhase},
};
use bevy::prelude::*;
use std::collections::BTreeMap;

#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct CameraScale(f32);

impl CameraScale {
    pub fn scale(&self) -> f32 {
        self.0
    }

    pub fn set_clamped(&mut self, scale: f32, min_scale: f32) {
        self.0 = scale.clamp(min_scale, MAX_CAMERA_SCALE);
    }

    pub fn zoom_in(&self) -> Self {
        CameraScale((self.0 * KEYBOARD_ZOOM_FACTOR).min(MAX_CAMERA_SCALE))
    }

    pub fn zoom_out(&self) -> Self {
        CameraScale(self.0 / KEYBOARD_ZOOM_FACTOR)
    }

    /// Whether a tile is currently drawn too small to be committed to by touch.
    ///
    /// Zooming out past this stays allowed, because orienting on a large map
    /// needs it. What changes below the floor is what a tap may do, not whether
    /// the player may look.
    pub fn is_below_touch_floor(&self) -> bool {
        self.0 < touch_floor_scale()
    }
}

/// The scale at which a tile reaches [`TOUCH_FLOOR_TILE_PX`].
pub fn touch_floor_scale() -> f32 {
    TOUCH_FLOOR_TILE_PX / TILE_SIZE
}

impl Default for CameraScale {
    fn default() -> Self {
        CameraScale(DEFAULT_CAMERA_SCALE)
    }
}

/// The smallest a tile may be drawn and still be reliably tapped, in logical
/// pixels.
///
/// Below Apple's 44pt and Material's 48dp, and deliberately so. Those minimums
/// describe a blind tap on a control the finger cannot adjust. Neither gesture
/// here is that: a drag lands anywhere and is corrected before release while
/// the route redraws, and a tap is pulled to the nearest reachable tile. The
/// floor therefore only has to protect the bare tap that selects a unit, which
/// is the cheapest and most recoverable thing a player can do. Holding a strict
/// 44 would cost about three tiles of view on a 390px phone and buy nothing the
/// interaction model does not already provide.
pub const TOUCH_FLOOR_TILE_PX: f32 = 40.0;

const DEFAULT_CAMERA_SCALE: f32 = 2.0;
const KEYBOARD_ZOOM_FACTOR: f32 = 1.25;
const MAX_CAMERA_SCALE: f32 = 4.0;
const MIN_CAMERA_SCALE: f32 = 0.2;
const TOUCH_WHEEL_PIXEL_ZOOM_RATE: f32 = 0.0015;
const TOUCH_WHEEL_LINE_ZOOM_RATE: f32 = 0.12;

#[derive(Debug, Clone, Copy)]
struct TouchCameraContact {
    position: Vec2,
    previous_position: Vec2,
}

#[derive(Resource, Debug, Default)]
struct TouchCameraState {
    contacts: BTreeMap<u64, TouchCameraContact>,
}

/// Bring this part of the board into view.
///
/// Selecting a unit on a phone is worth nothing if the tiles it can reach are
/// off screen, and a player should not have to pan to see the consequence of
/// the tap they just made.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct FocusBoardOn {
    /// The middle of what must be visible, in world units.
    pub world: Vec2,
}

/// Where the camera is travelling to, if anywhere.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
struct CameraGoal(Option<Vec2>);

/// How much of the remaining distance the camera closes each second.
///
/// The board is a place, not a document: it slides to what the player asked for
/// rather than cutting, which is what keeps a phone player oriented when the
/// view moves without them having moved it.
const CAMERA_EASE_PER_SECOND: f32 = 12.0;

/// Close enough to stop; below this the movement is invisible anyway.
const CAMERA_EASE_EPSILON: f32 = 0.25;

fn setup_camera(mut commands: Commands, camera_scale: Res<CameraScale>) {
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: bevy::camera::ScalingMode::WindowSize,
            scale: 1.0 / camera_scale.scale(),
            ..OrthographicProjection::default_2d()
        }),
        Msaa::Off,
    ));
}

fn setup_unit_atlas(
    mut commands: Commands,
    asset_loader: ClientAssetLoader,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let texture = asset_loader.load_unit_texture();
    let layout = TextureAtlasLayout::from_grid(
        UVec2::new(
            awbrn_content::UNIT_SPRITE_WIDTH,
            awbrn_content::UNIT_SPRITE_HEIGHT,
        ),
        awbrn_content::UNIT_SPRITESHEET_COLUMNS,
        awbrn_content::UNIT_SPRITESHEET_ROWS,
        Some(UVec2::new(
            awbrn_content::UNIT_SPRITESHEET_PADDING_X,
            awbrn_content::UNIT_SPRITESHEET_PADDING_Y,
        )),
        Some(UVec2::new(
            awbrn_content::UNIT_SPRITESHEET_OFFSET_X,
            awbrn_content::UNIT_SPRITESHEET_OFFSET_Y,
        )),
    );
    let layout = texture_atlas_layouts.add(layout);

    commands.insert_resource(UnitAtlasResource { texture, layout });
}

pub(crate) fn compute_map_dimensions(
    game_map: &GameMap,
    camera_scale: &CameraScale,
) -> MapDimensions {
    let map_size = map_visual_world_size(game_map);
    MapDimensions {
        width: map_size.x * camera_scale.scale(),
        height: map_size.y * camera_scale.scale(),
    }
}

fn map_world_size(game_map: &GameMap) -> Vec2 {
    map_visual_world_size(game_map)
}

fn minimum_camera_scale(game_map: &GameMap, window: &Window) -> f32 {
    let map_size = map_world_size(game_map);
    if map_size.x <= 0.0 || map_size.y <= 0.0 {
        return DEFAULT_CAMERA_SCALE;
    }

    let fit_scale = (window.width() / map_size.x).min(window.height() / map_size.y);
    fit_scale.clamp(MIN_CAMERA_SCALE, DEFAULT_CAMERA_SCALE)
}

fn viewport_to_world(
    camera_translation: Vec2,
    world_units_per_viewport_pixel: f32,
    window: &Window,
    viewport_position: Vec2,
) -> Vec2 {
    camera_translation
        + Vec2::new(
            viewport_position.x - window.width() * 0.5,
            window.height() * 0.5 - viewport_position.y,
        ) * world_units_per_viewport_pixel
}

fn viewport_delta_to_world_delta(
    viewport_delta: Vec2,
    world_units_per_viewport_pixel: f32,
) -> Vec2 {
    Vec2::new(viewport_delta.x, -viewport_delta.y) * world_units_per_viewport_pixel
}

fn device_pixel_snapped_camera_translation(
    camera_translation: Vec2,
    world_units_per_viewport_pixel: f32,
    window: &Window,
) -> Option<Vec2> {
    let scale_factor = window.resolution.scale_factor();
    if !scale_factor.is_finite()
        || scale_factor <= 0.0
        || !world_units_per_viewport_pixel.is_finite()
        || world_units_per_viewport_pixel <= 0.0
    {
        return None;
    }

    let physical_origin = Vec2::new(
        window.physical_width() as f32 * 0.5,
        window.physical_height() as f32 * 0.5,
    );
    let physical_pixels_per_world_unit = scale_factor / world_units_per_viewport_pixel;
    let world_units_per_physical_pixel = world_units_per_viewport_pixel / scale_factor;
    let world_origin_physical = physical_origin
        + Vec2::new(
            -camera_translation.x * physical_pixels_per_world_unit,
            camera_translation.y * physical_pixels_per_world_unit,
        );
    let physical_delta = world_origin_physical.round() - world_origin_physical;

    Some(Vec2::new(
        camera_translation.x - physical_delta.x * world_units_per_physical_pixel,
        camera_translation.y + physical_delta.y * world_units_per_physical_pixel,
    ))
}

fn snap_camera_translation_to_device_pixels(
    transform: &mut Transform,
    window: &Window,
    world_units_per_viewport_pixel: f32,
) {
    let Some(snapped) = device_pixel_snapped_camera_translation(
        transform.translation.truncate(),
        world_units_per_viewport_pixel,
        window,
    ) else {
        return;
    };

    if !transform
        .translation
        .truncate()
        .abs_diff_eq(snapped, 0.000_001)
    {
        transform.translation.x = snapped.x;
        transform.translation.y = snapped.y;
    }
}

fn projection_world_units_per_viewport_pixel(projection: &Projection) -> Option<f32> {
    match projection {
        Projection::Orthographic(orthographic) => Some(orthographic.scale),
        _ => None,
    }
}

fn apply_camera_scale_to_projection(camera_scale: CameraScale, projection: &mut Projection) {
    if let Projection::Orthographic(orthographic) = projection {
        orthographic.scale = 1.0 / camera_scale.scale();
    }
}

fn zoom_camera_at_viewport_position(
    transform: &mut Transform,
    projection: &mut Projection,
    camera_scale: &mut CameraScale,
    window: &Window,
    game_map: &GameMap,
    viewport_position: Vec2,
    target_scale: f32,
) {
    let min_scale = minimum_camera_scale(game_map, window);
    let before_projection_scale =
        projection_world_units_per_viewport_pixel(projection).unwrap_or(1.0 / camera_scale.scale());
    let before = viewport_to_world(
        transform.translation.truncate(),
        before_projection_scale,
        window,
        viewport_position,
    );

    camera_scale.set_clamped(target_scale, min_scale);
    apply_camera_scale_to_projection(*camera_scale, projection);

    let after_projection_scale =
        projection_world_units_per_viewport_pixel(projection).unwrap_or(1.0 / camera_scale.scale());
    let after = viewport_to_world(
        transform.translation.truncate(),
        after_projection_scale,
        window,
        viewport_position,
    );
    transform.translation += (before - after).extend(0.0);
}

fn handle_camera_scaling(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    game_map: Res<GameMap>,
    mut camera_scale: ResMut<CameraScale>,
    mut goal: ResMut<CameraGoal>,
    mut query: Query<(&mut Projection, &mut Transform), With<Camera>>,
    mut wheel_reader: MessageReader<MouseWheel>,
) {
    let Ok(window) = windows.single() else {
        return;
    };

    let Ok((mut projection, mut transform)) = query.single_mut() else {
        return;
    };

    if keyboard_input.just_pressed(KeyCode::Equal) {
        goal.0 = None;
        let target = camera_scale.zoom_in().scale();
        let viewport_center = Vec2::new(window.width() * 0.5, window.height() * 0.5);
        zoom_camera_at_viewport_position(
            &mut transform,
            &mut projection,
            &mut camera_scale,
            window,
            game_map.as_ref(),
            viewport_center,
            target,
        );
    } else if keyboard_input.just_pressed(KeyCode::Minus) {
        goal.0 = None;
        let target = camera_scale.zoom_out().scale();
        let viewport_center = Vec2::new(window.width() * 0.5, window.height() * 0.5);
        zoom_camera_at_viewport_position(
            &mut transform,
            &mut projection,
            &mut camera_scale,
            window,
            game_map.as_ref(),
            viewport_center,
            target,
        );
    }

    for wheel in wheel_reader.read() {
        goal.0 = None;
        let rate = match wheel.unit {
            MouseScrollUnit::Line => TOUCH_WHEEL_LINE_ZOOM_RATE,
            MouseScrollUnit::Pixel => TOUCH_WHEEL_PIXEL_ZOOM_RATE,
        };
        let target = camera_scale.scale() * (-wheel.y * rate).exp();
        let anchor = window
            .cursor_position()
            .unwrap_or_else(|| Vec2::new(window.width() * 0.5, window.height() * 0.5));

        zoom_camera_at_viewport_position(
            &mut transform,
            &mut projection,
            &mut camera_scale,
            window,
            game_map.as_ref(),
            anchor,
            target,
        );
    }
}

fn handle_touch_camera(
    windows: Query<&Window>,
    game_map: Res<GameMap>,
    mut camera_scale: ResMut<CameraScale>,
    mut goal: ResMut<CameraGoal>,
    mut touch_reader: MessageReader<TouchInput>,
    mut touch_state: ResMut<TouchCameraState>,
    mut query: Query<(&mut Projection, &mut Transform), With<Camera>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((mut projection, mut transform)) = query.single_mut() else {
        return;
    };

    let mut changed = false;
    let mut contact_set_changed = false;
    for touch in touch_reader.read() {
        changed = true;
        match touch.phase {
            TouchPhase::Started => {
                contact_set_changed = true;
                touch_state.contacts.insert(
                    touch.id,
                    TouchCameraContact {
                        position: touch.position,
                        previous_position: touch.position,
                    },
                );
            }
            TouchPhase::Moved => {
                if let Some(contact) = touch_state.contacts.get_mut(&touch.id) {
                    contact.position = touch.position;
                }
            }
            TouchPhase::Ended | TouchPhase::Canceled => {
                contact_set_changed = true;
                touch_state.contacts.remove(&touch.id);
            }
        }
    }

    if !changed {
        return;
    }

    if contact_set_changed {
        for contact in touch_state.contacts.values_mut() {
            contact.previous_position = contact.position;
        }
        return;
    }

    // A single contact is a gesture, not a camera movement. It may turn out to
    // be a unit being dragged, and only the play machine knows that, so
    // one-finger panning happens in `pan_camera_on_unclaimed_drag` after the
    // claim has been made. Two contacts are always a pinch.
    if touch_state.contacts.len() == 2 {
        let contacts = touch_state.contacts.values().copied().collect::<Vec<_>>();
        let previous_centroid =
            (contacts[0].previous_position + contacts[1].previous_position) * 0.5;
        let current_centroid = (contacts[0].position + contacts[1].position) * 0.5;
        let previous_distance = contacts[0]
            .previous_position
            .distance(contacts[1].previous_position);
        let current_distance = contacts[0].position.distance(contacts[1].position);

        if previous_distance > 0.0 && current_distance > 0.0 {
            goal.0 = None;
            let Some(before_projection_scale) =
                projection_world_units_per_viewport_pixel(&projection)
            else {
                return;
            };
            let target = camera_scale.scale() * current_distance / previous_distance;
            let before = viewport_to_world(
                transform.translation.truncate(),
                before_projection_scale,
                window,
                previous_centroid,
            );

            let min_scale = minimum_camera_scale(game_map.as_ref(), window);
            camera_scale.set_clamped(target, min_scale);
            apply_camera_scale_to_projection(*camera_scale, &mut projection);

            let Some(after_projection_scale) =
                projection_world_units_per_viewport_pixel(&projection)
            else {
                return;
            };
            let after = viewport_to_world(
                transform.translation.truncate(),
                after_projection_scale,
                window,
                current_centroid,
            );
            transform.translation += (before - after).extend(0.0);
        }
    }

    for contact in touch_state.contacts.values_mut() {
        contact.previous_position = contact.position;
    }
}

/// Pan on any drag the play machine did not claim for a unit.
///
/// This is the whole of pointer panning, for mouse and finger alike. The left
/// button used to pan and commit a tile at the same time, because the click
/// fired on press while the pan ran on the same button; a drag now commits
/// nothing, and a tap is the only thing that does.
fn pan_camera_on_unclaimed_drag(
    owner: Res<DragOwner>,
    mut goal: ResMut<CameraGoal>,
    mut gestures: MessageReader<PointerGesture>,
    mut query: Query<(&Projection, &mut Transform), With<Camera>>,
) {
    if *owner != DragOwner::Camera {
        gestures.clear();
        return;
    }

    let Ok((projection, mut transform)) = query.single_mut() else {
        return;
    };
    let Some(projection_scale) = projection_world_units_per_viewport_pixel(projection) else {
        return;
    };

    for gesture in gestures.read() {
        if gesture.kind != PointerGestureKind::DragMove {
            continue;
        }
        // A player who pans has taken the wheel. Continuing to steer toward an
        // earlier goal would drag the view back out from under them.
        goal.0 = None;
        let world_delta = viewport_delta_to_world_delta(gesture.delta, projection_scale);
        transform.translation -= world_delta.extend(0.0);
    }
}

/// Bring the board up to the touch floor the first time a finger arrives.
///
/// The default zoom is chosen for a mouse, and on a phone it is the trap this
/// whole pass exists to remove: the zoom that lets a player see the battlefield
/// is the zoom at which they cannot reliably touch it. Nothing knows the
/// pointer is coarse until one is used, so the correction happens on the first
/// touch rather than at load.
fn apply_touch_floor_on_first_board(
    coarse: Res<PointerIsCoarse>,
    mut requests: MessageReader<ReturnToTouchFloor>,
    windows: Query<&Window>,
    game_map: Res<GameMap>,
    mut camera_scale: ResMut<CameraScale>,
    mut query: Query<(&mut Projection, &mut Transform), With<Camera>>,
) {
    let asked = requests.read().last().is_some();
    if (!asked && !(coarse.is_changed() && coarse.0)) || !camera_scale.is_below_touch_floor() {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((mut projection, mut transform)) = query.single_mut() else {
        return;
    };

    let viewport_center = Vec2::new(window.width() * 0.5, window.height() * 0.5);
    zoom_camera_at_viewport_position(
        &mut transform,
        &mut projection,
        &mut camera_scale,
        window,
        game_map.as_ref(),
        viewport_center,
        touch_floor_scale(),
    );
}

fn accept_focus_requests(mut requests: MessageReader<FocusBoardOn>, mut goal: ResMut<CameraGoal>) {
    if let Some(request) = requests.read().last() {
        goal.0 = Some(request.world);
    }
}

/// Slide toward whatever was last asked for, frame by frame.
fn ease_camera_toward_goal(
    time: Res<Time>,
    mut goal: ResMut<CameraGoal>,
    mut query: Query<&mut Transform, With<Camera>>,
) {
    let Some(target) = goal.0 else {
        return;
    };
    let Ok(mut transform) = query.single_mut() else {
        return;
    };

    let current = transform.translation.truncate();
    if current.distance(target) <= CAMERA_EASE_EPSILON {
        transform.translation.x = target.x;
        transform.translation.y = target.y;
        goal.0 = None;
        return;
    }

    let step = 1.0 - (-CAMERA_EASE_PER_SECOND * time.delta_secs()).exp();
    let next = current.lerp(target, step.clamp(0.0, 1.0));
    transform.translation.x = next.x;
    transform.translation.y = next.y;
}

fn snap_camera_to_device_pixels(
    windows: Query<&Window>,
    mut query: Query<(&Projection, &mut Transform), With<Camera>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((projection, mut transform)) = query.single_mut() else {
        return;
    };
    let Some(projection_scale) = projection_world_units_per_viewport_pixel(projection) else {
        return;
    };

    snap_camera_translation_to_device_pixels(&mut transform, window, projection_scale);
}

fn emit_map_dimensions_on_scale_change(
    game_map: Res<GameMap>,
    camera_scale: Res<CameraScale>,
    sink: Res<EventSink<MapDimensions>>,
) {
    sink.emit(compute_map_dimensions(&game_map, &camera_scale));
}

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraScale>()
            .init_resource::<TouchCameraState>()
            .init_resource::<CameraGoal>()
            .add_message::<FocusBoardOn>()
            .add_systems(Startup, (setup_camera, setup_unit_atlas))
            .add_systems(
                Update,
                (
                    handle_touch_camera.before(accept_focus_requests),
                    handle_camera_scaling.before(accept_focus_requests),
                    pan_camera_on_unclaimed_drag
                        .before(crate::modes::play::handle_play_pointer_gestures),
                    accept_focus_requests.after(pan_camera_on_unclaimed_drag),
                    ease_camera_toward_goal.after(accept_focus_requests),
                    snap_camera_to_device_pixels
                        .after(handle_touch_camera)
                        .after(handle_camera_scaling)
                        .after(ease_camera_toward_goal),
                )
                    .in_set(PointerSet::Consume),
            )
            .add_systems(
                Update,
                apply_touch_floor_on_first_board
                    .run_if(in_state(crate::core::AppState::InGame))
                    .after(PointerSet::Recognize)
                    .before(PointerSet::Consume),
            )
            .add_systems(
                Update,
                emit_map_dimensions_on_scale_change
                    .run_if(
                        in_state(crate::core::AppState::InGame)
                            .and_then(resource_changed::<CameraScale>)
                            .and_then(resource_exists::<EventSink<MapDimensions>>),
                    )
                    .after(PointerSet::Consume),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use awbrn_map::AwbrnMap;
    use awbrn_types::GraphicalTerrain;
    use bevy::window::WindowResolution;

    fn test_map(width: u8, height: u8) -> GameMap {
        let mut game_map = GameMap::default();
        game_map.set(AwbrnMap::new(
            awbrn_map::Dimensions::new(width, height),
            GraphicalTerrain::Plain,
        ));
        game_map
    }

    fn test_window(width: u32, height: u32) -> Window {
        Window {
            resolution: WindowResolution::new(width, height),
            ..default()
        }
    }

    fn test_window_with_scale_factor(width: u32, height: u32, scale_factor: f32) -> Window {
        let mut resolution = WindowResolution::new(width, height);
        resolution.set_scale_factor_override(Some(scale_factor));
        Window {
            resolution,
            ..default()
        }
    }

    fn world_origin_physical_position(
        camera_translation: Vec2,
        world_units_per_viewport_pixel: f32,
        window: &Window,
    ) -> Vec2 {
        let scale_factor = window.resolution.scale_factor();
        Vec2::new(
            window.physical_width() as f32 * 0.5
                - camera_translation.x * scale_factor / world_units_per_viewport_pixel,
            window.physical_height() as f32 * 0.5
                + camera_translation.y * scale_factor / world_units_per_viewport_pixel,
        )
    }

    #[test]
    fn zoom_keeps_anchor_world_position_stable() {
        let game_map = test_map(40, 40);
        let window = test_window(400, 300);
        let mut transform = Transform::from_xyz(10.0, 20.0, 999.0);
        let mut projection = Projection::Orthographic(OrthographicProjection {
            scaling_mode: bevy::camera::ScalingMode::WindowSize,
            scale: 0.5,
            ..OrthographicProjection::default_2d()
        });
        let mut camera_scale = CameraScale(2.0);
        let anchor = Vec2::new(120.0, 80.0);

        let before = viewport_to_world(
            transform.translation.truncate(),
            projection_world_units_per_viewport_pixel(&projection).unwrap(),
            &window,
            anchor,
        );

        zoom_camera_at_viewport_position(
            &mut transform,
            &mut projection,
            &mut camera_scale,
            &window,
            &game_map,
            anchor,
            3.0,
        );

        let after = viewport_to_world(
            transform.translation.truncate(),
            projection_world_units_per_viewport_pixel(&projection).unwrap(),
            &window,
            anchor,
        );

        assert!(before.abs_diff_eq(after, 0.001));
        assert!((camera_scale.scale() - 3.0).abs() < 0.001);
    }

    #[test]
    fn zoom_does_not_restrict_an_offscreen_camera_position() {
        let game_map = test_map(10, 10);
        let window = test_window(400, 300);
        let mut transform = Transform::from_xyz(10_000.0, -10_000.0, 999.0);
        let mut projection = Projection::Orthographic(OrthographicProjection {
            scaling_mode: bevy::camera::ScalingMode::WindowSize,
            scale: 0.5,
            ..OrthographicProjection::default_2d()
        });
        let mut camera_scale = CameraScale(2.0);
        let viewport_center = Vec2::new(window.width() * 0.5, window.height() * 0.5);

        zoom_camera_at_viewport_position(
            &mut transform,
            &mut projection,
            &mut camera_scale,
            &window,
            &game_map,
            viewport_center,
            3.0,
        );

        assert!(
            transform
                .translation
                .truncate()
                .abs_diff_eq(Vec2::new(10_000.0, -10_000.0), 0.001)
        );
    }

    #[test]
    fn user_zoom_cancels_an_in_flight_camera_goal() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<GameMap>()
            .init_resource::<CameraScale>()
            .init_resource::<CameraGoal>()
            .add_message::<MouseWheel>()
            .add_systems(Update, handle_camera_scaling);
        app.world_mut().spawn(test_window(400, 300));
        app.world_mut().spawn((
            Camera::default(),
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: bevy::camera::ScalingMode::WindowSize,
                scale: 0.5,
                ..OrthographicProjection::default_2d()
            }),
            Transform::default(),
        ));
        app.world_mut().resource_mut::<CameraGoal>().0 = Some(Vec2::new(100.0, 100.0));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Equal);

        app.update();

        assert_eq!(app.world().resource::<CameraGoal>().0, None);
    }

    #[test]
    fn minimum_camera_scale_can_zoom_out_to_fit_large_maps() {
        let game_map = test_map(40, 40);
        let window = test_window(400, 300);

        let min_scale = minimum_camera_scale(&game_map, &window);

        assert!(min_scale < DEFAULT_CAMERA_SCALE);
        assert!(min_scale >= MIN_CAMERA_SCALE);
    }

    #[test]
    fn viewport_drag_delta_converts_to_inverse_camera_motion() {
        let delta = viewport_delta_to_world_delta(Vec2::new(10.0, 12.0), 0.5);

        assert!(delta.abs_diff_eq(Vec2::new(5.0, -6.0), 0.001));
    }

    #[test]
    fn odd_physical_width_snaps_camera_to_device_pixels() {
        let window = test_window_with_scale_factor(2013, 1190, 1.25);
        let snapped = device_pixel_snapped_camera_translation(Vec2::ZERO, 0.5, &window).unwrap();

        assert!(snapped.abs_diff_eq(Vec2::new(-0.2, 0.0), 0.000_001));
        assert!(
            world_origin_physical_position(snapped, 0.5, &window)
                .abs_diff_eq(Vec2::new(1007.0, 595.0), 0.000_001)
        );
    }

    #[test]
    fn even_physical_width_keeps_aligned_camera_unchanged() {
        let window = test_window_with_scale_factor(2014, 1190, 1.25);
        let snapped = device_pixel_snapped_camera_translation(Vec2::ZERO, 0.5, &window).unwrap();

        assert!(snapped.abs_diff_eq(Vec2::ZERO, 0.000_001));
        assert!(
            world_origin_physical_position(snapped, 0.5, &window)
                .abs_diff_eq(Vec2::new(1007.0, 595.0), 0.000_001)
        );
    }

    #[test]
    fn nonzero_camera_translation_snaps_world_origin_to_integer_physical_pixels() {
        let window = test_window_with_scale_factor(2013, 1191, 1.25);
        let snapped =
            device_pixel_snapped_camera_translation(Vec2::new(3.7, -2.9), 0.5, &window).unwrap();
        let origin_physical = world_origin_physical_position(snapped, 0.5, &window);

        assert!(origin_physical.abs_diff_eq(origin_physical.round(), 0.000_001));
    }
}
