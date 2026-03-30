use crate::features::camera::CameraScale;
use awbrn_game::world::GameMap;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub trait EventBus<T: Serialize + Send + Sync + 'static>: Send + Sync {
    /// Publish an event to the bus
    fn publish_event(&self, payload: &ExternalEvent<T>);
}

#[derive(Resource)]
pub struct EventBusResource<T>(pub Arc<dyn EventBus<T>>);

impl<T> EventBusResource<T> {
    pub fn new(bus: Arc<dyn EventBus<T>>) -> Self {
        Self(bus)
    }
}

#[derive(Message, Debug, Clone)]
pub struct ExternalEvent<T: Serialize + Send + Sync + 'static> {
    pub payload: T,
}

pub fn event_forwarder<T: Serialize + Send + Sync + 'static>(
    mut events: MessageReader<ExternalEvent<T>>,
    bus: Option<Res<EventBusResource<T>>>,
) {
    let Some(bus) = bus else { return };

    for event in events.read() {
        bus.0.publish_event(event);
    }
}

/// Type alias for external events containing game events
pub type ExternalGameEvent = ExternalEvent<GameEvent>;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct NewDay {
    pub day: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct UnitMoved {
    pub unit_id: u32,
    pub from_x: usize,
    pub from_y: usize,
    pub to_x: usize,
    pub to_y: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct UnitBuilt {
    pub unit_id: u32,
    pub unit_type: String,
    pub x: usize,
    pub y: usize,
    pub player_id: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct TileSelected {
    pub x: usize,
    pub y: usize,
    pub terrain_type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct MapDimensions {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct ReplayLoadedPlayer {
    pub player_id: u32,
    pub user_id: u32,
    pub order: u32,
    pub team: Option<String>,
    pub eliminated: bool,
    pub faction_code: String,
    pub faction_name: String,
    pub co_key: Option<String>,
    pub co_name: Option<String>,
    pub tag_co_key: Option<String>,
    pub tag_co_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct ReplayLoaded {
    pub game_id: u32,
    pub map_id: u32,
    pub day: u32,
    pub fog: bool,
    pub team_game: bool,
    pub players: Vec<ReplayLoadedPlayer>,
}

/// Union type for all game events that can be sent to external systems
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(tag = "type")]
pub enum GameEvent {
    NewDay(NewDay),
    UnitMoved(UnitMoved),
    UnitBuilt(UnitBuilt),
    TileSelected(TileSelected),
    MapDimensions(MapDimensions),
    ReplayLoaded(ReplayLoaded),
}

pub(crate) fn emit_map_dimensions(
    game_map: Res<GameMap>,
    camera_scale: Res<CameraScale>,
    mut event_writer: MessageWriter<ExternalGameEvent>,
) {
    let dims = crate::features::camera::compute_map_dimensions(&game_map, &camera_scale);
    event_writer.write(ExternalGameEvent {
        payload: GameEvent::MapDimensions(dims),
    });
}

pub struct EventBusPlugin {
    event_bus: Arc<dyn EventBus<GameEvent>>,
}

impl EventBusPlugin {
    pub fn new(event_bus: Arc<dyn EventBus<GameEvent>>) -> Self {
        Self { event_bus }
    }
}

impl Plugin for EventBusPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(EventBusResource(self.event_bus.clone()))
            .add_systems(Update, event_forwarder::<GameEvent>);
    }
}
