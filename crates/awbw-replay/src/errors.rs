/// Context information about where a deserialization error occurred
#[derive(Debug, Clone)]
pub struct DeserializationContext {
    pub file_entry_index: usize,
    pub entry_kind: EntryKind,
}

#[derive(Debug, Clone)]
pub enum EntryKind {
    Game {
        game_index: usize,
    },
    Turn {
        turn_index: usize,
        player_id: u32,
        day: u32,
        action_index: Option<usize>,
    },
}

#[derive(Debug)]
pub struct ReplayError {
    pub(crate) kind: ReplayErrorKind,
}

#[derive(Debug)]
pub enum ReplayErrorKind {
    Zip(rawzip::Error),
    Io(std::io::Error),
    Php {
        error: phpserz::Error,
        path: Option<serde_path_to_error::Path>,
        context: Option<DeserializationContext>,
    },
    Json {
        error: serde_json::Error,
        path: Option<serde_path_to_error::Path>,
        context: Option<DeserializationContext>,
    },
    InvalidTurnData {
        context: Option<DeserializationContext>,
    },
}

impl std::error::Error for ReplayError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            ReplayErrorKind::Zip(err) => Some(err),
            ReplayErrorKind::Io(err) => Some(err),
            ReplayErrorKind::Php { error, .. } => Some(error),
            ReplayErrorKind::Json { error, .. } => Some(error),
            ReplayErrorKind::InvalidTurnData { .. } => None,
        }
    }
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            ReplayErrorKind::Zip(err) => write!(f, "Zip error: {}", err),
            ReplayErrorKind::Io(err) => write!(f, "IO error: {}", err),
            ReplayErrorKind::Php {
                error,
                path,
                context,
            } => {
                write!(f, "PHP error")?;
                if let Some(ctx) = context {
                    write!(f, " at {}", ctx)?;
                }
                if let Some(path) = path {
                    write!(f, " (field: {})", path)?;
                }
                write!(f, ": {}", error)
            }
            ReplayErrorKind::Json {
                error,
                path,
                context,
            } => {
                write!(f, "JSON error")?;
                if let Some(ctx) = context {
                    write!(f, " at {}", ctx)?;
                }
                if let Some(path) = path {
                    write!(f, " (field: {})", path)?;
                }
                write!(f, ": {}", error)
            }
            ReplayErrorKind::InvalidTurnData { context } => {
                write!(f, "Invalid turn data")?;
                if let Some(ctx) = context {
                    write!(f, " at {}", ctx)?;
                }
                Ok(())
            }
        }
    }
}

impl std::fmt::Display for DeserializationContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "file[{}]", self.file_entry_index)?;
        match &self.entry_kind {
            EntryKind::Game { game_index } => {
                write!(f, " game[{}]", game_index)
            }
            EntryKind::Turn {
                turn_index,
                player_id,
                day,
                action_index,
            } => {
                write!(
                    f,
                    " turn[{}] (player={}, day={})",
                    turn_index, player_id, day
                )?;
                if let Some(idx) = action_index {
                    write!(f, " action[{}]", idx)?;
                }
                Ok(())
            }
        }
    }
}

impl From<rawzip::Error> for ReplayError {
    fn from(err: rawzip::Error) -> Self {
        ReplayError {
            kind: ReplayErrorKind::Zip(err),
        }
    }
}

impl From<std::io::Error> for ReplayError {
    fn from(err: std::io::Error) -> Self {
        ReplayError {
            kind: ReplayErrorKind::Io(err),
        }
    }
}

impl From<phpserz::Error> for ReplayError {
    fn from(err: phpserz::Error) -> Self {
        ReplayError {
            kind: ReplayErrorKind::Php {
                error: err,
                path: None,
                context: None,
            },
        }
    }
}

impl From<serde_json::Error> for ReplayError {
    fn from(error: serde_json::Error) -> Self {
        ReplayError {
            kind: ReplayErrorKind::Json {
                error,
                path: None,
                context: None,
            },
        }
    }
}
