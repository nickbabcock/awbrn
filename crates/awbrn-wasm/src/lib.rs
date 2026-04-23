use awbrn_client::{
    ActionMenuEvent, AwbrnPlugin, ClientCommandReady, EventSink, MapAssetPathResolver,
    MapDimensions, NewDay, PendingGameStart, PendingMatchMap, PlayerRosterSnapshot, ReplayLoaded,
    ReplayToLoad, StaticAssetPathResolver, TileSelected, UnitBuilt, UnitMoved,
    core::coords::LogicalPx,
};
use awbrn_map::AwbwMapData;
use awbrn_types::{AwbwGamePlayerId, PlayerFaction};
use bevy::{
    app::PluginsState,
    input::{
        ButtonState,
        keyboard::{Key, KeyboardFocusLost, KeyboardInput, NativeKey},
        mouse::{MouseButton, MouseButtonInput, MouseScrollUnit, MouseWheel},
        touch::{TouchInput, TouchPhase},
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
mod web_key_code_generated;

const AWBW_API_ASSET_SOURCE: &str = "awbw_api";

#[cfg(target_arch = "wasm32")]
use bevy::asset::{
    AssetApp,
    io::{AssetSourceBuilder, wasm::HttpWasmAssetReader},
};

/// Discriminated union of all game events sent to JavaScript.
#[derive(Serialize, tsify::Tsify)]
#[tsify(into_wasm_abi)]
#[serde(tag = "type")]
pub enum GameEvent {
    NewDay(NewDay),
    UnitMoved(UnitMoved),
    UnitBuilt(UnitBuilt),
    TileSelected(TileSelected),
    MapDimensions(MapDimensions),
    ReplayLoaded(ReplayLoaded),
    PlayerRosterUpdated(PlayerRosterSnapshot),
    ActionMenuOpened(ActionMenuEvent),
    ActionMenuClosed,
    ClientCommandReady(ClientCommandReady),
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "(event: GameEvent) => void")]
    pub type GameEventCallback;
}

/// Wrapper around a JS callback that is safe to send across threads.
///
/// SAFETY: WASM runs on a single thread, so Send + Sync are safe here.
struct WasmCallback(js_sys::Function);
unsafe impl Send for WasmCallback {}
unsafe impl Sync for WasmCallback {}

impl WasmCallback {
    fn call(&self, event: GameEvent) {
        let Ok(js_value) = serde_wasm_bindgen::to_value(&event) else {
            return;
        };
        let _ = self.0.call1(&JsValue::NULL, &js_value);
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
    scale_factor: f32,
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

struct WasmMapAssetPathResolver;

impl MapAssetPathResolver for WasmMapAssetPathResolver {
    fn resolve_path(&self, map_id: u32) -> String {
        format!("{AWBW_API_ASSET_SOURCE}://api/awbw/map/{map_id}.json")
    }
}

#[cfg(target_arch = "wasm32")]
fn register_awbw_asset_source(app: &mut App) {
    app.register_asset_source(
        AWBW_API_ASSET_SOURCE,
        AssetSourceBuilder::new(|| Box::new(HttpWasmAssetReader::new("/")))
            .with_processed_reader(|| Box::new(HttpWasmAssetReader::new("/"))),
    );
}

#[cfg(not(target_arch = "wasm32"))]
fn register_awbw_asset_source(_app: &mut App) {}

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
        register_awbw_asset_source(&mut app);

        let mut resolution = WindowResolution::new(display.width as u32, display.height as u32);
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
                })
                // The URLs we reference are on the same origin and controlled
                // by the asset manifest on the JS side.
                .set(bevy::asset::io::web::WebAssetPlugin {
                    silence_startup_warning: true,
                }),
        )
        .add_systems(PreStartup, setup_added_window);

        let awbrn_plugin = AwbrnPlugin::new(Arc::new(WasmMapAssetPathResolver))
            .with_static_asset_resolver(Arc::new(WasmStaticAssetPathResolver::new(
                asset_config.static_asset_urls,
            )));

        app.add_plugins(awbrn_plugin);

        if let Some(callback) = event_callback {
            let js_value: JsValue = callback.into();
            let js_function: js_sys::Function = js_value.into();
            let cb = Arc::new(WasmCallback(js_function));

            macro_rules! wasm_sink {
                ($variant:ident, $payload:ty) => {{
                    let cb = cb.clone();
                    app.insert_resource(EventSink::<$payload>::new(move |p| {
                        cb.call(GameEvent::$variant(p));
                    }));
                }};
            }

            wasm_sink!(NewDay, NewDay);
            wasm_sink!(UnitMoved, UnitMoved);
            wasm_sink!(UnitBuilt, UnitBuilt);
            wasm_sink!(TileSelected, TileSelected);
            wasm_sink!(MapDimensions, MapDimensions);
            wasm_sink!(ReplayLoaded, ReplayLoaded);
            wasm_sink!(PlayerRosterUpdated, PlayerRosterSnapshot);
            wasm_sink!(ClientCommandReady, ClientCommandReady);
            app.insert_resource(EventSink::<ActionMenuEvent>::new({
                let cb = cb.clone();
                move |event| {
                    if event.actions.is_empty() {
                        cb.call(GameEvent::ActionMenuClosed);
                    } else {
                        cb.call(GameEvent::ActionMenuOpened(event));
                    }
                }
            }));
        }

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
        let scale_factor = size.scale_factor;

        if let Some(canvas) = world.get_non_send_resource_mut::<OffscreenCanvas>() {
            canvas.set_width(size.width as u32);
            canvas.set_height(size.height as u32);
        }

        // Update window resolutions
        for mut window in world.query::<&mut Window>().iter_mut(world) {
            window
                .resolution
                .set_scale_factor_override(Some(scale_factor));
            window
                .resolution
                .set_physical_resolution(size.width as u32, size.height as u32);
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
    pub fn handle_key_down_code(&mut self, key_code: u16, repeat: bool) {
        let Ok((window, _)) = self
            .app
            .world_mut()
            .query::<(Entity, &Window)>()
            .single(self.app.world_mut())
        else {
            warn!("No window found for key down event");
            return;
        };

        let event = KeyboardInput {
            key_code: keyboard::from_wire_code(key_code),
            logical_key: Key::Unidentified(NativeKey::Unidentified),
            state: ButtonState::Pressed,
            text: None,
            repeat,
            window,
        };

        self.app.world_mut().write_message(event);
    }

    #[wasm_bindgen]
    pub fn handle_key_up_code(&mut self, key_code: u16, repeat: bool) {
        let Ok((window, _)) = self
            .app
            .world_mut()
            .query::<(Entity, &Window)>()
            .single(self.app.world_mut())
        else {
            warn!("No window found for key up event");
            return;
        };

        let event: KeyboardInput = KeyboardInput {
            key_code: keyboard::from_wire_code(key_code),
            logical_key: Key::Unidentified(NativeKey::Unidentified),
            state: ButtonState::Released,
            text: None,
            repeat,
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
        // x and y are CSS / logical pixels (offsetX / offsetY from the DOM PointerEvent).
        let position = LogicalPx::new(x, y).to_vec2();
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
    pub fn handle_touch_start(&mut self, id: u32, x: f32, y: f32) {
        self.write_touch_input(id, TouchPhase::Started, x, y);
    }

    #[wasm_bindgen]
    pub fn handle_touch_move(&mut self, id: u32, x: f32, y: f32) {
        self.write_touch_input(id, TouchPhase::Moved, x, y);
    }

    #[wasm_bindgen]
    pub fn handle_touch_end(&mut self, id: u32, x: f32, y: f32) {
        self.write_touch_input(id, TouchPhase::Ended, x, y);
    }

    #[wasm_bindgen]
    pub fn handle_touch_cancel(&mut self, id: u32, x: f32, y: f32) {
        self.write_touch_input(id, TouchPhase::Canceled, x, y);
    }

    #[wasm_bindgen]
    pub fn handle_mouse_wheel(&mut self, x: f32, y: f32, line_units: bool) {
        let world = self.app.world_mut();
        let Some(window) = primary_window_entity(world) else {
            return;
        };

        let _ = world.write_message(MouseWheel {
            unit: if line_units {
                MouseScrollUnit::Line
            } else {
                MouseScrollUnit::Pixel
            },
            x,
            y,
            window,
        });
    }

    fn write_touch_input(&mut self, id: u32, phase: TouchPhase, x: f32, y: f32) {
        let world = self.app.world_mut();
        let Some(window) = primary_window_entity(world) else {
            return;
        };
        // x and y are CSS / logical pixels (offsetX / offsetY from the DOM TouchEvent).
        let _ = world.write_message(TouchInput {
            phase,
            position: LogicalPx::new(x, y).to_vec2(),
            window,
            force: None,
            id: id as u64,
        });
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

    #[wasm_bindgen]
    pub fn preview_map(&mut self, map_id: u32) -> Result<(), JsError> {
        self.app
            .world_mut()
            .insert_resource(PendingGameStart(map_id));

        Ok(())
    }

    #[wasm_bindgen]
    pub fn load_match_map(&mut self, map_data: JsValue) -> Result<(), JsError> {
        let map = serde_wasm_bindgen::from_value::<AwbwMapData>(map_data)
            .map_err(|error| JsError::new(&format!("Invalid AWBW match map data: {error}")))?;

        self.app.world_mut().insert_resource(PendingMatchMap(map));

        Ok(())
    }

    #[wasm_bindgen]
    pub fn load_match_state(
        &mut self,
        game_state: JsValue,
        participants: JsValue,
    ) -> Result<(), JsError> {
        let state =
            serde_wasm_bindgen::from_value::<awbrn_client::modes::play::MatchGameStateWire>(
                game_state,
            )
            .map_err(|error| JsError::new(&format!("Invalid match game state: {error}")))?;
        let participants = serde_wasm_bindgen::from_value::<
            Vec<awbrn_client::modes::play::MatchParticipantWire>,
        >(participants)
        .map_err(|error| JsError::new(&format!("Invalid match participants: {error}")))?;

        self.app
            .world_mut()
            .insert_resource(awbrn_client::modes::play::PendingMatchState {
                state,
                participants,
            });

        Ok(())
    }

    #[wasm_bindgen]
    pub fn apply_match_update(&mut self, update: JsValue) -> Result<(), JsError> {
        let update = serde_wasm_bindgen::from_value::<
            awbrn_client::modes::play::MatchPlayerUpdateWire,
        >(update)
        .map_err(|error| JsError::new(&format!("Invalid match update: {error}")))?;

        let world = self.app.world_mut();
        if let Some(mut pending) =
            world.get_resource_mut::<awbrn_client::modes::play::PendingMatchUpdates>()
        {
            pending.0.push(update);
        } else {
            world.insert_resource(awbrn_client::modes::play::PendingMatchUpdates(vec![update]));
        }

        Ok(())
    }

    #[wasm_bindgen]
    pub fn choose_action(&mut self, action: JsValue) -> Result<(), JsError> {
        let action = serde_wasm_bindgen::from_value::<awbrn_client::ActionMenuAction>(action)
            .map_err(|error| JsError::new(&format!("Invalid action menu action: {error}")))?;

        self.app
            .world_mut()
            .write_message(awbrn_client::modes::play::ChooseActionMenuAction { action });
        Ok(())
    }

    #[wasm_bindgen]
    pub fn cancel_action_menu(&mut self) {
        self.app
            .world_mut()
            .write_message(awbrn_client::modes::play::CancelActionMenu);
    }

    #[wasm_bindgen]
    pub fn set_player_display_faction(
        &mut self,
        player_id: u32,
        faction_id: Option<u8>,
    ) -> Result<(), JsError> {
        let faction = faction_id
            .map(|id| {
                PlayerFaction::from_id(id)
                    .ok_or_else(|| JsError::new(&format!("Invalid faction id: {id}")))
            })
            .transpose()?;

        self.app.world_mut().write_message(
            awbrn_client::features::player_display::SetPlayerDisplayFaction {
                player_id: AwbwGamePlayerId::new(player_id),
                faction,
            },
        );

        Ok(())
    }
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
