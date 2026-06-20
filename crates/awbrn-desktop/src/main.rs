use bevy::app::App;
use desktop_plugin::AwbrnDesktopPlugin;

mod desktop_plugin;
mod web_asset_plugin;

fn main() {
    let mut app = App::new();
    app.add_plugins(AwbrnDesktopPlugin);
    app.run();
}
