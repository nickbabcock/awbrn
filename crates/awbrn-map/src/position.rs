/// A position in a map
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Position {
    pub x: usize,
    pub y: usize,
}

impl Position {
    /// Create a new Position
    pub fn new(x: usize, y: usize) -> Self {
        Self { x, y }
    }
}
