use bevy::app::App;
use desktop_plugin::AwbrnDesktopPlugin;

#[cfg(feature = "debug-inspector")]
use bevy_inspector_egui::{bevy_egui::EguiPlugin, quick::WorldInspectorPlugin};

mod desktop_plugin;
mod web_asset_plugin;

fn main() {
    let mut app = App::new();
    app.add_plugins(AwbrnDesktopPlugin);

    #[cfg(feature = "debug-inspector")]
    app.add_plugins((EguiPlugin::default(), WorldInspectorPlugin::new()));

    app.run();
}
