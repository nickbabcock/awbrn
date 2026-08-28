use std::collections::BTreeMap;
use std::num::NonZeroU8;

use awbrn_map::{
    AwbrnMap, AwbrnMapDocument, AwbrnMapMetadata, AwbrnMapUnit, AwbwMapData, Pos, PredeployedUnit,
    ValidatedMapDocument,
};
use awbrn_types::{AwbwTerrain, FactionCode, PlayerFaction, Unit, VisualHp};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tsify::{Ts, Tsify};
use wasm_bindgen::prelude::*;

use crate::subscriber::LoggingConfig;
use crate::view::{VisibleTerrain, VisibleUnit};
use crate::{CaptureEvent, PlayerUpdate, PlayerView, SpectatorView};
use crate::{GameServer, GameSetup, PlayerSetup, StoredActionEvent};
use awbrn_types::{AwbwCoId, Co, CoExt};
use awvm::semantic::ObservedTransition;

#[derive(Debug, Clone, Copy, Tsify, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<LogLevel> for tracing::Level {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Error => Self::ERROR,
            LogLevel::Warn => Self::WARN,
            LogLevel::Info => Self::INFO,
            LogLevel::Debug => Self::DEBUG,
            LogLevel::Trace => Self::TRACE,
        }
    }
}

#[derive(Debug, Clone, Copy, Tsify, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoggingOptions {
    pub level: LogLevel,

    /// Writes how long each span was entered. Keep this off in a cloudflare
    /// worker, where the clock does not move between I/O operations.
    #[tsify(optional)]
    #[serde(default)]
    pub span_durations: bool,
}

#[wasm_bindgen(js_name = initLogging)]
pub fn init_logging(options: Ts<LoggingOptions>) -> Result<(), JsError> {
    let options = read_input("options", options)?;
    crate::console_writer::init_logging(LoggingConfig {
        max_level: options.level.into(),
        span_durations: options.span_durations,
    });
    Ok(())
}

/// Normalize and hash an upstream AWBW payload using the canonical Rust map implementation.
#[wasm_bindgen(js_name = importAwbwMapDocument)]
pub fn import_awbw_map_document(
    map_data: Ts<AwbwMapDataWire>,
) -> Result<Ts<ImportedMapDocument>, JsError> {
    let map_data = read_input("mapData", map_data)?;
    let source = AwbwMapData::from(map_data);
    let document = ValidatedMapDocument::try_from(&source)
        .map_err(|error| invalid_input("map", error.to_string()))?;
    let digests = document.digests();

    Ok(ImportedMapDocument {
        document: document.into(),
        content_hash: digests.content_hash.to_string(),
        property_signature: digests.property_signature.to_string(),
        unit_signature: digests.unit_signature.to_string(),
    }
    .into_ts()?)
}

/// Draws map screenshots, holding the decoded sprite atlases.
///
/// The atlases are the expensive part — about 22 ms to decode — so the host
/// builds one of these and keeps it for as long as it wants the atlases in
/// memory. Nothing here is global: an isolate that never draws a map never
/// decodes an atlas.
#[wasm_bindgen]
#[derive(Debug)]
pub struct MapRenderer {
    tilesets: awbrn_image::Tilesets,
}

#[wasm_bindgen]
impl MapRenderer {
    /// Decode the atlases. The host reads the same files the client loads.
    #[wasm_bindgen(constructor)]
    pub fn new(tiles: &[u8], units: &[u8], ui: &[u8], ui_atlas: &[u8]) -> Result<Self, JsError> {
        Ok(Self {
            tilesets: crate::map_image::load_atlases(tiles, units, ui, ui_atlas)
                .map_err(render_error)?,
        })
    }

    /// Draw a map at its starting position and return the PNG bytes.
    #[wasm_bindgen(js_name = renderFull)]
    pub fn render_full(&self, document: Ts<AwbrnMapDocumentWire>) -> Result<Vec<u8>, JsError> {
        let document = read_input("document", document)?;
        crate::map_image::full_screenshot(&self.tilesets, &validated_map(document)?)
            .map_err(render_error)
    }
}

/// Draw a map as a smallmap and return the PNG bytes.
///
/// Four pixels for each tile, terrain only, from a fixed palette. No atlas is
/// read, which is why this is a free function and not a [`MapRenderer`] method.
#[wasm_bindgen(js_name = renderSmallMapScreenshot)]
pub fn render_small_map_screenshot(document: Ts<AwbrnMapDocumentWire>) -> Result<Vec<u8>, JsError> {
    let document = read_input("document", document)?;
    crate::map_image::small_screenshot(&validated_map(document)?).map_err(render_error)
}

fn validated_map(document: AwbrnMapDocumentWire) -> Result<ValidatedMapDocument, JsError> {
    ValidatedMapDocument::try_from(document).map_err(|error| invalid_input("map", error))
}

#[derive(Debug, Tsify, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedMapDocument {
    pub document: AwbrnMapDocumentWire,
    pub content_hash: String,
    pub property_signature: String,
    pub unit_signature: String,
}

#[derive(Debug, Tsify, Serialize, Deserialize)]
pub struct AwbwMapDataWire {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Author")]
    pub author: String,
    #[serde(rename = "Player Count")]
    pub player_count: u32,
    #[serde(rename = "Published Date")]
    pub published_date: String,
    #[serde(rename = "Size X")]
    pub size_x: u32,
    #[serde(rename = "Size Y")]
    pub size_y: u32,
    #[serde(rename = "Terrain Map")]
    #[tsify(type = "number[][]")]
    pub terrain_map: Vec<Vec<AwbwTerrain>>,
    #[serde(rename = "Predeployed Units")]
    pub predeployed_units: Vec<PredeployedUnitWire>,
}

#[derive(Debug, Tsify, Serialize, Deserialize)]
pub struct PredeployedUnitWire {
    #[serde(rename = "Unit ID")]
    pub unit_id: u32,
    #[serde(rename = "Unit X")]
    pub unit_x: u32,
    #[serde(rename = "Unit Y")]
    pub unit_y: u32,
    #[serde(rename = "Unit HP")]
    pub unit_hp: u32,
    #[serde(rename = "Country Code")]
    pub country_code: String,
}

#[derive(Debug, Tsify, Serialize, Deserialize)]
pub struct AwbrnMapDocumentWire {
    pub map_format: u32,
    pub width: u32,
    pub height: u32,
    pub terrain: Vec<u8>,
    pub units: Vec<AwbrnMapUnitWire>,
    pub metadata: AwbrnMapMetadataWire,
}

#[derive(Debug, Tsify, Serialize, Deserialize)]
pub struct AwbrnMapUnitWire {
    pub position: [u8; 2],
    pub unit: String,
    pub faction: String,
    pub hp: u32,
}

#[derive(Debug, Tsify, Serialize, Deserialize)]
pub struct AwbrnMapMetadataWire {
    pub name: String,
    pub author: String,
    pub player_count: u32,
}

impl From<PredeployedUnitWire> for PredeployedUnit {
    fn from(unit: PredeployedUnitWire) -> Self {
        Self {
            unit_id: unit.unit_id,
            unit_x: unit.unit_x,
            unit_y: unit.unit_y,
            unit_hp: unit.unit_hp,
            country_code: unit.country_code,
        }
    }
}

impl From<AwbwMapDataWire> for AwbwMapData {
    fn from(map: AwbwMapDataWire) -> Self {
        Self {
            name: map.name,
            author: map.author,
            player_count: map.player_count,
            published_date: map.published_date,
            size_x: map.size_x,
            size_y: map.size_y,
            terrain_map: map.terrain_map,
            predeployed_units: map
                .predeployed_units
                .into_iter()
                .map(PredeployedUnit::from)
                .collect(),
        }
    }
}

impl From<ValidatedMapDocument> for AwbrnMapDocumentWire {
    fn from(document: ValidatedMapDocument) -> Self {
        document.to_document().into()
    }
}

impl From<AwbrnMapDocument> for AwbrnMapDocumentWire {
    fn from(document: AwbrnMapDocument) -> Self {
        Self {
            map_format: document.map_format,
            width: document.width,
            height: document.height,
            terrain: document
                .terrain
                .into_iter()
                .map(|terrain| terrain.id().get())
                .collect(),
            units: document
                .units
                .into_iter()
                .map(AwbrnMapUnitWire::from)
                .collect(),
            metadata: AwbrnMapMetadataWire {
                name: document.metadata.name,
                author: document.metadata.author,
                player_count: document.metadata.player_count,
            },
        }
    }
}

impl From<AwbrnMapUnit> for AwbrnMapUnitWire {
    fn from(unit: AwbrnMapUnit) -> Self {
        Self {
            position: [unit.position.x, unit.position.y],
            unit: unit.unit.as_str().to_owned(),
            faction: unit.faction.as_str().to_owned(),
            hp: u32::from(unit.hp.get()),
        }
    }
}

impl TryFrom<AwbrnMapDocumentWire> for ValidatedMapDocument {
    type Error = String;

    fn try_from(document: AwbrnMapDocumentWire) -> Result<Self, Self::Error> {
        AwbrnMapDocument::try_from(document)?
            .validate()
            .map_err(|error| error.to_string())
    }
}

impl TryFrom<AwbrnMapDocumentWire> for AwbrnMapDocument {
    type Error = String;

    fn try_from(document: AwbrnMapDocumentWire) -> Result<Self, Self::Error> {
        Ok(Self {
            map_format: document.map_format,
            width: document.width,
            height: document.height,
            terrain: document
                .terrain
                .into_iter()
                .map(|id| AwbwTerrain::try_from(id).map_err(|error| error.to_string()))
                .collect::<Result<_, _>>()?,
            units: document
                .units
                .into_iter()
                .map(AwbrnMapUnit::try_from)
                .collect::<Result<_, _>>()?,
            metadata: AwbrnMapMetadata {
                name: document.metadata.name,
                author: document.metadata.author,
                player_count: document.metadata.player_count,
            },
        })
    }
}

impl TryFrom<AwbrnMapUnitWire> for AwbrnMapUnit {
    type Error = String;

    fn try_from(unit: AwbrnMapUnitWire) -> Result<Self, Self::Error> {
        let kind = Unit::from_id(&unit.unit)
            .ok_or_else(|| format!("unknown unit kind '{}'", unit.unit))?;
        let faction = FactionCode::parse(&unit.faction)
            .ok_or_else(|| format!("unknown faction code '{}'", unit.faction))?;
        let hp = u8::try_from(unit.hp).map_err(|_| format!("unit HP {} is too large", unit.hp))?;

        Ok(Self {
            position: Pos::new(unit.position[0], unit.position[1]),
            unit: kind,
            faction,
            hp: VisualHp::new(hp),
        })
    }
}

#[wasm_bindgen]
#[derive(Debug)]
pub struct WasmMatch {
    server: GameServer,
    fog_enabled: bool,
}

#[wasm_bindgen]
impl WasmMatch {
    #[wasm_bindgen(constructor)]
    pub fn new(setup: Ts<MatchSetupInput>) -> Result<Self, JsError> {
        let setup = read_input("setup", setup)?;
        let fog_enabled = setup.fog_enabled;
        let setup: GameSetup = setup
            .try_into()
            .map_err(|reason| invalid_input("setup", reason))?;
        let server = GameServer::new(setup).map_err(setup_error)?;
        Ok(Self {
            server,
            fog_enabled,
        })
    }

    #[wasm_bindgen(js_name = reconstructFromEvents)]
    pub fn reconstruct_from_events(
        setup: Ts<MatchSetupInput>,
        events: Vec<Ts<StoredActionEvent>>,
    ) -> Result<Self, JsError> {
        let setup = read_input("setup", setup)?;
        let fog_enabled = setup.fog_enabled;
        let setup: GameSetup = setup
            .try_into()
            .map_err(|reason| invalid_input("setup", reason))?;
        let events = events
            .into_iter()
            .map(|event| read_input("events", event))
            .collect::<Result<Vec<crate::StoredActionEvent>, _>>()?;
        let server = crate::reconstruct_from_events(setup, &events).map_err(replay_error)?;
        Ok(Self {
            server,
            fog_enabled,
        })
    }

    /// Apply a game action submitted by a player.
    /// Returns route-ready websocket messages and replay event data.
    pub fn process_action(
        &mut self,
        player_slot: u8,
        command: Ts<crate::GameCommand>,
    ) -> Result<Ts<WasmActionResponse>, JsError> {
        let command = read_input("command", command)?;
        if !self.server.has_player(crate::PlayerId(player_slot)) {
            return Err(invalid_input(
                "player_slot",
                format!("unknown player slot {player_slot}"),
            ));
        }

        let stored_command = command.clone();

        let player = crate::PlayerId(player_slot);

        let result = self
            .server
            .submit_command(player, command)
            .map_err(command_error)?;
        let typed_transitions = result
            .observed_transitions
            .into_iter()
            .map(|(player, transition)| (player.0.to_string(), transition))
            .collect::<BTreeMap<_, _>>();

        let spectator_view = if self.fog_enabled {
            None
        } else {
            Some(self.server.spectator_view())
        };
        let public_players = spectator_view
            .as_ref()
            .map(public_player_states)
            .unwrap_or_default();
        let spectator_message = if self.fog_enabled {
            SpectatorMessage::SpectatorNotice { fog_active: true }
        } else {
            SpectatorMessage::SpectatorState {
                game_state: Box::new(spectator_game_state(
                    spectator_view
                        .as_ref()
                        .expect("non-fog matches should build spectator state"),
                    self.server
                        .spectator_observation()
                        .expect("a live match has a spectator observation"),
                )),
                transition: self
                    .server
                    .spectator_player()
                    .and_then(|player| typed_transitions.get(&player.0.to_string()))
                    .cloned()
                    .map(Box::new),
            }
        };

        let player_messages_by_slot = result
            .updates
            .into_iter()
            .map(|(id, update)| {
                (
                    id.0.to_string(),
                    player_update_message(
                        &update,
                        public_players.clone(),
                        typed_transitions
                            .get(&id.0.to_string())
                            .cloned()
                            .expect("every player update has a typed transition"),
                    ),
                )
            })
            .collect();

        let response = WasmActionResponse {
            player_messages_by_slot,
            stored_action_event: StoredActionEvent {
                player,
                command: stored_command,
                random: self.server.last_random().to_vec(),
            },
            spectator_message,
        };

        Ok(response.into_ts()?)
    }

    /// Return results for a finished non-cancelled match.
    #[wasm_bindgen(js_name = matchResults)]
    pub fn match_results(&self) -> Result<Option<Ts<crate::MatchResults>>, JsError> {
        self.server
            .results()
            .map(|results| results.into_ts())
            .transpose()
            .map_err(write_error)
    }

    #[wasm_bindgen(js_name = playerGameState)]
    pub fn player_game_state(&mut self, player_slot: u8) -> Result<Ts<MatchGameState>, JsError> {
        let player = crate::PlayerId(player_slot);
        if !self.server.has_player(player) {
            return Err(invalid_input(
                "player_slot",
                format!("unknown player slot {player_slot}"),
            ));
        }

        let view = self.server.player_view(player).ok_or_else(|| {
            invalid_input("player_slot", format!("unknown player slot {player_slot}"))
        })?;
        let observation = self.server.player_observation(player).ok_or_else(|| {
            invalid_input("player_slot", format!("unknown player slot {player_slot}"))
        })?;
        player_game_state(&view, player_slot, observation)
            .into_ts()
            .map_err(write_error)
    }

    #[wasm_bindgen(js_name = spectatorGameState)]
    pub fn spectator_game_state(&mut self) -> Result<Ts<SpectatorGameStateResponse>, JsError> {
        let game_state = if self.fog_enabled {
            None
        } else {
            Some(spectator_game_state(
                &self.server.spectator_view(),
                self.server
                    .spectator_observation()
                    .expect("a live match has a spectator observation"),
            ))
        };
        SpectatorGameStateResponse { game_state }
            .into_ts()
            .map_err(write_error)
    }

    pub fn player_view(&mut self, player_slot: u8) -> Result<JsValue, JsError> {
        let player = crate::PlayerId(player_slot);
        if !self.server.has_player(player) {
            return Err(invalid_input(
                "player_slot",
                format!("unknown player slot {player_slot}"),
            ));
        }

        serde_wasm_bindgen::to_value(&self.server.player_view(player).ok_or_else(|| {
            invalid_input("player_slot", format!("unknown player slot {player_slot}"))
        })?)
        .map_err(|error| JsError::new(&error.to_string()))
    }

    pub fn spectator_view(&mut self) -> Result<JsValue, JsError> {
        serde_wasm_bindgen::to_value(&self.server.spectator_view())
            .map_err(|error| JsError::new(&error.to_string()))
    }
}

const WASM_ERROR_PREFIX: &str = "AWBRN_MATCH_ERROR:";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WasmErrorPayload<T>
where
    T: Serialize,
{
    code: &'static str,
    message: String,
    http_status: u16,
    details: T,
}

#[derive(Debug, Tsify, Serialize)]
#[tsify(hashmap_as_object)]
#[serde(rename_all = "camelCase")]
pub struct WasmActionResponse {
    pub player_messages_by_slot: BTreeMap<String, PlayerUpdateMessage>,
    pub stored_action_event: StoredActionEvent,
    pub spectator_message: SpectatorMessage,
}

#[derive(Debug, Tsify, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpectatorGameStateResponse {
    #[tsify(type = "MatchGameState | null")]
    pub game_state: Option<MatchGameState>,
}

#[derive(Debug, Tsify, Serialize)]
#[tsify(hashmap_as_object)]
#[serde(rename_all = "camelCase")]
pub struct MatchGameState {
    #[tsify(type = "number | null")]
    pub viewer_slot_index: Option<u8>,
    pub day: u32,
    pub active_player_slot: u8,
    #[tsify(type = "unknown")]
    pub phase: Value,
    #[tsify(type = "number | null")]
    pub my_funds: Option<u32>,
    pub players: Vec<PublicPlayerState>,
    pub units: Vec<WireVisibleUnit>,
    pub terrain: Vec<WireVisibleTerrain>,
    #[tsify(type = "Observation")]
    pub observation: awvm::semantic::Observation,
}

#[derive(Debug, Tsify, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PublicPlayerState {
    pub slot_index: u8,
    pub funds: u32,
}

#[derive(Debug, Tsify, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireVisibleUnit {
    pub id: u64,
    #[tsify(type = "string")]
    pub unit_type: Value,
    #[tsify(type = "unknown")]
    pub faction: Value,
    pub position: WirePosition,
    #[tsify(type = "number | null")]
    pub hp: Option<u8>,
    #[tsify(type = "number | null")]
    pub fuel: Option<u32>,
    #[tsify(type = "number | null")]
    pub ammo: Option<u32>,
    pub capturing: bool,
    #[tsify(type = "number | null")]
    pub capture_progress: Option<u8>,
    pub hiding: bool,
}

#[derive(Debug, Tsify, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireVisibleTerrain {
    pub position: WirePosition,
    #[tsify(type = "unknown")]
    pub terrain: Value,
}

#[derive(Debug, Tsify, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WirePosition {
    pub x: u8,
    pub y: u8,
}

#[derive(Debug, Tsify, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SpectatorMessage {
    SpectatorNotice {
        fog_active: bool,
    },
    SpectatorState {
        game_state: Box<MatchGameState>,
        transition: Option<Box<ObservedTransition>>,
    },
}

#[derive(Debug, Tsify, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerUpdateMessage {
    #[serde(rename = "type")]
    #[tsify(type = "\"playerUpdate\"")]
    pub message_type: &'static str,
    pub day: u32,
    pub active_player_slot: u8,
    #[tsify(type = "unknown")]
    pub phase: Value,
    pub players: Vec<PublicPlayerState>,
    pub units_revealed: Vec<WireVisibleUnit>,
    pub units_moved: Vec<UnitMovedMessage>,
    pub units_removed: Vec<u64>,
    pub terrain_revealed: Vec<WireVisibleTerrain>,
    pub terrain_changed: Vec<WireVisibleTerrain>,
    pub combat_events: Vec<CombatEventMessage>,
    pub capture_events: Vec<CaptureEventMessage>,
    #[tsify(type = "TurnChangeMessage | null")]
    pub turn_change: Option<TurnChangeMessage>,
    #[tsify(type = "number | null")]
    pub funds_changed: Option<u32>,
    pub transition: ObservedTransition,
}

#[derive(Debug, Tsify, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitMovedMessage {
    pub id: u64,
    pub path: Vec<WirePosition>,
    pub from: WirePosition,
    pub to: WirePosition,
}

#[derive(Debug, Tsify, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CombatEventMessage {
    pub attacker_id: u64,
    pub defender_id: u64,
    #[tsify(type = "number | null")]
    pub attacker_visual_hp_after: Option<u8>,
    #[tsify(type = "number | null")]
    pub defender_visual_hp_after: Option<u8>,
}

#[derive(Debug, Tsify, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnChangeMessage {
    pub new_active_player_slot: u8,
    #[tsify(type = "number | null")]
    pub new_day: Option<u32>,
}

#[derive(Debug, Tsify, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CaptureEventMessage {
    CaptureContinued {
        tile: WirePosition,
        unit_id: u64,
        progress: u8,
    },
    PropertyCaptured {
        tile: WirePosition,
        #[tsify(type = "unknown")]
        new_faction: Value,
    },
}

fn player_game_state(
    view: &PlayerView,
    viewer_slot_index: u8,
    observation: awvm::semantic::Observation,
) -> MatchGameState {
    MatchGameState {
        viewer_slot_index: Some(viewer_slot_index),
        day: view.state.day,
        active_player_slot: view.state.active_player.0,
        phase: serialized_value(&view.state.phase),
        my_funds: Some(view.my_funds),
        players: view
            .players
            .iter()
            .map(|player| PublicPlayerState {
                slot_index: player.slot_index,
                funds: player.funds,
            })
            .collect(),
        units: visible_units(&view.units),
        terrain: visible_terrain(&view.terrain),
        observation,
    }
}

fn spectator_game_state(
    view: &SpectatorView,
    observation: awvm::semantic::Observation,
) -> MatchGameState {
    MatchGameState {
        viewer_slot_index: None,
        day: view.state.day,
        active_player_slot: view.state.active_player.0,
        phase: serialized_value(&view.state.phase),
        my_funds: None,
        players: public_player_states(view),
        units: visible_units(&view.units),
        terrain: visible_terrain(&view.terrain),
        observation,
    }
}

fn public_player_states(view: &SpectatorView) -> Vec<PublicPlayerState> {
    view.players
        .iter()
        .map(|player| PublicPlayerState {
            slot_index: player.slot_index,
            funds: player.funds,
        })
        .collect()
}

fn player_update_message(
    update: &PlayerUpdate,
    players: Vec<PublicPlayerState>,
    transition: ObservedTransition,
) -> PlayerUpdateMessage {
    PlayerUpdateMessage {
        message_type: "playerUpdate",
        day: update.state.day,
        active_player_slot: update.state.active_player.0,
        phase: serialized_value(&update.state.phase),
        players,
        units_revealed: visible_units(&update.units_revealed),
        units_moved: update
            .units_moved
            .iter()
            .map(|unit| UnitMovedMessage {
                id: unit.id.0,
                path: unit.path.iter().map(wire_position).collect(),
                from: wire_position(&unit.from),
                to: wire_position(&unit.to),
            })
            .collect(),
        units_removed: update.units_removed.iter().map(|id| id.0).collect(),
        terrain_revealed: visible_terrain(&update.terrain_revealed),
        terrain_changed: visible_terrain(&update.terrain_changed),
        combat_events: update
            .combat_event
            .as_ref()
            .map(|event| {
                vec![CombatEventMessage {
                    attacker_id: event.attacker_id.0,
                    defender_id: event.defender_id.0,
                    attacker_visual_hp_after: graphical_hp_value(event.attacker_hp_after),
                    defender_visual_hp_after: graphical_hp_value(event.defender_hp_after),
                }]
            })
            .unwrap_or_default(),
        capture_events: update
            .capture_event
            .as_ref()
            .map(|event| vec![capture_event_message(event)])
            .unwrap_or_default(),
        turn_change: update
            .turn_change
            .as_ref()
            .map(|turn_change| TurnChangeMessage {
                new_active_player_slot: turn_change.new_active_player.0,
                new_day: turn_change.new_day,
            }),
        funds_changed: update.my_funds,
        transition,
    }
}

fn visible_units(units: &[VisibleUnit]) -> Vec<WireVisibleUnit> {
    units
        .iter()
        .map(|unit| WireVisibleUnit {
            id: unit.id.0,
            unit_type: serialized_value(&unit.unit_type),
            faction: serialized_value(&unit.faction),
            position: wire_position(&unit.position),
            hp: unit.hp,
            fuel: unit.fuel,
            ammo: unit.ammo,
            capturing: unit.capturing,
            capture_progress: unit.capture_progress,
            hiding: unit.hiding,
        })
        .collect()
}

fn visible_terrain(terrain: &[VisibleTerrain]) -> Vec<WireVisibleTerrain> {
    terrain
        .iter()
        .map(|tile| WireVisibleTerrain {
            position: wire_position(&tile.position),
            terrain: serialized_value(&tile.terrain),
        })
        .collect()
}

fn graphical_hp_value(hp: awbrn_types::GraphicalHp) -> Option<u8> {
    if hp.is_destroyed() {
        None
    } else {
        hp.visible().map(awbrn_types::VisualHp::get)
    }
}

fn capture_event_message(event: &CaptureEvent) -> CaptureEventMessage {
    match event {
        CaptureEvent::CaptureContinued {
            tile,
            unit_id,
            progress,
        } => CaptureEventMessage::CaptureContinued {
            tile: wire_position(tile),
            unit_id: unit_id.0,
            progress: *progress,
        },
        CaptureEvent::PropertyCaptured { tile, new_faction } => {
            CaptureEventMessage::PropertyCaptured {
                tile: wire_position(tile),
                new_faction: serialized_value(new_faction),
            }
        }
    }
}

fn wire_position(position: &awbrn_map::Pos) -> WirePosition {
    WirePosition {
        x: position.x,
        y: position.y,
    }
}

fn serialized_value<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("wire field should serialize")
}

#[derive(Debug, Tsify, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchSetupInput {
    pub map: AwbrnMapDocumentWire,
    pub players: Vec<PlayerSetupInput>,
    pub fog_enabled: bool,
    pub starting_funds: u32,
    #[serde(default)]
    #[tsify(optional)]
    pub rng_seed: Option<u64>,
}

#[derive(Debug, Tsify, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSetupInput {
    pub faction_id: u8,
    #[tsify(type = "number | null")]
    pub team: Option<NonZeroU8>,
    pub starting_funds: u32,
    pub co_id: u32,
}

impl PlayerSetupInput {
    fn resolve_faction(&self) -> Result<PlayerFaction, String> {
        PlayerFaction::from_awbw_id(self.faction_id)
            .ok_or_else(|| format!("unknown AWBW factionId {}", self.faction_id))
    }
}

impl TryFrom<MatchSetupInput> for GameSetup {
    type Error = String;

    fn try_from(value: MatchSetupInput) -> Result<Self, Self::Error> {
        let document = ValidatedMapDocument::try_from(value.map)?;
        let awbw_map = document.into_map();

        Ok(Self {
            // The map carries its own starting units.
            map: AwbrnMap::from_map(&awbw_map),
            players: value
                .players
                .into_iter()
                .map(|player| {
                    let co_id = AwbwCoId::new(player.co_id);
                    let co = Co::from_awbw_id(co_id)
                        .ok_or_else(|| format!("unknown AWBW coId {}", co_id.as_u32()))?;

                    Ok(PlayerSetup {
                        faction: player.resolve_faction()?,
                        team: player.team,
                        starting_funds: player.starting_funds,
                        co,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?,
            fog_enabled: value.fog_enabled,
            rng_seed: value.rng_seed.unwrap_or(0),
        })
    }
}

/// Read one argument JavaScript passed, or report what was wrong with it.
///
/// Every value that crosses the boundary comes from a dynamic language, so a
/// caller can hand over anything at all. `Ts` keeps that a plain rejection:
/// `tsify`'s own `from_wasm_abi` conversion throws out of the argument list
/// instead, which leaks the memory it had already taken and leaves a match
/// object borrowed.
fn read_input<T>(field: &'static str, value: Ts<T>) -> Result<T, JsError>
where
    T: Tsify + serde::de::DeserializeOwned,
    <T as Tsify>::JsType: Clone,
{
    value
        .to_rust()
        .map_err(|error| invalid_input(field, error.to_string()))
}

/// Report a value the engine could not put into JavaScript.
///
/// Nothing a caller does causes this: the value is the engine's own, so a
/// failure here is a bug in what it wrote rather than in what it was given.
fn write_error(error: tsify::Error) -> JsError {
    js_error("internal", error.to_string(), 500, json!({}))
}

fn invalid_input(field: &'static str, reason: String) -> JsError {
    js_error(
        "invalidInput",
        format!("invalid {field}: {reason}"),
        400,
        json!({
            "field": field,
            "reason": reason,
        }),
    )
}

/// A screenshot that could not be drawn. The whole chain is reported: the top
/// of it names the step, and the rest says what the atlas or the encoder said.
fn render_error(error: anyhow::Error) -> JsError {
    js_error("renderFailed", format!("{error:#}"), 500, json!(null))
}

fn command_error(error: crate::CommandError) -> JsError {
    let (code, http_status) = match &error {
        crate::CommandError::NotYourTurn => ("notYourTurn", 403),
        crate::CommandError::GameOver => ("gameOver", 409),
        crate::CommandError::InvalidUnit(_)
        | crate::CommandError::UnitAlreadyActed(_)
        | crate::CommandError::InvalidPath { .. }
        | crate::CommandError::InvalidAction { .. }
        | crate::CommandError::InsufficientFunds { .. }
        | crate::CommandError::InsufficientPower { .. }
        | crate::CommandError::InvalidBuildLocation => ("invalidCommand", 400),
    };
    js_error(code, error.to_string(), http_status, json!(null))
}

fn setup_error(error: crate::SetupError) -> JsError {
    match error {
        crate::SetupError::InvalidPlayers { reason } => js_error(
            "setupError",
            format!("invalid game setup: {reason}"),
            400,
            json!({
                "type": "invalidPlayers",
                "reason": reason,
            }),
        ),
        crate::SetupError::InvalidMap { reason } => js_error(
            "setupError",
            format!("invalid game map: {reason}"),
            400,
            json!({
                "type": "invalidMap",
                "reason": reason,
            }),
        ),
    }
}

fn replay_error(error: crate::ReplayError) -> JsError {
    match error {
        crate::ReplayError::Setup(error) => setup_error(error),
        crate::ReplayError::Event { index, source } => js_error(
            "replayError",
            format!("failed to replay event {index}: {source}"),
            409,
            json!({
                "eventIndex": index,
                "reason": source.to_string(),
            }),
        ),
    }
}

fn js_error(
    code: &'static str,
    message: String,
    http_status: u16,
    details: impl Serialize,
) -> JsError {
    let payload = serde_json::to_string(&WasmErrorPayload {
        code,
        message,
        http_status,
        details,
    })
    .expect("wasm error payload should serialize");

    JsError::new(&format!("{WASM_ERROR_PREFIX}{payload}"))
}
