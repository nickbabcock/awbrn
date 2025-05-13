use crate::de::{Hidden, Masked};
use awbrn_core::{AwbwGamePlayerId, AwbwUnitId, PlayerFaction, Terrain, Unit};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

#[derive(Debug, PartialEq, Clone)]
pub enum TurnElement<'a> {
    Int(()),
    Data(Vec<&'a [u8]>),
}

impl<'de> Deserialize<'de> for TurnElement<'de> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ElementVisitor;

        impl<'de> serde::de::Visitor<'de> for ElementVisitor {
            type Value = TurnElement<'de>;

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

                    let data: &'de [u8] = seq
                        .next_element()?
                        .ok_or_else(|| serde::de::Error::custom("Expected action data"))?;

                    elements.push(data);
                }
            }
        }

        deserializer.deserialize_any(ElementVisitor)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
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
        discovered: indexmap::IndexMap<TargetedPlayer, Option<Discovery>>,
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
    Repair {
        #[serde(rename = "Move")]
        move_action: MoveAction,

        #[serde(rename = "Repair")]
        repair_action: RepairAction,
    },
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
        discovered: indexmap::IndexMap<TargetedPlayer, Option<Discovery>>,
    },
}

pub type UnitMap = indexmap::IndexMap<TargetedPlayer, Hidden<UnitProperty>>;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct UnitProperty {
    pub units_id: u32,
    pub units_games_id: Option<u32>,
    pub units_players_id: u32,
    #[serde(with = "crate::de::awbw_unit_name")]
    pub units_name: Unit,
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
    #[serde(with = "awbw_country_code")]
    pub countries_code: PlayerFaction,
}

/// A tile in a movement path
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct PathTile {
    pub unit_visible: bool,
    pub x: u32,
    pub y: u32,
}

/// Move action specific data
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct MoveAction {
    pub unit: UnitMap,
    pub paths: indexmap::IndexMap<TargetedPlayer, Vec<PathTile>>,
    pub dist: u32,
    pub trapped: bool,
    #[serde(deserialize_with = "empty_field_action")]
    pub discovered: Option<indexmap::IndexMap<TargetedPlayer, Option<Discovery>>>,
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

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct LoadAction {
    pub loaded: indexmap::IndexMap<TargetedPlayer, Hidden<u32>>,
    pub transport: indexmap::IndexMap<TargetedPlayer, Hidden<u32>>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct CaptureAction {
    #[serde(rename = "buildingInfo")]
    pub building_info: BuildingInfo,
    pub vision: indexmap::IndexMap<TargetedPlayer, BuildingVision>,
    pub income: Option<indexmap::IndexMap<TargetedPlayer, PlayerIncome>>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct PlayerIncome {
    pub player: AwbwGamePlayerId,
    pub income: u32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct BuildingInfo {
    pub buildings_capture: i32,
    pub buildings_id: u32,
    pub buildings_x: u32,
    pub buildings_y: u32,
    pub buildings_team: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct BuildingVision {
    #[serde(rename = "onCapture")]
    pub on_capture: Masked<Coordinate>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Coordinate {
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct JoinAction {
    #[serde(rename = "playerId")]
    pub player_id: u32,
    #[serde(rename = "newFunds")]
    pub new_funds: indexmap::IndexMap<TargetedPlayer, u32>,
    pub unit: UnitMap,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct SupplyAction {
    pub unit: indexmap::IndexMap<TargetedPlayer, Hidden<u32>>,
    pub rows: Vec<String>,
    pub supplied: indexmap::IndexMap<TargetedPlayer, Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct RepairAction {
    pub unit: indexmap::IndexMap<TargetedPlayer, Hidden<u32>>,
    pub repaired: indexmap::IndexMap<TargetedPlayer, RepairedUnit2>,
    pub funds: indexmap::IndexMap<TargetedPlayer, u32>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct AttackSeamAction {
    pub unit: indexmap::IndexMap<TargetedPlayer, AttackSeamCombat>,
    pub buildings_hit_points: i32,
    pub buildings_terrain_id: u32,
    #[serde(rename = "seamX")]
    pub seam_x: u32,
    #[serde(rename = "seamY")]
    pub seam_y: u32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct AttackSeamCombat {
    #[serde(rename = "hasVision")]
    pub has_vision: bool,
    #[serde(rename = "combatInfo")]
    pub combat_info: CombatUnit,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct PowerAction {
    #[serde(rename = "playerID")]
    pub player_id: AwbwGamePlayerId,
    #[serde(rename = "coName")]
    pub co_name: String,
    #[serde(rename = "coPower")]
    pub co_power: String,
    #[serde(rename = "powerName")]
    pub power_name: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct ResignAction {
    #[serde(rename = "playerId")]
    pub player_id: AwbwGamePlayerId,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct NextTurnAction {
    #[serde(rename = "nextPId")]
    pub next_player_id: u32,
    #[serde(rename = "nextFunds")]
    pub next_funds: indexmap::IndexMap<TargetedPlayer, Hidden<u32>>,
    #[serde(rename = "nextTimer")]
    pub next_timer: u32,
    #[serde(rename = "nextWeather")]
    pub next_weather: String,
    pub supplied: Option<indexmap::IndexMap<TargetedPlayer, Vec<AwbwUnitId>>>,
    pub repaired: Option<indexmap::IndexMap<TargetedPlayer, Vec<RepairedUnit>>>,
    pub day: u32,
    #[serde(rename = "nextTurnStart")]
    pub next_turn_start: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct GameOverAction {
    pub day: u32,
    #[serde(rename = "gameEndDate")]
    pub game_end_date: String,
    pub losers: Vec<u32>,
    pub message: String,
    pub winners: Vec<u32>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct FireAction {
    #[serde(rename = "combatInfoVision")]
    pub combat_info_vision: indexmap::IndexMap<TargetedPlayer, CombatInfoVision>,
    #[serde(rename = "copValues")]
    pub cop_values: CopValues,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct CombatInfoVision {
    #[serde(rename = "hasVision")]
    pub has_vision: bool,
    #[serde(rename = "combatInfo")]
    pub combat_info: CombatInfo,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct CombatInfo {
    pub attacker: CombatUnit,
    pub defender: CombatUnit,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct CombatUnit {
    pub units_ammo: u32,
    pub units_hit_points: Option<f64>,
    pub units_id: AwbwUnitId,
    pub units_x: u32,
    pub units_y: u32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct CopValues {
    pub attacker: CopValueInfo,
    pub defender: CopValueInfo,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct CopValueInfo {
    #[serde(rename = "playerId")]
    pub player_id: AwbwGamePlayerId,
    #[serde(rename = "copValue")]
    pub cop_value: u32,
    #[serde(rename = "tagValue")]
    pub tag_value: Option<u32>,
}

/// Updated info for turn end
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct UpdatedInfo {
    pub event: String,
    #[serde(rename = "nextPId")]
    pub next_player_id: u32,
    #[serde(rename = "nextFunds")]
    pub next_funds: indexmap::IndexMap<TargetedPlayer, Hidden<u32>>,
    #[serde(rename = "nextTimer")]
    pub next_timer: u32,
    #[serde(rename = "nextWeather")]
    pub next_weather: String,
    pub supplied: Option<indexmap::IndexMap<TargetedPlayer, Vec<String>>>,
    pub repaired: Option<indexmap::IndexMap<TargetedPlayer, Vec<RepairedUnit>>>,
    pub day: u32,
    #[serde(rename = "nextTurnStart")]
    pub next_turn_start: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct RepairedUnit {
    pub units_id: String,
    pub units_hit_points: u32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct RepairedUnit2 {
    pub units_id: AwbwUnitId,
    pub units_hit_points: u32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Discovery {
    #[serde(default)]
    pub buildings: Vec<BuildingDiscovery>,
    #[serde(default)]
    pub units: Vec<UnitProperty>,
}

/// Complete building details including terrain information
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct BuildingDiscovery {
    #[serde(rename = "0")]
    pub id: u32,
    pub buildings_id: u32,
    pub buildings_x: u32,
    pub buildings_y: u32,
    pub buildings_capture: i32,
    pub terrain_id: Terrain,
    pub terrain_name: String,
    pub terrain_defense: u32,
    pub is_occupied: bool,
    pub buildings_players_id: Option<u32>,
    pub buildings_team: Option<String>,
}

/// Players that receive the event
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TargetedPlayer {
    /// All players receive the event
    Global,

    /// A specific player receives the event
    Player(AwbwGamePlayerId),
}

impl Serialize for TargetedPlayer {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            TargetedPlayer::Global => serializer.serialize_str("global"),
            TargetedPlayer::Player(id) => id.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for TargetedPlayer {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct PlayerVisitor;

        impl serde::de::Visitor<'_> for PlayerVisitor {
            type Value = TargetedPlayer;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string \"global\" or a player ID")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if value == "global" {
                    Ok(TargetedPlayer::Global)
                } else {
                    let val = value.parse::<u32>().map_err(|_| {
                        E::custom(format!(
                            "Expected \"global\" or a player ID number, got {}",
                            value
                        ))
                    })?;
                    Ok(TargetedPlayer::Player(AwbwGamePlayerId::new(val)))
                }
            }

            fn visit_u32<E>(self, value: u32) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(TargetedPlayer::Player(AwbwGamePlayerId::new(value)))
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let val = u32::try_from(value)
                    .map_err(|_| E::custom(format!("Player ID out of range: {}", value)))?;
                Ok(TargetedPlayer::Player(AwbwGamePlayerId::new(val)))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let val = u32::try_from(value)
                    .map_err(|_| E::custom(format!("Player ID out of range: {}", value)))?;
                Ok(TargetedPlayer::Player(AwbwGamePlayerId::new(val)))
            }
        }

        // Use deserialize_any to support both string and number formats
        deserializer.deserialize_any(PlayerVisitor)
    }
}

mod awbw_country_code {
    use awbrn_core::PlayerFaction;
    use serde::de::Error;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(faction: &PlayerFaction, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        faction.country_code().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<PlayerFaction, D::Error>
    where
        D: Deserializer<'de>,
    {
        let code: &str = Deserialize::deserialize(deserializer)?;
        PlayerFaction::from_country_code(code)
            .ok_or_else(|| D::Error::custom(format!("Invalid country code: {}", code)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_or_player_serialize() {
        // Test serializing Global variant
        let global = TargetedPlayer::Global;
        let serialized = serde_json::to_string(&global).unwrap();
        assert_eq!(serialized, r#""global""#);

        // Test serializing Player variant
        let player_id = AwbwGamePlayerId::new(42);
        let player = TargetedPlayer::Player(player_id);
        let serialized = serde_json::to_string(&player).unwrap();
        // The exact serialization format depends on how AwbwGamePlayerId serializes
        // This assumes it serializes as a number
        assert_eq!(serialized, r#"42"#);
    }

    #[test]
    fn test_global_or_player_deserialize_global() {
        // Test deserializing the string "global"
        let deserialized: TargetedPlayer = serde_json::from_str(r#""global""#).unwrap();
        assert_eq!(deserialized, TargetedPlayer::Global);
    }

    #[test]
    fn test_global_or_player_deserialize_player_number() {
        // Test deserializing a number
        let deserialized: TargetedPlayer = serde_json::from_str("42").unwrap();
        assert_eq!(
            deserialized,
            TargetedPlayer::Player(AwbwGamePlayerId::new(42))
        );
    }

    #[test]
    fn test_global_or_player_deserialize_player_string() {
        // Test deserializing a string that contains a number
        let deserialized: TargetedPlayer = serde_json::from_str(r#""42""#).unwrap();
        assert_eq!(
            deserialized,
            TargetedPlayer::Player(AwbwGamePlayerId::new(42))
        );
    }

    #[test]
    fn test_global_or_player_deserialize_invalid() {
        // Test deserializing an invalid string (not "global" and not a number)
        let result = serde_json::from_str::<TargetedPlayer>(r#""invalid""#);
        assert!(result.is_err());
    }
}
