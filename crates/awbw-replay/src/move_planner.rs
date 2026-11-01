//! AWBW move planner exports.
//!
//! The move planner saves a JSON document with the units and buildings of one
//! game. It does not record the map, the commanders or the turn order. It
//! records a team only for a player that owns a building.
//! [`MovePlanner::to_replay`] takes the missing data from a [`PlannerSetup`]
//! and makes a replay with one game entry and no turns, which the replay tools
//! can open as a board.

use std::collections::{BTreeMap, HashMap};

use awbrn_types::{
    AwbwCoId, AwbwDateTime, AwbwGameId, AwbwGamePlayerId, AwbwMapId, AwbwPlayerId, AwbwTerrain,
    AwbwUnitId, PlayerFaction, Unit,
};
use serde::Deserialize;

use crate::AwbwReplay;
use crate::de::{bool_ynstr, deserialize_sub_dive};
use crate::game_models::{AwbwBuilding, AwbwGame, AwbwPlayer, AwbwUnit, CoPower, MatchType};

/// The AWBW identifier of the commander that has no abilities.
pub const NO_COMMANDER: AwbwCoId = AwbwCoId::new(31);

/// The units and buildings of one move planner document.
#[derive(Debug, Clone, PartialEq)]
pub struct MovePlanner {
    pub name: String,
    pub units: Vec<PlannerUnit>,
    pub buildings: Vec<PlannerBuilding>,
}

/// One player that holds a unit or a building in the planner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannerPlayer {
    pub id: AwbwGamePlayerId,
    pub faction: PlayerFaction,
    /// The team of the player's buildings, if the player owns one.
    pub team: Option<String>,
}

/// The game data that the planner does not record.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannerSetup {
    pub map_id: AwbwMapId,
    /// Settings for each player. A player without an entry has no commander,
    /// no funds and no power charge.
    pub seats: Vec<PlannerSeat>,
    /// The player who acts first. The other players follow in faction order,
    /// after this player. Without a player, the turn order is the faction
    /// order.
    pub first_player: Option<AwbwGamePlayerId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlannerSeat {
    pub player: AwbwGamePlayerId,
    pub co_id: AwbwCoId,
    /// Players with the same team name are allies. Without a team name, the
    /// player keeps the team that the planner records. A player without a
    /// recorded team is on a team of its own.
    pub team: Option<String>,
    pub funds: u32,
    /// The commander's power charge, in the units that AWBW records.
    pub co_power: u32,
    /// The power that is on, from the player's last turn or from this turn.
    pub co_power_on: CoPower,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PlannerUnit {
    #[serde(rename = "units_id")]
    pub id: AwbwUnitId,
    #[serde(rename = "units_games_id")]
    pub games_id: AwbwGameId,
    #[serde(rename = "units_players_id")]
    pub players_id: AwbwGamePlayerId,
    #[serde(rename = "units_name", with = "crate::de::awbw_unit_name")]
    pub name: Unit,
    #[serde(rename = "units_movement_points")]
    pub movement_points: u32,
    #[serde(rename = "units_vision")]
    pub vision: u32,
    #[serde(rename = "units_fuel")]
    pub fuel: u32,
    #[serde(rename = "units_fuel_per_turn")]
    pub fuel_per_turn: u32,
    #[serde(rename = "units_sub_dive", deserialize_with = "deserialize_sub_dive")]
    pub sub_dive: bool,
    #[serde(rename = "units_ammo")]
    pub ammo: u32,
    #[serde(rename = "units_short_range")]
    pub short_range: u32,
    #[serde(rename = "units_long_range")]
    pub long_range: u32,
    #[serde(rename = "units_second_weapon", deserialize_with = "bool_ynstr")]
    pub second_weapon: bool,
    #[serde(rename = "units_cost")]
    pub cost: u32,
    #[serde(rename = "units_movement_type")]
    pub movement_type: String,
    #[serde(rename = "units_x")]
    pub x: u32,
    #[serde(rename = "units_y")]
    pub y: u32,
    #[serde(rename = "units_moved")]
    pub moved: u32,
    #[serde(rename = "units_capture")]
    pub capture: u32,
    #[serde(rename = "units_fired")]
    pub fired: u32,
    /// Visible HP. The planner does not record exact HP.
    #[serde(rename = "units_hit_points")]
    pub hit_points: f64,
    #[serde(rename = "units_cargo1_units_id", deserialize_with = "cargo_id")]
    pub cargo1_units_id: AwbwUnitId,
    #[serde(rename = "units_cargo2_units_id", deserialize_with = "cargo_id")]
    pub cargo2_units_id: AwbwUnitId,
    #[serde(rename = "units_carried", deserialize_with = "bool_ynstr")]
    pub carried: bool,
    #[serde(rename = "countries_code", deserialize_with = "faction_code")]
    pub faction: PlayerFaction,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PlannerBuilding {
    #[serde(rename = "buildings_id")]
    pub id: u32,
    #[serde(rename = "terrain_id")]
    pub terrain_id: AwbwTerrain,
    #[serde(rename = "buildings_x")]
    pub x: u32,
    #[serde(rename = "buildings_y")]
    pub y: u32,
    /// Capture points left before the building changes owner.
    #[serde(rename = "buildings_capture")]
    pub capture: u32,
    #[serde(
        rename = "buildings_players_id",
        default,
        deserialize_with = "optional_player_id"
    )]
    pub players_id: Option<AwbwGamePlayerId>,
    #[serde(
        rename = "countries_code",
        default,
        deserialize_with = "optional_faction_code"
    )]
    pub faction: Option<PlayerFaction>,
    #[serde(rename = "buildings_team", default, deserialize_with = "optional_text")]
    pub team: Option<String>,
}

impl MovePlanner {
    pub fn from_slice(data: &[u8]) -> Result<Self, serde_json::Error> {
        let document: PlannerDocument = serde_json::from_slice(data)?;
        let mut buildings = document
            .buildings
            .into_values()
            .into_iter()
            .flat_map(PhpArray::into_values)
            .flatten()
            .collect::<Vec<_>>();
        buildings.sort_by_key(|building| (building.y, building.x));
        let mut units = document.units.into_values();
        units.sort_by_key(|unit| unit.id);
        Ok(Self {
            name: document.name,
            units,
            buildings,
        })
    }

    /// The players that hold a unit or a building, in faction order.
    pub fn players(&self) -> Vec<PlannerPlayer> {
        let mut players = BTreeMap::<AwbwGamePlayerId, PlannerPlayer>::new();
        for unit in &self.units {
            players.entry(unit.players_id).or_insert(PlannerPlayer {
                id: unit.players_id,
                faction: unit.faction,
                team: None,
            });
        }
        for building in &self.buildings {
            let (Some(id), Some(faction)) = (building.players_id, building.faction) else {
                continue;
            };
            let player = players.entry(id).or_insert(PlannerPlayer {
                id,
                faction,
                team: None,
            });
            if player.team.is_none() {
                player.team.clone_from(&building.team);
            }
        }
        let mut players = players.into_values().collect::<Vec<_>>();
        players.sort_by_key(|player| (player.faction.awbw_id(), player.id));
        players
    }

    /// Make a replay with one game entry that holds this board and no turns.
    pub fn to_replay(&self, setup: &PlannerSetup) -> AwbwReplay {
        AwbwReplay {
            games: vec![self.to_game(setup)],
            turns: Vec::new(),
        }
    }

    fn to_game(&self, setup: &PlannerSetup) -> AwbwGame {
        let epoch = AwbwDateTime::parse("2000-01-01 00:00:00").expect("valid AWBW timestamp");
        let games_id = self
            .units
            .first()
            .map_or(AwbwGameId::new(0), |unit| unit.games_id);
        let seats = setup
            .seats
            .iter()
            .map(|seat| (seat.player, seat))
            .collect::<HashMap<_, _>>();
        let mut players = self.players();
        if let Some(first) = setup
            .first_player
            .and_then(|first| players.iter().position(|player| player.id == first))
        {
            players.rotate_left(first);
        }
        let players = players
            .into_iter()
            .enumerate()
            .map(|(order, player)| {
                let seat = seats.get(&player.id);
                AwbwPlayer {
                    id: player.id,
                    users_id: AwbwPlayerId::new(player.id.as_u32()),
                    games_id,
                    faction: player.faction,
                    co_id: seat.map_or(NO_COMMANDER, |seat| seat.co_id),
                    funds: seat.map_or(0, |seat| seat.funds),
                    turn: None,
                    email: None,
                    uniq_id: None,
                    eliminated: false,
                    last_read: String::new(),
                    last_read_broadcasts: None,
                    emailpress: None,
                    signature: None,
                    co_power: seat.map_or(0, |seat| seat.co_power),
                    co_power_on: seat.map_or(CoPower::None, |seat| seat.co_power_on),
                    order: order as u32,
                    accept_draw: false,
                    co_max_power: 0,
                    co_max_spower: 0,
                    co_image: None,
                    team: seat
                        .and_then(|seat| seat.team.clone())
                        .or(player.team)
                        .unwrap_or_else(|| player.id.as_u32().to_string()),
                    aet_count: 0,
                    turn_start: String::new(),
                    turn_clock: 0,
                    tags_co_id: None,
                    tags_co_power: None,
                    tags_co_max_power: None,
                    tags_co_max_spower: None,
                    interface: false,
                }
            })
            .collect();

        AwbwGame {
            id: games_id,
            name: self.name.clone(),
            password: None,
            creator: AwbwPlayerId::new(0),
            start_date: epoch,
            end_date: None,
            activity_date: epoch,
            maps_id: setup.map_id,
            weather_type: "Clear".into(),
            weather_start: None,
            weather_code: "C".into(),
            win_condition: None,
            turn: 0,
            day: 1,
            active: true,
            funds: 1000,
            capture_win: 1000,
            fog: false,
            comment: None,
            game_type: MatchType::Normal,
            boot_interval: 0,
            starting_funds: 0,
            official: false,
            min_rating: None,
            max_rating: None,
            league: None,
            team: true,
            aet_interval: 0,
            aet_date: epoch,
            use_powers: true,
            players,
            buildings: self
                .buildings
                .iter()
                .map(|building| AwbwBuilding {
                    id: building.id,
                    games_id: games_id.as_u32(),
                    terrain_id: building.terrain_id,
                    x: building.x,
                    y: building.y,
                    capture: building.capture,
                    last_capture: building.capture,
                    last_updated: epoch,
                })
                .collect(),
            units: self
                .units
                .iter()
                .map(|unit| AwbwUnit {
                    id: unit.id,
                    games_id,
                    players_id: unit.players_id,
                    name: unit.name,
                    movement_points: unit.movement_points,
                    vision: unit.vision,
                    fuel: unit.fuel,
                    fuel_per_turn: unit.fuel_per_turn,
                    sub_dive: unit.sub_dive,
                    ammo: unit.ammo,
                    short_range: unit.short_range,
                    long_range: unit.long_range,
                    second_weapon: unit.second_weapon,
                    symbol: String::new(),
                    cost: unit.cost,
                    movement_type: unit.movement_type.clone(),
                    x: unit.x,
                    y: unit.y,
                    moved: unit.moved,
                    capture: unit.capture,
                    fired: unit.fired,
                    hit_points: unit.hit_points,
                    cargo1_units_id: unit.cargo1_units_id,
                    cargo2_units_id: unit.cargo2_units_id,
                    carried: unit.carried,
                })
                .collect(),
            timers_initial: None,
            timers_increment: 0,
            timers_max_turn: 0,
        }
    }
}

#[derive(Deserialize)]
struct PlannerDocument {
    #[serde(rename = "plannerName", default)]
    name: String,
    #[serde(rename = "unitsState")]
    units: PhpArray<PlannerUnit>,
    /// The buildings of each map column, keyed by row.
    #[serde(rename = "buildingsState")]
    buildings: PhpArray<PhpArray<Option<PlannerBuilding>>>,
}

/// A PHP array in JSON.
///
/// The planner keys units by ID and buildings by column and row. When the keys
/// start at zero and have no gaps, or when the array is empty, PHP writes a
/// JSON array instead of an object. Each unit and building also records its
/// own ID and coordinates, so the keys are not necessary.
#[derive(Deserialize)]
#[serde(untagged)]
enum PhpArray<T> {
    Keyed(BTreeMap<String, T>),
    Listed(Vec<T>),
}

impl<T> PhpArray<T> {
    fn into_values(self) -> Vec<T> {
        match self {
            PhpArray::Keyed(values) => values.into_values().collect(),
            PhpArray::Listed(values) => values,
        }
    }
}

fn faction_code<'de, D>(deserializer: D) -> Result<PlayerFaction, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let code = String::deserialize(deserializer)?;
    PlayerFaction::from_country_code(&code)
        .ok_or_else(|| serde::de::Error::custom(format!("unknown country code: {code}")))
}

fn optional_faction_code<'de, D>(deserializer: D) -> Result<Option<PlayerFaction>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let code = Option::<String>::deserialize(deserializer)?;
    code.map(|code| {
        PlayerFaction::from_country_code(&code)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown country code: {code}")))
    })
    .transpose()
}

/// A building owner. A neutral building that is being captured records an
/// empty string.
fn optional_player_id<'de, D>(deserializer: D) -> Result<Option<AwbwGamePlayerId>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Id(u32),
        Text(String),
    }

    match Option::<Raw>::deserialize(deserializer)? {
        None => Ok(None),
        Some(Raw::Id(id)) => Ok(Some(AwbwGamePlayerId::new(id))),
        Some(Raw::Text(text)) if text.is_empty() => Ok(None),
        Some(Raw::Text(text)) => text
            .parse()
            .map(|id| Some(AwbwGamePlayerId::new(id)))
            .map_err(|_| serde::de::Error::custom(format!("invalid player id: {text}"))),
    }
}

/// A transport's cargo slot. The planner hides the cargo of an enemy
/// transport as `?`, which this reads as an empty slot.
fn cargo_id<'de, D>(deserializer: D) -> Result<AwbwUnitId, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Id(AwbwUnitId),
        Masked(String),
    }

    match Raw::deserialize(deserializer)? {
        Raw::Id(id) => Ok(id),
        Raw::Masked(text) if text == "?" => Ok(AwbwUnitId::new(0)),
        Raw::Masked(text) => Err(serde::de::Error::custom(format!("invalid unit id: {text}"))),
    }
}

fn optional_text<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.filter(|text| !text.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn php_arrays_written_as_json_arrays_parse() {
        let planner = MovePlanner::from_slice(
            br#"{
                "unitsState": [],
                "buildingsState": [[null, {
                    "buildings_id": 7,
                    "terrain_id": 119,
                    "buildings_x": 0,
                    "buildings_y": 1,
                    "buildings_capture": 20,
                    "buildings_players_id": 4110461,
                    "countries_code": "ab",
                    "buildings_team": "A"
                }]]
            }"#,
        )
        .unwrap();
        assert!(planner.units.is_empty());
        assert_eq!(planner.buildings.len(), 1);
        assert_eq!((planner.buildings[0].x, planner.buildings[0].y), (0, 1));
    }
}
