//! Adapter between the server's compatibility vocabulary and authoritative
//! AWVM state, commands, events, and replay randomness.

use std::collections::HashMap;

use awbrn_map::{AwbrnMap, Position};
use awbrn_types::{
    AwbwTerrain, Co, Faction, GraphicalTerrain, MissileSiloStatus, PlayerFaction, Property,
    Unit as ServerUnit,
};
use awvm::event::{AttackTarget, Event};
use awvm::random::{RandomToken, Recording};
use awvm::ruleset::{CommanderKind, RULESET_ID, RULESET_REVISION, Terrain, UnitKind, WeatherKind};
use awvm::semantic::{
    Board, Commander, CommanderBans, Concealment, Location, Match, Phase, Player, PlayerId,
    PlayerStatus, Pos, PowerState, RulesetId, RulesetRef, RulesetRevision, Settings, Silo, State,
    Team, TeamId, TeamStatus, Tile, TileOwner, Toggle, Turn, Unit, UnitAction, UnitId, UnitStore,
    Weather, WeatherSetting,
};
use awvm::transition::{Command, ExecuteError, ExecuteOutcome, execute, execute_with};

use crate::command::{GameCommand, PostMoveAction};
use crate::error::CommandError;
use crate::player::PlayerId as ServerPlayerId;
use crate::setup::{GameRng, GameSetup, SetupError};
use crate::unit_id::ServerUnitId;

pub(crate) struct Authority {
    state: State,
    entropy: Recording<GameRng>,
    last_random: Vec<RandomToken>,
    faction_players: HashMap<PlayerFaction, PlayerId>,
    player_factions: Vec<PlayerFaction>,
    map: AwbrnMap,
}

pub(crate) struct AcceptedTransition {
    pub(crate) prior: State,
    pub(crate) events: Vec<Event>,
}

impl Authority {
    pub(crate) fn new(setup: &GameSetup) -> Result<Self, SetupError> {
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
        let faction_players = faction_players(setup);
        Ok(Self {
            state: state_from_setup(setup)?,
            entropy: Recording::new(GameRng::from_seed(setup.rng_seed)),
            last_random: Vec::new(),
            faction_players,
            player_factions: setup.players.iter().map(|player| player.faction).collect(),
            map: setup.map.clone(),
        })
    }

    pub(crate) fn execute(
        &mut self,
        player: ServerPlayerId,
        command: &GameCommand,
    ) -> Result<AcceptedTransition, CommandError> {
        let commands = commands(player, command, &self.state)?;
        let tape_start = self.entropy.tokens().len();
        let entropy_before = self.entropy.clone();
        let mut prior = None;
        let mut events = Vec::new();

        for command in commands {
            let context = command.clone();
            match execute_with(&self.state, command, &mut self.entropy) {
                Ok(ExecuteOutcome::Accepted(execution)) => {
                    let previous = std::mem::replace(&mut self.state, execution.state);
                    prior.get_or_insert(previous);
                    events.extend(execution.events);
                }
                Ok(ExecuteOutcome::Rejected(violation)) => {
                    if let Some(prior) = prior {
                        self.state = prior;
                    }
                    self.entropy = entropy_before;
                    return Err(command_error(&context, violation));
                }
                Err(error) => {
                    if let Some(prior) = prior {
                        self.state = prior;
                    }
                    self.entropy = entropy_before;
                    return Err(execute_error(error));
                }
            }
        }

        self.last_random = self.entropy.tokens()[tape_start..].to_vec();
        Ok(AcceptedTransition {
            prior: prior.expect("every server command lowers to at least one AWVM command"),
            events,
        })
    }

    pub(crate) fn execute_recorded(
        &mut self,
        player: ServerPlayerId,
        command: &GameCommand,
        random: &[RandomToken],
    ) -> Result<AcceptedTransition, CommandError> {
        let commands = commands(player, command, &self.state)?;
        let mut consumed = 0;
        let mut prior = None;
        let mut events = Vec::new();
        for command in commands {
            match execute(&self.state, command.clone(), &random[consumed..]) {
                Ok(ExecuteOutcome::Accepted(execution)) => {
                    consumed += execution.random_consumed;
                    let previous = std::mem::replace(&mut self.state, execution.state);
                    prior.get_or_insert(previous);
                    events.extend(execution.events);
                }
                Ok(ExecuteOutcome::Rejected(violation)) => {
                    if let Some(prior) = prior {
                        self.state = prior;
                    }
                    return Err(command_error(&command, violation));
                }
                Err(error) => {
                    if let Some(prior) = prior {
                        self.state = prior;
                    }
                    return Err(execute_error(error));
                }
            }
        }
        if consumed != random.len() {
            if let Some(prior) = prior {
                self.state = prior;
            }
            return Err(CommandError::InvalidAction {
                reason: format!(
                    "recorded command consumed {consumed} of {} random tokens",
                    random.len()
                ),
            });
        }
        self.last_random = random.to_vec();
        Ok(AcceptedTransition {
            prior: prior.expect("every server command lowers to at least one AWVM command"),
            events,
        })
    }

    pub(crate) fn spawn_unit(
        &mut self,
        id: ServerUnitId,
        position: Position,
        kind: ServerUnit,
        faction: PlayerFaction,
        active: bool,
    ) {
        let owner = self
            .faction_players
            .get(&faction)
            .cloned()
            .unwrap_or_else(|| panic!("spawned unit faction {faction:?} has no player"));
        let id = unit_id(id);
        self.state.units.push(Unit {
            id,
            kind: unit_kind(kind),
            owner,
            hp: 100,
            fuel: u64::from(kind.max_fuel()),
            ammo: u64::from(kind.max_ammo()),
            action: if active {
                UnitAction::Ready
            } else {
                UnitAction::Spent
            },
            concealment: Concealment::Exposed,
            location: Location::Board {
                position: pos(position),
            },
        });
        let next = id
            .get()
            .checked_add(1)
            .expect("server unit id exceeds AWVM's identifier domain");
        self.state.next_unit_id = Some(self.state.next_unit_id.unwrap_or(1).max(next));
    }

    pub(crate) fn random_tokens(&self) -> &[RandomToken] {
        self.entropy.tokens()
    }

    pub(crate) fn last_random_tokens(&self) -> &[RandomToken] {
        &self.last_random
    }

    pub(crate) fn state(&self) -> &State {
        &self.state
    }

    pub(crate) fn map(&self) -> &AwbrnMap {
        &self.map
    }

    pub(crate) fn player_faction(&self, player: &PlayerId) -> Option<PlayerFaction> {
        player
            .as_str()
            .parse::<usize>()
            .ok()
            .and_then(|index| self.player_factions.get(index))
            .copied()
    }

    pub(crate) fn players(&self) -> impl Iterator<Item = ServerPlayerId> + '_ {
        (0..self.player_factions.len()).map(|index| ServerPlayerId(index as u8))
    }

    pub(crate) fn player(&self, player: ServerPlayerId) -> PlayerId {
        player_id(player)
    }
}

fn execute_error(error: ExecuteError) -> CommandError {
    CommandError::InvalidAction {
        reason: format!("AWVM execution failed: {error}"),
    }
}

fn command_error(command: &Command, violation: awvm::violation::Violation) -> CommandError {
    use awvm::violation::Violation;

    match violation {
        Violation::MatchFinished => CommandError::GameOver,
        Violation::WrongPhase { .. } | Violation::NotActivePlayer { .. } => {
            CommandError::NotYourTurn
        }
        Violation::UnitNotFound { unit }
        | Violation::UnitNotOnBoard { unit }
        | Violation::UnitNotOwned { unit, .. } => {
            CommandError::InvalidUnit(ServerUnitId(u64::from(unit.get())))
        }
        Violation::UnitAlreadyActed { unit } => {
            CommandError::UnitAlreadyActed(ServerUnitId(u64::from(unit.get())))
        }
        Violation::InsufficientFunds {
            required,
            available,
        } => CommandError::InsufficientFunds {
            cost: u32::try_from(required).unwrap_or(u32::MAX),
            available: u32::try_from(available).unwrap_or(u32::MAX),
        },
        Violation::InvalidTarget { .. } if matches!(command, Command::ProduceUnit { .. }) => {
            CommandError::InvalidBuildLocation
        }
        Violation::DestinationOccupied { .. } if matches!(command, Command::ProduceUnit { .. }) => {
            CommandError::InvalidBuildLocation
        }
        violation @ (Violation::DestinationOccupied { .. } | Violation::PathOccupied { .. })
            if matches!(command, Command::Unload { .. } | Command::MoveJoin { .. }) =>
        {
            CommandError::InvalidAction {
                reason: format!("{violation:?}"),
            }
        }
        violation @ (Violation::PathOriginMismatch { .. }
        | Violation::PathNonAdjacent { .. }
        | Violation::PathRepeatedPosition { .. }
        | Violation::PathOutOfBounds { .. }
        | Violation::TerrainImpassable { .. }
        | Violation::PathOccupied { .. }
        | Violation::InsufficientMovement { .. }
        | Violation::InsufficientFuel { .. }
        | Violation::DestinationOccupied { .. }) => CommandError::InvalidPath {
            reason: format!("{violation:?}"),
        },
        violation => CommandError::InvalidAction {
            reason: format!("{violation:?}"),
        },
    }
}

fn commands(
    player: ServerPlayerId,
    command: &GameCommand,
    state: &State,
) -> Result<Vec<Command>, CommandError> {
    let player = player_id(player);
    let one = |command| Ok(vec![command]);
    match command {
        GameCommand::Build {
            position,
            unit_type,
        } => one(Command::ProduceUnit {
            player,
            position: pos(*position),
            kind: unit_kind(*unit_type),
        }),
        GameCommand::EndTurn => one(Command::EndTurn { player }),
        GameCommand::MoveUnit {
            unit_id: server_unit_id,
            path,
            action,
        } => {
            let unit = unit_id(*server_unit_id);
            let path = path.iter().copied().map(pos).collect::<Vec<_>>();
            match action {
                Some(PostMoveAction::Attack { target }) => {
                    let target_position = pos(*target);
                    let target = state
                        .units
                        .iter()
                        .find(|candidate| {
                            matches!(
                                candidate.location,
                                Location::Board { position } if position == target_position
                            )
                        })
                        .map_or(
                            AttackTarget::Tile {
                                position: target_position,
                            },
                            |target| AttackTarget::Unit { unit: target.id },
                        );
                    one(Command::MoveAttack {
                        player,
                        unit,
                        path,
                        target,
                    })
                }
                Some(PostMoveAction::Capture) => one(Command::MoveCapture { player, unit, path }),
                Some(PostMoveAction::Load { transport_id }) => one(Command::MoveLoad {
                    player,
                    unit,
                    path,
                    transport: unit_id(*transport_id),
                }),
                Some(PostMoveAction::Unload { cargo_id, position }) => Ok(vec![
                    Command::MoveWait {
                        player: player.clone(),
                        unit,
                        path,
                    },
                    Command::Unload {
                        player,
                        transport: unit,
                        cargo: unit_id(*cargo_id),
                        destination: pos(*position),
                    },
                ]),
                Some(PostMoveAction::Supply) => one(Command::MoveSupply { player, unit, path }),
                Some(PostMoveAction::Hide) => one(Command::MoveHide { player, unit, path }),
                Some(PostMoveAction::Unhide) => one(Command::MoveReveal { player, unit, path }),
                Some(PostMoveAction::Join { target_id }) => one(Command::MoveJoin {
                    player,
                    unit,
                    path,
                    target: unit_id(*target_id),
                }),
                Some(PostMoveAction::Wait) | None => one(Command::MoveWait { player, unit, path }),
            }
        }
    }
}

fn state_from_setup(setup: &GameSetup) -> Result<State, SetupError> {
    let starting_funds = u64::from(
        setup
            .players
            .first()
            .expect("the server validates a non-empty roster")
            .starting_funds,
    );
    let players = setup
        .players
        .iter()
        .enumerate()
        .map(|(index, player)| {
            let id = player_id(ServerPlayerId(index as u8));
            Player {
                id,
                team: team_id(index, player.team.map(|team| team.get())),
                funds: u64::from(player.starting_funds),
                status: PlayerStatus::Active,
                commanders: vec![Commander {
                    id: commander(player.co),
                    active: true,
                    power_charge: 0,
                    power_uses: 0,
                }],
                power_state: PowerState::None,
            }
        })
        .collect::<Vec<_>>();
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
    let active_player = players
        .first()
        .expect("the server validates a non-empty roster")
        .id
        .clone();

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
        board: board(&setup.map, &faction_players)?,
        teams,
        players: players.clone(),
        turn: Turn {
            day: 1,
            active_player,
            phase: Phase::UnitAction,
            order: players.into_iter().map(|player| player.id).collect(),
            position: 0,
        },
        weather: Weather {
            kind: WeatherKind::Clear,
            remaining_turns: 0,
        },
        units: UnitStore::default(),
        next_unit_id: Some(1),
        match_state: Match::Active {
            draw_offers: Vec::new(),
        },
    })
}

fn board(
    map: &AwbrnMap,
    faction_players: &HashMap<PlayerFaction, PlayerId>,
) -> Result<Board, SetupError> {
    let width = u8::try_from(map.width()).map_err(|_| SetupError::InvalidMap {
        reason: format!("map width {} exceeds AWVM's 255-tile limit", map.width()),
    })?;
    let height = u8::try_from(map.height()).map_err(|_| SetupError::InvalidMap {
        reason: format!("map height {} exceeds AWVM's 255-tile limit", map.height()),
    })?;
    let mut tiles = Vec::with_capacity(map.width() * map.height());
    for y in 0..map.height() {
        for x in 0..map.width() {
            let terrain = map
                .terrain_at(Position::new(x, y))
                .expect("map coordinates inside its rectangle have terrain");
            tiles.push(tile(terrain, faction_players));
        }
    }
    Board::new(width, height, tiles).map_err(|error| SetupError::InvalidMap {
        reason: error.to_string(),
    })
}

fn tile(graphical: GraphicalTerrain, faction_players: &HashMap<PlayerFaction, PlayerId>) -> Tile {
    let terrain = graphical.as_terrain();
    let mut tile = Tile::new(semantic_terrain(terrain));
    if let AwbwTerrain::Property(property) = terrain {
        tile.owner = match property.faction() {
            Faction::Neutral => TileOwner::Neutral,
            Faction::Player(faction) => faction_players
                .get(&faction)
                .cloned()
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
    if matches!(terrain, AwbwTerrain::PipeSeam(_)) {
        tile.set_destructible_hp(Some(99));
    }
    tile
}

pub(crate) fn semantic_terrain(terrain: AwbwTerrain) -> Terrain {
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

fn commander(co: Co) -> CommanderKind {
    match co {
        Co::Andy => CommanderKind::Andy,
        Co::Nell => CommanderKind::Nell,
        Co::Hachi => CommanderKind::Hachi,
        Co::Jake => CommanderKind::Jake,
        Co::Rachel => CommanderKind::Rachel,
        Co::Colin => CommanderKind::Colin,
        Co::Sasha => CommanderKind::Sasha,
        Co::Grimm => CommanderKind::Grimm,
        Co::Grit => CommanderKind::Grit,
        Co::Olaf => CommanderKind::Olaf,
        Co::Eagle => CommanderKind::Eagle,
        Co::Drake => CommanderKind::Drake,
        Co::Jess => CommanderKind::Jess,
        Co::Javier => CommanderKind::Javier,
        Co::Max => CommanderKind::Max,
        Co::Adder => CommanderKind::Adder,
        Co::Flak => CommanderKind::Flak,
        Co::Lash => CommanderKind::Lash,
        Co::Hawke => CommanderKind::Hawke,
        Co::Jugger => CommanderKind::Jugger,
        Co::Kindle => CommanderKind::Kindle,
        Co::Koal => CommanderKind::Koal,
        Co::Sami => CommanderKind::Sami,
        Co::Sonja => CommanderKind::Sonja,
        Co::Kanbei => CommanderKind::Kanbei,
        Co::Sensei => CommanderKind::Sensei,
        Co::Sturm => CommanderKind::Sturm,
        Co::VonBolt => CommanderKind::VonBolt,
        Co::NoCo => CommanderKind::Neutral,
    }
}

fn unit_kind(unit: ServerUnit) -> UnitKind {
    match unit {
        ServerUnit::AntiAir => UnitKind::AntiAir,
        ServerUnit::APC => UnitKind::Apc,
        ServerUnit::Artillery => UnitKind::Artillery,
        ServerUnit::BCopter => UnitKind::BCopter,
        ServerUnit::Battleship => UnitKind::Battleship,
        ServerUnit::BlackBoat => UnitKind::BlackBoat,
        ServerUnit::BlackBomb => UnitKind::BlackBomb,
        ServerUnit::Bomber => UnitKind::Bomber,
        ServerUnit::Carrier => UnitKind::Carrier,
        ServerUnit::Cruiser => UnitKind::Cruiser,
        ServerUnit::Fighter => UnitKind::Fighter,
        ServerUnit::Infantry => UnitKind::Infantry,
        ServerUnit::Lander => UnitKind::Lander,
        ServerUnit::MdTank => UnitKind::MdTank,
        ServerUnit::Mech => UnitKind::Mech,
        ServerUnit::MegaTank => UnitKind::MegaTank,
        ServerUnit::Missile => UnitKind::Missile,
        ServerUnit::NeoTank => UnitKind::NeoTank,
        ServerUnit::PipeRunner => UnitKind::Piperunner,
        ServerUnit::Recon => UnitKind::Recon,
        ServerUnit::Rocket => UnitKind::Rocket,
        ServerUnit::Stealth => UnitKind::Stealth,
        ServerUnit::Sub => UnitKind::Sub,
        ServerUnit::TCopter => UnitKind::TCopter,
        ServerUnit::Tank => UnitKind::Tank,
    }
}

fn player_id(player: ServerPlayerId) -> PlayerId {
    PlayerId::from(player.0.to_string())
}

fn team_id(index: usize, team: Option<u8>) -> TeamId {
    match team {
        Some(team) => TeamId::from(format!("team-{team}")),
        None => TeamId::from(format!("player-{index}")),
    }
}

fn faction_players(setup: &GameSetup) -> HashMap<PlayerFaction, PlayerId> {
    let mut players = HashMap::new();
    for (index, player) in setup.players.iter().enumerate() {
        // `PlayerRegistry::player_for_faction` resolves the first matching
        // seat, so preserve that behavior if a malformed setup repeats one.
        players
            .entry(player.faction)
            .or_insert_with(|| player_id(ServerPlayerId(index as u8)));
    }
    players
}

fn unit_id(id: ServerUnitId) -> UnitId {
    UnitId::new(u32::try_from(id.0).expect("server unit id exceeds AWVM's identifier domain"))
}

fn pos(position: Position) -> Pos {
    Pos::new(
        u8::try_from(position.x).expect("validated x coordinate fits AWVM"),
        u8::try_from(position.y).expect("validated y coordinate fits AWVM"),
    )
}
