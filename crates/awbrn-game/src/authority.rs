//! Adapter between the server's compatibility vocabulary and authoritative
//! AWVM state, commands, events, and replay randomness.

use std::collections::HashMap;

use awbrn_map::AwbrnMap;
use awbrn_types::{PlayerFaction, Unit as ServerUnit};
use awvm::event::{AttackTarget, Event};
use awvm::random::{RandomToken, Recording};
use awvm::ruleset::profile;
use awvm::semantic::{Concealment, Location, PlayerId, Pos, State, Unit, UnitAction, UnitId};
use awvm::transition::{Command, ExecuteError, ExecuteOutcome, execute, execute_with};

use crate::command::{GameCommand, PostMoveAction};
use crate::error::CommandError;
use crate::player::PlayerId as ServerPlayerId;
use crate::unit_id::ServerUnitId;
use crate::{GameSetup, SetupError, faction_players, state_from_setup};

#[derive(Debug)]
pub struct Authority {
    state: State,
    entropy: Recording<GameRng>,
    last_random: Vec<RandomToken>,
    faction_players: HashMap<PlayerFaction, PlayerId>,
    player_factions: Vec<PlayerFaction>,
    map: AwbrnMap,
}

#[derive(Debug)]
pub struct AcceptedTransition {
    pub prior: State,
    pub events: Vec<Event>,
}

impl AcceptedTransition {
    /// Project this transition for one recipient.
    ///
    /// `authority` must be the same [`Authority`] that produced the transition,
    /// and it must have executed no command since. The projection reads the
    /// authority's current state as the post-state, so a later command makes
    /// the result describe a transition that never happened.
    pub fn observe(
        &self,
        authority: &Authority,
        recipient: &PlayerId,
    ) -> Result<awvm::semantic::ObservedTransition, awvm::semantic::ObserveError> {
        awvm::semantic::observe_transition(
            &awvm::semantic::AwbwVisibility,
            &self.prior,
            authority.state(),
            &self.events,
            recipient,
        )
    }
}

impl Authority {
    pub fn new(setup: &GameSetup) -> Result<Self, SetupError> {
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

    pub fn execute(
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

    pub fn execute_recorded(
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

    pub fn spawn_unit(
        &mut self,
        id: ServerUnitId,
        position: Pos,
        kind: ServerUnit,
        faction: PlayerFaction,
        active: bool,
    ) {
        let owner = self
            .faction_players
            .get(&faction)
            .and_then(|player| self.state.player_index(player))
            .unwrap_or_else(|| panic!("spawned unit faction {faction:?} has no player"));
        let id = unit_id(id);
        let profile = profile(kind);
        self.state.units.push(Unit {
            id,
            kind,
            owner,
            hp: 100,
            fuel: profile.max_fuel,
            ammo: profile.max_ammo,
            action: if active {
                UnitAction::Ready
            } else {
                UnitAction::Spent
            },
            concealment: Concealment::Exposed,
            location: Location::Board { position },
        });
        let next = id
            .get()
            .checked_add(1)
            .expect("server unit id exceeds AWVM's identifier domain");
        self.state.next_unit_id = Some(self.state.next_unit_id.unwrap_or(1).max(next));
    }

    pub fn random_tokens(&self) -> &[RandomToken] {
        self.entropy.tokens()
    }

    pub fn last_random_tokens(&self) -> &[RandomToken] {
        &self.last_random
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn map(&self) -> &AwbrnMap {
        &self.map
    }

    pub fn player_faction(&self, player: &PlayerId) -> Option<PlayerFaction> {
        player
            .as_str()
            .parse::<usize>()
            .ok()
            .and_then(|index| self.player_factions.get(index))
            .copied()
    }

    pub fn players(&self) -> impl Iterator<Item = ServerPlayerId> + '_ {
        (0..self.player_factions.len()).map(|index| ServerPlayerId(index as u8))
    }

    pub fn player(&self, player: ServerPlayerId) -> PlayerId {
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
        Violation::InsufficientPower {
            required,
            available,
        } => CommandError::InsufficientPower {
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
            position: *position,
            kind: *unit_type,
        }),
        GameCommand::ActivatePower { level } => one(Command::ActivatePower {
            player,
            level: *level,
        }),
        GameCommand::EndTurn => one(Command::EndTurn { player }),
        GameCommand::DeleteUnit { unit_id } => one(Command::DeleteUnit {
            player,
            unit: command_unit_id(unit_id.0)?,
        }),
        GameCommand::Unload {
            transport_id,
            cargo_id,
            position,
        } => one(Command::Unload {
            player,
            transport: command_unit_id(transport_id.0)?,
            cargo: command_unit_id(cargo_id.0)?,
            destination: *position,
        }),
        GameCommand::MoveUnit {
            unit_id: server_unit_id,
            path,
            action,
        } => {
            let unit = command_unit_id(server_unit_id.0)?;
            let path = path.to_vec();
            match action {
                Some(PostMoveAction::Attack { target }) => {
                    let target_position = *target;
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
                    transport: command_unit_id(*transport_id)?,
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
                        cargo: command_unit_id(*cargo_id)?,
                        destination: *position,
                    },
                ]),
                Some(PostMoveAction::Supply) => one(Command::MoveSupply { player, unit, path }),
                Some(PostMoveAction::Repair { target_id }) => one(Command::MoveRepair {
                    player,
                    unit,
                    path,
                    target: command_unit_id(*target_id)?,
                }),
                Some(PostMoveAction::Hide) => one(Command::MoveHide { player, unit, path }),
                Some(PostMoveAction::Unhide) => one(Command::MoveReveal { player, unit, path }),
                Some(PostMoveAction::Join { target_id }) => one(Command::MoveJoin {
                    player,
                    unit,
                    path,
                    target: command_unit_id(*target_id)?,
                }),
                Some(PostMoveAction::Launch { target }) => one(Command::MoveLaunch {
                    player,
                    unit,
                    path,
                    target: *target,
                }),
                Some(PostMoveAction::Explode) => one(Command::MoveExplode { player, unit, path }),
                Some(PostMoveAction::Wait) | None => one(Command::MoveWait { player, unit, path }),
            }
        }
    }
}

fn unit_id(id: ServerUnitId) -> UnitId {
    UnitId::new(u32::try_from(id.0).expect("server unit id exceeds AWVM's identifier domain"))
}

fn command_unit_id(id: u64) -> Result<UnitId, CommandError> {
    u32::try_from(id)
        .map(UnitId::new)
        .map_err(|_| CommandError::InvalidAction {
            reason: format!("unit id {id} exceeds AWVM's identifier domain"),
        })
}

/// The entropy an authority draws from.
///
/// A seed gives one tape, and the tape is what a replay repeats.
#[derive(Clone, Debug)]
pub struct GameRng {
    state: u64,
}

impl GameRng {
    pub fn from_seed(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9e37_79b9_7f4a_7c15
            } else {
                seed
            },
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    /// Returns a uniformly distributed value in `0..range`.
    ///
    /// Samples again when a draw falls in the tail that the modulus would
    /// count twice, which keeps each value equally likely.
    fn below(&mut self, range: u64) -> u64 {
        let max_usable = u64::MAX - (u64::MAX % range);

        loop {
            let sample = self.next_u64();
            if sample < max_usable {
                return sample % range;
            }
        }
    }

    /// Returns a uniformly distributed value in `0..=max`.
    pub fn roll(&mut self, max: u8) -> u8 {
        if max == 0 {
            return 0;
        }

        self.below(u64::from(max) + 1) as u8
    }
}

impl awvm::random::Entropy for GameRng {
    fn luck(
        &mut self,
        _polarity: awvm::random::Luck,
        domain: awvm::commander::Domain,
    ) -> Result<i64, awvm::random::RandomError> {
        let width = u64::try_from(domain.maximum - domain.minimum)
            .expect("commander luck domains are ordered");
        let offset = if width == 0 { 0 } else { self.below(width + 1) };
        Ok(domain.minimum + offset as i64)
    }

    fn weather(&mut self) -> Result<awvm::ruleset::WeatherKind, awvm::random::RandomError> {
        Ok(match self.roll(2) {
            0 => awvm::ruleset::WeatherKind::Clear,
            1 => awvm::ruleset::WeatherKind::Rain,
            _ => awvm::ruleset::WeatherKind::Snow,
        })
    }
}

fn player_id(player: ServerPlayerId) -> PlayerId {
    crate::semantic_player_id(usize::from(player.0))
}
