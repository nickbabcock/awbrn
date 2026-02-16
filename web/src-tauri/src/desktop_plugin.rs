use crate::web_asset_plugin::{WebAssetPlugin, WebMapAssetPathResolver};
use awbrn_bevy::{AwbrnPlugin, EventBus, ExternalEvent, GameEvent};
use bevy::{
    app::Plugin,
    asset::AssetMetaCheck,
    prelude::{
        Added, App, AssetPlugin, Commands, DefaultPlugins, Entity, ImagePlugin, NonSend,
        PluginGroup, Query, Window,
    },
    window::{RawHandleWrapper, WindowPlugin, WindowWrapper},
};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

#[derive(Clone)]
pub struct TauriEventBus {
    app_handle: AppHandle,
    window_label: String,
}

impl TauriEventBus {
    pub fn new(app_handle: AppHandle, window_label: String) -> Self {
        Self {
            app_handle,
            window_label,
        }
    }
}

impl EventBus<GameEvent> for TauriEventBus {
    fn publish_event(&self, event: &ExternalEvent<GameEvent>) {
        let Some(window) = self.app_handle.get_webview_window(&self.window_label) else {
            return;
        };

        if let Err(error) = window.emit("game-event", &event.payload) {
            log::error!("failed to emit game event: {error}");
        }
    }
}

pub struct AwbrnDesktopPlugin {
    window: WebviewWindow,
    event_bus: Arc<TauriEventBus>,
}

impl AwbrnDesktopPlugin {
    pub fn new(app_handle: AppHandle, window: WebviewWindow, window_label: String) -> Self {
        Self {
            event_bus: Arc::new(TauriEventBus::new(app_handle, window_label)),
            window,
        }
    }
}

struct NativeWindowHandle(WebviewWindow);

impl Plugin for AwbrnDesktopPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(WebAssetPlugin)
            .add_plugins(
                DefaultPlugins
                    .set(WindowPlugin {
                        primary_window: Some(Window::default()),
                        exit_condition: bevy::window::ExitCondition::DontExit,
                        ..Default::default()
                    })
                    .set(ImagePlugin::default_nearest())
                    .set(AssetPlugin {
                        file_path: String::from("../../assets"),
                        meta_check: AssetMetaCheck::Never,
                        ..AssetPlugin::default()
                    }),
            )
            .add_plugins(
                AwbrnPlugin::new(Arc::new(WebMapAssetPathResolver))
                    .with_event_bus(self.event_bus.clone()),
            )
            .insert_non_send_resource(NativeWindowHandle(self.window.clone()))
            .add_systems(bevy::app::PreStartup, setup_added_window);
    }
}

fn setup_added_window(
    mut commands: Commands,
    window: NonSend<NativeWindowHandle>,
    mut new_windows: Query<Entity, Added<Window>>,
) {
    let Some(entity) = new_windows.iter_mut().next() else {
        return;
    };

    let handle = RawHandleWrapper::new(&WindowWrapper::new(window.0.clone())).expect(
        "to create desktop raw handle wrapper. If this fails, multiple threads are trying to access the same window",
    );

    commands.entity(entity).insert(handle);
}
