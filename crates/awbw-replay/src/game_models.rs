use crate::de::{bool_ynstr, values_only};
use awbrn_core::{
    AwbwGameId, AwbwGamePlayerId, AwbwMapId, AwbwPlayerId, AwbwUnitId, PlayerFaction, Terrain, Unit,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct AwbwGame {
    pub id: AwbwGameId,
    pub name: String,
    pub password: Option<String>,
    pub creator: AwbwPlayerId,
    pub start_date: String,
    pub end_date: Option<String>,
    pub activity_date: String,
    pub maps_id: AwbwMapId,
    pub weather_type: String,
    pub weather_start: Option<u32>,
    pub weather_code: String,
    pub win_condition: Option<String>,
    pub turn: u32,
    pub day: u32,
    pub active: String,
    pub funds: u32,
    pub capture_win: u32,
    pub fog: String,
    pub comment: Option<String>,
    #[serde(rename = "type")]
    pub game_type: String,
    pub boot_interval: i32,
    pub starting_funds: u32,
    pub official: String,
    pub min_rating: u32,
    pub max_rating: Option<u32>,
    pub league: Option<String>,
    pub team: String,
    pub aet_interval: i32,
    pub aet_date: String,
    #[serde(deserialize_with = "bool_ynstr")]
    pub use_powers: bool,
    #[serde(deserialize_with = "values_only")]
    pub players: Vec<AwbwPlayer>,
    #[serde(deserialize_with = "values_only")]
    pub buildings: Vec<AwbwBuilding>,
    #[serde(deserialize_with = "values_only")]
    pub units: Vec<AwbwUnit>,
    pub timers_initial: u32,
    pub timers_increment: u32,
    pub timers_max_turn: u32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct AwbwPlayer {
    // Player ID used in the game
    pub id: AwbwPlayerId,

    // Global ID used across all games
    pub users_id: AwbwPlayerId,
    pub games_id: AwbwGameId,

    #[serde(alias = "countries_id", with = "player_faction_id")]
    pub faction: PlayerFaction,
    pub co_id: u32,
    pub funds: u32,
    pub turn: Option<String>,
    pub email: Option<String>,
    pub uniq_id: Option<String>,
    #[serde(deserialize_with = "bool_ynstr")]
    pub eliminated: bool,
    pub last_read: String,
    pub last_read_broadcasts: Option<String>,
    pub emailpress: Option<String>,
    pub signature: Option<String>,
    pub co_power: u32,
    pub co_power_on: CoPower,
    pub order: u32,
    #[serde(deserialize_with = "bool_ynstr")]
    pub accept_draw: bool,
    pub co_max_power: u32,
    pub co_max_spower: u32,
    pub co_image: String,
    pub team: String,
    pub aet_count: u32,
    pub turn_start: String,
    pub turn_clock: u32,
    pub tags_co_id: Option<String>,
    pub tags_co_power: Option<String>,
    pub tags_co_max_power: Option<String>,
    pub tags_co_max_spower: Option<String>,
    #[serde(deserialize_with = "bool_ynstr")]
    pub interface: bool,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct AwbwBuilding {
    pub id: u32,
    pub games_id: u32,
    pub terrain_id: Terrain,
    pub x: u32,
    pub y: u32,
    pub capture: u32,
    pub last_capture: u32,
    pub last_updated: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct AwbwUnit {
    pub id: AwbwUnitId,
    pub games_id: AwbwGameId,
    pub players_id: AwbwGamePlayerId,
    #[serde(with = "crate::de::awbw_unit_name")]
    pub name: Unit,
    pub movement_points: u32,
    pub vision: u32,
    pub fuel: u32,
    pub fuel_per_turn: u32,
    #[serde(deserialize_with = "bool_ynstr")]
    pub sub_dive: bool,
    pub ammo: u32,
    pub short_range: u32,
    pub long_range: u32,
    #[serde(deserialize_with = "bool_ynstr")]
    pub second_weapon: bool,
    pub symbol: String,
    pub cost: u32,
    pub movement_type: String,
    pub x: u32,
    pub y: u32,
    pub moved: u32,
    pub capture: u32,
    pub fired: u32,
    pub hit_points: f64,
    pub cargo1_units_id: AwbwUnitId,
    pub cargo2_units_id: AwbwUnitId,
    #[serde(deserialize_with = "bool_ynstr")]
    pub carried: bool,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]
pub enum CoPower {
    #[serde(rename = "N")]
    None,
    #[serde(rename = "Y")]
    Power,
    #[serde(rename = "S")]
    SuperPower,
}

mod player_faction_id {
    use awbrn_core::PlayerFaction;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(x: &PlayerFaction, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        s.serialize_u8(x.awbw_id().as_u8())
    }

    pub fn deserialize<'de, D>(d: D) -> Result<PlayerFaction, D::Error>
    where
        D: Deserializer<'de>,
    {
        let x = u8::deserialize(d)?;
        PlayerFaction::from_awbw_id(x)
            .ok_or_else(|| serde::de::Error::custom(format!("Invalid faction ID: {}", x)))
    }
}
