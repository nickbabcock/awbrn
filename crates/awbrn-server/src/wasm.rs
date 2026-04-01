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
struct MatchSetupInput {
    map: AwbwMapData,
    players: Vec<PlayerSetupInput>,
    fog_enabled: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PlayerSetupInput {
    faction: PlayerFaction,
    team: Option<NonZeroU8>,
    starting_funds: u32,
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
                .map(|player| PlayerSetup {
                    faction: player.faction,
                    team: player.team,
                    starting_funds: player.starting_funds,
                })
                .collect(),
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
