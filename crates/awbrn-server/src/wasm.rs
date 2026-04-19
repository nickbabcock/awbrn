use std::collections::BTreeMap;
use std::num::NonZeroU8;

use awbrn_map::{AwbrnMap, AwbwMap, AwbwMapData};
use awbrn_types::PlayerFaction;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tsify::Tsify;
use wasm_bindgen::prelude::*;

use crate::view::{VisibleTerrain, VisibleUnit};
use crate::{CaptureEvent, PlayerUpdate, PlayerView, SpectatorView};
use crate::{CombatOutcome, GameServer, GameSetup, PlayerSetup, StoredActionEvent};
use awbrn_types::{AwbwCoId, Co};

#[wasm_bindgen]
pub struct WasmMatch {
    server: GameServer,
    fog_enabled: bool,
}

#[wasm_bindgen]
impl WasmMatch {
    #[wasm_bindgen(constructor)]
    pub fn new(setup: MatchSetupInput) -> Result<Self, JsError> {
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
        setup: MatchSetupInput,
        events: JsValue,
    ) -> Result<Self, JsError> {
        let fog_enabled = setup.fog_enabled;
        let setup: GameSetup = setup
            .try_into()
            .map_err(|reason| invalid_input("setup", reason))?;
        let events: Vec<crate::StoredActionEvent> = serde_wasm_bindgen::from_value(events)
            .map_err(|error| invalid_input("events", error.to_string()))?;
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
        action: JsValue,
    ) -> Result<WasmActionResponse, JsError> {
        if !self.server.has_player(crate::player::PlayerId(player_slot)) {
            return Err(invalid_input(
                "player_slot",
                format!("unknown player slot {player_slot}"),
            ));
        }

        let command = parse_action(action)?;
        let stored_command = command.clone();

        let player = crate::player::PlayerId(player_slot);

        let result = self
            .server
            .submit_command(player, command)
            .map_err(command_error)?;
        let combat_outcome = result.combat_outcome;

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
                game_state: spectator_game_state(
                    spectator_view
                        .as_ref()
                        .expect("non-fog matches should build spectator state"),
                ),
            }
        };

        let player_messages_by_slot = result
            .updates
            .into_iter()
            .map(|(id, update)| {
                (
                    id.0.to_string(),
                    player_update_message(&update, public_players.clone()),
                )
            })
            .collect();

        Ok(WasmActionResponse {
            player_messages_by_slot,
            stored_action_event: StoredActionEvent {
                command: stored_command,
                combat_outcome,
            },
            spectator_message,
            combat_outcome,
        })
    }

    #[wasm_bindgen(js_name = playerGameState)]
    pub fn player_game_state(&mut self, player_slot: u8) -> Result<MatchGameState, JsError> {
        let player = crate::player::PlayerId(player_slot);
        if !self.server.has_player(player) {
            return Err(invalid_input(
                "player_slot",
                format!("unknown player slot {player_slot}"),
            ));
        }

        Ok(player_game_state(
            &self.server.player_view(player),
            player_slot,
        ))
    }

    #[wasm_bindgen(js_name = spectatorGameState)]
    pub fn spectator_game_state(&mut self) -> Result<SpectatorGameStateResponse, JsError> {
        let game_state = if self.fog_enabled {
            None
        } else {
            Some(spectator_game_state(&self.server.spectator_view()))
        };
        Ok(SpectatorGameStateResponse { game_state })
    }

    pub fn player_view(&mut self, player_slot: u8) -> Result<JsValue, JsError> {
        let player = crate::player::PlayerId(player_slot);
        if !self.server.has_player(player) {
            return Err(invalid_input(
                "player_slot",
                format!("unknown player slot {player_slot}"),
            ));
        }

        serde_wasm_bindgen::to_value(&self.server.player_view(player))
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

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct WasmActionResponse {
    pub player_messages_by_slot: BTreeMap<String, PlayerUpdateMessage>,
    #[tsify(type = "unknown")]
    pub stored_action_event: StoredActionEvent,
    pub spectator_message: SpectatorMessage,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "unknown")]
    pub combat_outcome: Option<CombatOutcome>,
}

fn parse_action(action: JsValue) -> Result<crate::command::GameCommand, JsError> {
    if let Some(action_str) = action.as_string() {
        serde_json::from_str(&action_str).map_err(|e| invalid_input("action", e.to_string()))
    } else {
        serde_wasm_bindgen::from_value(action).map_err(|e| invalid_input("action", e.to_string()))
    }
}

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct SpectatorGameStateResponse {
    #[tsify(type = "MatchGameState | null")]
    pub game_state: Option<MatchGameState>,
}

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
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
}

#[derive(Tsify, Serialize, Clone)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct PublicPlayerState {
    pub slot_index: u8,
    pub funds: u32,
}

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
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

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct WireVisibleTerrain {
    pub position: WirePosition,
    #[tsify(type = "unknown")]
    pub terrain: Value,
}

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct WirePosition {
    pub x: usize,
    pub y: usize,
}

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SpectatorMessage {
    SpectatorNotice { fog_active: bool },
    SpectatorState { game_state: MatchGameState },
}

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
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
}

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct UnitMovedMessage {
    pub id: u64,
    pub path: Vec<WirePosition>,
    pub from: WirePosition,
    pub to: WirePosition,
}

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct CombatEventMessage {
    pub attacker_id: u64,
    pub defender_id: u64,
    #[tsify(type = "number | null")]
    pub attacker_visual_hp_after: Option<u8>,
    #[tsify(type = "number | null")]
    pub defender_visual_hp_after: Option<u8>,
}

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct TurnChangeMessage {
    pub new_active_player_slot: u8,
    #[tsify(type = "number | null")]
    pub new_day: Option<u32>,
}

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
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

fn player_game_state(view: &PlayerView, viewer_slot_index: u8) -> MatchGameState {
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
    }
}

fn spectator_game_state(view: &SpectatorView) -> MatchGameState {
    MatchGameState {
        viewer_slot_index: None,
        day: view.state.day,
        active_player_slot: view.state.active_player.0,
        phase: serialized_value(&view.state.phase),
        my_funds: None,
        players: public_player_states(view),
        units: visible_units(&view.units),
        terrain: visible_terrain(&view.terrain),
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
            hp: Some(unit.hp),
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

fn graphical_hp_value(hp: awbrn_game::world::GraphicalHp) -> Option<u8> {
    if hp.is_destroyed() {
        None
    } else {
        Some(hp.value())
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

fn wire_position(position: &awbrn_map::Position) -> WirePosition {
    WirePosition {
        x: position.x,
        y: position.y,
    }
}

fn serialized_value<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("wire field should serialize")
}

#[derive(Tsify, Deserialize)]
#[tsify(from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct MatchSetupInput {
    #[tsify(type = "unknown")]
    pub map: AwbwMapData,
    pub players: Vec<PlayerSetupInput>,
    pub fog_enabled: bool,
    pub starting_funds: u32,
    #[serde(default)]
    #[tsify(optional)]
    pub rng_seed: Option<u64>,
}

#[derive(Tsify, Deserialize)]
#[tsify(from_wasm_abi)]
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
        let awbw_map = AwbwMap::try_from(&value.map).map_err(|error| error.to_string())?;

        Ok(Self {
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

fn command_error(error: crate::error::CommandError) -> JsError {
    let (code, http_status) = match &error {
        crate::error::CommandError::NotYourTurn => ("notYourTurn", 403),
        crate::error::CommandError::GameOver => ("gameOver", 409),
        crate::error::CommandError::InvalidUnit(_)
        | crate::error::CommandError::UnitAlreadyActed(_)
        | crate::error::CommandError::InvalidPath { .. }
        | crate::error::CommandError::InvalidAction { .. }
        | crate::error::CommandError::InsufficientFunds { .. }
        | crate::error::CommandError::InvalidBuildLocation => ("invalidCommand", 400),
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
