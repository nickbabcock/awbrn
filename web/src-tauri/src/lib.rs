mod commands;
mod desktop_plugin;
mod web_asset_plugin;

use awbrn_bevy::ReplayToLoad;
use bevy::{
    app::{App, PluginsState},
    ecs::entity::Entity,
    input::{
        ButtonInput, ButtonState,
        keyboard::{Key, KeyboardInput, NativeKey},
        mouse::MouseButton,
    },
    math::Vec2,
    prelude::Window,
};
use commands::{AppState, InteractionSnapshot};
use desktop_plugin::AwbrnDesktopPlugin;
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};
use tauri::{Manager, RunEvent};

#[cfg(feature = "debug-inspector")]
use bevy_inspector_egui::{bevy_egui::EguiPlugin, quick::WorldInspectorPlugin};

#[derive(Default)]
struct AppliedInputState {
    mouse_buttons: [bool; 3],
    pressed_keys: HashSet<String>,
}

struct RuntimeState {
    app: App,
    applied_input: AppliedInputState,
}

impl RuntimeState {
    fn new(app_handle: tauri::AppHandle, window: tauri::WebviewWindow) -> Self {
        let window_label = window.label().to_string();

        let mut app = App::new();
        app.add_plugins(AwbrnDesktopPlugin::new(app_handle, window, window_label));

        #[cfg(feature = "debug-inspector")]
        app.add_plugins((EguiPlugin::default(), WorldInspectorPlugin::new()));

        Self {
            app,
            applied_input: AppliedInputState::default(),
        }
    }

    fn apply_snapshot(&mut self, snapshot: InteractionSnapshot, pending_replay: Option<Vec<u8>>) {
        let _ = snapshot.wheel_lines;
        let world = self.app.world_mut();

        if let Some(data) = pending_replay {
            world.insert_resource(ReplayToLoad(data));
        }

        {
            let mut window_query = world.query::<(Entity, &mut Window)>();
            if let Some((_window_entity, mut window)) = window_query.iter_mut(world).next() {
                if let Some(metrics) = snapshot.window_metrics {
                    let width = metrics.width.max(1.0);
                    let height = metrics.height.max(1.0);
                    let scale_factor = metrics.scale_factor.max(0.1);

                    window.resolution.set(width, height);
                    window
                        .resolution
                        .set_scale_factor_override(Some(scale_factor));
                }

                window.set_cursor_position(snapshot.cursor.map(|(x, y)| Vec2::new(x, y)));
            }
        }

        {
            let mut mouse_input = world.resource_mut::<ButtonInput<MouseButton>>();

            for (index, pressed) in snapshot.mouse_buttons.iter().copied().enumerate() {
                if self.applied_input.mouse_buttons[index] == pressed {
                    continue;
                }

                if let Some(button) = map_mouse_button(index as u8) {
                    if pressed {
                        mouse_input.press(button);
                    } else {
                        mouse_input.release(button);
                    }
                }

                self.applied_input.mouse_buttons[index] = pressed;
            }
        }

        {
            let mut window_query = world.query::<(Entity, &Window)>();
            let window_entity = window_query.iter(world).next().map(|(entity, _)| entity);

            let keys_to_release: Vec<_> = self
                .applied_input
                .pressed_keys
                .difference(&snapshot.pressed_keys)
                .cloned()
                .collect();
            for code in keys_to_release {
                if let Some(window_entity) = window_entity {
                    world.write_message(KeyboardInput {
                        key_code: awbrn_bevy::from_web_code(&code),
                        logical_key: Key::Unidentified(NativeKey::Web(code.clone().into())),
                        state: ButtonState::Released,
                        text: None,
                        repeat: false,
                        window: window_entity,
                    });
                }
            }

            let keys_to_press: Vec<_> = snapshot
                .pressed_keys
                .difference(&self.applied_input.pressed_keys)
                .cloned()
                .collect();
            for code in keys_to_press {
                if let Some(window_entity) = window_entity {
                    world.write_message(KeyboardInput {
                        key_code: awbrn_bevy::from_web_code(&code),
                        logical_key: Key::Unidentified(NativeKey::Web(code.clone().into())),
                        state: ButtonState::Pressed,
                        text: None,
                        repeat: false,
                        window: window_entity,
                    });
                }
            }
        }

        self.applied_input.pressed_keys = snapshot.pressed_keys;
    }

    fn update(&mut self) {
        if self.app.plugins_state() != PluginsState::Cleaned {
            if self.app.plugins_state() == PluginsState::Ready {
                self.app.finish();
                self.app.cleanup();
            }
            return;
        }

        self.app.update();
    }
}

fn map_mouse_button(button: u8) -> Option<MouseButton> {
    match button {
        0 => Some(MouseButton::Left),
        1 => Some(MouseButton::Middle),
        2 => Some(MouseButton::Right),
        _ => None,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let tauri_app = tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::new_replay,
            commands::interaction_cursor_moved,
            commands::interaction_mouse_button,
            commands::interaction_mouse_wheel,
            commands::interaction_key,
            commands::set_window_metrics,
        ])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    let app_handle_for_timer = tauri_app.handle().clone();
    std::thread::spawn(move || {
        let frame_duration = Duration::from_secs_f64(1.0 / 60.0);
        loop {
            std::thread::sleep(frame_duration);
            let _ = app_handle_for_timer.run_on_main_thread(|| {});
        }
    });

    let target_frame_duration = Duration::from_secs_f64(1.0 / 60.0);
    let mut last_frame = Instant::now();
    let mut runtime_state: Option<RuntimeState> = None;

    tauri_app.run(move |app_handle, event| match event {
        RunEvent::MainEventsCleared => {
            if runtime_state.is_none() {
                let window = app_handle
                    .get_webview_window("main")
                    .or_else(|| app_handle.webview_windows().into_values().next());

                let Some(window) = window else {
                    return;
                };

                runtime_state = Some(RuntimeState::new(app_handle.clone(), window));
            }

            if last_frame.elapsed() < target_frame_duration {
                return;
            }

            last_frame = Instant::now();

            let state = app_handle.state::<AppState>();
            let snapshot = state.take_interaction_snapshot();
            let pending_replay = state.take_pending_replay();

            if let Some(runtime_state) = runtime_state.as_mut() {
                runtime_state.apply_snapshot(snapshot, pending_replay);
                runtime_state.update();
            }
        }
        _ => {}
    });
}
