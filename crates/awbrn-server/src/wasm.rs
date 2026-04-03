use std::num::NonZeroU8;

use awbrn_map::{AwbrnMap, AwbwMap, AwbwMapData};
use awbrn_types::PlayerFaction;
use serde::{Deserialize, Serialize};
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::{GameServer, GameSetup, PlayerSetup};

#[wasm_bindgen]
pub struct WasmMatch;

#[wasm_bindgen]
impl WasmMatch {
    #[wasm_bindgen(constructor)]
    pub fn new(setup: JsValue) -> Result<Self, JsError> {
        let setup = serde_wasm_bindgen::from_value::<MatchSetupInput>(setup)
            .map_err(|error| invalid_input("setup", error.to_string()))?
            .try_into()
            .map_err(|reason| invalid_input("setup", reason))?;
        GameServer::new(setup).map_err(setup_error)?;
        Ok(Self)
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(dead_code)]
struct MatchSetupInput {
    match_id: Option<String>,
    map_id: Option<u32>,
    map: Option<AwbwMapData>,
    players: Vec<PlayerSetupInput>,
    fog_enabled: bool,
    starting_funds: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PlayerSetupInput {
    faction: Option<PlayerFaction>,
    faction_id: Option<u8>,
    team: Option<NonZeroU8>,
    starting_funds: Option<u32>,
    co_id: Option<u32>,
}

impl MatchSetupInput {
    fn resolve_map(&self) -> Result<&AwbwMapData, String> {
        self.map
            .as_ref()
            .ok_or_else(|| "map payload is required".to_string())
    }
}

impl PlayerSetupInput {
    fn resolve_faction(&self) -> Result<PlayerFaction, String> {
        if let Some(faction) = self.faction {
            return Ok(faction);
        }

        let Some(faction_id) = self.faction_id else {
            return Err("player faction is required".to_string());
        };

        PlayerFaction::from_awbw_id(faction_id)
            .ok_or_else(|| format!("unknown AWBW factionId {faction_id}"))
    }
}

impl TryFrom<MatchSetupInput> for GameSetup {
    type Error = String;

    fn try_from(value: MatchSetupInput) -> Result<Self, Self::Error> {
        let default_starting_funds = value.starting_funds.unwrap_or(0);
        let awbw_map =
            AwbwMap::try_from(value.resolve_map()?).map_err(|error| error.to_string())?;

        Ok(Self {
            map: AwbrnMap::from_map(&awbw_map),
            players: value
                .players
                .into_iter()
                .map(|player| {
                    Ok(PlayerSetup {
                        faction: player.resolve_faction()?,
                        team: player.team,
                        starting_funds: player.starting_funds.unwrap_or(default_starting_funds),
                        co_id: player.co_id,
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
