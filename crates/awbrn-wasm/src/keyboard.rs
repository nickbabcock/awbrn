use crate::web_key_code_generated::WebKeyCode;
use bevy::input::keyboard::KeyCode;

pub fn from_wire_code(code: u16) -> KeyCode {
    WebKeyCode::from_wire_code(code).to_bevy_key_code()
}
