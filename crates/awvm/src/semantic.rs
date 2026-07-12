//! Pure, presentation-independent AWVM state and recipient observation values.
//!
//! Identifier domains are distinct even where their wire representations are
//! strings. Adapters from replay/ECS identifiers belong at the boundary and
//! must not make this model depend on Bevy entities or AWBW replay IDs.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::ops::Deref;

use serde::{Deserialize, Serialize};

use crate::commander;

pub type Position = [usize; 2];

macro_rules! string_id {
    ($($name:ident),+ $(,)?) => {
        $(
            #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
            #[serde(transparent)]
            pub struct $name(String);

            impl $name {
                pub fn as_str(&self) -> &str {
                    &self.0
                }
            }

            impl Deref for $name {
                type Target = str;

                fn deref(&self) -> &Self::Target {
                    self.as_str()
                }
            }

            impl fmt::Display for $name {
                fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                    self.0.fmt(formatter)
                }
            }

            impl From<&str> for $name {
                fn from(value: &str) -> Self {
                    Self(value.into())
                }
            }

            impl From<String> for $name {
                fn from(value: String) -> Self {
                    Self(value)
                }
            }

            impl PartialEq<str> for $name {
                fn eq(&self, other: &str) -> bool {
                    self.as_str() == other
                }
            }

            impl PartialEq<&str> for $name {
                fn eq(&self, other: &&str) -> bool {
                    self.as_str() == *other
                }
            }
        )+
    };
}

string_id!(
    RulesetId,
    PlayerId,
    TeamId,
    CommanderId,
    UnitKindId,
    TerrainId,
    TeleporterId,
    TraitId,
    ReasonId,
);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnitId(u32);

impl UnitId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for UnitId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl From<u32> for UnitId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RulesetRef {
    pub id: RulesetId,
    pub revision: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub fog: bool,
    pub income_per_property: u64,
    pub starting_funds: u64,
    pub powers: Toggle,
    pub tags: bool,
    pub weather: WeatherSetting,
    #[serde(deserialize_with = "deserialize_unit_kind_set")]
    pub lab_units: Vec<UnitKindId>,
    pub unit_bans: Vec<UnitKindId>,
    pub commander_bans: CommanderBans,
    pub capture_limit: Option<u64>,
    pub day_limit: Option<u64>,
    pub unit_limit: Option<u64>,
}

fn deserialize_unit_kind_set<'de, D>(deserializer: D) -> Result<Vec<UnitKindId>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let kinds = Vec::<UnitKindId>::deserialize(deserializer)?;
    let units: serde_json::Value = serde_json::from_str(include_str!(
        "../../../spec/rulesets/awbw/2026-07-10/units.json"
    ))
    .expect("embedded units table");
    let profiles = units["units"]
        .as_object()
        .expect("embedded units table has a units object");
    let mut seen = HashSet::with_capacity(kinds.len());
    for kind in &kinds {
        if !profiles.contains_key(kind.as_str()) {
            return Err(serde::de::Error::custom(format!(
                "unknown lab unit kind {kind}"
            )));
        }
        if !seen.insert(kind.as_str()) {
            return Err(serde::de::Error::custom(format!(
                "duplicate lab unit kind {kind}"
            )));
        }
    }
    Ok(kinds)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Toggle {
    Enabled,
    Disabled,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WeatherSetting {
    Clear,
    Rain,
    Snow,
    Random,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommanderBans {
    pub lead: Vec<CommanderId>,
    pub backup: Vec<CommanderId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub ruleset: RulesetRef,
    pub settings: Settings,
    pub board: Board,
    pub teams: Vec<Team>,
    pub players: Vec<Player>,
    pub turn: Turn,
    pub weather: Weather,
    pub units: Vec<Unit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_unit_id: Option<u32>,
    #[serde(rename = "match")]
    pub match_state: Match,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<Vec<Tile>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tile {
    pub terrain: TerrainId,
    #[serde(
        default,
        deserialize_with = "deserialize_present_nullable_owner",
        skip_serializing_if = "Option::is_none"
    )]
    pub owner: Option<Option<PlayerId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_points: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub silo: Option<Silo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructible_hp: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub teleporter: Option<TeleporterId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trait_state: Option<BTreeMap<TraitId, serde_json::Value>>,
}

fn deserialize_present_nullable_owner<'de, D>(
    deserializer: D,
) -> Result<Option<Option<PlayerId>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<PlayerId>::deserialize(deserializer).map(Some)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Silo {
    Ready,
    Spent,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Team {
    pub id: TeamId,
    pub status: TeamStatus,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TeamStatus {
    Active,
    Eliminated,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub team: TeamId,
    pub funds: u64,
    pub status: PlayerStatus,
    pub commanders: Vec<Commander>,
    pub power_state: PowerState,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlayerStatus {
    Active,
    Resigned,
    TimedOut,
    Eliminated,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commander {
    pub id: CommanderId,
    pub active: bool,
    pub power_charge: u64,
    pub power_uses: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PowerState {
    None,
    Cop { commander_slot: u8 },
    Scop { commander_slot: u8 },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    pub day: u64,
    pub active_player: PlayerId,
    pub phase: Phase,
    pub order: Vec<PlayerId>,
    pub position: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    TurnStart,
    UnitAction,
    TurnEnd,
    Finished,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Weather {
    pub kind: WeatherKind,
    pub remaining_turns: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WeatherKind {
    Clear,
    Rain,
    Snow,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unit {
    pub id: UnitId,
    pub kind: UnitKindId,
    pub owner: PlayerId,
    pub hp: u8,
    pub fuel: u64,
    pub ammo: u64,
    pub action: UnitAction,
    pub concealment: Concealment,
    pub location: Location,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnitAction {
    Ready,
    Moved,
    Spent,
    Immobilized,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Concealment {
    Exposed,
    Hidden,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Location {
    Board { position: Position },
    Cargo { transport: UnitId, slot: usize },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum Match {
    Active { draw_offers: Vec<PlayerId> },
    Finished { outcome: Outcome },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Outcome {
    Victory {
        winners: Vec<TeamId>,
        reason: ReasonId,
    },
    Draw {
        teams: Vec<TeamId>,
        reason: ReasonId,
    },
    Cancelled {
        reason: ReasonId,
    },
}

/// Ruleset-owned visibility. Implementations may build this from `world::fog`;
/// the state projection remains independent of Bevy and cached viewpoints.
pub trait Visibility {
    fn visible_position(&self, state: &State, team: &str, position: Position) -> bool;
    fn visible_unit(&self, state: &State, team: &str, unit: &Unit) -> bool;
}

#[derive(Clone, Debug, Deserialize)]
struct UnitVision {
    domain: String,
    vision: i64,
}

#[derive(Clone, Debug, Deserialize)]
struct UnitsTable {
    units: HashMap<String, UnitVision>,
}

#[derive(Clone, Debug, Deserialize)]
struct TerrainVision {
    traits: HashSet<String>,
    vision_bonus: Option<i64>,
    vision_limit: Option<usize>,
}

#[derive(Clone, Debug, Deserialize)]
struct TerrainTable {
    terrains: HashMap<String, TerrainVision>,
}

#[derive(Clone, Debug, Deserialize)]
struct UnitCapabilities {
    elevated_vision: HashSet<String>,
}

/// Visibility operators for the embedded `awbw/2026-07-10` profile.
#[derive(Clone, Debug)]
pub struct AwbwVisibility {
    units: HashMap<String, UnitVision>,
    terrains: HashMap<String, TerrainVision>,
    elevated_vision: HashSet<String>,
}

impl AwbwVisibility {
    pub fn new() -> Result<Self, serde_json::Error> {
        let units: UnitsTable = serde_json::from_str(include_str!(
            "../../../spec/rulesets/awbw/2026-07-10/units.json"
        ))?;
        let terrains: TerrainTable = serde_json::from_str(include_str!(
            "../../../spec/rulesets/awbw/2026-07-10/terrain.json"
        ))?;
        let capabilities: UnitCapabilities = serde_json::from_str(include_str!(
            "../../../spec/rulesets/awbw/2026-07-10/unit-capabilities.json"
        ))?;
        Ok(Self {
            units: units.units,
            terrains: terrains.terrains,
            elevated_vision: capabilities.elevated_vision,
        })
    }

    fn team_players<'a>(&self, state: &'a State, team: &str) -> HashSet<&'a str> {
        state
            .players
            .iter()
            .filter(|player| player.team == team)
            .map(|player| player.id.as_str())
            .collect()
    }

    fn vision_level(&self, state: &State, team: &str, position: Position) -> VisionLevel {
        if position[0] >= state.board.width || position[1] >= state.board.height {
            return VisionLevel::None;
        }
        if !state.settings.fog {
            return VisionLevel::Full;
        }
        let team_players = self.team_players(state, team);
        let tile = &state.board.tiles[position[1]][position[0]];
        if tile
            .owner
            .as_ref()
            .and_then(Option::as_deref)
            .is_some_and(|owner| team_players.contains(owner))
        {
            return VisionLevel::Full;
        }

        let target_terrain = self
            .terrains
            .get(tile.terrain.as_str())
            .expect("state terrain must exist in the ruleset");
        if target_terrain.traits.contains("always-visible") {
            return VisionLevel::Full;
        }
        let mut level = VisionLevel::None;
        for unit in &state.units {
            if !team_players.contains(unit.owner.as_str()) {
                continue;
            }
            let Location::Board { position: source } = unit.location else {
                continue;
            };
            let profile = self
                .units
                .get(unit.kind.as_str())
                .expect("state unit kind must exist in the ruleset");
            let source_terrain = &state.board.tiles[source[1]][source[0]].terrain;
            let bonus = if self.elevated_vision.contains(unit.kind.as_str()) {
                self.terrains
                    .get(source_terrain.as_str())
                    .expect("state terrain must exist in the ruleset")
                    .vision_bonus
                    .unwrap_or(0)
            } else {
                0
            };
            let rain = -i64::from(matches!(state.weather.kind, WeatherKind::Rain));
            let vision = commander::effective_vision(state, unit, profile.vision, &profile.domain);
            let sight = (vision + bonus + rain).max(1) as usize;
            let distance = source[0].abs_diff(position[0]) + source[1].abs_diff(position[1]);
            if distance > sight {
                continue;
            }
            let contribution = if commander::reveals_concealing_terrain(state, unit)
                || target_terrain
                    .vision_limit
                    .is_none_or(|limit| distance <= limit)
            {
                VisionLevel::Full
            } else {
                VisionLevel::AirOnly
            };
            level = level.max(contribution);
        }
        level
    }
}

impl Default for AwbwVisibility {
    fn default() -> Self {
        Self::new().expect("embedded AWBW visibility tables are valid")
    }
}

#[derive(Clone, Copy, Debug, Ord, PartialOrd, Eq, PartialEq)]
enum VisionLevel {
    None,
    AirOnly,
    Full,
}

impl Visibility for AwbwVisibility {
    fn visible_position(&self, state: &State, team: &str, position: Position) -> bool {
        self.vision_level(state, team, position) == VisionLevel::Full
    }

    fn visible_unit(&self, state: &State, team: &str, unit: &Unit) -> bool {
        let team_players = self.team_players(state, team);
        if team_players.contains(unit.owner.as_str()) {
            return true;
        }
        let Location::Board { position } = unit.location else {
            return false;
        };
        if state.board.tiles[position[1]][position[0]]
            .owner
            .as_ref()
            .and_then(Option::as_deref)
            .is_some_and(|owner| team_players.contains(owner))
        {
            return true;
        }
        if unit.concealment == Concealment::Hidden {
            return state.units.iter().any(|source| {
                team_players.contains(source.owner.as_str())
                    && matches!(
                            source.location,
                            Location::Board { position: source_position }
                                if source_position[0].abs_diff(position[0])
                                    + source_position[1].abs_diff(position[1])
                                    == 1
                    )
            });
        }
        if !state.settings.fog {
            return true;
        }
        match self.vision_level(state, team, position) {
            VisionLevel::Full => true,
            VisionLevel::AirOnly => {
                self.units
                    .get(unit.kind.as_str())
                    .expect("state unit kind must exist in the ruleset")
                    .domain
                    == "air"
            }
            VisionLevel::None => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Observation {
    pub ruleset: RulesetRef,
    pub recipient: PlayerId,
    pub settings: Settings,
    pub board: ObservedBoard,
    pub teams: Vec<Team>,
    pub players: Vec<ObservedPlayer>,
    pub turn: Turn,
    pub weather: Weather,
    pub units: Vec<ObservedUnit>,
    #[serde(rename = "match")]
    pub match_state: ObservedMatch,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ObservedUnitRef {
    Friendly { unit: UnitId },
    Enemy { position: Position },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ObservedUnit {
    #[serde(rename = "ref")]
    pub reference: ObservedUnitRef,
    pub kind: UnitKindId,
    pub owner: PlayerId,
    pub hp: u8,
    pub fuel: u64,
    pub ammo: u64,
    pub action: UnitAction,
    pub concealment: Concealment,
    pub location: Location,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ObservedBoard {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<Vec<ObservedTile>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ObservedTile {
    pub terrain: TerrainId,
    pub visibility: TileVisibility,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<Option<PlayerId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_points: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub silo: Option<Silo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructible_hp: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub teleporter: Option<TeleporterId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trait_state: Option<BTreeMap<TraitId, serde_json::Value>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TileVisibility {
    Visible,
    Fogged,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum ObservedPlayer {
    Private {
        id: PlayerId,
        team: TeamId,
        relation: Relation,
        funds: u64,
        status: PlayerStatus,
        commanders: Vec<Commander>,
        power_state: PowerState,
    },
    Public {
        id: PlayerId,
        team: TeamId,
        relation: Relation,
        status: PlayerStatus,
        commanders: Vec<PublicCommander>,
        power_state: PowerState,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Relation {
    #[serde(rename = "self")]
    Self_,
    Ally,
    Opponent,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PublicCommander {
    pub id: CommanderId,
    pub active: bool,
    pub power_charge: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum ObservedMatch {
    Active { own_team_offers: Vec<PlayerId> },
    Finished { outcome: Outcome },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObserveError {
    UnknownRecipient(PlayerId),
    InvalidBoardShape,
    UnknownUnitOwner(PlayerId),
    InvalidEvent(String),
}

pub fn observe(
    rules: &impl Visibility,
    state: &State,
    recipient: &str,
) -> Result<Observation, ObserveError> {
    let recipient_player = state
        .players
        .iter()
        .find(|p| p.id == recipient)
        .ok_or_else(|| ObserveError::UnknownRecipient(recipient.into()))?;
    if state.board.tiles.len() != state.board.height
        || state
            .board
            .tiles
            .iter()
            .any(|r| r.len() != state.board.width)
    {
        return Err(ObserveError::InvalidBoardShape);
    }
    let owners: HashMap<&str, &str> = state
        .players
        .iter()
        .map(|p| (p.id.as_str(), p.team.as_str()))
        .collect();
    let team = recipient_player.team.as_str();
    let tiles = state
        .board
        .tiles
        .iter()
        .enumerate()
        .map(|(y, row)| {
            row.iter()
                .enumerate()
                .map(|(x, t)| {
                    let visible =
                        !state.settings.fog || rules.visible_position(state, team, [x, y]);
                    ObservedTile {
                        terrain: t.terrain.clone(),
                        visibility: if visible {
                            TileVisibility::Visible
                        } else {
                            TileVisibility::Fogged
                        },
                        owner: visible.then(|| t.owner.clone()).flatten(),
                        capture_points: visible.then_some(t.capture_points).flatten(),
                        silo: visible.then_some(t.silo.clone()).flatten(),
                        destructible_hp: visible.then_some(t.destructible_hp).flatten(),
                        teleporter: t.teleporter.clone(),
                        trait_state: visible.then_some(t.trait_state.clone()).flatten(),
                    }
                })
                .collect()
        })
        .collect();
    let players = state
        .players
        .iter()
        .map(|p| {
            if p.team == team {
                ObservedPlayer::Private {
                    id: p.id.clone(),
                    team: p.team.clone(),
                    relation: if p.id == recipient {
                        Relation::Self_
                    } else {
                        Relation::Ally
                    },
                    funds: p.funds,
                    status: p.status.clone(),
                    commanders: p.commanders.clone(),
                    power_state: p.power_state.clone(),
                }
            } else {
                ObservedPlayer::Public {
                    id: p.id.clone(),
                    team: p.team.clone(),
                    relation: Relation::Opponent,
                    status: p.status.clone(),
                    commanders: p
                        .commanders
                        .iter()
                        .map(|c| PublicCommander {
                            id: c.id.clone(),
                            active: c.active,
                            power_charge: c.power_charge,
                        })
                        .collect(),
                    power_state: p.power_state.clone(),
                }
            }
        })
        .collect();
    let mut units = Vec::new();
    for u in &state.units {
        let owner_team = *owners
            .get(u.owner.as_str())
            .ok_or_else(|| ObserveError::UnknownUnitOwner(u.owner.clone()))?;
        if owner_team == team
            || (matches!(u.location, Location::Board { .. }) && rules.visible_unit(state, team, u))
        {
            units.push(observed_unit_snapshot(u, owner_team == team));
        }
    }
    units.sort_by(|a, b| a.reference.cmp(&b.reference));
    let match_state = match &state.match_state {
        Match::Active { draw_offers } => {
            let mut offers: Vec<_> = draw_offers
                .iter()
                .filter(|id| owners.get(id.as_str()).is_some_and(|t| *t == team))
                .cloned()
                .collect();
            offers.sort();
            ObservedMatch::Active {
                own_team_offers: offers,
            }
        }
        Match::Finished { outcome } => ObservedMatch::Finished {
            outcome: outcome.clone(),
        },
    };
    Ok(Observation {
        ruleset: state.ruleset.clone(),
        recipient: recipient.into(),
        settings: state.settings.clone(),
        board: ObservedBoard {
            width: state.board.width,
            height: state.board.height,
            tiles,
        },
        teams: state.teams.clone(),
        players,
        turn: state.turn.clone(),
        weather: state.weather.clone(),
        units,
        match_state,
    })
}

/// Project authoritative transition events for one recipient.
pub fn observe_events(
    rules: &impl Visibility,
    state: &State,
    next_state: &State,
    events: &[serde_json::Value],
    recipient: &str,
) -> Result<Vec<serde_json::Value>, ObserveError> {
    let recipient_player = state
        .players
        .iter()
        .find(|player| player.id == recipient)
        .ok_or_else(|| ObserveError::UnknownRecipient(recipient.into()))?;
    let team = recipient_player.team.as_str();
    let team_players: HashSet<&str> = state
        .players
        .iter()
        .filter(|player| player.team == team)
        .map(|player| player.id.as_str())
        .collect();
    observe(rules, state, recipient)?;
    let post = observe(rules, next_state, recipient)?;
    let visible_pre: HashSet<UnitId> = state
        .units
        .iter()
        .filter(|unit| {
            team_players.contains(unit.owner.as_str()) || rules.visible_unit(state, team, unit)
        })
        .map(|unit| unit.id)
        .collect();
    let visible_post: HashSet<UnitId> = next_state
        .units
        .iter()
        .filter(|unit| {
            team_players.contains(unit.owner.as_str()) || rules.visible_unit(next_state, team, unit)
        })
        .map(|unit| unit.id)
        .collect();
    let mut appeared = HashSet::new();
    let mut disappeared = HashSet::new();
    let mut output = Vec::new();

    for event in events {
        let kind = event_string(event, "type")?;
        let reason = event
            .get("reason")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(kind);
        match kind {
            "unit-action-changed"
            | "unit-damaged"
            | "unit-repaired"
            | "unit-resourced"
            | "concealment-changed"
            | "automatic-repair" => {
                project_unit_fact(
                    event_unit_id(event, "unit")?,
                    reason,
                    state,
                    next_state,
                    &visible_pre,
                    &visible_post,
                    &team_players,
                    &mut appeared,
                    &mut disappeared,
                    &mut output,
                );
            }
            "automatic-supply" => {
                let units = event
                    .get("units")
                    .and_then(serde_json::Value::as_array)
                    .ok_or_else(|| ObserveError::InvalidEvent("automatic-supply.units".into()))?;
                for unit in units {
                    let id = value_unit_id(unit)?;
                    project_unit_fact(
                        id,
                        kind,
                        state,
                        next_state,
                        &visible_pre,
                        &visible_post,
                        &team_players,
                        &mut appeared,
                        &mut disappeared,
                        &mut output,
                    );
                }
            }
            "unit-moved" => {
                let id = event_unit_id(event, "unit")?;
                let from = event_position(event, "from")?;
                let to = event_position(event, "to")?;
                let path = event
                    .get("path")
                    .and_then(serde_json::Value::as_array)
                    .ok_or_else(|| ObserveError::InvalidEvent("unit-moved.path".into()))?;
                let unit = state
                    .units
                    .iter()
                    .find(|unit| unit.id == id)
                    .ok_or_else(|| ObserveError::InvalidEvent(format!("unknown unit {id}")))?;
                if team_players.contains(unit.owner.as_str()) {
                    output.push(serde_json::json!({
                        "type": "unit-moved",
                        "unit": ObservedUnitRef::Friendly { unit: id },
                        "from": from,
                        "to": to,
                        "path": path
                    }));
                } else if visible_pre.contains(&id) && !visible_post.contains(&id) {
                    push_disappeared(id, from, &mut disappeared, &mut output);
                } else if !visible_pre.contains(&id) && visible_post.contains(&id) {
                    if let Some(snapshot) = next_state.units.iter().find(|unit| unit.id == id) {
                        push_appeared(snapshot, to, false, &mut appeared, &mut output);
                    }
                } else if visible_pre.contains(&id) && visible_post.contains(&id) {
                    let post_unit = next_state
                        .units
                        .iter()
                        .find(|unit| unit.id == id)
                        .ok_or_else(|| ObserveError::InvalidEvent(format!("unknown unit {id}")))?;
                    let mut observed_path = Vec::with_capacity(path.len());
                    for node in path {
                        let position = value_position(node)?;
                        let mut probe = post_unit.clone();
                        probe.location = Location::Board { position };
                        if rules.visible_unit(state, team, &probe)
                            || rules.visible_unit(next_state, team, &probe)
                        {
                            observed_path.push(node);
                        }
                    }
                    output.push(serde_json::json!({
                        "type": "unit-moved", "unit": enemy_unit_ref(to), "from": from, "to": to,
                        "path": observed_path
                    }));
                }
            }
            "movement-trapped" => {
                let id = event_unit_id(event, "unit")?;
                let actor = state.units.iter().find(|unit| unit.id == id);
                if actor.is_some_and(|unit| team_players.contains(unit.owner.as_str())) {
                    output.push(serde_json::json!({
                        "type":"movement-stopped",
                        "unit":ObservedUnitRef::Friendly { unit:id }
                    }));
                }
            }
            "unit-created" => {
                let id = event_unit_id(event, "unit")?;
                if visible_post.contains(&id)
                    && let Some(unit) = next_state.units.iter().find(|unit| unit.id == id)
                {
                    push_appeared(
                        unit,
                        event_position(event, "position")?,
                        team_players.contains(unit.owner.as_str()),
                        &mut appeared,
                        &mut output,
                    );
                }
            }
            "unit-removed" => {
                project_removal(
                    event_unit_id(event, "unit")?,
                    reason,
                    rules,
                    state,
                    next_state,
                    team,
                    &team_players,
                    &visible_pre,
                    &mut disappeared,
                    &mut output,
                )?;
            }
            "units-joined" => {
                project_removal(
                    event_unit_id(event, "source")?,
                    kind,
                    rules,
                    state,
                    next_state,
                    team,
                    &team_players,
                    &visible_pre,
                    &mut disappeared,
                    &mut output,
                )?;
                project_unit_fact(
                    event_unit_id(event, "target")?,
                    kind,
                    state,
                    next_state,
                    &visible_pre,
                    &visible_post,
                    &team_players,
                    &mut appeared,
                    &mut disappeared,
                    &mut output,
                );
            }
            "unit-loaded" => {
                let id = event_unit_id(event, "unit")?;
                let own = state
                    .units
                    .iter()
                    .find(|unit| unit.id == id)
                    .is_some_and(|unit| team_players.contains(unit.owner.as_str()));
                if own {
                    project_unit_fact(
                        id,
                        kind,
                        state,
                        next_state,
                        &visible_pre,
                        &visible_post,
                        &team_players,
                        &mut appeared,
                        &mut disappeared,
                        &mut output,
                    );
                } else if visible_pre.contains(&id)
                    && let Some(position) = state
                        .units
                        .iter()
                        .find_map(|unit| (unit.id == id).then(|| board_position(unit)).flatten())
                {
                    push_disappeared(id, position, &mut disappeared, &mut output);
                }
            }
            "unit-unloaded" => {
                let id = event_unit_id(event, "unit")?;
                let own = next_state
                    .units
                    .iter()
                    .find(|unit| unit.id == id)
                    .is_some_and(|unit| team_players.contains(unit.owner.as_str()));
                if own {
                    project_unit_fact(
                        id,
                        kind,
                        state,
                        next_state,
                        &visible_pre,
                        &visible_post,
                        &team_players,
                        &mut appeared,
                        &mut disappeared,
                        &mut output,
                    );
                } else if visible_post.contains(&id)
                    && let Some(unit) = next_state.units.iter().find(|unit| unit.id == id)
                {
                    push_appeared(
                        unit,
                        event_position(event, "position")?,
                        false,
                        &mut appeared,
                        &mut output,
                    );
                }
            }
            "tile-owner-changed"
            | "tile-terrain-changed"
            | "capture-changed"
            | "silo-changed"
            | "destructible-damaged" => {
                let position = event_position(event, "position")?;
                if rules.visible_position(state, team, position)
                    || rules.visible_position(next_state, team, position)
                {
                    let tile = &post.board.tiles[position[1]][position[0]];
                    output.push(serde_json::json!({
                        "type":"tile-changed", "position":position, "tile":tile, "reason":reason
                    }));
                }
            }
            "funds-changed" => {
                let player = event_string(event, "player")?;
                if team_players.contains(player)
                    && let Some(snapshot) = post.players.iter().find(|candidate| match candidate {
                        ObservedPlayer::Private { id, .. } | ObservedPlayer::Public { id, .. } => {
                            id == player
                        }
                    })
                {
                    output.push(serde_json::json!({
                        "type":"player-changed", "player":player, "state":snapshot,
                        "reason":reason
                    }));
                }
            }
            "power-charge-changed" => {
                let player = event_string(event, "player")?;
                if let Some(snapshot) = post.players.iter().find(|candidate| match candidate {
                    ObservedPlayer::Private { id, .. } | ObservedPlayer::Public { id, .. } => {
                        id == player
                    }
                }) {
                    output.push(serde_json::json!({
                        "type":"player-changed", "player":player, "state":snapshot,
                        "reason":reason
                    }));
                }
            }
            "draw-offer-changed" => {
                if team_players.contains(event_string(event, "player")?) {
                    output.push(
                        serde_json::json!({"type":"public-event","kind":"draw-offer-changed"}),
                    );
                }
            }
            "phase-changed"
            | "turn-selected"
            | "day-advanced"
            | "weather-changed"
            | "power-activated"
            | "power-ended"
            | "commander-swapped"
            | "player-status-changed"
            | "team-eliminated"
            | "match-completed" => {
                output.push(serde_json::json!({"type":"public-event","kind":kind}));
            }
            "area-strike-resolved" => output.push(event.clone()),
            _ => {}
        }
    }
    Ok(output)
}

#[allow(clippy::too_many_arguments)]
fn project_unit_fact(
    id: UnitId,
    reason: &str,
    state: &State,
    next_state: &State,
    visible_pre: &HashSet<UnitId>,
    visible_post: &HashSet<UnitId>,
    team_players: &HashSet<&str>,
    appeared: &mut HashSet<UnitId>,
    disappeared: &mut HashSet<UnitId>,
    output: &mut Vec<serde_json::Value>,
) {
    let Some(unit) = next_state.units.iter().find(|unit| unit.id == id) else {
        return;
    };
    match (visible_pre.contains(&id), visible_post.contains(&id)) {
        (true, true) => {
            let friendly = team_players.contains(unit.owner.as_str());
            let snapshot = observed_unit_snapshot(unit, friendly);
            output.push(serde_json::json!({
                "type":"unit-changed", "unit":snapshot.reference, "state":snapshot, "reason":reason
            }));
        }
        (false, true) => {
            if let Some(position) = board_position(unit) {
                push_appeared(
                    unit,
                    position,
                    team_players.contains(unit.owner.as_str()),
                    appeared,
                    output,
                );
            }
        }
        (true, false) => {
            if let Some(position) = state
                .units
                .iter()
                .find(|unit| unit.id == id)
                .and_then(board_position)
            {
                push_disappeared(id, position, disappeared, output);
            }
        }
        (false, false) => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn project_removal(
    id: UnitId,
    reason: &str,
    rules: &impl Visibility,
    state: &State,
    next_state: &State,
    team: &str,
    team_players: &HashSet<&str>,
    visible_pre: &HashSet<UnitId>,
    disappeared: &mut HashSet<UnitId>,
    output: &mut Vec<serde_json::Value>,
) -> Result<(), ObserveError> {
    let unit = state
        .units
        .iter()
        .find(|unit| unit.id == id)
        .ok_or_else(|| ObserveError::InvalidEvent(format!("unknown removed unit {id}")))?;
    if team_players.contains(unit.owner.as_str()) {
        output.push(serde_json::json!({
            "type":"unit-removed",
            "unit":ObservedUnitRef::Friendly { unit:id },
            "reason":reason
        }));
    } else if visible_pre.contains(&id) {
        let position = board_position(unit)
            .ok_or_else(|| ObserveError::InvalidEvent(format!("enemy cargo removed {id}")))?;
        if rules.visible_position(next_state, team, position) {
            output.push(serde_json::json!({
                "type":"unit-removed","unit":enemy_unit_ref(position),"reason":reason
            }));
        } else {
            push_disappeared(id, position, disappeared, output);
        }
    }
    Ok(())
}

fn push_appeared(
    unit: &Unit,
    position: Position,
    friendly: bool,
    appeared: &mut HashSet<UnitId>,
    output: &mut Vec<serde_json::Value>,
) {
    if appeared.insert(unit.id) {
        output.push(serde_json::json!({
            "type":"unit-appeared",
            "unit":observed_unit_snapshot(unit, friendly),
            "position":position
        }));
    }
}

fn push_disappeared(
    id: UnitId,
    position: Position,
    disappeared: &mut HashSet<UnitId>,
    output: &mut Vec<serde_json::Value>,
) {
    if disappeared.insert(id) {
        output.push(serde_json::json!({
            "type":"unit-disappeared","unit":enemy_unit_ref(position),"position":position
        }));
    }
}

fn observed_unit_snapshot(unit: &Unit, friendly: bool) -> ObservedUnit {
    ObservedUnit {
        reference: if friendly {
            ObservedUnitRef::Friendly { unit: unit.id }
        } else {
            enemy_unit_ref(
                board_position(unit).expect("an observed enemy unit must be on the board"),
            )
        },
        kind: unit.kind.clone(),
        owner: unit.owner.clone(),
        hp: unit.hp,
        fuel: unit.fuel,
        ammo: unit.ammo,
        action: unit.action.clone(),
        concealment: unit.concealment.clone(),
        location: unit.location.clone(),
    }
}

fn enemy_unit_ref(position: Position) -> ObservedUnitRef {
    ObservedUnitRef::Enemy { position }
}

fn board_position(unit: &Unit) -> Option<Position> {
    match unit.location {
        Location::Board { position } => Some(position),
        Location::Cargo { .. } => None,
    }
}

fn event_string<'a>(event: &'a serde_json::Value, field: &str) -> Result<&'a str, ObserveError> {
    event
        .get(field)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| ObserveError::InvalidEvent(field.into()))
}

fn event_unit_id(event: &serde_json::Value, field: &str) -> Result<UnitId, ObserveError> {
    event
        .get(field)
        .ok_or_else(|| ObserveError::InvalidEvent(field.into()))
        .and_then(value_unit_id)
}

fn value_unit_id(value: &serde_json::Value) -> Result<UnitId, ObserveError> {
    let raw = value
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| ObserveError::InvalidEvent("unit id".into()))?;
    Ok(UnitId::new(raw))
}

fn event_position(event: &serde_json::Value, field: &str) -> Result<Position, ObserveError> {
    event
        .get(field)
        .ok_or_else(|| ObserveError::InvalidEvent(field.into()))
        .and_then(value_position)
}

fn value_position(value: &serde_json::Value) -> Result<Position, ObserveError> {
    serde_json::from_value(value.clone())
        .map_err(|error| ObserveError::InvalidEvent(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    struct NoneVisible;
    impl Visibility for NoneVisible {
        fn visible_position(&self, _: &State, _: &str, _: Position) -> bool {
            false
        }
        fn visible_unit(&self, _: &State, _: &str, _: &Unit) -> bool {
            false
        }
    }
    #[test]
    fn relation_self_serializes_as_schema_value() {
        assert_eq!(serde_json::to_value(Relation::Self_).unwrap(), "self");
    }

    #[test]
    fn lab_unit_kinds_must_be_valid_and_unique() {
        let settings = serde_json::json!({
            "fog": false,
            "income_per_property": 1000,
            "starting_funds": 0,
            "powers": "disabled",
            "tags": false,
            "weather": "clear",
            "lab_units": ["infantry", "infantry"],
            "unit_bans": [],
            "commander_bans": { "lead": [], "backup": [] },
            "capture_limit": null,
            "day_limit": null,
            "unit_limit": null
        });
        assert!(
            serde_json::from_value::<Settings>(settings.clone())
                .unwrap_err()
                .to_string()
                .contains("duplicate lab unit kind infantry")
        );

        let mut unknown = settings;
        unknown["lab_units"] = serde_json::json!(["not-a-unit"]);
        assert!(
            serde_json::from_value::<Settings>(unknown)
                .unwrap_err()
                .to_string()
                .contains("unknown lab unit kind not-a-unit")
        );
    }

    #[test]
    fn hidden_enemy_substitution_does_not_change_observation() {
        let mut s = fixture();
        let a = observe(&NoneVisible, &s, "p1").unwrap();
        s.units[1].id = UnitId::new(2);
        s.units.push(Unit {
            id: UnitId::new(3),
            ..s.units[1].clone()
        });
        assert_eq!(a, observe(&NoneVisible, &s, "p1").unwrap());
    }

    #[test]
    fn visible_enemy_authoritative_id_is_not_observed() {
        struct AllVisible;
        impl Visibility for AllVisible {
            fn visible_position(&self, _: &State, _: &str, _: Position) -> bool {
                true
            }
            fn visible_unit(&self, _: &State, _: &str, _: &Unit) -> bool {
                true
            }
        }

        let mut state = fixture();
        let before = observe(&AllVisible, &state, "p1").unwrap();
        state.units[1].id = UnitId::new(99);
        assert_eq!(before, observe(&AllVisible, &state, "p1").unwrap());
        assert_eq!(
            before
                .units
                .iter()
                .find(|unit| unit.owner == "p2")
                .unwrap()
                .reference,
            ObservedUnitRef::Enemy { position: [0, 0] }
        );
    }

    #[test]
    fn enemy_path_keeps_visible_positions_on_both_sides_of_woods() {
        let mut state = fixture();
        state.board.width = 6;
        state.board.tiles[0] = (0..6)
            .map(|x| Tile {
                terrain: if x == 4 { "wood" } else { "plain" }.into(),
                owner: None,
                capture_points: None,
                silo: None,
                destructible_hp: None,
                teleporter: None,
                trait_state: None,
            })
            .collect();
        state.units[0].kind = "recon".into();
        state.units[0].location = Location::Board { position: [0, 0] };
        state.units[1].kind = "tank".into();
        state.units[1].location = Location::Board { position: [5, 0] };

        let mut next_state = state.clone();
        next_state.units[1].location = Location::Board { position: [3, 0] };
        let events = vec![serde_json::json!({
            "type": "unit-moved",
            "unit": 1,
            "from": [5, 0],
            "to": [3, 0],
            "path": [[5, 0], [4, 0], [3, 0]],
            "fuel_spent": 2
        })];

        assert_eq!(
            observe_events(
                &AwbwVisibility::default(),
                &state,
                &next_state,
                &events,
                "p1"
            )
            .unwrap(),
            vec![serde_json::json!({
                "type": "unit-moved",
                "unit": {"type": "enemy", "position": [3,0]},
                "from": [5, 0],
                "to": [3, 0],
                "path": [[5, 0], [3, 0]]
            })]
        );
    }
    fn fixture() -> State {
        State {
            ruleset: RulesetRef {
                id: "awbw".into(),
                revision: "2026-07-10".into(),
            },
            settings: Settings {
                fog: true,
                income_per_property: 1000,
                starting_funds: 0,
                powers: Toggle::Enabled,
                tags: false,
                weather: WeatherSetting::Clear,
                lab_units: vec![],
                unit_bans: vec![],
                commander_bans: CommanderBans {
                    lead: vec![],
                    backup: vec![],
                },
                capture_limit: None,
                day_limit: None,
                unit_limit: None,
            },
            board: Board {
                width: 1,
                height: 1,
                tiles: vec![vec![Tile {
                    terrain: "plain".into(),
                    owner: None,
                    capture_points: None,
                    silo: None,
                    destructible_hp: None,
                    teleporter: None,
                    trait_state: None,
                }]],
            },
            teams: vec![
                Team {
                    id: "t1".into(),
                    status: TeamStatus::Active,
                },
                Team {
                    id: "t2".into(),
                    status: TeamStatus::Active,
                },
            ],
            players: vec![player("p1", "t1"), player("p2", "t2")],
            turn: Turn {
                day: 1,
                active_player: "p1".into(),
                phase: Phase::UnitAction,
                order: vec!["p1".into(), "p2".into()],
                position: 0,
            },
            weather: Weather {
                kind: WeatherKind::Clear,
                remaining_turns: 0,
            },
            units: vec![unit(0, "p1"), unit(1, "p2")],
            next_unit_id: None,
            match_state: Match::Active {
                draw_offers: vec![],
            },
        }
    }
    fn player(id: &str, team: &str) -> Player {
        Player {
            id: id.into(),
            team: team.into(),
            funds: 0,
            status: PlayerStatus::Active,
            commanders: vec![Commander {
                id: "andy".into(),
                active: true,
                power_charge: 0,
                power_uses: 0,
            }],
            power_state: PowerState::None,
        }
    }
    fn unit(id: u32, owner: &str) -> Unit {
        Unit {
            id: id.into(),
            kind: "infantry".into(),
            owner: owner.into(),
            hp: 100,
            fuel: 99,
            ammo: 0,
            action: UnitAction::Ready,
            concealment: Concealment::Exposed,
            location: Location::Board { position: [0, 0] },
        }
    }
}
