#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MapError {
    ParseTerrainId {
        row: usize,
        col: usize,
        value: String,
    },
    InvalidTerrain {
        row: usize,
        col: usize,
        id: u8,
    },
    UnevenDimensions {
        expected: usize,
        found: usize,
        row: usize,
    },
    EmptyMap,
}

impl std::fmt::Display for MapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MapError::ParseTerrainId { row, col, value } => write!(
                f,
                "Failed to parse terrain ID at row {}, column {}: '{}'",
                row, col, value
            ),
            MapError::InvalidTerrain { row, col, id } => write!(
                f,
                "Invalid terrain ID {} at row {}, column {}",
                id, row, col
            ),
            MapError::UnevenDimensions {
                expected,
                found,
                row,
            } => write!(
                f,
                "Uneven dimensions in map data at row {}: expected width {}, found {}",
                row, expected, found
            ),
            MapError::EmptyMap => write!(f, "Map data is empty or contains no valid terrain"),
        }
    }
}

impl std::error::Error for MapError {}
