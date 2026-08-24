//! Sends tracing output to the browser or worker console.

use tracing::Level;
use wasm_bindgen::prelude::*;

use crate::subscriber::{LogOutput, LogSubscriber, LoggingConfig};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn console_log(message: &str);
    #[wasm_bindgen(js_namespace = console, js_name = debug)]
    fn console_debug(message: &str);
    #[wasm_bindgen(js_namespace = console, js_name = warn)]
    fn console_warn(message: &str);
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(message: &str);
    #[wasm_bindgen(js_namespace = performance, js_name = now)]
    fn performance_now() -> f64;
}

/// Sends tracing output to the console.
///
/// Repeat calls do nothing but write a warning, because a wasm instance can
/// have only one subscriber.
pub fn init_logging(config: LoggingConfig) {
    let subscriber = LogSubscriber::new(config, ConsoleOutput);
    if let Err(e) = tracing::subscriber::set_global_default(subscriber) {
        console_warn(&format!("unable to set the tracing subscriber: {e}"));
    }
}

/// Writes to the console, at the method that matches the level.
struct ConsoleOutput;

impl LogOutput for ConsoleOutput {
    fn write(&self, level: &Level, message: &str) {
        match *level {
            Level::ERROR => console_error(message),
            Level::WARN => console_warn(message),
            Level::INFO => console_log(message),
            Level::DEBUG | Level::TRACE => console_debug(message),
        }
    }

    fn now_ms(&self) -> f64 {
        performance_now()
    }
}
