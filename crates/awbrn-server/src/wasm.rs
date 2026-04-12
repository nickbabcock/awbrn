use std::num::NonZeroU8;

use awbrn_map::{AwbrnMap, AwbwMap, AwbwMapData};
use awbrn_types::PlayerFaction;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tsify::Tsify;
use wasm_bindgen::prelude::*;

use crate::{GameServer, GameSetup, PlayerSetup};

#[wasm_bindgen]
pub struct WasmMatch {
    #[allow(dead_code)]
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

    /// Stub: future implementation will validate and apply game actions from WebSocket messages.
    pub fn process_action(&mut self, _action: JsValue) -> Result<JsValue, JsError> {
        Err(JsError::new("game actions are not yet implemented"))
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

#[derive(Tsify, Deserialize)]
#[tsify(from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct MatchSetupInput {
    #[tsify(type = "unknown")]
    pub map: AwbwMapData,
    pub players: Vec<PlayerSetupInput>,
    pub fog_enabled: bool,
    pub starting_funds: u32,
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
                    Ok(PlayerSetup {
                        faction: player.resolve_faction()?,
                        team: player.team,
                        starting_funds: player.starting_funds,
                        co_id: Some(player.co_id),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?,
            fog_enabled: value.fog_enabled,
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
