use std::fmt;

use crate::server::GameServer;
use awbrn_game::{GameSetup, ReplayEventError, SetupError, StoredActionEvent};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReplayError {
    Setup(SetupError),
    Event {
        index: usize,
        source: ReplayEventError,
    },
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
