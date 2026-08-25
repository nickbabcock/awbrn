//! The conversion from a setup to an AWVM state.

use std::collections::HashMap;

use awbrn_types::{
    AwbwTerrain, Faction, GraphicalTerrain, MissileSiloStatus, PlayerFaction, Property,
};
use awvm::ruleset::{RULESET_ID, RULESET_REVISION, Terrain};
use awvm::semantic::{
    Board, CommanderBans, Player, PlayerId, PlayerIdx, Roster, RulesetId, RulesetRef,
    RulesetRevision, Settings, Silo, State, TeamId, Tile, TileOwner, Toggle, WeatherSetting,
};
use awvm::setup::{MatchSetup, PlayerSetup as AwvmPlayerSetup, UnitDeployment};

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
    let setup_players = setup
        .players
        .iter()
        .enumerate()
        .map(|(index, player)| {
            AwvmPlayerSetup::new(
                semantic_player_id(index),
                team_id(index, player.team.map(|team| team.get())),
                u64::from(player.starting_funds),
                vec![player.co],
            )
            .map_err(|error| SetupError::InvalidPlayers {
                reason: error.to_string(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let players = Roster::new(
        setup_players
            .iter()
            .map(|player| Player::new(player.id().clone(), player.team().clone()))
            .collect(),
    )
    .map_err(|error| SetupError::InvalidPlayers {
        reason: error.to_string(),
    })?;
    let faction_players = faction_players(setup);
    // A held tile names a seat, and `players` below is the roster those seats
    // index, so a faction maps straight to the seat its player sits in.
    let faction_seats: HashMap<PlayerFaction, PlayerIdx> = faction_players
        .iter()
        .filter_map(|(faction, id)| Some((*faction, players.seat(id)?)))
        .collect();
    let match_setup = MatchSetup::new(
        RulesetRef {
            id: RulesetId::from(RULESET_ID),
            revision: RulesetRevision::from(RULESET_REVISION),
        },
        Settings {
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
        board(&setup.map, &faction_seats)?,
        setup_players,
        deployed_units(setup, &faction_seats, &players)?,
    )
    .map_err(|error| SetupError::InvalidMap {
        reason: error.to_string(),
    })?;
    awvm::transition::initialize_match(match_setup, &[])
        .map(|execution| execution.state)
        .map_err(|error| SetupError::InvalidMap {
            reason: format!("the map cannot open a match: {error}"),
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
    players: &Roster,
) -> Result<Vec<UnitDeployment>, SetupError> {
    let deployments = setup.map.deployments();
    let mut units = Vec::with_capacity(deployments.len());

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
        units.push(
            UnitDeployment::new(
                deployed.unit,
                players[owner.get()].id().clone(),
                // The map file writes health on the 0 to 10 scale; the reducer
                // counts on the 0 to 100 one.
                deployed.hp.get() * 10,
                position,
            )
            .map_err(|error| SetupError::InvalidMap {
                reason: error.to_string(),
            })?,
        );
    }
    Ok(units)
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
        tile.capture_points = Some(awvm::semantic::CAPTURE_REQUIRED_POINTS);
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

#[cfg(test)]
mod tests {
    use super::*;
    use awbrn_map::{AwbwMap, Deployments};
    use awbrn_types::Co;
    use awvm::semantic::Dimensions;

    use crate::setup::PlayerSetup;

    const STARTING_FUNDS: u32 = 500;

    fn orange(property: fn(Faction) -> Property) -> AwbwTerrain {
        AwbwTerrain::Property(property(Faction::Player(PlayerFaction::OrangeStar)))
    }

    /// A one-row board of `terrain`, with two seats on it.
    fn setup(terrain: Vec<AwbwTerrain>) -> GameSetup {
        let dimensions = Dimensions::new(
            u8::try_from(terrain.len()).expect("the test board is small"),
            1,
        );
        let map = AwbwMap::from_parts(dimensions, terrain, Deployments::new(dimensions))
            .expect("the test board is a rectangle");
        GameSetup {
            map: AwbrnMap::from_map(&map),
            players: [PlayerFaction::OrangeStar, PlayerFaction::BlueMoon]
                .into_iter()
                .map(|faction| PlayerSetup {
                    faction,
                    team: None,
                    starting_funds: STARTING_FUNDS,
                    co: Co::Andy,
                })
                .collect(),
            fog_enabled: false,
            rng_seed: 1,
        }
    }

    fn funds(state: &State) -> Vec<u64> {
        state.players.iter().map(|player| player.funds).collect()
    }

    /// The first player opens the match inside their own turn-start, so the
    /// properties the map gives them pay before they act.
    #[test]
    fn the_first_player_collects_day_one_income() {
        let state = state_from_setup(&setup(vec![
            AwbwTerrain::Property(Property::HQ(PlayerFaction::OrangeStar)),
            orange(Property::City),
            orange(Property::Base),
            AwbwTerrain::Plain,
        ]))
        .expect("the test setup is valid");

        assert_eq!(
            funds(&state),
            vec![u64::from(STARTING_FUNDS) + 3_000, u64::from(STARTING_FUNDS)],
            "the first player holds their starting funds and three properties of income"
        );
    }

    /// Every other seat collects at the boundary that opens its turn, not here.
    #[test]
    fn a_later_player_collects_nothing_at_setup() {
        let state = state_from_setup(&setup(vec![
            AwbwTerrain::Property(Property::City(Faction::Player(PlayerFaction::BlueMoon))),
            AwbwTerrain::Plain,
        ]))
        .expect("the test setup is valid");

        assert_eq!(
            funds(&state),
            vec![u64::from(STARTING_FUNDS); 2],
            "a property of the second player pays at their own turn-start"
        );
    }

    /// A com tower and a lab are ownable and carry no income trait.
    #[test]
    fn a_com_tower_and_a_lab_pay_no_day_one_income() {
        let state = state_from_setup(&setup(vec![
            orange(Property::ComTower),
            orange(Property::Lab),
            AwbwTerrain::Property(Property::City(Faction::Neutral)),
            AwbwTerrain::Plain,
        ]))
        .expect("the test setup is valid");

        assert_eq!(
            funds(&state),
            vec![u64::from(STARTING_FUNDS); 2],
            "neither tower, lab, nor neutral city pays income"
        );
    }

    /// A first player with no property still opens with the funds they were
    /// given, and an infantry stays out of reach until they capture something.
    #[test]
    fn a_propertyless_first_player_collects_nothing() {
        let state = state_from_setup(&setup(vec![AwbwTerrain::Plain, AwbwTerrain::Plain]))
            .expect("the test setup is valid");

        assert_eq!(funds(&state), vec![u64::from(STARTING_FUNDS); 2]);
    }
}
