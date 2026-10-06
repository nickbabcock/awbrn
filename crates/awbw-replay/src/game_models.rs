use crate::de::{bool_ynstr, deserialize_sub_dive, non_negative_u32, values_only};
use awbrn_types::{
    AwbwCoId, AwbwDateTime, AwbwGameId, AwbwGamePlayerId, AwbwMapId, AwbwPlayerId, AwbwTerrain,
    AwbwUnitId, Co, CoExt, PlayerFaction, Unit,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct AwbwGame {
    pub id: AwbwGameId,
    pub name: String,
    pub password: Option<String>,
    pub creator: AwbwPlayerId,
    pub start_date: AwbwDateTime,
    /// AWBW currently sends this field as null.
    pub end_date: Option<AwbwDateTime>,
    pub activity_date: AwbwDateTime,
    pub maps_id: AwbwMapId,
    pub weather_type: String,
    pub weather_start: Option<u32>,
    pub weather_code: String,
    pub win_condition: Option<String>,
    pub turn: u32,
    pub day: u32,
    #[serde(deserialize_with = "bool_ynstr")]
    pub active: bool,
    pub funds: u32,
    pub capture_win: u32,
    #[serde(deserialize_with = "bool_ynstr")]
    pub fog: bool,
    pub comment: Option<String>,
    #[serde(rename = "type")]
    pub game_type: MatchType,
    pub boot_interval: i32,
    pub starting_funds: u32,
    #[serde(deserialize_with = "bool_ynstr")]
    pub official: bool,
    pub min_rating: Option<u32>,
    pub max_rating: Option<u32>,
    pub league: Option<String>,
    #[serde(deserialize_with = "bool_ynstr")]
    pub team: bool,
    pub aet_interval: i32,
    pub aet_date: AwbwDateTime,
    #[serde(deserialize_with = "bool_ynstr")]
    pub use_powers: bool,
    #[serde(deserialize_with = "values_only")]
    pub players: Vec<AwbwPlayer>,
    #[serde(deserialize_with = "values_only")]
    pub buildings: Vec<AwbwBuilding>,
    #[serde(deserialize_with = "values_only")]
    pub units: Vec<AwbwUnit>,
    #[serde(deserialize_with = "non_negative_u32")]
    pub timers_initial: Option<u32>,
    pub timers_increment: u32,
    pub timers_max_turn: u32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct AwbwPlayer {
    pub id: AwbwGamePlayerId,
    pub users_id: AwbwPlayerId,
    pub games_id: AwbwGameId,
    #[serde(alias = "countries_id", with = "player_faction_id")]
    pub faction: PlayerFaction,
    pub co_id: AwbwCoId,
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
    pub co_image: Option<String>,
    pub team: String,
    pub aet_count: u32,
    pub turn_start: String,
    pub turn_clock: u32,
    pub tags_co_id: Option<AwbwCoId>,
    pub tags_co_power: Option<u32>,
    pub tags_co_max_power: Option<u32>,
    pub tags_co_max_spower: Option<u32>,
    #[serde(deserialize_with = "bool_ynstr")]
    pub interface: bool,
}

impl AwbwPlayer {
    pub fn co(&self) -> Option<Co> {
        Co::from_awbw_id(self.co_id)
    }

    pub fn tag_co(&self) -> Option<Co> {
        self.tags_co_id.and_then(Co::from_awbw_id)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct AwbwBuilding {
    pub id: u32,
    pub games_id: u32,
    pub terrain_id: AwbwTerrain,
    pub x: u32,
    pub y: u32,
    /// AWBW overloads this wire field:
    /// capturable buildings use it for capture progress, while pipe seams use
    /// it for seam HP. Internal gameplay state should translate that overload
    /// into separate concepts immediately.
    pub capture: u32,
    pub last_capture: u32,
    pub last_updated: AwbwDateTime,
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
    #[serde(deserialize_with = "deserialize_sub_dive")]
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
    use awbrn_types::PlayerFaction;
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

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum MatchType {
    #[serde(rename = "L")]
    League,
    #[serde(rename = "N")]
    Normal,
    #[serde(rename = "A")]
    Tag,
    #[serde(rename = "V")]
    LiveQueue,
    #[serde(rename = "W")]
    LiveLeague,
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_php_game_records_can_have_a_null_minimum_rating() {
        let bytes = concat!(
            r#"O:8:"awbwGame":36:{"#,
            r#"s:2:"id";i:1598747;"#,
            r#"s:4:"name";s:42:"AC6 D1 [H] R4G01 - (8) uxna vs. (41) DarKz";"#,
            r#"s:8:"password";N;"#,
            r#"s:7:"creator";i:195551;"#,
            r#"s:10:"start_date";s:19:"2026-06-10 18:20:17";"#,
            r#"s:8:"end_date";N;"#,
            r#"s:13:"activity_date";s:19:"2026-06-13 21:34:25";"#,
            r#"s:7:"maps_id";i:153972;"#,
            r#"s:12:"weather_type";s:5:"Clear";"#,
            r#"s:13:"weather_start";N;"#,
            r#"s:12:"weather_code";s:1:"C";"#,
            r#"s:13:"win_condition";N;"#,
            r#"s:4:"turn";i:3842324;"#,
            r#"s:3:"day";i:9;"#,
            r#"s:6:"active";s:1:"Y";"#,
            r#"s:5:"funds";i:1000;"#,
            r#"s:11:"capture_win";i:31;"#,
            r#"s:3:"fog";s:1:"N";"#,
            r#"s:7:"comment";N;"#,
            r#"s:4:"type";s:1:"N";"#,
            r#"s:13:"boot_interval";i:-1;"#,
            r#"s:14:"starting_funds";i:0;"#,
            r#"s:8:"official";s:1:"N";"#,
            r#"s:10:"min_rating";N;"#,
            r#"s:10:"max_rating";N;"#,
            r#"s:6:"league";N;"#,
            r#"s:4:"team";s:1:"N";"#,
            r#"s:12:"aet_interval";i:-1;"#,
            r#"s:8:"aet_date";s:19:"2026-06-13 21:34:25";"#,
            r#"s:10:"use_powers";s:1:"Y";"#,
            r#"s:7:"players";a:0:{}"#,
            r#"s:9:"buildings";a:0:{}"#,
            r#"s:5:"units";a:0:{}"#,
            r#"s:14:"timers_initial";i:7200;"#,
            r#"s:16:"timers_increment";i:2160;"#,
            r#"s:15:"timers_max_turn";i:7200;"#,
            "}",
        )
        .as_bytes();
        let game = AwbwGame::deserialize(&mut phpserz::PhpDeserializer::new(bytes)).unwrap();
        assert_eq!(game.min_rating, None);
        let numeric = String::from_utf8(bytes.to_vec())
            .unwrap()
            .replace("s:10:\"min_rating\";N;", "s:10:\"min_rating\";i:0;");
        let game =
            AwbwGame::deserialize(&mut phpserz::PhpDeserializer::new(numeric.as_bytes())).unwrap();
        assert_eq!(game.min_rating, Some(0));
    }
}
