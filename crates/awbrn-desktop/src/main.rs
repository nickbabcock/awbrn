use bevy::app::App;
use desktop_plugin::AwbrnDesktopPlugin;

mod desktop_plugin;
mod web_asset_plugin;

fn main() {
    App::new().add_plugins(AwbrnDesktopPlugin).run();
}
