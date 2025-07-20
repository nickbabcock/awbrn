use awbrn_bevy::{AwbrnPlugin, ReplayToLoad};
use bevy::{
    app::PluginsState,
    input::{
        ButtonState,
        keyboard::{KeyboardInput, NativeKey},
    },
    prelude::*,
    render::camera::{Projection, ScalingMode},
    window::{RawHandleWrapper, WindowResolution, WindowWrapper},
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use web_sys::OffscreenCanvas;

mod offscreen_window_handle;

#[derive(Resource, Copy, Clone, Debug, Deserialize, Serialize, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct CanvasDisplay {
    width: f32,
    height: f32,
    scale_factor: f32,
}

#[derive(Resource, Copy, Clone, Debug, Deserialize, Serialize, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
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
    pub fn new(canvas: web_sys::OffscreenCanvas, display: CanvasDisplay) -> Self {
        let mut app = App::new();

        let mut resolution = WindowResolution::new(display.width, display.height);
        resolution.set_scale_factor_override(Some(display.scale_factor));

        app.add_plugins(
            DefaultPlugins
                .set(bevy::window::WindowPlugin {
                    primary_window: Some(Window {
                        resolution,
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
        .add_plugins(AwbrnPlugin::default())
        .insert_non_send_resource(canvas);

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

    #[wasm_bindgen]
    pub fn handle_key_down(&mut self, event: KeyboardEvent) {
        let Ok((window, _)) = self
            .app
            .world_mut()
            .query::<(Entity, &Window)>()
            .single(self.app.world_mut())
        else {
            warn!("No window found for key down event");
            return;
        };

        let code = match event.key.as_str() {
            "-" => KeyCode::Minus,
            "=" => KeyCode::Equal,
            "ArrowRight" => KeyCode::ArrowRight,
            "ArrowLeft" => KeyCode::ArrowLeft,
            _ => {
                warn!("Unhandled key down event: {}", event.key);
                return;
            }
        };

        let event = KeyboardInput {
            key_code: code,
            logical_key: bevy::input::keyboard::Key::Unidentified(NativeKey::Web(
                event.key_code.into(),
            )),
            state: ButtonState::Pressed,
            text: None,
            repeat: event.repeat,
            window,
        };

        self.app.world_mut().send_event(event);
    }

    #[wasm_bindgen]
    pub fn handle_key_up(&mut self, event: KeyboardEvent) {
        let Ok((window, _)) = self
            .app
            .world_mut()
            .query::<(Entity, &Window)>()
            .single(self.app.world_mut())
        else {
            warn!("No window found for key up event");
            return;
        };

        let code = match event.key.as_str() {
            "-" => KeyCode::Minus,
            "=" => KeyCode::Equal,
            "ArrowRight" => KeyCode::ArrowRight,
            "ArrowLeft" => KeyCode::ArrowLeft,
            _ => {
                warn!("Unhandled key down event: {}", event.key);
                return;
            }
        };

        let event: KeyboardInput = KeyboardInput {
            key_code: code,
            logical_key: bevy::input::keyboard::Key::Unidentified(NativeKey::Web(
                event.key_code.into(),
            )),
            state: ButtonState::Released,
            text: None,
            repeat: event.repeat,
            window,
        };

        self.app.world_mut().send_event(event);
    }

    #[wasm_bindgen]
    pub fn new_replay(&mut self, data: Vec<u8>) -> Result<(), JsError> {
        // Signal that a new replay should be loaded
        self.app.world_mut().insert_resource(ReplayToLoad(data));

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct KeyboardEvent {
    key: String,
    key_code: String,
    repeat: bool,
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
