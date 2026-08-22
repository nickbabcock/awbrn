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
    InvalidJson,
    JsonDeserialize {
        error: String,
    },
    UnsupportedMapFormat {
        format: u32,
    },
    TerrainSizeMismatch {
        expected: usize,
        found: usize,
    },
    UnitOutOfBounds {
        x: u32,
        y: u32,
    },
    DimensionsOutOfRange {
        width: u32,
        height: u32,
        limit: u32,
    },
    UnitHpOutOfRange {
        x: u32,
        y: u32,
        hp: u32,
    },
    UnitPositionOccupied {
        x: u32,
        y: u32,
    },
    UnknownUnitId {
        id: u32,
    },
    UnknownCountryCode {
        code: String,
    },
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
            MapError::InvalidJson => write!(f, "Failed to parse JSON map data: invalid format"),
            MapError::JsonDeserialize { error } => {
                write!(f, "Failed to deserialize JSON map data: {}", error)
            }
            MapError::UnsupportedMapFormat { format } => {
                write!(f, "Unsupported map format: {}", format)
            }
            MapError::TerrainSizeMismatch { expected, found } => write!(
                f,
                "Terrain size mismatch: expected {} tiles, found {}",
                expected, found
            ),
            MapError::UnitOutOfBounds { x, y } => {
                write!(f, "Predeployed unit at ({}, {}) is out of bounds", x, y)
            }
            MapError::DimensionsOutOfRange {
                width,
                height,
                limit,
            } => write!(
                f,
                "Map dimensions {}x{} exceed the maximum dimension of {} per axis",
                width, height, limit
            ),
            MapError::UnitHpOutOfRange { x, y, hp } => write!(
                f,
                "Predeployed unit at ({}, {}) has out of range HP: {}",
                x, y, hp
            ),
            MapError::UnitPositionOccupied { x, y } => write!(
                f,
                "Multiple predeployed units occupy position ({}, {})",
                x, y
            ),
            MapError::UnknownUnitId { id } => write!(f, "Unknown AWBW unit ID: {}", id),
            MapError::UnknownCountryCode { code } => {
                write!(f, "Unknown AWBW country code: '{}'", code)
            }
        }
    }
}

impl std::error::Error for MapError {}
