use crate::{MapError, Position};
use awbrn_types::{AwbwTerrain, Faction, PLAYER_FACTION_METADATA, PlayerFaction};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// Represents a game map with terrain data
#[derive(Debug, Clone, PartialEq)]
pub struct AwbwMap {
    /// Width of the map in tiles
    width: usize,

    /// Height of the map in tiles
    height: usize,

    /// Terrain data stored as a flattened 2D array (row-major order)
    terrain: Vec<awbrn_types::AwbwTerrain>,
}

impl AwbwMap {
    /// Creates a new map with specified dimensions and default terrain
    pub fn new(width: usize, height: usize, default_terrain: AwbwTerrain) -> Self {
        Self {
            width,
            height,
            terrain: vec![default_terrain; width * height],
        }
    }

    /// Parses a Awbw text map
    ///
    /// Ref: https://awbw.amarriner.com/text_map.php?maps_id=162795
    pub fn parse_txt(data: &str) -> Result<Self, MapError> {
        let mut result = Vec::new();
        let mut width = 0;

        for (row_idx, row) in data.split('\n').enumerate() {
            if row.trim().is_empty() {
                continue;
            }

            let mut cols = Vec::new();
            for (col_idx, cell) in row.split(',').enumerate() {
                let terrain_id =
                    cell.trim()
                        .parse::<u8>()
                        .map_err(|_| MapError::ParseTerrainId {
                            row: row_idx,
                            col: col_idx,
                            value: cell.to_string(),
                        })?;

                let terrain =
                    AwbwTerrain::try_from(terrain_id).map_err(|_| MapError::InvalidTerrain {
                        row: row_idx,
                        col: col_idx,
                        id: terrain_id,
                    })?;

                cols.push(terrain);
            }

            if width == 0 {
                width = cols.len();
            } else if width != cols.len() {
                return Err(MapError::UnevenDimensions {
                    expected: width,
                    found: cols.len(),
                    row: row_idx,
                });
            }

            result.extend(cols);
        }

        if result.is_empty() {
            return Err(MapError::EmptyMap);
        }

        let height = result.len() / width;
        Ok(AwbwMap {
            width,
            height,
            terrain: result,
        })
    }

    /// Parses a Awbw JSON map
    ///
    /// Ref: https://awbw.amarriner.com/api/map/map_info.php?maps_id=162795
    pub fn parse_json(data: &[u8]) -> Result<Self, MapError> {
        let map_data =
            serde_json::from_slice::<AwbwMapData>(data).map_err(|e| MapError::JsonDeserialize {
                error: e.to_string(),
            })?;

        AwbwMap::try_from(&map_data)
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// Render every tile as a unique glyph (never a space), preserving full
    /// faction identity. See [`AwbwTerrain::symbol`]. Pair with [`Self::legend`]
    /// to decode the non-ASCII glyphs.
    pub fn lossless(&self) -> LosslessSymbols<'_> {
        LosslessSymbols(self)
    }

    /// Render using the AWBW text-format ASCII symbols, with a space for any
    /// terrain the format can't represent. Combine with [`Self::collapse_factions`]
    /// for a faithful AWBW grid.
    pub fn awbw(&self) -> AwbwSymbols<'_> {
        AwbwSymbols(self)
    }

    /// A key naming every non-ASCII glyph used by [`Self::lossless`].
    pub fn legend(&self) -> Legend<'_> {
        Legend(self)
    }

    /// Collapse this map's player factions onto the canonical (AWBW-id) faction
    /// order in first-appearance order, so the map uses its minimal set (e.g. a
    /// 1v1 becomes Orange Star / Blue Moon). Neutral tiles are unchanged. The
    /// canonical order is taken from [`PLAYER_FACTION_METADATA`], which lists every
    /// player faction exactly once, so each distinct faction keeps its own slot.
    pub fn collapse_factions(&self) -> AwbwMap {
        // `seen` records distinct player factions in first-appearance order; a
        // faction's slot in `seen` indexes its canonical replacement.
        let mut seen: Vec<PlayerFaction> = Vec::new();
        let terrain = self
            .terrain
            .iter()
            .map(|terrain| match terrain {
                AwbwTerrain::Property(property) => match property.faction() {
                    Faction::Player(old) => {
                        let slot = seen.iter().position(|f| *f == old).unwrap_or_else(|| {
                            seen.push(old);
                            seen.len() - 1
                        });
                        let new = PLAYER_FACTION_METADATA[slot].faction();
                        AwbwTerrain::Property(property.with_owner(Faction::Player(new)))
                    }
                    Faction::Neutral => *terrain,
                },
                _ => *terrain,
            })
            .collect();

        AwbwMap {
            width: self.width,
            height: self.height,
            terrain,
        }
    }

    /// Get the terrain at the specified position
    pub fn terrain_at(&self, pos: Position) -> Option<AwbwTerrain> {
        if pos.x >= self.width || pos.y >= self.height() {
            return None;
        }

        self.terrain.get(pos.y * self.width + pos.x).copied()
    }

    /// Set the terrain at a specific position
    pub fn terrain_at_mut(&mut self, pos: Position) -> Option<&mut AwbwTerrain> {
        self.terrain.get_mut(pos.y * self.width + pos.x)
    }

    pub fn iter(&self) -> impl Iterator<Item = (Position, AwbwTerrain)> {
        self.terrain.iter().enumerate().map(move |(idx, terrain)| {
            let y = idx / self.width;
            let x = idx % self.width;
            (Position::new(x, y), *terrain)
        })
    }
}

/// Writes the map as a grid, one `glyph` per tile, rows separated by newlines.
fn write_grid(
    f: &mut fmt::Formatter<'_>,
    map: &AwbwMap,
    glyph: impl Fn(&AwbwTerrain) -> char,
) -> fmt::Result {
    let height = map.height();
    for y in 0..height {
        for x in 0..map.width {
            write!(f, "{}", glyph(&map.terrain[y * map.width + x]))?;
        }

        // Add a newline after each row unless it's the last row
        if y < height - 1 {
            writeln!(f)?;
        }
    }

    Ok(())
}

/// Lossless map rendering: every tile is a unique glyph, never a space. The
/// default [`fmt::Display`] for [`AwbwMap`]. See [`AwbwMap::lossless`].
pub struct LosslessSymbols<'a>(&'a AwbwMap);

impl fmt::Display for LosslessSymbols<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_grid(f, self.0, AwbwTerrain::symbol)
    }
}

/// AWBW text-format rendering: ASCII symbols, with a space for any terrain the
/// AWBW format can't represent. See [`AwbwMap::awbw`].
pub struct AwbwSymbols<'a>(&'a AwbwMap);

impl fmt::Display for AwbwSymbols<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_grid(f, self.0, |terrain| terrain.awbw_symbol().unwrap_or(' '))
    }
}

/// A key for the non-ASCII glyphs used by [`AwbwMap::lossless`], mapping each to
/// its terrain name. See [`AwbwMap::legend`].
pub struct Legend<'a>(&'a AwbwMap);

impl fmt::Display for Legend<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // A terrain needs a legend entry exactly when it has no AWBW symbol, i.e.
        // its lossless glyph is the non-ASCII fallback rather than an ASCII symbol.
        let mut entries: BTreeMap<char, &'static str> = BTreeMap::new();
        for terrain in &self.0.terrain {
            if terrain.awbw_symbol().is_none() {
                entries.insert(terrain.symbol(), terrain.name());
            }
        }

        for (i, (glyph, name)) in entries.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{glyph}  {name}")?;
        }

        Ok(())
    }
}

/// The default rendering is the lossless glyph grid; use [`AwbwMap::awbw`] for the
/// ASCII AWBW text format.
impl fmt::Display for AwbwMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.lossless().fmt(f)
    }
}

#[derive(Serialize, Deserialize)]
pub struct AwbwMapData {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Author")]
    pub author: String,
    #[serde(rename = "Player Count")]
    pub player_count: u32,
    #[serde(rename = "Published Date")]
    pub published_date: String,
    #[serde(rename = "Size X")]
    pub size_x: u32,
    #[serde(rename = "Size Y")]
    pub size_y: u32,
    #[serde(rename = "Terrain Map")]
    pub terrain_map: Vec<Vec<AwbwTerrain>>,
    #[serde(rename = "Predeployed Units")]
    pub predeployed_units: Vec<PredeployedUnit>,
}

impl TryFrom<&'_ AwbwMapData> for AwbwMap {
    type Error = MapError;

    fn try_from(data: &AwbwMapData) -> Result<Self, Self::Error> {
        // Get dimensions from the column-major data
        let column_count = data.terrain_map.len();
        let row_count = data.terrain_map.first().map(|v| v.len()).unwrap_or(0);

        // Check if all columns have the same height
        for (idx, col) in data.terrain_map.iter().enumerate() {
            if col.len() != row_count {
                return Err(MapError::UnevenDimensions {
                    expected: row_count,
                    found: col.len(),
                    row: idx,
                });
            }
        }

        if column_count == 0 || row_count == 0 {
            return Err(MapError::EmptyMap);
        }

        // Transpose from column-major to row-major
        let mut terrain = Vec::with_capacity(column_count * row_count);
        for row_idx in 0..row_count {
            for col_idx in 0..column_count {
                terrain.push(data.terrain_map[col_idx][row_idx]);
            }
        }

        Ok(AwbwMap {
            width: column_count,
            height: row_count,
            terrain,
        })
    }
}

#[derive(Serialize, Deserialize)]
pub struct PredeployedUnit {
    #[serde(rename = "Unit ID")]
    pub unit_id: u32,
    #[serde(rename = "Unit X")]
    pub unit_x: u32,
    #[serde(rename = "Unit Y")]
    pub unit_y: u32,
    #[serde(rename = "Unit HP")]
    pub unit_hp: u32,
    #[serde(rename = "Country Code")]
    pub country_code: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MapError;

    #[test]
    fn test_parse_empty_input() {
        let result = AwbwMap::parse_txt("");
        assert!(matches!(result, Err(MapError::EmptyMap)));
    }

    #[test]
    fn test_parse_only_whitespace() {
        let result = AwbwMap::parse_txt("  \n  \t  ");
        assert!(matches!(result, Err(MapError::EmptyMap)));
    }

    #[test]
    fn test_parse_invalid_terrain_id_format() {
        // Test with non-numeric input
        let result = AwbwMap::parse_txt("1,2,3\n4,x,6");
        assert!(
            matches!(result, Err(MapError::ParseTerrainId { row: 1, col: 1, value }) if value == "x")
        );

        // Test with decimal number
        let result = AwbwMap::parse_txt("1,2,3\n4,5.5,6");
        assert!(
            matches!(result, Err(MapError::ParseTerrainId { row: 1, col: 1, value }) if value == "5.5")
        );

        // Test with negative number
        let result = AwbwMap::parse_txt("1,2,3\n4,-5,6");
        assert!(
            matches!(result, Err(MapError::ParseTerrainId { row: 1, col: 1, value }) if value == "-5")
        );
    }

    #[test]
    fn test_parse_invalid_terrain_id_value() {
        // Using terrain ID 255 which doesn't exist
        let result = AwbwMap::parse_txt("1,2,3\n4,255,6");
        assert!(matches!(
            result,
            Err(MapError::InvalidTerrain {
                row: 1,
                col: 1,
                id: 255
            })
        ));
    }

    #[test]
    fn test_parse_uneven_dimensions() {
        // Second row has 4 columns while first has 3
        let result = AwbwMap::parse_txt("1,2,3\n4,5,6,7");
        assert!(matches!(
            result,
            Err(MapError::UnevenDimensions {
                expected: 3,
                found: 4,
                row: 1
            })
        ));

        // Third row has 2 columns while first has 3
        let result = AwbwMap::parse_txt("1,2,3\n4,5,6\n7,8");
        assert!(matches!(
            result,
            Err(MapError::UnevenDimensions {
                expected: 3,
                found: 2,
                row: 2
            })
        ));
    }

    #[test]
    fn test_parse_with_empty_rows() {
        // Map with empty rows should skip them and parse successfully
        let result = AwbwMap::parse_txt("1,2,3\n\n4,5,6");
        assert!(result.is_ok());

        let map = result.unwrap();
        assert_eq!(map.width(), 3);
        assert_eq!(map.height(), 2);
    }

    #[test]
    fn test_parse_with_whitespace() {
        // Test with whitespace around terrain IDs
        let result = AwbwMap::parse_txt(" 1, 2, 3 \n 4, 5, 6 ");
        assert!(result.is_ok());

        let map = result.unwrap();
        assert_eq!(map.width(), 3);
        assert_eq!(map.height(), 2);
    }

    #[test]
    fn test_display_implementation() {
        // Create a small map with known terrain types
        let map_data = "1,2,3\n28,34,42"; // Plain, Mountain, Wood, Sea, Neutral City, Orange Star HQ
        let map = AwbwMap::parse_txt(map_data).unwrap();

        // Expected display output based on terrain symbols
        // Plain (.), Mountain (^), Wood (@)
        // Sea (,), Neutral City (a), Orange Star HQ (i)
        let expected = ".^@\n,ai";

        assert_eq!(map.to_string(), expected);
    }

    #[test]
    fn collapse_factions_normalizes_to_canonical_order() {
        // Black Hole HQ (95) then Teal Galaxy HQ (167): two distinct player factions.
        let map = AwbwMap::parse_txt("95,167").unwrap();
        let collapsed = map.collapse_factions();

        // First-seen faction -> Orange Star (HQ 'i'), second -> Blue Moon (HQ 'o').
        assert_eq!(collapsed.awbw().to_string(), "io");
    }

    #[test]
    fn lossless_renders_exotic_faction_with_legend() {
        // Teal Galaxy HQ (167) has no AWBW symbol, so `awbw()` leaves a space, but
        // `lossless()` emits a unique non-ASCII glyph that the legend names.
        let map = AwbwMap::parse_txt("167").unwrap();

        let rendered = map.lossless().to_string();
        let glyph = rendered.chars().next().unwrap();
        assert_eq!(rendered.chars().count(), 1);
        assert!(!glyph.is_ascii());

        assert_eq!(map.awbw().to_string(), " ");
        assert_eq!(map.legend().to_string(), format!("{glyph}  Teal Galaxy HQ"));
    }

    #[test]
    fn collapse_handles_more_than_seven_factions() {
        // 9 distinct player HQs — more than the 7 ASCII-symbol factions — must not
        // panic, and every tile still renders to a unique non-space lossless glyph.
        let map = AwbwMap::parse_txt("42,47,52,57,85,90,95,100,120").unwrap();
        let glyphs: Vec<char> = map
            .collapse_factions()
            .lossless()
            .to_string()
            .chars()
            .collect();

        assert_eq!(glyphs.len(), 9);
        assert!(glyphs.iter().all(|glyph| *glyph != ' '));
    }
}
