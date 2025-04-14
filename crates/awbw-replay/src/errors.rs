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
    },
    Json {
        error: serde_json::Error,
        path: Option<serde_path_to_error::Path>,
    },
}

impl std::error::Error for ReplayError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            ReplayErrorKind::Zip(err) => Some(err),
            ReplayErrorKind::Io(err) => Some(err),
            ReplayErrorKind::Php { error, .. } => Some(error),
            ReplayErrorKind::Json { error, .. } => Some(error),
        }
    }
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            ReplayErrorKind::Zip(err) => write!(f, "Zip error: {}", err),
            ReplayErrorKind::Io(err) => write!(f, "IO error: {}", err),
            ReplayErrorKind::Php { error, path } => {
                if let Some(path) = path {
                    write!(f, "PHP error at {}: {}", path, error)
                } else {
                    write!(f, "PHP error: {}", error)
                }
            }
            ReplayErrorKind::Json { error, path } => {
                if let Some(path) = path {
                    write!(f, "JSON error at {}: {}", path, error)
                } else {
                    write!(f, "JSON error: {}", error)
                }
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
            },
        }
    }
}

impl From<serde_json::Error> for ReplayError {
    fn from(error: serde_json::Error) -> Self {
        ReplayError {
            kind: ReplayErrorKind::Json { error, path: None },
        }
    }
}
