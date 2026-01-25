use bevy::app::App;
use desktop_plugin::AwbrnDesktopPlugin;

#[cfg(feature = "debug")]
use bevy_inspector_egui::{bevy_egui::EguiPlugin, quick::WorldInspectorPlugin};

mod desktop_plugin;
mod web_asset_plugin;

fn main() {
    let mut app = App::new();
    app.add_plugins(AwbrnDesktopPlugin);

    #[cfg(feature = "debug")]
    app.add_plugins((EguiPlugin::default(), WorldInspectorPlugin::new()));

    #[cfg(feature = "debug")]
    {
        use std::fs::File;
        use std::io::Write;

        // Dump Update schedule graph
        let update_dot = bevy_mod_debugdump::schedule_graph_dot(
            &mut app,
            bevy::app::Update,
            &bevy_mod_debugdump::schedule_graph::Settings::default(),
        );
        File::create("schedule_graph.dot")
            .and_then(|mut f| f.write_all(update_dot.as_bytes()))
            .expect("Failed to write schedule_graph.dot");
        println!("Schedule graph written to schedule_graph.dot");

        // Dump Main schedule graph
        let main_dot = bevy_mod_debugdump::schedule_graph_dot(
            &mut app,
            bevy::app::Main,
            &bevy_mod_debugdump::schedule_graph::Settings::default(),
        );
        File::create("main_schedule_graph.dot")
            .and_then(|mut f| f.write_all(main_dot.as_bytes()))
            .expect("Failed to write main_schedule_graph.dot");
        println!("Main schedule graph written to main_schedule_graph.dot");

        // Dump render graph
        let render_dot = bevy_mod_debugdump::render_graph_dot(
            &app,
            &bevy_mod_debugdump::render_graph::Settings::default(),
        );
        File::create("render_graph.dot")
            .and_then(|mut f| f.write_all(render_dot.as_bytes()))
            .expect("Failed to write render_graph.dot");
        println!("Render graph written to render_graph.dot");
    }

    app.run();
}
