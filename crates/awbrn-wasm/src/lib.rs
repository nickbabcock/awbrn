use awbrn_client::{
    AwbrnPlugin, EventBus, ExternalEvent, GameEvent, ReplayToLoad, StaticAssetPathResolver,
};
use bevy::{
    app::PluginsState,
    input::{
        ButtonState,
        keyboard::{KeyboardFocusLost, KeyboardInput, NativeKey},
        mouse::{MouseButton, MouseButtonInput},
    },
    prelude::*,
    window::{CursorLeft, CursorMoved, RawHandleWrapper, WindowResolution, WindowWrapper},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};
use wasm_bindgen::prelude::*;
use web_sys::OffscreenCanvas;

mod keyboard;
mod offscreen_window_handle;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "(event: GameEvent) => void")]
    pub type GameEventCallback;
}

/// WASM EventBus implementation that sends events to JavaScript
pub struct WasmEventBus {
    callback: js_sys::Function,
}

// SAFETY: In WASM, everything runs on a single thread, so Send + Sync are safe
unsafe impl Send for WasmEventBus {}
unsafe impl Sync for WasmEventBus {}

impl WasmEventBus {
    pub fn new(callback: js_sys::Function) -> Self {
        Self { callback }
    }
}

impl EventBus<GameEvent> for WasmEventBus {
    fn publish_event(&self, event: &ExternalEvent<GameEvent>) {
        // Serialize the event directly to JsValue using serde-wasm-bindgen
        let Ok(js_value) = serde_wasm_bindgen::to_value(&event.payload) else {
            return;
        };
        let _ = self.callback.call1(&JsValue::NULL, &js_value);
    }
}

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
    scale_factor: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct MouseButtonEvent {
    button: i16,
}

#[derive(Clone, Debug, Deserialize, Serialize, tsify::Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct GameAssetConfig {
    static_asset_urls: BTreeMap<String, String>,
}

struct WasmStaticAssetPathResolver {
    entries: BTreeMap<String, String>,
}

impl WasmStaticAssetPathResolver {
    fn new(entries: BTreeMap<String, String>) -> Self {
        Self { entries }
    }
}

impl StaticAssetPathResolver for WasmStaticAssetPathResolver {
    fn resolve_path(&self, logical_path: &str) -> String {
        self.entries
            .get(logical_path)
            .cloned()
            .unwrap_or_else(|| logical_path.to_string())
    }
}

#[wasm_bindgen]
pub struct BevyApp {
    app: App,
}

#[wasm_bindgen]
impl BevyApp {
    #[wasm_bindgen(constructor)]
    pub fn new(
        canvas: web_sys::OffscreenCanvas,
        display: CanvasDisplay,
        asset_config: GameAssetConfig,
        event_callback: Option<GameEventCallback>,
    ) -> Self {
        let mut app = App::new();

        let mut resolution = WindowResolution::new(
            (display.width * display.scale_factor).round() as u32,
            (display.height * display.scale_factor).round() as u32,
        );
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
        .add_systems(PreStartup, setup_added_window);

        let mut awbrn_plugin = AwbrnPlugin::default().with_static_asset_resolver(Arc::new(
            WasmStaticAssetPathResolver::new(asset_config.static_asset_urls),
        ));
        if let Some(callback) = event_callback {
            let js_value: JsValue = callback.into();
            let js_function: js_sys::Function = js_value.into();
            let event_bus = Arc::new(WasmEventBus::new(js_function));
            awbrn_plugin = awbrn_plugin.with_event_bus(event_bus);
        }

        app.add_plugins(awbrn_plugin)
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
        let scale_factor = size.scale_factor as f32;

        if let Some(canvas) = world.get_non_send_resource_mut::<OffscreenCanvas>() {
            canvas.set_width((size.width * scale_factor).round() as u32);
            canvas.set_height((size.height * scale_factor).round() as u32);
        }

        // Update window resolutions
        for mut window in world.query::<&mut Window>().iter_mut(world) {
            window
                .resolution
                .set_scale_factor_override(Some(scale_factor));
            window.resolution.set(size.width, size.height);
        }

        // Find all the cameras and update their projections if they're orthographic
        for (_camera, mut projection) in world.query::<(&Camera, &mut Projection)>().iter_mut(world)
        {
            if let Projection::Orthographic(ref mut ortho) = *projection {
                ortho.scaling_mode = bevy::camera::ScalingMode::WindowSize;
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

        let code = keyboard::from_web_code(event.key_code.as_str());
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

        self.app.world_mut().write_message(event);
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

        let code = keyboard::from_web_code(event.key_code.as_str());
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

        self.app.world_mut().write_message(event);
    }

    #[wasm_bindgen]
    pub fn handle_mouse_move(&mut self, x: f32, y: f32) {
        let world = self.app.world_mut();
        let Some(window) = primary_window_entity(world) else {
            return;
        };
        let position = Vec2::new(x, y);
        let previous = update_cursor_position(world, window, Some(position));

        let _ = world.write_message(CursorMoved {
            window,
            position,
            delta: previous.map(|previous| position - previous),
        });
    }

    #[wasm_bindgen]
    pub fn handle_mouse_down(&mut self, event: MouseButtonEvent) {
        let world = self.app.world_mut();
        let Some(window) = primary_window_entity(world) else {
            return;
        };
        let _ = world.write_message(MouseButtonInput {
            button: mouse_button_from_web(event.button),
            state: ButtonState::Pressed,
            window,
        });
    }

    #[wasm_bindgen]
    pub fn handle_mouse_up(&mut self, event: MouseButtonEvent) {
        let world = self.app.world_mut();
        let Some(window) = primary_window_entity(world) else {
            return;
        };
        let _ = world.write_message(MouseButtonInput {
            button: mouse_button_from_web(event.button),
            state: ButtonState::Released,
            window,
        });
    }

    #[wasm_bindgen]
    pub fn handle_mouse_leave(&mut self) {
        let world = self.app.world_mut();
        let Some(window) = primary_window_entity(world) else {
            return;
        };

        update_cursor_position(world, window, None);
        let _ = world.write_message(CursorLeft { window });
    }

    #[wasm_bindgen]
    pub fn handle_canvas_blur(&mut self) {
        let _ = self.app.world_mut().write_message(KeyboardFocusLost);
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

fn primary_window_entity(world: &mut World) -> Option<Entity> {
    let mut query = world.query_filtered::<Entity, With<Window>>();
    let Ok(window) = query.single(world) else {
        warn!("No window found for input event");
        return None;
    };

    Some(window)
}

fn update_cursor_position(
    world: &mut World,
    window: Entity,
    position: Option<Vec2>,
) -> Option<Vec2> {
    let previous = world
        .get::<Window>(window)
        .and_then(Window::cursor_position);

    let Some(mut window_ref) = world.get_mut::<Window>(window) else {
        warn!("Window entity {:?} missing for input event", window);
        return previous;
    };

    window_ref.set_cursor_position(position);
    previous
}

fn mouse_button_from_web(button: i16) -> MouseButton {
    match button {
        0 => MouseButton::Left,
        1 => MouseButton::Middle,
        2 => MouseButton::Right,
        3 => MouseButton::Back,
        4 => MouseButton::Forward,
        value if value >= 0 => MouseButton::Other(value as u16),
        value => {
            warn!("Ignoring unexpected negative mouse button value {value}, mapping to Left");
            MouseButton::Left
        }
    }
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
