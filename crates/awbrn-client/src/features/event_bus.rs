use crate::features::camera::{CameraScale, compute_map_dimensions};
use awbrn_game::world::GameMap;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// A resource that receives events of type T.
///
/// Its presence in the world signals that someone is listening for T events.
/// Systems dedicated solely to emitting T can use
/// `run_if(resource_exists::<EventSink<T>>)` to skip work when no listener
/// is registered.
#[derive(Resource)]
pub struct EventSink<T: Send + Sync + 'static>(Arc<dyn Fn(T) + Send + Sync + 'static>);

impl<T: Send + Sync + 'static> EventSink<T> {
    pub fn new(f: impl Fn(T) + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }

    pub fn emit(&self, payload: T) {
        (self.0)(payload);
    }
}

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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct ProductionSite {
    pub x: usize,
    pub y: usize,
    pub facility: awvm::ruleset::Terrain,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct ProductionOption {
    pub unit: awvm::ruleset::UnitKind,
    pub name: String,
    pub cost: u32,
    /// Whether the player's funds reach `cost`. An unaffordable unit is still
    /// listed, priced, and struck through, the way the source game lists it.
    pub affordable: bool,
}

/// The production menu implied by the current board selection.
/// `site: None` tells presentation clients to close any open menu.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct ProductionOptionsChanged {
    pub site: Option<ProductionSite>,
    pub options: Vec<ProductionOption>,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct PlayerRosterStats {
    pub funds: Option<u32>,
    pub unit_count: Option<u32>,
    pub unit_value: Option<u32>,
    pub income: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct PlayerRosterEntry {
    pub player_id: u32,
    pub user_id: u32,
    pub turn_order: u32,
    pub team: Option<String>,
    pub eliminated: bool,
    pub actual_faction_code: String,
    pub actual_faction_name: String,
    pub display_faction_code: String,
    pub display_faction_name: String,
    pub faction_code: String,
    pub faction_name: String,
    pub co_key: Option<String>,
    pub co_name: Option<String>,
    pub tag_co_key: Option<String>,
    pub tag_co_name: Option<String>,
    /// Public CO power charge. `None` when no observation has reported one.
    pub power_charge: Option<u32>,
    /// Current charge required to activate this CO's normal power.
    pub cop_cost: Option<u32>,
    /// Current charge required to activate this CO's super power.
    pub scop_cost: Option<u32>,
    /// Charge one power star is worth, so a meter can be drawn in segments.
    pub power_star_charge: Option<u32>,
    pub stats: PlayerRosterStats,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(target_family = "wasm", derive(tsify::Tsify))]
#[cfg_attr(target_family = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct PlayerRosterSnapshot {
    pub match_id: u32,
    pub map_id: u32,
    pub day: u32,
    pub active_player_id: Option<u32>,
    pub players: Vec<PlayerRosterEntry>,
}

pub(crate) fn emit_map_dimensions(
    game_map: Res<GameMap>,
    camera_scale: Res<CameraScale>,
    sink: Res<EventSink<MapDimensions>>,
) {
    sink.emit(compute_map_dimensions(&game_map, &camera_scale));
}
