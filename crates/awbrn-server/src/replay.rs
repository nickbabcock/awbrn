use std::fmt;

use crate::command::{GameCommand, PostMoveAction};
use crate::damage::CombatOutcome;
use crate::error::CommandError;
use crate::server::GameServer;
use crate::setup::{GameSetup, SetupError};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StoredActionEvent {
    pub command: GameCommand,
    pub combat_outcome: Option<CombatOutcome>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReplayError {
    Setup(SetupError),
    Event {
        index: usize,
        source: ReplayEventError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReplayEventError {
    Command(CommandError),
    MissingCombatOutcome,
    UnexpectedCombatOutcome,
    InvalidCombatOutcome { reason: String },
}

impl From<CommandError> for ReplayEventError {
    fn from(error: CommandError) -> Self {
        Self::Command(error)
    }
}

impl ReplayError {
    pub fn event_index(&self) -> Option<usize> {
        match self {
            Self::Setup(_) => None,
            Self::Event { index, .. } => Some(*index),
        }
    }
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Setup(error) => write!(f, "failed to initialize replay server: {error}"),
            Self::Event { index, source } => {
                write!(f, "failed to replay event {index}: {source}")
            }
        }
    }
}

impl std::error::Error for ReplayError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Setup(error) => Some(error),
            Self::Event { source, .. } => Some(source),
        }
    }
}

impl fmt::Display for ReplayEventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command(error) => write!(f, "{error}"),
            Self::MissingCombatOutcome => write!(f, "attack event is missing combat outcome"),
            Self::UnexpectedCombatOutcome => {
                write!(f, "non-attack event unexpectedly included combat outcome")
            }
            Self::InvalidCombatOutcome { reason } => write!(f, "invalid combat outcome: {reason}"),
        }
    }
}

impl std::error::Error for ReplayEventError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Command(error) => Some(error),
            Self::MissingCombatOutcome
            | Self::UnexpectedCombatOutcome
            | Self::InvalidCombatOutcome { .. } => None,
        }
    }
}

pub fn reconstruct_from_events(
    setup: GameSetup,
    events: &[StoredActionEvent],
) -> Result<GameServer, ReplayError> {
    let mut server = GameServer::new(setup).map_err(ReplayError::Setup)?;

    for (index, event) in events.iter().enumerate() {
        server
            .replay_stored_action_event(event)
            .map_err(|source| ReplayError::Event { index, source })?;
    }

    Ok(server)
}

pub(crate) fn command_is_attack(command: &GameCommand) -> bool {
    matches!(
        command,
        GameCommand::MoveUnit {
            action: Some(PostMoveAction::Attack { .. }),
            ..
        }
    )
}
