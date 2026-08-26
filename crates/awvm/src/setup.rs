//! One-time inputs for match initialization.

use std::collections::HashSet;

use crate::ruleset::CommanderKind;
use crate::semantic::{
    Board, PlayerId, Pos, RulesetRef, Settings, StateInvariant, TeamId, TerrainId, UnitKindId,
    validate_board_invariants,
};

/// The inputs from which [`crate::transition::initialize_match`] creates a
/// match.
///
/// A setup has no day, phase, current weather, unit resources, power state, or
/// match status. Initialization assigns those values and consumes this setup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchSetup {
    pub(crate) ruleset: RulesetRef,
    pub(crate) settings: Settings,
    pub(crate) board: Board,
    pub(crate) players: Vec<PlayerSetup>,
    pub(crate) deployments: Vec<UnitDeployment>,
}

impl MatchSetup {
    pub fn new(
        ruleset: RulesetRef,
        settings: Settings,
        board: Board,
        players: Vec<PlayerSetup>,
        deployments: Vec<UnitDeployment>,
    ) -> Result<Self, MatchSetupError> {
        validate_players(&players, settings.tags)?;
        let player_ids: HashSet<&PlayerId> = players.iter().map(PlayerSetup::id).collect();
        validate_board_invariants(&board, players.len()).map_err(MatchSetupError::from_board)?;
        let mut occupied = HashSet::with_capacity(deployments.len());
        for deployment in &deployments {
            validate_deployment_hp(deployment.hp)?;
            if !player_ids.contains(deployment.owner()) {
                return Err(MatchSetupError::UnknownDeploymentOwner {
                    owner: deployment.owner.clone(),
                });
            }
            if !board.contains(deployment.position) {
                return Err(MatchSetupError::DeploymentOffBoard {
                    position: deployment.position,
                });
            }
            if !occupied.insert(deployment.position) {
                return Err(MatchSetupError::DuplicateDeploymentPosition {
                    position: deployment.position,
                });
            }
        }
        Ok(Self {
            ruleset,
            settings,
            board,
            players,
            deployments,
        })
    }

    pub fn ruleset(&self) -> &RulesetRef {
        &self.ruleset
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn players(&self) -> &[PlayerSetup] {
        &self.players
    }

    pub fn deployments(&self) -> &[UnitDeployment] {
        &self.deployments
    }
}

/// One player seated in initialization order.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PlayerSetup {
    pub(crate) id: PlayerId,
    pub(crate) team: TeamId,
    pub(crate) starting_funds: u64,
    pub(crate) commanders: Vec<CommanderKind>,
}

impl PlayerSetup {
    pub fn new(
        id: PlayerId,
        team: TeamId,
        starting_funds: u64,
        commanders: Vec<CommanderKind>,
    ) -> Result<Self, MatchSetupError> {
        if !(1..=2).contains(&commanders.len()) {
            return Err(MatchSetupError::CommanderCount {
                player: id,
                found: commanders.len(),
                expected: "one or two",
            });
        }
        Ok(Self {
            id,
            team,
            starting_funds,
            commanders,
        })
    }

    pub fn id(&self) -> &PlayerId {
        &self.id
    }

    pub fn team(&self) -> &TeamId {
        &self.team
    }

    pub const fn starting_funds(&self) -> u64 {
        self.starting_funds
    }

    pub fn commanders(&self) -> &[CommanderKind] {
        &self.commanders
    }
}

/// One map-defined unit present when a match starts.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UnitDeployment {
    pub(crate) kind: UnitKindId,
    pub(crate) owner: PlayerId,
    pub(crate) hp: u8,
    pub(crate) position: Pos,
}

impl UnitDeployment {
    pub fn new(
        kind: UnitKindId,
        owner: PlayerId,
        hp: u8,
        position: Pos,
    ) -> Result<Self, MatchSetupError> {
        validate_deployment_hp(hp)?;
        Ok(Self {
            kind,
            owner,
            hp,
            position,
        })
    }

    pub const fn kind(&self) -> UnitKindId {
        self.kind
    }

    pub fn owner(&self) -> &PlayerId {
        &self.owner
    }

    pub const fn hp(&self) -> u8 {
        self.hp
    }

    pub const fn position(&self) -> Pos {
        self.position
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MatchSetupError {
    #[error("a match needs at least one player")]
    NoPlayers,
    #[error("a match supports at most 255 players, found {found}")]
    TooManyPlayers { found: usize },
    #[error("player {player} appears more than once")]
    DuplicatePlayer { player: PlayerId },
    #[error("player {player} has {found} commanders; expected {expected}")]
    CommanderCount {
        player: PlayerId,
        found: usize,
        expected: &'static str,
    },
    #[error("tile {position} names a player outside the setup roster")]
    BoardOwnerOffRoster { position: Pos },
    #[error("tile {position} carries ownership its terrain {terrain} does not admit")]
    BoardOwnershipDisagreesWithTerrain { position: Pos, terrain: TerrainId },
    #[error("tile {position} records capture progress but cannot be owned")]
    CapturePointsOnUnownableTile { position: Pos },
    #[error("tile {position} has {hp} HP above its maximum of {maximum}")]
    DestructibleHpAboveMaximum {
        position: Pos,
        hp: u64,
        maximum: u64,
    },
    #[error("tile {position} has destructible HP but its terrain is not destructible")]
    DestructibleHpOnIndestructibleTile { position: Pos },
    #[error("a deployment belongs to unknown player {owner}")]
    UnknownDeploymentOwner { owner: PlayerId },
    #[error("a deployment at {position} is outside the board")]
    DeploymentOffBoard { position: Pos },
    #[error("more than one deployment occupies {position}")]
    DuplicateDeploymentPosition { position: Pos },
    #[error("deployment HP {hp} is outside 1..=100")]
    DeploymentHpOutOfRange { hp: u8 },
}

impl MatchSetupError {
    fn from_board(error: StateInvariant) -> Self {
        match error {
            StateInvariant::TileOwnerOffTheRoster { position, .. } => {
                Self::BoardOwnerOffRoster { position }
            }
            StateInvariant::TileOwnershipDisagreesWithTerrain { position, terrain } => {
                Self::BoardOwnershipDisagreesWithTerrain { position, terrain }
            }
            StateInvariant::CapturePointsOnUnownableTile { position } => {
                Self::CapturePointsOnUnownableTile { position }
            }
            StateInvariant::DestructibleHpAboveMaximum {
                position,
                hp,
                maximum,
            } => Self::DestructibleHpAboveMaximum {
                position,
                hp,
                maximum,
            },
            StateInvariant::DestructibleHpOnIndestructibleTile { position } => {
                Self::DestructibleHpOnIndestructibleTile { position }
            }
            _ => unreachable!("the board validator returns only board invariants"),
        }
    }
}

pub(crate) fn validate_players(players: &[PlayerSetup], tags: bool) -> Result<(), MatchSetupError> {
    if players.is_empty() {
        return Err(MatchSetupError::NoPlayers);
    }
    if players.len() > usize::from(u8::MAX) {
        return Err(MatchSetupError::TooManyPlayers {
            found: players.len(),
        });
    }
    let mut ids = HashSet::with_capacity(players.len());
    for player in players {
        if !ids.insert(&player.id) {
            return Err(MatchSetupError::DuplicatePlayer {
                player: player.id.clone(),
            });
        }
        validate_commander_count(&player.id, player.commanders.len(), tags)?;
    }
    Ok(())
}

fn validate_commander_count(
    player: &PlayerId,
    found: usize,
    tags: bool,
) -> Result<(), MatchSetupError> {
    let valid = if tags { found == 2 } else { found == 1 };
    if valid {
        Ok(())
    } else {
        Err(MatchSetupError::CommanderCount {
            player: player.clone(),
            found,
            expected: if tags {
                "two for tags"
            } else {
                "one without tags"
            },
        })
    }
}

fn validate_deployment_hp(hp: u8) -> Result<(), MatchSetupError> {
    if (1..=100).contains(&hp) {
        Ok(())
    } else {
        Err(MatchSetupError::DeploymentHpOutOfRange { hp })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ruleset::UnitKind;

    #[test]
    fn a_player_constructor_accepts_solo_and_tag_rosters() {
        let solo = PlayerSetup::new(
            PlayerId::from("red"),
            TeamId::from("red-team"),
            0,
            vec![CommanderKind::Andy],
        );
        let tag = PlayerSetup::new(
            PlayerId::from("blue"),
            TeamId::from("blue-team"),
            0,
            vec![CommanderKind::Andy, CommanderKind::Olaf],
        );

        solo.unwrap();
        tag.unwrap();
    }

    #[test]
    fn a_player_constructor_rejects_an_empty_commander_roster() {
        assert!(matches!(
            PlayerSetup::new(
                PlayerId::from("red"),
                TeamId::from("red-team"),
                0,
                Vec::new(),
            ),
            Err(MatchSetupError::CommanderCount { found: 0, .. })
        ));
    }

    #[test]
    fn a_deployment_constructor_rejects_zero_hp() {
        assert!(matches!(
            UnitDeployment::new(UnitKind::Infantry, PlayerId::from("red"), 0, Pos::new(0, 0),),
            Err(MatchSetupError::DeploymentHpOutOfRange { hp: 0 })
        ));
    }
}
