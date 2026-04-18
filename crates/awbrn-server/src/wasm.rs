use std::num::NonZeroU8;

use awbrn_map::{AwbrnMap, AwbwMap, AwbwMapData};
use awbrn_types::PlayerFaction;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tsify::Tsify;
use wasm_bindgen::prelude::*;

use crate::{CombatOutcome, GameServer, GameSetup, PlayerSetup, StoredActionEvent};
use awbrn_types::{AwbwCoId, Co};

#[wasm_bindgen]
pub struct WasmMatch {
    server: GameServer,
}

#[wasm_bindgen]
impl WasmMatch {
    #[wasm_bindgen(constructor)]
    pub fn new(setup: MatchSetupInput) -> Result<Self, JsError> {
        let setup: GameSetup = setup
            .try_into()
            .map_err(|reason| invalid_input("setup", reason))?;
        let server = GameServer::new(setup).map_err(setup_error)?;
        Ok(Self { server })
    }

    #[wasm_bindgen(js_name = reconstructFromEvents)]
    pub fn reconstruct_from_events(
        setup: MatchSetupInput,
        events: JsValue,
    ) -> Result<Self, JsError> {
        let setup: GameSetup = setup
            .try_into()
            .map_err(|reason| invalid_input("setup", reason))?;
        let events: Vec<crate::StoredActionEvent> = serde_wasm_bindgen::from_value(events)
            .map_err(|error| invalid_input("events", error.to_string()))?;
        let server = crate::reconstruct_from_events(setup, &events).map_err(replay_error)?;
        Ok(Self { server })
    }

    /// Apply a game action submitted by a player.
    /// Returns the requesting player's update and replay event data as a JSON string.
    pub fn process_action(&mut self, player_slot: u8, action: JsValue) -> Result<JsValue, JsError> {
        let action_str = action
            .as_string()
            .ok_or_else(|| invalid_input("action", "expected JSON string".into()))?;

        let command: crate::command::GameCommand = serde_json::from_str(&action_str)
            .map_err(|e| invalid_input("action", e.to_string()))?;
        let stored_command = command.clone();

        let player = crate::player::PlayerId(player_slot);

        let mut result = self
            .server
            .submit_command(player, command)
            .map_err(command_error)?;
        let combat_outcome = result.combat_outcome;

        // Extract only the requesting player's update to avoid leaking other
        // players' fog-of-war reveal/remove diffs.
        let update = result
            .updates
            .iter_mut()
            .find(|(id, _)| *id == player)
            .map(|(_, update)| update as &_)
            .ok_or_else(|| JsError::new("no update for requesting player"))?;

        let response = WasmActionResponse {
            update,
            stored_action_event: StoredActionEvent {
                command: stored_command,
                combat_outcome,
            },
            combat_outcome,
        };
        let output =
            serde_json::to_string(&response).expect("WasmActionResponse should be serializable");
        Ok(JsValue::from_str(&output))
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

#[derive(Serialize)]
struct WasmActionResponse<'a> {
    update: &'a crate::PlayerUpdate,
    stored_action_event: StoredActionEvent,
    #[serde(skip_serializing_if = "Option::is_none")]
    combat_outcome: Option<CombatOutcome>,
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
