use crate::{CommandError, GameCommand, PlayerId};

/// One accepted command and the entropy it consumed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StoredActionEvent {
    pub player: PlayerId,
    pub command: GameCommand,
    pub random: Vec<awvm::random::RandomToken>,
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

impl std::fmt::Display for ReplayEventError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
