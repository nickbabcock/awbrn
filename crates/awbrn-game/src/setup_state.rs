//! The conversion from a setup to an AWVM state.

use std::collections::HashMap;

use awbrn_types::{
    AwbwTerrain, Faction, GraphicalTerrain, MissileSiloStatus, PlayerFaction, Property,
};
use awvm::ruleset::{RULESET_ID, RULESET_REVISION, Terrain, WeatherKind, profile};
use awvm::semantic::{
    Board, Commander, CommanderBans, Concealment, Location, Match, Phase, Player, PlayerId,
    PlayerIdx, Roster, RulesetId, RulesetRef, RulesetRevision, Settings, Silo, State, Team, TeamId,
    TeamStatus, Tile, TileOwner, Toggle, Turn, Unit, UnitAction, UnitId, UnitStore, Weather,
    WeatherSetting,
};

use crate::setup::{GameSetup, SetupError};
use awbrn_map::AwbrnMap;

/// Converts a game setup to an AWVM state.
///
/// This function does not perform server work. It does not compute fog or
/// record entropy.
pub fn state_from_setup(setup: &GameSetup) -> Result<State, SetupError> {
    if setup.players.is_empty() {
        return Err(SetupError::InvalidPlayers {
            reason: "game must contain at least one player".into(),
        });
    }
    if setup.players.len() > u8::MAX as usize {
        return Err(SetupError::InvalidPlayers {
            reason: format!(
                "game supports at most {} players, got {}",
                u8::MAX,
                setup.players.len()
            ),
        });
    }

    let starting_funds = u64::from(setup.players[0].starting_funds);
    let players = setup
        .players
        .iter()
        .enumerate()
        .map(|(index, player)| {
            let id = semantic_player_id(index);
            Player::new(id, team_id(index, player.team.map(|team| team.get())))
                .with_funds(u64::from(player.starting_funds))
                .with_commanders(vec![Commander {
                    id: player.co,
                    active: true,
                    power_charge: 0,
                    power_uses: 0,
                }])
        })
        .collect::<Vec<_>>();
    let players = Roster::new(players).map_err(|error| SetupError::InvalidPlayers {
        reason: error.to_string(),
    })?;
    let teams = players
        .iter()
        .fold(Vec::<Team>::new(), |mut teams, player| {
            if !teams.iter().any(|team| team.id == player.team) {
                teams.push(Team {
                    id: player.team.clone(),
                    status: TeamStatus::Active,
                });
            }
            teams
        });
    let faction_players = faction_players(setup);
    // A held tile names a seat, and `players` below is the roster those seats
    // index, so a faction maps straight to the seat its player sits in.
    let faction_seats: HashMap<PlayerFaction, PlayerIdx> = faction_players
        .iter()
        .filter_map(|(faction, id)| Some((*faction, players.seat(id)?)))
        .collect();
    let active_player = players[0].id().clone();
    let (units, next_unit_id) = deployed_units(setup, &faction_seats)?;

    Ok(State {
        ruleset: RulesetRef {
            id: RulesetId::from(RULESET_ID),
            revision: RulesetRevision::from(RULESET_REVISION),
        },
        settings: Settings {
            fog: setup.fog_enabled,
            income_per_property: 1_000,
            starting_funds,
            powers: Toggle::Enabled,
            tags: false,
            weather: WeatherSetting::Clear,
            lab_units: Vec::new(),
            unit_bans: Vec::new(),
            commander_bans: CommanderBans {
                lead: Vec::new(),
                backup: Vec::new(),
            },
            capture_limit: None,
            day_limit: None,
            unit_limit: None,
        },
        board: board(&setup.map, &faction_seats)?,
        teams,
        players: players.clone(),
        turn: Turn {
            day: 1,
            active_player,
            phase: Phase::UnitAction,
            order: players.iter().map(|player| player.id().clone()).collect(),
            position: 0,
        },
        weather: Weather {
            kind: WeatherKind::Clear,
            remaining_turns: 0,
        },
        units,
        next_unit_id: Some(next_unit_id),
        match_state: Match::Active {
            draw_offers: Vec::new(),
        },
    })
}

/// The units the map starts on the board.
///
/// Identifiers are handed out in the order the map file lists them, from 1, and
/// `next_unit_id` continues past the last one. Every unit is ready to act:
/// AWBW gives a predeployed unit its first turn like any other.
fn deployed_units(
    setup: &GameSetup,
    faction_seats: &HashMap<PlayerFaction, PlayerIdx>,
) -> Result<(UnitStore, u32), SetupError> {
    let deployments = setup.map.deployments();
    let mut units = Vec::with_capacity(deployments.len());
    let mut next = 1u32;

    for (position, deployed) in deployments.iter() {
        let owner =
            *faction_seats
                .get(&deployed.faction)
                .ok_or_else(|| SetupError::InvalidMap {
                    reason: format!(
                        "the map starts a {:?} unit and no player holds that faction",
                        deployed.faction
                    ),
                })?;
        let profile = profile(deployed.unit);
        units.push(Unit {
            id: UnitId::new(next),
            kind: deployed.unit,
            owner,
            // The map file writes health on the 0 to 10 scale; the reducer
            // counts on the 0 to 100 one.
            hp: deployed.hp.get() * 10,
            fuel: profile.max_fuel,
            ammo: profile.max_ammo,
            action: UnitAction::Ready,
            concealment: Concealment::Exposed,
            location: Location::Board { position },
        });
        next = next.checked_add(1).ok_or_else(|| SetupError::InvalidMap {
            reason: "the map starts more units than an identifier can name".to_owned(),
        })?;
    }

    let units = UnitStore::new(units).map_err(|error| SetupError::InvalidMap {
        reason: error.to_string(),
    })?;
    Ok((units, next))
}

fn board(
    map: &AwbrnMap,
    faction_seats: &HashMap<PlayerFaction, PlayerIdx>,
) -> Result<Board, SetupError> {
    let dimensions = map.dimensions();
    let mut tiles = Vec::with_capacity(dimensions.len());
    // Seam HP belongs to the board, not to the tile, so it is collected here
    // and applied once the rectangle is built.
    let mut seams = Vec::new();
    for (position, terrain) in map.iter() {
        let (built, seam_hp) = tile(terrain, faction_seats);
        if let Some(hp) = seam_hp {
            seams.push((position, hp));
        }
        tiles.push(built);
    }
    let mut board =
        Board::new(dimensions.width(), dimensions.height(), tiles).map_err(|error| {
            SetupError::InvalidMap {
                reason: error.to_string(),
            }
        })?;
    for (position, hp) in seams {
        board.set_destructible_hp(position, Some(hp));
    }
    Ok(board)
}

fn tile(
    graphical: GraphicalTerrain,
    faction_seats: &HashMap<PlayerFaction, PlayerIdx>,
) -> (Tile, Option<u64>) {
    let terrain = graphical.as_terrain();
    let mut tile = Tile::new(semantic_terrain(terrain));
    if let AwbwTerrain::Property(property) = terrain {
        tile.owner = match property.faction() {
            Faction::Neutral => TileOwner::Neutral,
            Faction::Player(faction) => faction_seats
                .get(&faction)
                .copied()
                .map_or(TileOwner::Neutral, TileOwner::Owned),
        };
        tile.capture_points = Some(20);
    }
    if let AwbwTerrain::MissileSilo(status) = terrain {
        tile.silo = Some(match status {
            MissileSiloStatus::Loaded => Silo::Ready,
            MissileSiloStatus::Unloaded => Silo::Spent,
        });
    }
    let seam_hp = matches!(terrain, AwbwTerrain::PipeSeam(_)).then_some(99);
    (tile, seam_hp)
}

/// The AWVM terrain an AWBW terrain becomes.
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

/// The identifier of the seat at `index`.
///
/// A seat is named by its position in the roster, so the same setup always
/// gives the same identifiers.
pub fn semantic_player_id(index: usize) -> PlayerId {
    PlayerId::from(index.to_string())
}

fn team_id(index: usize, team: Option<u8>) -> TeamId {
    match team {
        Some(team) => TeamId::from(format!("team-{team}")),
        None => TeamId::from(format!("player-{index}")),
    }
}

/// The seat identifier that each faction in `setup` holds.
pub fn faction_players(setup: &GameSetup) -> HashMap<PlayerFaction, PlayerId> {
    let mut players = HashMap::new();
    for (index, player) in setup.players.iter().enumerate() {
        // A malformed setup can repeat a faction. The first seat wins, which
        // is what the server's own faction lookup does.
        players
            .entry(player.faction)
            .or_insert_with(|| semantic_player_id(index));
    }
    players
}
