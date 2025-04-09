use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;

#[derive(Debug)]
pub enum TurnElement {
    Int(()),
    Data(Vec<ActionData>),
}

impl<'de> Deserialize<'de> for TurnElement {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ElementVisitor;

        impl<'de> serde::de::Visitor<'de> for ElementVisitor {
            type Value = TurnElement;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("an integer or a data structure")
            }

            fn visit_i64<E>(self, _value: i64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(TurnElement::Int(()))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut elements = Vec::new();
                loop {
                    let _index = match seq.next_element::<u32>()? {
                        Some(elem) => elem,
                        None => return Ok(TurnElement::Data(elements)),
                    };

                    let action: ActionData = match seq.next_element()? {
                        Some(elem) => elem,
                        None => return Ok(TurnElement::Data(elements)),
                    };

                    elements.push(action);
                }
            }
        }

        deserializer.deserialize_any(ElementVisitor)
    }
}

/// Top level structure representing a player's turn
#[derive(Debug, Serialize, Deserialize)]
pub struct PlayerTurn {
    pub player_id: u32,
    pub turn_number: u32,
    pub actions: Vec<(u32, ActionData)>,
}

/// String content representing various action types
#[derive(Debug)]
pub struct ActionData(pub Action);

impl<'de> Deserialize<'de> for ActionData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let action_json: String = Deserialize::deserialize(deserializer)?;
        let action: Action =
            serde_json::from_str(&action_json).map_err(serde::de::Error::custom)?;
        Ok(ActionData(action))
    }
}

impl Serialize for ActionData {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let action_str = serde_json::to_string(&self.0).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&action_str)
    }
}

/// Once deserialized from the ActionData string
#[derive(Debug, Serialize, Deserialize)]
pub struct Action {
    pub action: String,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // pub unit: Option<UnitVisibility>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // pub paths: Option<PathVisibility>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // pub dist: Option<u32>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // pub trapped: Option<bool>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // pub discovered: Option<HashMap<String, Option<String>>>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // #[serde(rename = "Move")]
    // pub move_action: Option<MoveAction>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // #[serde(rename = "Fire")]
    // pub fire_action: Option<FireAction>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // #[serde(rename = "newUnit")]
    // pub new_unit: Option<NewUnitVisibility>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // #[serde(rename = "updatedInfo")]
    // pub updated_info: Option<UpdatedInfo>,
}

/// Represents unit visibility for different players
#[derive(Debug, Serialize, Deserialize)]
pub struct UnitVisibility {
    pub global: HashMap<String, i32>,
    pub players: HashMap<String, HashMap<String, UnitProperty>>,
}

/// Properties of a unit
#[derive(Debug, Serialize, Deserialize)]
pub struct UnitProperty {
    #[serde(rename = "0")]
    pub id: u32,
    pub units_id: u32,
    pub units_games_id: u32,
    pub units_players_id: u32,
    pub units_name: String,
    pub units_movement_points: u32,
    pub units_vision: u32,
    pub units_fuel: u32,
    pub units_fuel_per_turn: u32,
    pub units_sub_dive: String,
    pub units_ammo: u32,
    pub units_short_range: u32,
    pub units_long_range: u32,
    pub units_second_weapon: String,
    pub units_symbol: String,
    pub units_cost: u32,
    pub units_movement_type: String,
    pub units_x: u32,
    pub units_y: u32,
    pub units_moved: u32,
    pub units_capture: u32,
    pub units_fired: u32,
    pub units_hit_points: f64,
    pub units_cargo1_units_id: u32,
    pub units_cargo2_units_id: u32,
    pub units_carried: String,
    pub countries_code: String,
}

/// Path visibility for a move
#[derive(Debug, Serialize, Deserialize)]
pub struct PathVisibility {
    pub global: Vec<PathTile>,
}

/// A tile in a movement path
#[derive(Debug, Serialize, Deserialize)]
pub struct PathTile {
    pub unit_visible: bool,
    pub x: u32,
    pub y: u32,
}

/// Move action specific data
#[derive(Debug, Serialize, Deserialize)]
pub struct MoveAction {
    pub action: String,
    pub unit: UnitVisibility,
    pub paths: PathVisibility,
    pub dist: u32,
    pub trapped: bool,
    pub discovered: HashMap<String, Option<String>>,
}

/// Fire action specific data
#[derive(Debug, Serialize, Deserialize)]
pub struct FireAction {
    pub action: String,
    #[serde(rename = "combatInfoVision")]
    pub combat_info_vision: CombatInfoVision,
    #[serde(rename = "copValues")]
    pub cop_values: CopValues,
}

/// Combat info for a fire action
#[derive(Debug, Serialize, Deserialize)]
pub struct CombatInfoVision {
    pub global: CombatInfoGlobal,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CombatInfoGlobal {
    #[serde(rename = "hasVision")]
    pub has_vision: bool,
    #[serde(rename = "combatInfo")]
    pub combat_info: CombatInfo,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CombatInfo {
    pub attacker: CombatUnit,
    pub defender: CombatUnit,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CombatUnit {
    pub units_ammo: u32,
    pub units_hit_points: f64,
    pub units_id: u32,
    pub units_x: u32,
    pub units_y: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CopValues {
    pub attacker: CopValueInfo,
    pub defender: CopValueInfo,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CopValueInfo {
    #[serde(rename = "playerId")]
    pub player_id: u32,
    #[serde(rename = "copValue")]
    pub cop_value: u32,
    #[serde(rename = "tagValue")]
    pub tag_value: Option<u32>,
}

/// New unit info for a build action
#[derive(Debug, Serialize, Deserialize)]
pub struct NewUnitVisibility {
    pub global: HashMap<String, UnitProperty>,
}

/// Updated info for turn end
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdatedInfo {
    pub event: String,
    #[serde(rename = "nextPId")]
    pub next_player_id: u32,
    #[serde(rename = "nextFunds")]
    pub next_funds: HashMap<String, u32>,
    #[serde(rename = "nextTimer")]
    pub next_timer: u32,
    #[serde(rename = "nextWeather")]
    pub next_weather: String,
    pub supplied: HashMap<String, Vec<String>>,
    pub repaired: HashMap<String, Vec<RepairedUnit>>,
    pub day: u32,
    #[serde(rename = "nextTurnStart")]
    pub next_turn_start: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RepairedUnit {
    pub units_id: String,
    pub units_hit_points: u32,
}
