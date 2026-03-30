use crate::web_asset_plugin::WebMapAssetPathResolver;
use awbrn_client::{AwbrnPlugin, EventBus, ExternalEvent, GameEvent, ReplayToLoad};
use bevy::{asset::AssetMetaCheck, prelude::*};
use std::{fs, sync::Arc};

/// Desktop EventBus implementation that logs events to console
pub struct DesktopEventBus;

impl EventBus<GameEvent> for DesktopEventBus {
    fn publish_event(&self, event: &ExternalEvent<GameEvent>) {
        match &event.payload {
            GameEvent::NewDay(day_event) => {
                info!("🌅 New Day: Day {}", day_event.day);
            }
            GameEvent::UnitMoved(move_event) => {
                info!(
                    "🚶 Unit {} moved from ({}, {}) to ({}, {})",
                    move_event.unit_id,
                    move_event.from_x,
                    move_event.from_y,
                    move_event.to_x,
                    move_event.to_y
                );
            }
            GameEvent::UnitBuilt(build_event) => {
                info!(
                    "🏗️ Unit {} ({}) built at ({}, {}) by player {}",
                    build_event.unit_id,
                    build_event.unit_type,
                    build_event.x,
                    build_event.y,
                    build_event.player_id
                );
            }
            GameEvent::TileSelected(select_event) => {
                info!(
                    "👆 Tile selected at ({}, {}) - terrain: {}",
                    select_event.x, select_event.y, select_event.terrain_type
                );
            }
            GameEvent::MapDimensions(dims) => {
                info!("Map dimensions: {}x{}", dims.width, dims.height);
            }
            GameEvent::ReplayLoaded(replay) => {
                info!(
                    "Replay loaded: game {} map {} with {} players",
                    replay.game_id,
                    replay.map_id,
                    replay.players.len()
                );
            }
            GameEvent::PlayerRosterUpdated(_) => {}
        }
    }
}

pub struct AwbrnDesktopPlugin;

impl Plugin for AwbrnDesktopPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(
            DefaultPlugins
                .set(ImagePlugin::default_nearest())
                .set(AssetPlugin {
                    file_path: String::from("../../assets"),
                    meta_check: AssetMetaCheck::Never,
                    ..AssetPlugin::default()
                }),
        )
        .add_plugins(
            AwbrnPlugin::new(Arc::new(WebMapAssetPathResolver))
                .with_event_bus(Arc::new(DesktopEventBus)),
        )
        .add_systems(Update, handle_file_drop);
    }
}

fn handle_file_drop(mut commands: Commands, mut file_drop_events: MessageReader<FileDragAndDrop>) {
    for event in file_drop_events.read() {
        let FileDragAndDrop::DroppedFile { path_buf, .. } = event else {
            continue;
        };

        let data = match fs::read(path_buf) {
            Ok(data) => data,
            Err(e) => {
                error!("Failed to read file: {:?}", e);
                continue;
            }
        };

        // Signal that a new replay should be loaded (parsing will happen in Bevy)
        commands.insert_resource(ReplayToLoad(data));
    }
}
