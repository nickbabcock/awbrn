use crate::core::grid::GridSystem;
use crate::features::event_bus::{EventSink, MapDimensions};
use crate::loading::ClientAssetLoader;
use crate::render::UnitAtlasResource;
use awbrn_game::world::GameMap;
use bevy::prelude::*;

#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct CameraScale(f32);

impl CameraScale {
    pub fn scale(&self) -> f32 {
        self.0
    }

    pub fn zoom_in(&self) -> Self {
        let current_index = ZOOM_LEVELS
            .iter()
            .position(|&z| (z - self.0).abs() < 0.01)
            .unwrap_or(0);

        let new_scale = ZOOM_LEVELS[current_index.saturating_add(1).min(ZOOM_LEVELS.len() - 1)];

        CameraScale(new_scale)
    }

    pub fn zoom_out(&self) -> Self {
        let current_index = ZOOM_LEVELS
            .iter()
            .position(|&z| (z - self.0).abs() < 0.01)
            .unwrap_or(0);

        let new_scale = ZOOM_LEVELS[current_index.saturating_sub(1)];

        CameraScale(new_scale)
    }
}

impl Default for CameraScale {
    fn default() -> Self {
        CameraScale(2.0)
    }
}

const ZOOM_LEVELS: [f32; 3] = [1.0, 1.5, 2.0];

pub(crate) fn setup_camera(mut commands: Commands, camera_scale: Res<CameraScale>) {
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

pub(crate) fn setup_unit_atlas(
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
    MapDimensions {
        width: game_map.width() as f32 * GridSystem::TILE_SIZE * camera_scale.scale(),
        height: (game_map.height() as f32 + 1.0) * GridSystem::TILE_SIZE * camera_scale.scale(),
    }
}

pub(crate) fn handle_camera_scaling(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut camera_scale: ResMut<CameraScale>,
    mut query: Query<&mut Projection, With<Camera>>,
) {
    let new_zoom = if keyboard_input.just_pressed(KeyCode::Equal) {
        camera_scale.zoom_in()
    } else if keyboard_input.just_pressed(KeyCode::Minus) {
        camera_scale.zoom_out()
    } else {
        return;
    };

    *camera_scale = new_zoom;

    if let Ok(mut projection) = query.single_mut()
        && let Projection::Orthographic(orthographic) = &mut *projection
    {
        orthographic.scale = 1.0 / camera_scale.scale();
    }
}

pub(crate) fn emit_map_dimensions_on_scale_change(
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
            .add_systems(Startup, (setup_camera, setup_unit_atlas))
            .add_systems(
                Update,
                handle_camera_scaling.run_if(in_state(crate::core::AppState::InGame)),
            )
            .add_systems(
                Update,
                emit_map_dimensions_on_scale_change
                    .run_if(
                        in_state(crate::core::AppState::InGame)
                            .and(resource_changed::<CameraScale>)
                            .and(resource_exists::<EventSink<MapDimensions>>),
                    )
                    .after(handle_camera_scaling),
            );
    }
}
