use serde::Deserialize;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tauri::State;

#[derive(Clone, Default)]
pub struct AppState {
    pending_replay: Arc<Mutex<Option<Vec<u8>>>>,
    interaction_state: Arc<Mutex<InteractionState>>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowMetrics {
    pub width: f32,
    pub height: f32,
    pub scale_factor: f32,
}

#[derive(Debug, Clone, Default)]
pub struct InteractionSnapshot {
    pub cursor: Option<(f32, f32)>,
    pub mouse_buttons: [bool; 3],
    pub wheel_lines: f32,
    pub pressed_keys: HashSet<String>,
    pub window_metrics: Option<WindowMetrics>,
}

#[derive(Debug, Default)]
struct InteractionState {
    cursor: Option<(f32, f32)>,
    mouse_buttons: [bool; 3],
    wheel_lines: f32,
    pressed_keys: HashSet<String>,
    window_metrics: Option<WindowMetrics>,
}

impl AppState {
    pub fn take_pending_replay(&self) -> Option<Vec<u8>> {
        self.pending_replay
            .lock()
            .ok()
            .and_then(|mut guard| guard.take())
    }

    pub fn take_interaction_snapshot(&self) -> InteractionSnapshot {
        self.interaction_state
            .lock()
            .map(|mut guard| {
                let wheel_lines = guard.wheel_lines;
                guard.wheel_lines = 0.0;

                InteractionSnapshot {
                    cursor: guard.cursor,
                    mouse_buttons: guard.mouse_buttons,
                    wheel_lines,
                    pressed_keys: guard.pressed_keys.clone(),
                    window_metrics: guard.window_metrics,
                }
            })
            .unwrap_or_default()
    }

    fn set_pending_replay(&self, data: Vec<u8>) -> Result<(), String> {
        let mut guard = self
            .pending_replay
            .lock()
            .map_err(|_| "Failed to lock replay state".to_string())?;
        *guard = Some(data);
        Ok(())
    }

    fn set_cursor(&self, x: f32, y: f32) -> Result<(), String> {
        let mut guard = self
            .interaction_state
            .lock()
            .map_err(|_| "Failed to lock interaction state".to_string())?;
        guard.cursor = Some((x, y));
        Ok(())
    }

    fn set_mouse_button(&self, button: u8, pressed: bool) -> Result<(), String> {
        if button > 2 {
            return Ok(());
        }

        let mut guard = self
            .interaction_state
            .lock()
            .map_err(|_| "Failed to lock interaction state".to_string())?;
        guard.mouse_buttons[button as usize] = pressed;
        Ok(())
    }

    fn add_mouse_wheel(&self, lines: f32) -> Result<(), String> {
        let mut guard = self
            .interaction_state
            .lock()
            .map_err(|_| "Failed to lock interaction state".to_string())?;
        guard.wheel_lines += lines;
        Ok(())
    }

    fn set_key(&self, code: String, pressed: bool) -> Result<(), String> {
        let mut guard = self
            .interaction_state
            .lock()
            .map_err(|_| "Failed to lock interaction state".to_string())?;

        if pressed {
            guard.pressed_keys.insert(code);
        } else {
            guard.pressed_keys.remove(&code);
        }

        Ok(())
    }

    fn set_window_metrics(&self, metrics: WindowMetrics) -> Result<(), String> {
        let mut guard = self
            .interaction_state
            .lock()
            .map_err(|_| "Failed to lock interaction state".to_string())?;
        guard.window_metrics = Some(metrics);
        Ok(())
    }
}

#[tauri::command]
pub fn new_replay(data: Vec<u8>, state: State<'_, AppState>) -> Result<(), String> {
    state.set_pending_replay(data)
}

#[tauri::command]
pub fn interaction_cursor_moved(x: f32, y: f32, state: State<'_, AppState>) -> Result<(), String> {
    state.set_cursor(x, y)
}

#[tauri::command]
pub fn interaction_mouse_button(
    button: u8,
    pressed: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.set_mouse_button(button, pressed)
}

#[tauri::command]
pub fn interaction_mouse_wheel(lines: f32, state: State<'_, AppState>) -> Result<(), String> {
    state.add_mouse_wheel(lines)
}

#[tauri::command]
pub fn interaction_key(
    code: String,
    pressed: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.set_key(code, pressed)
}

#[tauri::command]
pub fn set_window_metrics(
    width: f32,
    height: f32,
    scale_factor: f32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.set_window_metrics(WindowMetrics {
        width,
        height,
        scale_factor,
    })
}
