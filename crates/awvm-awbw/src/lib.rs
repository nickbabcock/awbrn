//! Bevy-free adaptation between archived AWBW replays and AWVM values.
//!
//! Historical AWBW replays record recipient-targeted outcomes rather than an
//! exact random tape. This crate therefore has two distinct responsibilities:
//! construct the deterministic state before the first action, and (in later
//! phases) translate recorded outcomes into typed observations. It must not
//! pretend that a legacy replay can be deterministically re-executed.

use std::collections::{HashMap, HashSet};

use awbrn_map::{AwbwMap, AwbwMapData, Position};
use awbrn_types::{
    AwbwGamePlayerId, AwbwTerrain, Faction, MissileSiloStatus, PlayerFaction, Property,
};
use awbw_replay::AwbwReplay;
use awbw_replay::game_models::{AwbwGame, AwbwPlayer, AwbwUnit, CoPower, MatchType};
use awvm::ruleset::{RULESET_ID, RULESET_REVISION, Terrain, WeatherKind};
use awvm::semantic::{
    Board, Commander, CommanderBans, Concealment, Location, Match, Phase, Player, PlayerId,
    PlayerIdx, PlayerStatus, Pos, PowerState, Roster, RulesetId, RulesetRef, RulesetRevision,
    Settings, Silo, State, StateInvariant, Team, TeamId, TeamStatus, Tile, TileOwner, Toggle, Turn,
    Unit, UnitAction, UnitId, UnitStore, Weather, WeatherSetting,
};

mod command;
mod compatibility;
mod recorded;
mod targeting;

pub use command::{CommandAdapterError, diagnostic_command};
pub use compatibility::{
    CandidateCounts, HpAssignment, InsufficientReplayData, LocalCompatibility,
    LocalCompatibilityMatch, LocalDivergence, diagnose_local_compatibility,
    diagnose_local_compatibility_until_match,
};
pub use recorded::{RecordedAdapter, RecordedAdapterError, RecordedTransition};

/// Construct the AWVM state immediately before the first archived action.
///
/// The game entry supplies the replay roster and semantic overrides; the map
/// API response supplies the terrain rectangle. The returned state has already
/// passed [`State::validate`].
pub fn initial_state(replay: &AwbwReplay, map_data: &AwbwMapData) -> Result<State, AdapterError> {
    let game = replay.games.first().ok_or(AdapterError::MissingGame)?;
    let map =
        AwbwMap::try_from(map_data).map_err(|error| AdapterError::InvalidMap(error.to_string()))?;
    initial_state_from_map(game, &map)
}

/// Lower one replay game entry and its already-parsed map.
///
/// This form is useful to callers that cache [`AwbwMap`] independently of the
/// API response metadata.
pub fn initial_state_from_map(game: &AwbwGame, map: &AwbwMap) -> Result<State, AdapterError> {
    if game.players.is_empty() {
        return Err(AdapterError::EmptyRoster);
    }

    let mut ordered_players = game.players.iter().collect::<Vec<_>>();
    ordered_players.sort_by_key(|player| player.order);
    ensure_unique_players(&ordered_players)?;

    // A held tile and a unit both name a seat on this roster, which is built in
    // this order, so a player maps straight to the seat they sit in.
    let players = Roster::new(
        ordered_players
            .iter()
            .map(|player| lower_player(game, player))
            .collect::<Result<Vec<_>, _>>()?,
    )
    .map_err(|error| AdapterError::RosterTooLarge(error.found))?;
    let player_seats = ordered_players
        .iter()
        .zip(players.seats())
        .map(|(source, (seat, _))| (source.id, seat))
        .collect::<HashMap<_, _>>();
    let faction_seats = ordered_players
        .iter()
        .zip(players.seats())
        .map(|(source, (seat, _))| (source.faction, seat))
        .collect::<HashMap<_, _>>();

    let teams = lower_teams(&players);
    let order = players
        .iter()
        .map(|player| player.id().clone())
        .collect::<Vec<_>>();
    let active_player = order[0].clone();

    let board = lower_board(game, map, &faction_seats)?;
    let units = lower_units(game, &player_seats)?;
    let next_unit_id = units
        .iter()
        .map(|unit| unit.id.get())
        .max()
        .map_or(1, |highest| highest.saturating_add(1));

    let state = State {
        ruleset: RulesetRef {
            id: RulesetId::from(RULESET_ID),
            revision: RulesetRevision::from(RULESET_REVISION),
        },
        settings: Settings {
            fog: game.fog,
            income_per_property: u64::from(game.funds),
            starting_funds: u64::from(game.starting_funds),
            powers: if game.use_powers {
                Toggle::Enabled
            } else {
                Toggle::Disabled
            },
            tags: matches!(game.game_type, MatchType::Tag),
            weather: weather_setting(&game.weather_type)?,
            lab_units: Vec::new(),
            unit_bans: Vec::new(),
            commander_bans: CommanderBans {
                lead: Vec::new(),
                backup: Vec::new(),
            },
            capture_limit: (game.capture_win > 0 && game.capture_win < 1_000)
                .then_some(u64::from(game.capture_win)),
            day_limit: None,
            unit_limit: None,
        },
        board,
        teams,
        players,
        turn: Turn {
            day: 1,
            active_player,
            phase: Phase::UnitAction,
            order,
            position: 0,
        },
        weather: Weather {
            kind: weather_kind(&game.weather_code)?,
            remaining_turns: 0,
        },
        units,
        next_unit_id: Some(next_unit_id),
        match_state: Match::Active {
            draw_offers: Vec::new(),
        },
    };
    state.validate().map_err(AdapterError::InvalidState)?;
    Ok(state)
}

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("the replay contains no game entry")]
    MissingGame,
    #[error("the replay roster is empty")]
    EmptyRoster,
    #[error("the replay roster holds {0} players, more than AWVM seats")]
    RosterTooLarge(usize),
    #[error("player {0} appears more than once in the replay roster")]
    DuplicatePlayer(u32),
    #[error("faction {0:?} belongs to more than one replay player")]
    DuplicateFaction(PlayerFaction),
    #[error("invalid AWBW map: {0}")]
    InvalidMap(String),
    #[error("map {axis} {value} exceeds AWVM's 255-tile coordinate domain")]
    MapDimension { axis: &'static str, value: usize },
    #[error("building ({x}, {y}) is outside the {width}x{height} map")]
    BuildingOutOfBounds {
        x: u32,
        y: u32,
        width: usize,
        height: usize,
    },
    #[error("more than one replay building describes ({x}, {y})")]
    DuplicateBuilding { x: u32, y: u32 },
    #[error("property at ({x}, {y}) belongs to faction {faction:?}, which has no player")]
    UnknownPropertyOwner {
        x: usize,
        y: usize,
        faction: PlayerFaction,
    },
    #[error("player {player} has unknown AWBW commander id {commander}")]
    UnknownCommander { player: u32, commander: u32 },
    #[error("unsupported AWBW weather setting {0:?}")]
    UnknownWeatherSetting(String),
    #[error("unsupported AWBW weather code {0:?}")]
    UnknownWeatherCode(String),
    #[error("unit {unit} belongs to unknown replay player {player}")]
    UnknownUnitOwner { unit: u32, player: u32 },
    #[error("unit {unit} has unrepresentable HP {value}")]
    InvalidUnitHp { unit: u32, value: f64 },
    #[error("unit {unit} coordinate ({x}, {y}) is outside AWVM's coordinate domain")]
    UnitCoordinate { unit: u32, x: u32, y: u32 },
    #[error("carried unit {unit} is absent from every transport cargo slot")]
    MissingTransport { unit: u32 },
    #[error("unit {unit} appears in multiple transport cargo slots")]
    DuplicateCargoSlot { unit: u32 },
    #[error("unit {0} appears more than once")]
    DuplicateUnit(u32),
    #[error("could not construct the AWVM board: {0}")]
    Board(String),
    #[error("adapted state violates an AWVM invariant: {0}")]
    InvalidState(#[source] StateInvariant),
}

fn ensure_unique_players(players: &[&AwbwPlayer]) -> Result<(), AdapterError> {
    let mut ids = HashSet::with_capacity(players.len());
    let mut factions = HashSet::with_capacity(players.len());
    for player in players {
        if !ids.insert(player.id) {
            return Err(AdapterError::DuplicatePlayer(player.id.as_u32()));
        }
        if !factions.insert(player.faction) {
            return Err(AdapterError::DuplicateFaction(player.faction));
        }
    }
    Ok(())
}

fn lower_player(game: &AwbwGame, player: &AwbwPlayer) -> Result<Player, AdapterError> {
    let lead = player.co().ok_or(AdapterError::UnknownCommander {
        player: player.id.as_u32(),
        commander: player.co_id.as_u32(),
    })?;
    let mut commanders = vec![Commander {
        id: lead,
        active: true,
        power_charge: awbw_power_charge(player.co_power),
        power_uses: 0,
    }];
    if let Some(tag_id) = player.tags_co_id {
        let tag = player.tag_co().ok_or(AdapterError::UnknownCommander {
            player: player.id.as_u32(),
            commander: tag_id.as_u32(),
        })?;
        commanders.push(Commander {
            id: tag,
            active: false,
            power_charge: awbw_power_charge(player.tags_co_power.unwrap_or(0)),
            power_uses: 0,
        });
    }

    Ok(Player::new(player_id(player.id), team_id(game, player))
        .with_funds(u64::from(player.funds))
        .with_status(if player.eliminated {
            PlayerStatus::Eliminated
        } else {
            PlayerStatus::Active
        })
        .with_commanders(commanders)
        .with_power_state(match player.co_power_on {
            CoPower::None => PowerState::None,
            CoPower::Power => PowerState::Cop { commander_slot: 0 },
            CoPower::SuperPower => PowerState::Scop { commander_slot: 0 },
        }))
}

pub(crate) fn awbw_power_charge(value: u32) -> u64 {
    u64::from(value / 10)
}

fn lower_teams(players: &[Player]) -> Vec<Team> {
    let mut teams = Vec::<Team>::new();
    for player in players {
        if let Some(team) = teams.iter_mut().find(|team| team.id == player.team) {
            if player.status == PlayerStatus::Active {
                team.status = TeamStatus::Active;
            }
        } else {
            teams.push(Team {
                id: player.team.clone(),
                status: if player.status == PlayerStatus::Active {
                    TeamStatus::Active
                } else {
                    TeamStatus::Eliminated
                },
            });
        }
    }
    teams
}

fn lower_board(
    game: &AwbwGame,
    map: &AwbwMap,
    faction_seats: &HashMap<PlayerFaction, PlayerIdx>,
) -> Result<Board, AdapterError> {
    let width = u8::try_from(map.width()).map_err(|_| AdapterError::MapDimension {
        axis: "width",
        value: map.width(),
    })?;
    let height = u8::try_from(map.height()).map_err(|_| AdapterError::MapDimension {
        axis: "height",
        value: map.height(),
    })?;
    let mut buildings = HashMap::with_capacity(game.buildings.len());
    for building in &game.buildings {
        if building.x as usize >= map.width() || building.y as usize >= map.height() {
            return Err(AdapterError::BuildingOutOfBounds {
                x: building.x,
                y: building.y,
                width: map.width(),
                height: map.height(),
            });
        }
        if buildings
            .insert((building.x, building.y), building)
            .is_some()
        {
            return Err(AdapterError::DuplicateBuilding {
                x: building.x,
                y: building.y,
            });
        }
    }

    let mut tiles = Vec::with_capacity(map.width() * map.height());
    // Seam HP belongs to the board, not to the tile, so it is collected here
    // and applied once the rectangle is built.
    let mut seams = Vec::new();
    for y in 0..map.height() {
        for x in 0..map.width() {
            let position = Position::new(x, y);
            let mut terrain = map
                .terrain_at(position)
                .expect("coordinates inside an AwbwMap rectangle have terrain");
            let building = buildings.get(&(x as u32, y as u32)).copied();
            if let Some(building) = building {
                terrain = building.terrain_id;
            }
            let (tile, seam_hp) = lower_tile(
                terrain,
                building.map(|building| building.capture),
                x,
                y,
                faction_seats,
            )?;
            if let Some(hp) = seam_hp {
                seams.push((Pos::new(x as u8, y as u8), hp));
            }
            tiles.push(tile);
        }
    }
    let mut board =
        Board::new(width, height, tiles).map_err(|error| AdapterError::Board(error.to_string()))?;
    for (position, hp) in seams {
        board.set_destructible_hp(position, Some(hp));
    }
    Ok(board)
}

fn lower_tile(
    terrain: AwbwTerrain,
    building_capture: Option<u32>,
    x: usize,
    y: usize,
    faction_seats: &HashMap<PlayerFaction, PlayerIdx>,
) -> Result<(Tile, Option<u64>), AdapterError> {
    let mut tile = Tile::new(semantic_terrain(terrain));
    if let AwbwTerrain::Property(property) = terrain {
        tile.owner = match property.faction() {
            Faction::Neutral => TileOwner::Neutral,
            Faction::Player(faction) => TileOwner::Owned(
                faction_seats
                    .get(&faction)
                    .copied()
                    .ok_or(AdapterError::UnknownPropertyOwner { x, y, faction })?,
            ),
        };
        // AWBW records remaining capture points, so an untouched property
        // reports the full value. A property archived mid-capture keeps its
        // recorded progress.
        tile.capture_points =
            Some(building_capture.map_or(20, |remaining| remaining.min(20) as u8));
    }
    if let AwbwTerrain::MissileSilo(status) = terrain {
        tile.silo = Some(match status {
            MissileSiloStatus::Loaded => Silo::Ready,
            MissileSiloStatus::Unloaded => Silo::Spent,
        });
    }
    let seam_hp = matches!(terrain, AwbwTerrain::PipeSeam(_))
        .then(|| u64::from(building_capture.unwrap_or(99)));
    Ok((tile, seam_hp))
}

fn lower_units(
    game: &AwbwGame,
    player_seats: &HashMap<AwbwGamePlayerId, PlayerIdx>,
) -> Result<UnitStore, AdapterError> {
    let mut cargo = HashMap::<awbrn_types::AwbwUnitId, (awbrn_types::AwbwUnitId, usize)>::new();
    for transport in &game.units {
        for (slot, cargo_id) in [transport.cargo1_units_id, transport.cargo2_units_id]
            .into_iter()
            .enumerate()
            .filter(|(_, id)| id.as_u32() != 0)
        {
            if cargo.insert(cargo_id, (transport.id, slot)).is_some() {
                return Err(AdapterError::DuplicateCargoSlot {
                    unit: cargo_id.as_u32(),
                });
            }
        }
    }

    let units = game
        .units
        .iter()
        .map(|unit| lower_unit(unit, player_seats, &cargo))
        .collect::<Result<Vec<_>, _>>()?;
    UnitStore::new(units).map_err(|error| AdapterError::DuplicateUnit(error.0.get()))
}

fn lower_unit(
    unit: &AwbwUnit,
    player_seats: &HashMap<AwbwGamePlayerId, PlayerIdx>,
    cargo: &HashMap<awbrn_types::AwbwUnitId, (awbrn_types::AwbwUnitId, usize)>,
) -> Result<Unit, AdapterError> {
    let owner =
        player_seats
            .get(&unit.players_id)
            .copied()
            .ok_or(AdapterError::UnknownUnitOwner {
                unit: unit.id.as_u32(),
                player: unit.players_id.as_u32(),
            })?;
    let location = if unit.carried {
        let (transport, slot) =
            cargo
                .get(&unit.id)
                .copied()
                .ok_or(AdapterError::MissingTransport {
                    unit: unit.id.as_u32(),
                })?;
        Location::Cargo {
            transport: UnitId::new(transport.as_u32()),
            slot,
        }
    } else {
        let x = u8::try_from(unit.x).map_err(|_| AdapterError::UnitCoordinate {
            unit: unit.id.as_u32(),
            x: unit.x,
            y: unit.y,
        })?;
        let y = u8::try_from(unit.y).map_err(|_| AdapterError::UnitCoordinate {
            unit: unit.id.as_u32(),
            x: unit.x,
            y: unit.y,
        })?;
        Location::Board {
            position: Pos::new(x, y),
        }
    };

    Ok(Unit {
        id: UnitId::new(unit.id.as_u32()),
        kind: unit.name,
        owner,
        hp: exact_hp(unit)?,
        fuel: u64::from(unit.fuel),
        ammo: u64::from(unit.ammo),
        action: UnitAction::Ready,
        concealment: if unit.sub_dive {
            Concealment::Hidden
        } else {
            Concealment::Exposed
        },
        location,
    })
}

fn exact_hp(unit: &AwbwUnit) -> Result<u8, AdapterError> {
    let scaled = unit.hit_points * 10.0;
    let rounded = scaled.round();
    if !scaled.is_finite() || !(1.0..=100.0).contains(&rounded) || (scaled - rounded).abs() > 1e-6 {
        return Err(AdapterError::InvalidUnitHp {
            unit: unit.id.as_u32(),
            value: unit.hit_points,
        });
    }
    Ok(rounded as u8)
}

pub(crate) fn player_id(id: AwbwGamePlayerId) -> PlayerId {
    PlayerId::from(id.as_u32().to_string())
}

fn team_id(game: &AwbwGame, player: &AwbwPlayer) -> TeamId {
    if game.team {
        TeamId::from(format!("team:{}", player.team))
    } else {
        TeamId::from(format!("player:{}", player.id.as_u32()))
    }
}

fn weather_setting(value: &str) -> Result<WeatherSetting, AdapterError> {
    match value.to_ascii_lowercase().as_str() {
        "clear" => Ok(WeatherSetting::Clear),
        "rain" => Ok(WeatherSetting::Rain),
        "snow" => Ok(WeatherSetting::Snow),
        "random" => Ok(WeatherSetting::Random),
        _ => Err(AdapterError::UnknownWeatherSetting(value.into())),
    }
}

fn weather_kind(value: &str) -> Result<WeatherKind, AdapterError> {
    match value.to_ascii_lowercase().as_str() {
        "c" | "clear" => Ok(WeatherKind::Clear),
        "r" | "rain" => Ok(WeatherKind::Rain),
        "s" | "snow" => Ok(WeatherKind::Snow),
        _ => Err(AdapterError::UnknownWeatherCode(value.into())),
    }
}

/// Erase AWBW's graphical terrain spelling to the ruleset's semantic terrain.
pub fn semantic_terrain(terrain: AwbwTerrain) -> Terrain {
    match terrain {
        AwbwTerrain::Plain | AwbwTerrain::PipeRubble(_) => Terrain::Plain,
        AwbwTerrain::Mountain => Terrain::Mountain,
        AwbwTerrain::Wood => Terrain::Wood,
        AwbwTerrain::River(_) => Terrain::River,
        AwbwTerrain::Road(_) => Terrain::Road,
        AwbwTerrain::Bridge(_) => Terrain::Bridge,
        AwbwTerrain::Sea => Terrain::Sea,
        AwbwTerrain::Shoal(_) => Terrain::Shoal,
        AwbwTerrain::Reef => Terrain::Reef,
        AwbwTerrain::Property(property) => match property {
            Property::City(_) => Terrain::City,
            Property::Base(_) => Terrain::Base,
            Property::Airport(_) => Terrain::Airport,
            Property::Port(_) => Terrain::Port,
            Property::ComTower(_) => Terrain::ComTower,
            Property::Lab(_) => Terrain::Lab,
            Property::HQ(_) => Terrain::Hq,
        },
        AwbwTerrain::Pipe(_) => Terrain::Pipe,
        AwbwTerrain::MissileSilo(_) => Terrain::MissileSilo,
        AwbwTerrain::PipeSeam(_) => Terrain::PipeSeam,
        AwbwTerrain::Teleporter => Terrain::Teleporter,
    }
}
