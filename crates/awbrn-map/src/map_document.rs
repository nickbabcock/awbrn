use crate::{AwbwMap, AwbwMapData, MapError, Position, PredeployedUnit};
use awbrn_types::{AwbwTerrain, FactionCode, Unit, UnitExt};
use awvm::semantic::Dimensions;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fmt};

/// Canonical map-document format version.
pub const MAP_FORMAT: u32 = 1;

/// Maximum map width or height supported by the VM.
pub const MAX_DIMENSION: u32 = Dimensions::MAX_AXIS as u32;

/// Maximum HP for a predeployed unit.
const MAX_UNIT_HP: u32 = 10;

/// Domain-separation tags for the canonical digests.
const CONTENT_TAG: &str = "awbrn-map-content-v1\n";
const PROPERTY_TAG: &str = "awbrn-map-property-v1\n";
const UNIT_TAG: &str = "awbrn-map-unit-v1\n";

/// Canonical map document stored for each revision.
///
/// Terrain is row-major and retains AWBW IDs; units use AWBRN IDs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AwbrnMapDocument {
    pub map_format: u32,
    pub width: u32,
    pub height: u32,
    /// Row-major AWBW terrain IDs.
    pub terrain: Vec<AwbwTerrain>,
    pub units: Vec<AwbrnMapUnit>,
    /// Mutable metadata excluded from content hashes.
    pub metadata: AwbrnMapMetadata,
}

/// Map document that has passed structural validation.
///
/// Deserialization also validates the document before exposing this type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ValidatedMapDocument(AwbrnMapDocument);

impl<'de> Deserialize<'de> for ValidatedMapDocument {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        AwbrnMapDocument::deserialize(deserializer)?
            .validate()
            .map_err(serde::de::Error::custom)
    }
}

impl std::ops::Deref for ValidatedMapDocument {
    type Target = AwbrnMapDocument;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// A predeployed unit and its playable state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AwbrnMapUnit {
    #[serde(flatten)]
    pub position: Position,
    pub unit: Unit,
    pub faction: FactionCode,
    /// Included in the content hash.
    pub hp: u32,
}

/// Mutable metadata that does not identify map content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AwbrnMapMetadata {
    pub name: String,
    pub author: String,
    pub player_count: u32,
}

/// SHA-256 digest of a canonical preimage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MapDigest([u8; 32]);

impl MapDigest {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Rendered as lowercase hex for storage identifiers.
impl fmt::Display for MapDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }

        Ok(())
    }
}

/// Digests recorded for one map revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapDigests {
    pub content_hash: MapDigest,
    pub property_signature: MapDigest,
    pub unit_signature: MapDigest,
}

impl AwbrnMapDocument {
    /// Builds a document from an AWBW map and metadata.
    pub fn from_awbw_map(
        map: &AwbwMap,
        units: Vec<AwbrnMapUnit>,
        metadata: AwbrnMapMetadata,
    ) -> Self {
        Self {
            map_format: MAP_FORMAT,
            width: map.width() as u32,
            height: map.height() as u32,
            terrain: map.iter().map(|(_, terrain)| terrain).collect(),
            units,
            metadata,
        }
    }

    /// Validates the document and returns the hashable form.
    pub fn validate(self) -> Result<ValidatedMapDocument, MapError> {
        if self.map_format != MAP_FORMAT {
            return Err(MapError::UnsupportedMapFormat {
                format: self.map_format,
            });
        }

        if self.width == 0 || self.height == 0 {
            return Err(MapError::EmptyMap);
        }

        if self.width > MAX_DIMENSION || self.height > MAX_DIMENSION {
            return Err(MapError::DimensionsOutOfRange {
                width: self.width,
                height: self.height,
                limit: MAX_DIMENSION,
            });
        }

        // Keep the multiplication checked if the VM's coordinate type changes.
        let expected = (self.width as usize)
            .checked_mul(self.height as usize)
            .ok_or(MapError::DimensionsOutOfRange {
                width: self.width,
                height: self.height,
                limit: MAX_DIMENSION,
            })?;
        if self.terrain.len() != expected {
            return Err(MapError::TerrainSizeMismatch {
                expected,
                found: self.terrain.len(),
            });
        }

        let mut occupied: HashSet<Position> = HashSet::with_capacity(self.units.len());
        for unit in &self.units {
            if unit.position.x >= self.width as usize || unit.position.y >= self.height as usize {
                return Err(MapError::UnitOutOfBounds {
                    x: unit.position.x,
                    y: unit.position.y,
                });
            }

            if unit.hp == 0 || unit.hp > MAX_UNIT_HP {
                return Err(MapError::UnitHpOutOfRange {
                    x: unit.position.x,
                    y: unit.position.y,
                    hp: unit.hp,
                });
            }

            // A revision cannot contain overlapping units.
            if !occupied.insert(unit.position) {
                return Err(MapError::UnitPositionOccupied {
                    x: unit.position.x,
                    y: unit.position.y,
                });
            }
        }

        Ok(ValidatedMapDocument(self))
    }
}

impl ValidatedMapDocument {
    /// Returns the validated document.
    pub fn document(&self) -> &AwbrnMapDocument {
        &self.0
    }

    /// Returns the document as an unchecked value.
    pub fn into_document(self) -> AwbrnMapDocument {
        self.0
    }

    /// Builds the content-hash preimage.
    ///
    /// Units are sorted row-major; `map_format` and `metadata` are excluded.
    pub fn content_preimage(&self) -> String {
        let mut units = self.units.clone();
        units.sort_by_key(content_sort_key);

        let view = ContentView {
            width: self.width,
            height: self.height,
            terrain: &self.terrain,
            units,
        };

        preimage(CONTENT_TAG, &view)
    }

    /// Builds the replay property-signature preimage.
    pub fn property_preimage(&self) -> String {
        // The validated terrain is already row-major.
        let entries: Vec<PropertyEntry> = self
            .terrain
            .iter()
            .enumerate()
            .filter(|(_, terrain)| is_signature_tile(**terrain))
            .map(|(index, terrain)| PropertyEntry {
                x: (index as u32) % self.width,
                y: (index as u32) / self.width,
                terrain: *terrain,
            })
            .collect();

        preimage(PROPERTY_TAG, &entries)
    }

    /// Builds the replay unit-signature preimage.
    ///
    /// HP is excluded because replay matching does not include it.
    pub fn unit_preimage(&self) -> String {
        let mut entries: Vec<UnitEntry> = self
            .units
            .iter()
            .map(|unit| UnitEntry {
                position: unit.position,
                unit: unit.unit,
                faction: unit.faction,
            })
            .collect();
        entries.sort_by_key(|entry| {
            (
                entry.position.y,
                entry.position.x,
                entry.unit.as_str(),
                entry.faction.as_str(),
            )
        });

        preimage(UNIT_TAG, &entries)
    }

    /// Content identity for storage and change detection.
    pub fn content_hash(&self) -> MapDigest {
        digest(&self.content_preimage())
    }

    /// Signature of the replay property layer.
    pub fn property_signature(&self) -> MapDigest {
        digest(&self.property_preimage())
    }

    /// Signature of the replay predeployed-unit layer.
    pub fn unit_signature(&self) -> MapDigest {
        digest(&self.unit_preimage())
    }

    /// Computes all digests for this revision.
    pub fn digests(&self) -> MapDigests {
        MapDigests {
            content_hash: self.content_hash(),
            property_signature: self.property_signature(),
            unit_signature: self.unit_signature(),
        }
    }
}

/// Terrain represented in replay `buildings`; pipe rubble is omitted.
fn is_signature_tile(terrain: AwbwTerrain) -> bool {
    matches!(
        terrain,
        AwbwTerrain::Property(_) | AwbwTerrain::PipeSeam(_) | AwbwTerrain::MissileSilo(_)
    )
}

fn content_sort_key(unit: &AwbrnMapUnit) -> (usize, usize, &'static str, &'static str, u32) {
    (
        unit.position.y,
        unit.position.x,
        unit.unit.as_str(),
        unit.faction.as_str(),
        unit.hp,
    )
}

/// Builds a tagged compact-JSON preimage.
fn preimage<T: Serialize>(tag: &str, value: &T) -> String {
    let body = serde_json::to_string(value).expect("canonical views serialize infallibly");
    format!("{tag}{body}")
}

fn digest(preimage: &str) -> MapDigest {
    MapDigest(Sha256::digest(preimage.as_bytes()).into())
}

/// Fields included in the content hash.
#[derive(Serialize)]
struct ContentView<'a> {
    width: u32,
    height: u32,
    terrain: &'a [AwbwTerrain],
    units: Vec<AwbrnMapUnit>,
}

#[derive(Serialize)]
struct PropertyEntry {
    x: u32,
    y: u32,
    terrain: AwbwTerrain,
}

#[derive(Serialize)]
struct UnitEntry {
    #[serde(flatten)]
    position: Position,
    unit: Unit,
    faction: FactionCode,
}

impl TryFrom<&'_ AwbwMapData> for ValidatedMapDocument {
    type Error = MapError;

    fn try_from(data: &AwbwMapData) -> Result<Self, Self::Error> {
        // AwbwMap handles terrain conversion and dimension checks.
        let map = AwbwMap::try_from(data)?;

        let units = data
            .predeployed_units
            .iter()
            .map(AwbrnMapUnit::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        let metadata = AwbrnMapMetadata {
            name: data.name.clone(),
            author: data.author.clone(),
            player_count: data.player_count,
        };

        AwbrnMapDocument::from_awbw_map(&map, units, metadata).validate()
    }
}

impl TryFrom<&'_ PredeployedUnit> for AwbrnMapUnit {
    type Error = MapError;

    fn try_from(unit: &PredeployedUnit) -> Result<Self, Self::Error> {
        let kind =
            Unit::from_awbw_id(unit.unit_id).ok_or(MapError::UnknownUnitId { id: unit.unit_id })?;

        let faction =
            FactionCode::parse(&unit.country_code).ok_or_else(|| MapError::UnknownCountryCode {
                code: unit.country_code.clone(),
            })?;

        Ok(AwbrnMapUnit {
            position: Position::new(unit.unit_x as usize, unit.unit_y as usize),
            unit: kind,
            faction,
            hp: unit.unit_hp,
        })
    }
}

impl TryFrom<&'_ ValidatedMapDocument> for AwbwMap {
    type Error = MapError;

    fn try_from(document: &ValidatedMapDocument) -> Result<Self, Self::Error> {
        let width = document.width as usize;
        let mut map = AwbwMap::new(width, document.height as usize, AwbwTerrain::Plain);
        for (idx, terrain) in document.terrain.iter().enumerate() {
            let position = Position::new(idx % width, idx / width);
            if let Some(slot) = map.terrain_at_mut(position) {
                *slot = *terrain;
            }
        }

        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use awbrn_types::PLAYER_FACTION_METADATA;

    #[test]
    fn unit_names_match_the_awvm_wire_spelling() {
        // Keep canonical IDs aligned with the ruleset's wire spelling.
        for unit in Unit::ALL {
            let awvm = serde_json::to_string(&unit).unwrap();
            assert_eq!(format!("\"{}\"", unit.as_str()), awvm);
        }
    }

    #[test]
    fn identifiers_are_unique() {
        let mut units: Vec<&str> = Unit::ALL.into_iter().map(Unit::as_str).collect();
        units.sort_unstable();
        units.dedup();
        assert_eq!(units.len(), Unit::ALL.len());

        let mut factions: Vec<&str> = PLAYER_FACTION_METADATA
            .iter()
            .map(|metadata| FactionCode::from(metadata.faction()).as_str())
            .collect();
        factions.sort_unstable();
        factions.dedup();
        assert_eq!(factions.len(), PLAYER_FACTION_METADATA.len());
    }

    #[test]
    fn digest_renders_as_lowercase_hex() {
        // SHA-256 of empty input.
        assert_eq!(
            digest("").to_string(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
