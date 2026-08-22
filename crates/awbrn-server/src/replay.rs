use std::fmt;

use crate::command::GameCommand;
use crate::error::CommandError;
use crate::player::PlayerId;
use crate::server::GameServer;
use awbrn_map::{GameSetup, SetupError};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StoredActionEvent {
    pub player: PlayerId,
    pub command: GameCommand,
    pub random: Vec<awvm::random::RandomToken>,
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
        }
    }
}

impl std::error::Error for ReplayEventError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Command(error) => Some(error),
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
