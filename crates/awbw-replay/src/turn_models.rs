use crate::de::{Hidden, Masked};
use awbrn_core::{AwbwGamePlayerId, AwbwTerrain, AwbwUnitId, PlayerFaction, Unit};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

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
        #[serde(rename = "Move", deserialize_with = "empty_field_action")]
        move_action: Option<MoveAction>,

        #[serde(rename = "Join")]
        join_action: JoinAction,
    },
    Load {
        #[serde(rename = "Move", deserialize_with = "empty_field_action")]
        move_action: Option<MoveAction>,

        #[serde(rename = "Load")]
        load_action: LoadAction,
    },
    Move(MoveAction),
    Power(PowerAction),
    Repair {
        #[serde(rename = "Move", deserialize_with = "empty_field_action")]
        move_action: Option<MoveAction>,

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
        #[serde(rename = "Move", deserialize_with = "empty_field_action")]
        move_action: Option<MoveAction>,

        #[serde(rename = "Supply")]
        supply_action: SupplyAction,
    },
    Unload {
        unit: UnitMap,
        #[serde(rename = "transportID")]
        transport_id: AwbwUnitId,
        discovered: indexmap::IndexMap<TargetedPlayer, Option<Discovery>>,
    },
    Delete {
        #[serde(rename = "Delete")]
        delete_action: DeleteAction,
    },
    Hide {
        #[serde(rename = "Move", deserialize_with = "empty_field_action")]
        move_action: Option<MoveAction>,
    },
    Unhide {
        #[serde(rename = "Move", deserialize_with = "empty_field_action")]
        move_action: Option<MoveAction>,
    },
    Tag {
        #[serde(rename = "updatedInfo")]
        updated_info: UpdatedInfo,
    },
}

impl Action {
    pub fn kind_name(&self) -> &'static str {
        match self {
            Action::AttackSeam { .. } => "AttackSeam",
            Action::Build { .. } => "Build",
            Action::Capt { .. } => "Capt",
            Action::End { .. } => "End",
            Action::Fire { .. } => "Fire",
            Action::Join { .. } => "Join",
            Action::Load { .. } => "Load",
            Action::Move(_) => "Move",
            Action::Power(_) => "Power",
            Action::Repair { .. } => "Repair",
            Action::Resign { .. } => "Resign",
            Action::Supply { .. } => "Supply",
            Action::Unload { .. } => "Unload",
            Action::Delete { .. } => "Delete",
            Action::Hide { .. } => "Hide",
            Action::Unhide { .. } => "Unhide",
            Action::Tag { .. } => "Tag",
        }
    }

    pub fn move_action(&self) -> Option<&MoveAction> {
        match self {
            Action::Move(action) => Some(action),
            Action::AttackSeam { move_action, .. } => move_action.as_ref(),
            Action::Capt { move_action, .. } => move_action.as_ref(),
            Action::Fire { move_action, .. } => move_action.as_ref(),
            Action::Join { move_action, .. } => move_action.as_ref(),
            Action::Load { move_action, .. } => move_action.as_ref(),
            Action::Repair { move_action, .. } => move_action.as_ref(),
            Action::Supply { move_action, .. } => move_action.as_ref(),
            Action::Hide { move_action, .. } => move_action.as_ref(),
            Action::Unhide { move_action, .. } => move_action.as_ref(),
            _ => None,
        }
    }
}

pub type UnitMap = indexmap::IndexMap<TargetedPlayer, Hidden<UnitProperty>>;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct UnitProperty {
    pub units_id: AwbwUnitId,
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
    pub units_hit_points: AwbwHpDisplay,
    #[serde(default)]
    pub units_cargo1_units_id: Masked<u32>,
    #[serde(default)]
    pub units_cargo2_units_id: Masked<u32>,
    pub units_carried: Option<String>,
    #[serde(with = "awbw_country_code")]
    pub countries_code: PlayerFaction,
}

/// Unit hit points in Awbw replays are only tracked in deciles [0-10]
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]
pub struct AwbwHpDisplay(u8);

impl AwbwHpDisplay {
    pub fn value(&self) -> u8 {
        self.0
    }

    pub fn is_full_health(&self) -> bool {
        self.0 >= 10
    }

    pub fn is_destroyed(&self) -> bool {
        self.0 == 0
    }
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

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct DeleteAction {
    #[serde(rename = "unitId")]
    pub unit_id: Option<indexmap::IndexMap<TargetedPlayer, Hidden<AwbwUnitId>>>,
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
    pub loaded: indexmap::IndexMap<TargetedPlayer, Hidden<AwbwUnitId>>,
    pub transport: indexmap::IndexMap<TargetedPlayer, Hidden<AwbwUnitId>>,
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
    #[serde(deserialize_with = "crate::de::str_or_u32")]
    pub x: u32,
    #[serde(deserialize_with = "crate::de::str_or_u32")]
    pub y: u32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct JoinAction {
    #[serde(rename = "playerId")]
    pub player_id: u32,
    #[serde(rename = "newFunds")]
    pub new_funds: indexmap::IndexMap<TargetedPlayer, u32>,
    pub unit: UnitMap,
    #[serde(rename = "joinID")]
    pub join_id: indexmap::IndexMap<TargetedPlayer, Hidden<u32>>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct SupplyAction {
    pub unit: indexmap::IndexMap<TargetedPlayer, Hidden<u32>>,
    pub rows: Vec<String>,
    pub supplied: indexmap::IndexMap<TargetedPlayer, Vec<AwbwUnitId>>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct RepairAction {
    pub unit: indexmap::IndexMap<TargetedPlayer, Hidden<u32>>,
    pub repaired: indexmap::IndexMap<TargetedPlayer, RepairedUnit>,
    pub funds: indexmap::IndexMap<TargetedPlayer, Hidden<u32>>,
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
    pub combat_info: Masked<CombatUnit>,
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

    /// leftover CO power meter after activation
    #[serde(rename = "playersCOP")]
    pub players_cop: u32,

    /// Global stat boosts applied to all of the activating player's units
    #[serde(rename = "global", default, skip_serializing_if = "Option::is_none")]
    pub global: Option<GlobalStatBoost>,

    /// Bulk HP/fuel changes applied to all units of specified players
    #[serde(rename = "hpChange", default, skip_serializing_if = "Option::is_none")]
    pub hp_change: Option<HpChange>,

    /// Per-unit stat overrides (used by Jess fuel refill, Rachel missile damage, etc.)
    #[serde(
        rename = "unitReplace",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub unit_replace: Option<indexmap::IndexMap<TargetedPlayer, UnitReplaceGroup>>,

    /// Units spawned by the power (Sensei only)
    #[serde(rename = "unitAdd", default, skip_serializing_if = "Option::is_none")]
    pub unit_add: Option<indexmap::IndexMap<TargetedPlayer, UnitAddGroup>>,

    /// Per-player fund/meter changes (Sasha only)
    #[serde(
        rename = "playerReplace",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub player_replace: Option<
        indexmap::IndexMap<TargetedPlayer, indexmap::IndexMap<TargetedPlayer, PlayerChange>>,
    >,

    #[serde(
        rename = "missileCoords",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub missile_coords: Option<Vec<Coordinate>>,

    #[serde(rename = "weather", default, skip_serializing_if = "Option::is_none")]
    pub weather: Option<WeatherChange>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct WeatherChange {
    #[serde(rename = "weatherCode")]
    pub weather_code: WeatherCode,
    #[serde(rename = "weatherName")]
    pub weather_name: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]
pub enum WeatherCode {
    #[serde(rename = "C", alias = "c")]
    Clear,
    #[serde(rename = "R")]
    #[serde(alias = "r")]
    Rain,
    #[serde(rename = "S")]
    #[serde(alias = "s")]
    Snow,
}

impl From<WeatherCode> for awbrn_core::Weather {
    fn from(value: WeatherCode) -> Self {
        match value {
            WeatherCode::Clear => awbrn_core::Weather::Clear,
            WeatherCode::Rain => awbrn_core::Weather::Rain,
            WeatherCode::Snow => awbrn_core::Weather::Snow,
        }
    }
}

/// Movement/vision boosts applied globally to the activating player's units
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct GlobalStatBoost {
    pub units_movement_points: i32,
    pub units_vision: i32,
}

/// Bulk HP and fuel changes applied to all units of specified players.
/// Both fields are optional in the JSON — they appear as `""` when absent.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct HpChange {
    #[serde(
        rename = "hpGain",
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::de::empty_str_or_struct",
        default
    )]
    pub hp_gain: Option<HpEffect>,
    #[serde(
        rename = "hpLoss",
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::de::empty_str_or_struct",
        default
    )]
    pub hp_loss: Option<HpEffect>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct HpEffect {
    pub players: Vec<AwbwGamePlayerId>,
    /// HP delta (positive = gain, negative = loss)
    pub hp: i32,
    /// Fuel multiplier (1.0 = full refill, 0.5 = halved)
    pub units_fuel: f64,
}

/// A group of unit stat overrides scoped to a TargetedPlayer
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct UnitReplaceGroup {
    pub units: Option<Vec<UnitChange>>,
}

/// Sparse unit stat override — only the fields that changed are present
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct UnitChange {
    pub units_id: AwbwUnitId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub units_hit_points: Option<AwbwHpDisplay>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub units_ammo: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub units_fuel: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub units_movement_points: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub units_long_range: Option<u32>,
    /// -1 = stunned (Von Bolt), 0 = reactivated (Eagle), absent = unchanged
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub units_moved: Option<i32>,
}

/// Units spawned by a power (Sensei's Copter Command / Great Journey)
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct UnitAddGroup {
    #[serde(rename = "playerId")]
    pub player_id: AwbwGamePlayerId,
    #[serde(rename = "unitName", with = "crate::de::awbw_unit_name")]
    pub unit_name: Unit,
    pub units: Vec<NewUnit>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct NewUnit {
    pub units_id: AwbwUnitId,
    pub units_x: u32,
    pub units_y: u32,
}

/// Per-player fund/meter changes applied by a power (Sasha only)
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct PlayerChange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub players_funds: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub players_co_power: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags_co_power: Option<u32>,
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
    pub next_weather: WeatherCode,
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
    pub attacker: Masked<CombatUnit>,
    pub defender: Masked<CombatUnit>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct CombatUnit {
    pub units_ammo: u32,
    pub units_hit_points: Option<AwbwHpDisplay>,
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
    pub next_weather: WeatherCode,
    pub supplied: Option<indexmap::IndexMap<TargetedPlayer, Vec<AwbwUnitId>>>,
    pub repaired: Option<indexmap::IndexMap<TargetedPlayer, Vec<RepairedUnit>>>,
    pub day: u32,
    #[serde(rename = "nextTurnStart")]
    pub next_turn_start: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct RepairedUnit {
    pub units_id: AwbwUnitId,
    pub units_hit_points: AwbwHpDisplay,
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
    pub terrain_id: AwbwTerrain,
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

    /// A team represented by a single capital letter
    Team(u8),
}

impl Serialize for TargetedPlayer {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            TargetedPlayer::Global => serializer.serialize_str("global"),
            TargetedPlayer::Player(id) => id.serialize(serializer),
            TargetedPlayer::Team(c) => serializer.serialize_str(&c.to_string()),
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
                formatter.write_str("a string \"global\", a team letter (A-Z), or a player ID")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if value == "global" {
                    return Ok(TargetedPlayer::Global);
                }

                if let Ok(val) = value.parse::<u32>() {
                    return Ok(TargetedPlayer::Player(AwbwGamePlayerId::new(val)));
                }

                let bytes = value.as_bytes();
                if bytes.len() == 1 && bytes[0].is_ascii_uppercase() {
                    return Ok(TargetedPlayer::Team(bytes[0]));
                }

                Err(E::custom(format!(
                    "Expected \"global\", a team letter (A-Z), or a player ID number, got {}",
                    value
                )))
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
    fn test_global_stat_boost() {
        let json = r#"{
            "playerID": 3189356, "coName": "Koal", "coPower": "Y",
            "powerName": "Forced March", "playersCOP": 30000,
            "global": {"units_movement_points": 1, "units_vision": 0}
        }"#;
        let action: PowerAction = serde_json::from_str(json).unwrap();
        assert_eq!(
            action.global,
            Some(GlobalStatBoost {
                units_movement_points: 1,
                units_vision: 0
            })
        );
    }

    #[test]
    fn test_hp_change_gain_only() {
        let json = r#"{
            "playerID": 3276843, "coName": "Andy", "coPower": "S",
            "powerName": "Hyper Upgrade", "playersCOP": 0,
            "hpChange": {
                "hpGain": {"players": [3276843], "hp": 5, "units_fuel": 1},
                "hpLoss": ""
            }
        }"#;
        let action: PowerAction = serde_json::from_str(json).unwrap();
        let hpc = action.hp_change.unwrap();
        assert!(hpc.hp_loss.is_none());
        let gain = hpc.hp_gain.unwrap();
        assert_eq!(gain.hp, 5);
        assert_eq!(gain.units_fuel, 1.0);
    }

    #[test]
    fn test_hp_change_loss_with_fuel_fraction() {
        let json = r#"{
            "playerID": 3279740, "coName": "Drake", "coPower": "S",
            "powerName": "Typhoon", "playersCOP": 0,
            "hpChange": {
                "hpGain": "",
                "hpLoss": {"players": [3277011, 3276855], "hp": -2, "units_fuel": 0.5}
            }
        }"#;
        let action: PowerAction = serde_json::from_str(json).unwrap();
        let hpc = action.hp_change.unwrap();
        assert!(hpc.hp_gain.is_none());
        let loss = hpc.hp_loss.unwrap();
        assert_eq!(loss.hp, -2);
        assert_eq!(loss.units_fuel, 0.5);
        assert_eq!(loss.players.len(), 2);
    }

    #[test]
    fn test_unit_replace() {
        let json = r#"{
            "playerID": 3707970, "coName": "Rachel", "coPower": "S",
            "powerName": "Covering Fire", "playersCOP": 0,
            "unitReplace": {
                "global": {"units": [
                    {"units_id": 190042235, "units_hit_points": 4},
                    {"units_id": 190522630, "units_movement_points": 7, "units_ammo": 9, "units_fuel": 70}
                ]}
            }
        }"#;
        let action: PowerAction = serde_json::from_str(json).unwrap();
        let replace = action.unit_replace.unwrap();
        let group = &replace[&TargetedPlayer::Global];
        let units = group.units.as_ref().unwrap();
        assert_eq!(units[0].units_id, AwbwUnitId::new(190042235));
        assert_eq!(units[0].units_hit_points, Some(AwbwHpDisplay(4)));
        assert_eq!(units[1].units_movement_points, Some(7));
    }

    #[test]
    fn test_unit_add() {
        let json = r#"{
            "playerID": 3313081, "coName": "Sensei", "coPower": "Y",
            "powerName": "Copter Command", "playersCOP": 47500,
            "unitAdd": {
                "global": {
                    "playerId": 3313081, "unitName": "Infantry",
                    "units": [{"units_id": 175117046, "units_x": 0, "units_y": 11}]
                }
            }
        }"#;
        let action: PowerAction = serde_json::from_str(json).unwrap();
        let add = action.unit_add.unwrap();
        let group = &add[&TargetedPlayer::Global];
        assert_eq!(group.unit_name, Unit::Infantry);
        assert_eq!(group.units[0].units_x, 0);
        assert_eq!(group.units[0].units_y, 11);
    }

    #[test]
    fn test_supply_action_parses_string_unit_ids() {
        let json = r#"{
            "unit": {"global": 170404311},
            "rows": ["170319279"],
            "supplied": {
                "3189356": ["170319279"],
                "3189394": ["170319279"],
                "3189442": []
            }
        }"#;
        let action: SupplyAction = serde_json::from_str(json).unwrap();

        assert_eq!(
            action.supplied[&TargetedPlayer::Player(AwbwGamePlayerId::new(3189356))],
            vec![AwbwUnitId::new(170319279)]
        );
        assert!(
            action.supplied[&TargetedPlayer::Player(AwbwGamePlayerId::new(3189442))].is_empty()
        );
    }

    #[test]
    fn test_updated_info_parses_string_supplied_and_repaired_ids() {
        let json = r#"{
            "event": "NextTurn",
            "nextPId": 3189812,
            "nextFunds": {"global": 17400},
            "nextTimer": 1260250,
            "nextWeather": "C",
            "supplied": {"global": ["170319279"]},
            "repaired": {
                "global": [{"units_id": "170480506", "units_hit_points": 4}]
            },
            "day": 18,
            "nextTurnStart": "2025-03-12 00:00:00"
        }"#;
        let updated: UpdatedInfo = serde_json::from_str(json).unwrap();
        assert_eq!(updated.next_weather, WeatherCode::Clear);

        assert_eq!(
            updated.supplied.unwrap()[&TargetedPlayer::Global],
            vec![AwbwUnitId::new(170319279)]
        );
        assert_eq!(
            updated.repaired.unwrap()[&TargetedPlayer::Global][0].units_id,
            AwbwUnitId::new(170480506)
        );
    }

    #[test]
    fn test_player_replace() {
        let json = r#"{
            "playerID": 3653682, "coName": "Sasha", "coPower": "Y",
            "powerName": "Market Crash", "playersCOP": 115000,
            "playerReplace": {
                "global": {"3654564": {"players_co_power": 0}}
            }
        }"#;
        let action: PowerAction = serde_json::from_str(json).unwrap();
        let pr = action.player_replace.unwrap();
        let global = &pr[&TargetedPlayer::Global];
        let target_id = AwbwGamePlayerId::new(3654564);
        let change = &global[&TargetedPlayer::Player(target_id)];
        assert_eq!(change.players_co_power, Some(0));
    }

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
    fn test_global_or_player_deserialize_team() {
        // Test deserializing a team letter
        let deserialized: TargetedPlayer = serde_json::from_str(r#""A""#).unwrap();
        assert_eq!(deserialized, TargetedPlayer::Team(b'A'));

        let deserialized: TargetedPlayer = serde_json::from_str(r#""Z""#).unwrap();
        assert_eq!(deserialized, TargetedPlayer::Team(b'Z'));
    }

    #[test]
    fn test_weather_change() {
        let json = r#"{
            "playerID": 3252473,
            "coName": "Olaf",
            "coPower": "S",
            "powerName": "Winter Fury",
            "playersCOP": 0,
            "weather": {"weatherCode": "S", "weatherName": "Snow"}
        }"#;
        let action: PowerAction = serde_json::from_str(json).unwrap();
        assert_eq!(
            action.weather,
            Some(WeatherChange {
                weather_code: WeatherCode::Snow,
                weather_name: "Snow".to_string(),
            })
        );
    }

    #[test]
    fn test_weather_absent() {
        let json = r#"{
            "playerID": 3189356,
            "coName": "Koal",
            "coPower": "Y",
            "powerName": "Forced March",
            "playersCOP": 30000
        }"#;
        let action: PowerAction = serde_json::from_str(json).unwrap();
        assert!(action.weather.is_none());
    }

    #[test]
    fn test_next_weather_parses_lowercase_weather_code() {
        let json = r#"{
            "event": "NextTurn",
            "nextPId": 3189812,
            "nextFunds": {"global": 17400},
            "nextTimer": 1260250,
            "nextWeather": "r",
            "supplied": null,
            "repaired": null,
            "day": 18,
            "nextTurnStart": "2025-03-12 00:00:00"
        }"#;
        let updated: UpdatedInfo = serde_json::from_str(json).unwrap();

        assert_eq!(updated.next_weather, WeatherCode::Rain);
    }

    #[test]
    fn test_next_weather_parses_clear_weather_code() {
        let json = r#"{
            "event": "NextTurn",
            "nextPId": 3189812,
            "nextFunds": {"global": 17400},
            "nextTimer": 1260250,
            "nextWeather": "c",
            "supplied": null,
            "repaired": null,
            "day": 18,
            "nextTurnStart": "2025-03-12 00:00:00"
        }"#;
        let updated: UpdatedInfo = serde_json::from_str(json).unwrap();

        assert_eq!(updated.next_weather, WeatherCode::Clear);
    }

    #[test]
    fn test_missile_coords_string_encoded() {
        let json = r#"{
            "playerID": 3707970,
            "coName": "Rachel",
            "coPower": "S",
            "powerName": "Covering Fire",
            "playersCOP": 0,
            "missileCoords": [{"x": "4", "y": "16"}, {"x": "18", "y": "14"}]
        }"#;
        let action: PowerAction = serde_json::from_str(json).unwrap();
        let coords = action.missile_coords.unwrap();
        assert_eq!(coords.len(), 2);
        assert_eq!(coords[0], Coordinate { x: 4, y: 16 });
        assert_eq!(coords[1], Coordinate { x: 18, y: 14 });
    }

    #[test]
    fn test_missile_coords_absent() {
        let json = r#"{
            "playerID": 3189356,
            "coName": "Koal",
            "coPower": "Y",
            "powerName": "Forced March",
            "playersCOP": 30000
        }"#;
        let action: PowerAction = serde_json::from_str(json).unwrap();
        assert!(action.missile_coords.is_none());
    }

    #[test]
    fn test_global_or_player_deserialize_invalid() {
        // Test deserializing an invalid string (not "global", not a team letter, and not a number)
        let result = serde_json::from_str::<TargetedPlayer>(r#""invalid""#);
        assert!(result.is_err());

        // Test lowercase letter should fail
        let result = serde_json::from_str::<TargetedPlayer>(r#""a""#);
        assert!(result.is_err());
    }
}
