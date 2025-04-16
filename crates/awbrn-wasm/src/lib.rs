use bevy::{
    app::PluginsState,
    prelude::*,
    render::camera::{Projection, ScalingMode},
    window::{RawHandleWrapper, WindowResolution, WindowWrapper},
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use web_sys::OffscreenCanvas;

mod offscreen_window_handle;

#[derive(Resource, Copy, Clone, Debug, Deserialize, Serialize, tsify_next::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct CanvasSize {
    width: f32,
    height: f32,
}

#[wasm_bindgen]
pub struct BevyApp {
    app: App,
}

#[wasm_bindgen]
impl BevyApp {
    #[wasm_bindgen(constructor)]
    pub fn new(canvas: web_sys::OffscreenCanvas, canvas_size: CanvasSize) -> Self {
        let mut app = App::new();

        app.add_plugins(
            DefaultPlugins
                .set(bevy::window::WindowPlugin {
                    primary_window: Some(Window {
                        resolution: WindowResolution::new(canvas_size.width, canvas_size.height),
                        ..Default::default()
                    }),
                    exit_condition: bevy::window::ExitCondition::DontExit,
                    ..Default::default()
                })
                .set(ImagePlugin::default_nearest())
                .set(AssetPlugin {
                    file_path: String::from("../../assets"),
                    meta_check: bevy::asset::AssetMetaCheck::Never,
                    ..AssetPlugin::default()
                }),
        )
        .add_systems(PreStartup, setup_added_window)
        .add_systems(Startup, (setup_camera, setup_test_sprite));

        app.insert_non_send_resource(canvas);

        BevyApp { app }
    }

    #[wasm_bindgen]
    pub fn update(&mut self) {
        if self.app.plugins_state() != PluginsState::Cleaned {
            if self.app.plugins_state() == PluginsState::Ready {
                self.app.finish();
                self.app.cleanup();
            }
        } else {
            self.app.update();
        }
    }

    #[wasm_bindgen]
    pub fn resize(&mut self, size: CanvasSize) {
        let world = self.app.world_mut();

        // Update window resolutions
        for mut window in world.query::<&mut Window>().iter_mut(world) {
            window.resolution.set(size.width, size.height);
        }

        // Find all the cameras and update their projections if they're orthographic
        for (_camera, mut projection) in world.query::<(&Camera, &mut Projection)>().iter_mut(world)
        {
            if let Projection::Orthographic(ref mut ortho) = *projection {
                ortho.scaling_mode = ScalingMode::WindowSize;
            }
        }

        // TODO: do we send a WindowResized event here?
    }
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn setup_test_sprite(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let texture = asset_server.load("textures/tiles.png");
    let layout = TextureAtlasLayout::from_grid(UVec2::new(16, 32), 64, 27, None, None);
    let texture_atlas_layout = texture_atlas_layouts.add(layout);
    commands.spawn(Sprite::from_atlas_image(
        texture,
        TextureAtlas {
            layout: texture_atlas_layout,
            index: 1,
        },
    ));
}

#[wasm_bindgen]
extern "C" {
    // Use `js_namespace` here to bind `console.log(..)` instead of just
    // `log(..)`
    #[wasm_bindgen(js_namespace = console)]
    pub(crate) fn log(s: &str);
}

fn setup_added_window(
    mut commands: Commands,
    canvas: NonSendMut<OffscreenCanvas>,
    mut new_windows: Query<Entity, Added<Window>>,
) {
    // This system should only be called once at startup and there should only
    // be one window that's been added.
    let Some(entity) = new_windows.iter_mut().next() else {
        return;
    };

    let handle = offscreen_window_handle::OffscreenWindowHandle::new(&canvas);

    let handle = RawHandleWrapper::new(&WindowWrapper::new(handle))
        .expect("to create offscreen raw handle wrapper. If this fails, multiple threads are trying to access the same canvas!");

    commands.entity(entity).insert(handle);
}
