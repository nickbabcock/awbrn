use crate::core::coords::{map_visual_top_world_y, map_visual_world_size};
use crate::features::event_bus::{EventSink, MapDimensions};
use crate::loading::ClientAssetLoader;
use crate::render::UnitAtlasResource;
use awbrn_game::world::GameMap;
use bevy::input::{
    ButtonState,
    mouse::{MouseButtonInput, MouseScrollUnit, MouseWheel},
    touch::{TouchInput, TouchPhase},
};
use bevy::prelude::*;
use bevy::window::CursorMoved;
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
}

impl Default for CameraScale {
    fn default() -> Self {
        CameraScale(DEFAULT_CAMERA_SCALE)
    }
}

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

#[derive(Resource, Debug, Default)]
struct MousePanState {
    dragging: bool,
}

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

fn clamp_camera_translation(
    transform: &mut Transform,
    window: &Window,
    game_map: &GameMap,
    world_units_per_viewport_pixel: f32,
) {
    let map_size = map_world_size(game_map);
    if map_size.x <= 0.0 || map_size.y <= 0.0 {
        return;
    }

    let visible_size = Vec2::new(window.width(), window.height()) * world_units_per_viewport_pixel;
    let half_visible = visible_size * 0.5;
    let left = -map_size.x * 0.5;
    let right = map_size.x * 0.5;
    let top = map_visual_top_world_y(game_map);
    let bottom = top - map_size.y;
    let center = Vec2::new((left + right) * 0.5, (top + bottom) * 0.5);

    transform.translation.x = if visible_size.x >= map_size.x {
        center.x
    } else {
        transform
            .translation
            .x
            .clamp(left + half_visible.x, right - half_visible.x)
    };

    transform.translation.y = if visible_size.y >= map_size.y {
        center.y
    } else {
        transform
            .translation
            .y
            .clamp(bottom + half_visible.y, top - half_visible.y)
    };
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
    clamp_camera_translation(transform, window, game_map, after_projection_scale);
}

fn handle_camera_scaling(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    game_map: Res<GameMap>,
    mut camera_scale: ResMut<CameraScale>,
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

    match touch_state.contacts.len() {
        1 => {
            let contact = touch_state.contacts.values().next().copied().unwrap();
            let viewport_delta = contact.position - contact.previous_position;
            let Some(projection_scale) = projection_world_units_per_viewport_pixel(&projection)
            else {
                return;
            };
            let world_delta = viewport_delta_to_world_delta(viewport_delta, projection_scale);
            transform.translation -= world_delta.extend(0.0);
            clamp_camera_translation(&mut transform, window, game_map.as_ref(), projection_scale);
        }
        2 => {
            let contacts = touch_state.contacts.values().copied().collect::<Vec<_>>();
            let previous_centroid =
                (contacts[0].previous_position + contacts[1].previous_position) * 0.5;
            let current_centroid = (contacts[0].position + contacts[1].position) * 0.5;
            let previous_distance = contacts[0]
                .previous_position
                .distance(contacts[1].previous_position);
            let current_distance = contacts[0].position.distance(contacts[1].position);

            if previous_distance > 0.0 && current_distance > 0.0 {
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
                clamp_camera_translation(
                    &mut transform,
                    window,
                    game_map.as_ref(),
                    after_projection_scale,
                );
            }
        }
        _ => {}
    }

    for contact in touch_state.contacts.values_mut() {
        contact.previous_position = contact.position;
    }
}

fn handle_mouse_pan(
    windows: Query<&Window>,
    game_map: Res<GameMap>,
    mut pan_state: ResMut<MousePanState>,
    mut button_reader: MessageReader<MouseButtonInput>,
    mut cursor_reader: MessageReader<CursorMoved>,
    mut query: Query<(&mut Projection, &mut Transform), With<Camera>>,
) {
    for event in button_reader.read() {
        if event.button == MouseButton::Left {
            pan_state.dragging = event.state == ButtonState::Pressed;
        }
    }

    if !pan_state.dragging {
        // Consume pending cursor events so they don't accumulate while not dragging
        for _ in cursor_reader.read() {}
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((projection, mut transform)) = query.single_mut() else {
        return;
    };
    let Some(projection_scale) = projection_world_units_per_viewport_pixel(&projection) else {
        return;
    };

    for cursor in cursor_reader.read() {
        if let Some(delta) = cursor.delta {
            let world_delta = viewport_delta_to_world_delta(delta, projection_scale);
            transform.translation -= world_delta.extend(0.0);
            clamp_camera_translation(&mut transform, window, game_map.as_ref(), projection_scale);
        }
    }
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
            .init_resource::<MousePanState>()
            .add_systems(Startup, (setup_camera, setup_unit_atlas))
            .add_systems(
                Update,
                (handle_touch_camera, handle_camera_scaling, handle_mouse_pan)
                    .run_if(in_state(crate::core::AppState::InGame)),
            )
            .add_systems(
                Update,
                emit_map_dimensions_on_scale_change
                    .run_if(
                        in_state(crate::core::AppState::InGame)
                            .and(resource_changed::<CameraScale>)
                            .and(resource_exists::<EventSink<MapDimensions>>),
                    )
                    .after(handle_touch_camera)
                    .after(handle_camera_scaling)
                    .after(handle_mouse_pan),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use awbrn_map::AwbrnMap;
    use awbrn_types::GraphicalTerrain;
    use bevy::window::WindowResolution;

    fn test_map(width: usize, height: usize) -> GameMap {
        let mut game_map = GameMap::default();
        game_map.set(AwbrnMap::new(width, height, GraphicalTerrain::Plain));
        game_map
    }

    fn test_window(width: u32, height: u32) -> Window {
        Window {
            resolution: WindowResolution::new(width, height),
            ..default()
        }
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
    fn camera_translation_is_clamped_to_map_bounds() {
        let game_map = test_map(20, 20);
        let window = test_window(320, 240);
        let mut transform = Transform::from_xyz(10_000.0, -10_000.0, 999.0);

        clamp_camera_translation(&mut transform, &window, &game_map, 0.5);

        assert!(transform.translation.x <= 80.0);
        assert!(transform.translation.y >= -208.0);
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
}
