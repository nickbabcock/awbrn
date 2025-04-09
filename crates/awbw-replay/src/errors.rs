#[derive(Debug)]
pub struct ReplayError {
    pub(crate) kind: ReplayErrorKind,
}

#[derive(Debug)]
pub enum ReplayErrorKind {
    Zip(rawzip::Error),
    Io(std::io::Error),
    Php(phpserz::Error),
}

impl std::error::Error for ReplayError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            ReplayErrorKind::Zip(err) => Some(err),
            ReplayErrorKind::Io(err) => Some(err),
            ReplayErrorKind::Php(e) => Some(e),
        }
    }
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            ReplayErrorKind::Zip(err) => write!(f, "Zip error: {}", err),
            ReplayErrorKind::Io(err) => write!(f, "IO error: {}", err),
            ReplayErrorKind::Php(err) => write!(f, "PHP error: {}", err),
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
            kind: ReplayErrorKind::Php(err),
        }
    }
}
