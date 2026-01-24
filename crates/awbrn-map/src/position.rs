use std::fmt;

/// Represents a 2D position on the map
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
#[cfg_attr(feature = "bevy", derive(bevy::reflect::Reflect))]
pub struct Position {
    pub x: usize,
    pub y: usize,
}

impl Position {
    /// Create a new Position
    pub fn new(x: usize, y: usize) -> Self {
        Self { x, y }
    }

    /// Calculate Manhattan distance to another position
    pub fn manhattan(&self, other: &Position) -> usize {
        self.x.abs_diff(other.x) + self.y.abs_diff(other.y)
    }

    pub fn up_overflowing(&self) -> Self {
        Self::new(self.x, self.y.wrapping_sub(1))
    }

    pub fn down_overflowing(&self) -> Self {
        Self::new(self.x, self.y.wrapping_add(1))
    }

    pub fn left_overflowing(&self) -> Self {
        Self::new(self.x.wrapping_sub(1), self.y)
    }

    pub fn right_overflowing(&self) -> Self {
        Self::new(self.x.wrapping_add(1), self.y)
    }

    pub fn movement(&self, dx: isize, dy: isize) -> Self {
        Self::new(
            (self.x as isize).wrapping_add(dx) as usize,
            (self.y as isize).wrapping_add(dy) as usize,
        )
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_distance() {
        let p1 = Position::new(1, 1);
        let p2 = Position::new(4, 5);

        assert_eq!(p1.manhattan(&p2), 7); // |4-1| + |5-1| = 3 + 4 = 7
    }

    #[test]
    fn test_position_display() {
        let pos = Position::new(3, 7);
        assert_eq!(format!("{}", pos), "(3, 7)");
    }
}
