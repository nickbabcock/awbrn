use std::fmt;

use crate::unit_id::ServerUnitId;

/// Error returned when a command cannot be executed.
#[derive(Debug, Clone)]
pub enum CommandError {
    /// It is not this player's turn.
    NotYourTurn,
    /// The referenced unit does not exist or is not owned by this player.
    InvalidUnit(ServerUnitId),
    /// The unit has already acted this turn.
    UnitAlreadyActed(ServerUnitId),
    /// The movement path is invalid (blocked, too long, wrong terrain).
    InvalidPath { reason: String },
    /// The post-move action is invalid.
    InvalidAction { reason: String },
    /// Insufficient funds to build the requested unit.
    InsufficientFunds { cost: u32, available: u32 },
    /// The position cannot produce the requested unit type.
    InvalidBuildLocation,
    /// The game is already over.
    GameOver,
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotYourTurn => write!(f, "it is not your turn"),
            Self::InvalidUnit(id) => write!(f, "invalid unit {:?}", id),
            Self::UnitAlreadyActed(id) => write!(f, "unit {:?} has already acted", id),
            Self::InvalidPath { reason } => write!(f, "invalid path: {reason}"),
            Self::InvalidAction { reason } => write!(f, "invalid action: {reason}"),
            Self::InsufficientFunds { cost, available } => {
                write!(f, "insufficient funds: need {cost}, have {available}")
            }
            Self::InvalidBuildLocation => write!(f, "invalid build location"),
            Self::GameOver => write!(f, "game is over"),
        }
    }
}

impl std::error::Error for CommandError {}
