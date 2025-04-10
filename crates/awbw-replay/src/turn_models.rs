use crate::de::{Hidden, Masked};
use serde::{Deserialize, Deserializer, Serialize};

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

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum Action {
    AttackSeam {
        #[serde(rename = "Move", deserialize_with = "empty_field_action")]
        move_action: Option<MoveAction>,

        #[serde(rename = "AttackSeam")]
        attack_seam_action: AttackSeamAction,
    },
    Build {
        #[serde(rename = "newUnit")]
        new_unit: UnitMap,
        discovered: indexmap::IndexMap<String, Option<Discovery>>,
    },
    Capt {
        #[serde(rename = "Move", deserialize_with = "empty_field_action")]
        move_action: Option<MoveAction>,

        #[serde(rename = "Capt")]
        capture_action: CaptureAction,
    },
    End {
        #[serde(rename = "updatedInfo")]
        updated_info: UpdatedInfo,
    },
    Fire {
        #[serde(rename = "Move", deserialize_with = "empty_field_action")]
        move_action: Option<MoveAction>,

        #[serde(rename = "Fire")]
        fire_action: FireAction,
    },
    Join {
        #[serde(rename = "Move")]
        move_action: MoveAction,

        #[serde(rename = "Join")]
        join_action: JoinAction,
    },
    Load {
        #[serde(rename = "Move")]
        move_action: MoveAction,

        #[serde(rename = "Load")]
        load_action: LoadAction,
    },
    Move(MoveAction),
    Power(PowerAction),
    Resign {
        #[serde(rename = "Resign")]
        resign_action: ResignAction,

        #[serde(rename = "NextTurn")]
        next_turn_action: Option<NextTurnAction>,

        #[serde(rename = "GameOver")]
        game_over_action: Option<GameOverAction>,
    },
    Supply {
        #[serde(rename = "Move")]
        move_action: MoveAction,

        #[serde(rename = "Supply")]
        supply_action: SupplyAction,
    },
    Unload {
        unit: UnitMap,
        #[serde(rename = "transportID")]
        transport_id: u32,
        discovered: indexmap::IndexMap<String, Option<Discovery>>,
    },
}

pub type UnitMap = indexmap::IndexMap<String, Hidden<UnitProperty>>;

#[derive(Debug, Serialize, Deserialize)]
pub struct UnitProperty {
    pub units_id: u32,
    pub units_games_id: Option<u32>,
    pub units_players_id: u32,
    pub units_name: String,
    pub units_movement_points: Option<u32>,
    pub units_vision: Option<u32>,
    pub units_fuel: Option<u32>,
    pub units_fuel_per_turn: Option<u32>,
    pub units_sub_dive: String,
    pub units_ammo: Option<u32>,
    pub units_short_range: Option<u32>,
    pub units_long_range: Option<u32>,
    pub units_second_weapon: Option<String>,
    pub units_symbol: Option<String>,
    pub units_cost: Option<u32>,
    pub units_movement_type: String,
    pub units_x: Option<u32>,
    pub units_y: Option<u32>,
    pub units_moved: Option<u32>,
    pub units_capture: Option<u32>,
    pub units_fired: Option<u32>,
    pub units_hit_points: f64,
    #[serde(default)]
    pub units_cargo1_units_id: Masked<u32>,
    #[serde(default)]
    pub units_cargo2_units_id: Masked<u32>,
    pub units_carried: Option<String>,
    pub countries_code: String,
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
    pub unit: UnitMap,
    pub paths: indexmap::IndexMap<String, Vec<PathTile>>,
    pub dist: u32,
    pub trapped: bool,
    #[serde(deserialize_with = "empty_field_action")]
    pub discovered: Option<indexmap::IndexMap<String, Option<Discovery>>>,
}

fn empty_field_action<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct FieldVisitor<T> {
        marker: std::marker::PhantomData<T>,
    }

    impl<'de, T> serde::de::Visitor<'de> for FieldVisitor<T>
    where
        T: Deserialize<'de>,
    {
        type Value = Option<T>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a sequence or structure")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
                // Skip elements
            }

            Ok(None)
        }

        fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::MapAccess<'de>,
        {
            let deser = serde::de::value::MapAccessDeserializer::new(map);
            let result = T::deserialize(deser)?;
            Ok(Some(result))
        }
    }

    deserializer.deserialize_any(FieldVisitor {
        marker: std::marker::PhantomData,
    })
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoadAction {
    pub loaded: indexmap::IndexMap<String, Hidden<u32>>,
    pub transport: indexmap::IndexMap<String, Hidden<u32>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CaptureAction {
    #[serde(rename = "buildingInfo")]
    pub building_info: BuildingInfo,
    pub vision: indexmap::IndexMap<String, BuildingVision>,
    pub income: Option<indexmap::IndexMap<String, PlayerIncome>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlayerIncome {
    pub player: u32,
    pub income: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BuildingInfo {
    pub buildings_capture: i32,
    pub buildings_id: u32,
    pub buildings_x: u32,
    pub buildings_y: u32,
    pub buildings_team: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BuildingVision {
    #[serde(rename = "onCapture")]
    pub on_capture: Masked<Coordinate>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Coordinate {
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JoinAction {
    #[serde(rename = "playerId")]
    pub player_id: u32,
    #[serde(rename = "newFunds")]
    pub new_funds: indexmap::IndexMap<String, u32>,
    pub unit: UnitMap,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SupplyAction {
    pub unit: indexmap::IndexMap<String, Hidden<u32>>,
    pub rows: Vec<String>,
    pub supplied: indexmap::IndexMap<String, Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AttackSeamAction {
    pub unit: indexmap::IndexMap<String, AttackSeamCombat>,
    pub buildings_hit_points: i32,
    pub buildings_terrain_id: u32,
    #[serde(rename = "seamX")]
    pub seam_x: u32,
    #[serde(rename = "seamY")]
    pub seam_y: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AttackSeamCombat {
    #[serde(rename = "hasVision")]
    pub has_vision: bool,
    #[serde(rename = "combatInfo")]
    pub combat_info: CombatUnit,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PowerAction {
    #[serde(rename = "playerID")]
    pub player_id: u32,
    #[serde(rename = "coName")]
    pub co_name: String,
    #[serde(rename = "coPower")]
    pub co_power: String,
    #[serde(rename = "powerName")]
    pub power_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResignAction {
    #[serde(rename = "playerId")]
    pub player_id: u32,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NextTurnAction {
    #[serde(rename = "nextPId")]
    pub next_player_id: u32,
    #[serde(rename = "nextFunds")]
    pub next_funds: indexmap::IndexMap<String, Hidden<u32>>,
    #[serde(rename = "nextTimer")]
    pub next_timer: u32,
    #[serde(rename = "nextWeather")]
    pub next_weather: String,
    pub supplied: Option<indexmap::IndexMap<String, Vec<String>>>,
    pub repaired: Option<indexmap::IndexMap<String, Vec<RepairedUnit>>>,
    pub day: u32,
    #[serde(rename = "nextTurnStart")]
    pub next_turn_start: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GameOverAction {
    pub day: u32,
    #[serde(rename = "gameEndDate")]
    pub game_end_date: String,
    pub losers: Vec<u32>,
    pub message: String,
    pub winners: Vec<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FireAction {
    #[serde(rename = "combatInfoVision")]
    pub combat_info_vision: indexmap::IndexMap<String, CombatInfoVision>,
    #[serde(rename = "copValues")]
    pub cop_values: CopValues,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CombatInfoVision {
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
    pub units_hit_points: Option<f64>,
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

/// Updated info for turn end
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdatedInfo {
    pub event: String,
    #[serde(rename = "nextPId")]
    pub next_player_id: u32,
    #[serde(rename = "nextFunds")]
    pub next_funds: indexmap::IndexMap<String, Hidden<u32>>,
    #[serde(rename = "nextTimer")]
    pub next_timer: u32,
    #[serde(rename = "nextWeather")]
    pub next_weather: String,
    pub supplied: Option<indexmap::IndexMap<String, Vec<String>>>,
    pub repaired: Option<indexmap::IndexMap<String, Vec<RepairedUnit>>>,
    pub day: u32,
    #[serde(rename = "nextTurnStart")]
    pub next_turn_start: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RepairedUnit {
    pub units_id: String,
    pub units_hit_points: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Discovery {
    #[serde(default)]
    pub buildings: Vec<BuildingDiscovery>,
    #[serde(default)]
    pub units: Vec<UnitProperty>,
}

/// Complete building details including terrain information
#[derive(Debug, Serialize, Deserialize)]
pub struct BuildingDiscovery {
    #[serde(rename = "0")]
    pub id: u32,
    pub buildings_id: u32,
    pub buildings_x: u32,
    pub buildings_y: u32,
    pub buildings_capture: i32,
    pub terrain_id: u32,
    pub terrain_name: String,
    pub terrain_defense: u32,
    pub is_occupied: bool,
    pub buildings_players_id: Option<u32>,
    pub buildings_team: Option<String>,
}
