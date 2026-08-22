use crate::deployment::{Deployment, Deployments, MAX_UNIT_HP};
use crate::{AwbwMap, AwbwMapData, MapError, PredeployedUnit};
use awbrn_types::{AwbwTerrain, FactionCode, Unit, VisualHp};
use awvm::semantic::{Dimensions, Pos};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

/// Canonical map-document format version.
pub const MAP_FORMAT: u32 = 1;

/// Maximum map width or height supported by the VM.
pub const MAX_DIMENSION: u32 = Dimensions::MAX_AXIS as u32;

/// Domain-separation tags for the canonical digests.
const CONTENT_TAG: &str = "awbrn-map-content-v1\n";
const PROPERTY_TAG: &str = "awbrn-map-property-v1\n";
const UNIT_TAG: &str = "awbrn-map-unit-v1\n";

/// The board shape `width` by `height` describes, if the VM can run it.
///
/// A board is at most [`MAX_DIMENSION`] on each axis, because a coordinate is
/// a pair of bytes. Checking that here is what lets every coordinate past this
/// point be a [`Pos`] rather than a pair that might not fit one.
pub(crate) fn dimensions(width: usize, height: usize) -> Result<Dimensions, MapError> {
    if width == 0 || height == 0 {
        return Err(MapError::EmptyMap);
    }

    let out_of_range = || MapError::DimensionsOutOfRange {
        width: width as u32,
        height: height as u32,
        limit: MAX_DIMENSION,
    };

    let width = u8::try_from(width).map_err(|_| out_of_range())?;
    let height = u8::try_from(height).map_err(|_| out_of_range())?;
    Ok(Dimensions::new(width, height))
}

/// Canonical map document stored for each revision.
///
/// This is the wire shape, so it can hold a document that is not a map: the
/// terrain length need not match the dimensions and two units may claim one
/// tile. [`AwbrnMapDocument::validate`] is what rules those out, and it hands
/// back a [`ValidatedMapDocument`] that cannot express them.
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

/// A map document that has passed structural validation.
///
/// It holds the map itself rather than the document it came from: validation's
/// whole job is to turn a wire shape into a board, so the result is a board.
/// That is why reading the map back out of it is an accessor and not another
/// fallible conversion.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedMapDocument {
    map: AwbwMap,
    metadata: AwbrnMapMetadata,
}

impl Serialize for ValidatedMapDocument {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_document().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ValidatedMapDocument {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        AwbrnMapDocument::deserialize(deserializer)?
            .validate()
            .map_err(serde::de::Error::custom)
    }
}

/// A predeployed unit and its playable state, as the document spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AwbrnMapUnit {
    pub position: Pos,
    pub unit: Unit,
    pub faction: FactionCode,
    /// Included in the content hash.
    pub hp: VisualHp,
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
    ///
    /// The units come from the map, so a document always describes the same
    /// board the map does.
    pub fn from_awbw_map(map: &AwbwMap, metadata: AwbrnMapMetadata) -> Self {
        Self {
            map_format: MAP_FORMAT,
            width: u32::from(map.width()),
            height: u32::from(map.height()),
            terrain: map.iter().map(|(_, terrain)| terrain).collect(),
            units: map
                .deployments()
                .iter()
                .map(|(position, deployment)| AwbrnMapUnit {
                    position,
                    unit: deployment.unit,
                    faction: deployment.faction.into(),
                    hp: deployment.hp,
                })
                .collect(),
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

        let shape = dimensions(self.width as usize, self.height as usize)?;

        let mut deployments = Deployments::new(shape);
        for unit in &self.units {
            let hp = u32::from(unit.hp.get());
            if hp == 0 || hp > MAX_UNIT_HP {
                return Err(MapError::UnitHpOutOfRange {
                    x: u32::from(unit.position.x),
                    y: u32::from(unit.position.y),
                    hp,
                });
            }

            // Rejects both an off-board tile and a tile already taken.
            deployments.insert(
                unit.position,
                Deployment {
                    unit: unit.unit,
                    hp: unit.hp,
                    faction: unit.faction.into(),
                },
            )?;
        }

        let found = self.terrain.len();
        let map = AwbwMap::from_parts(shape, self.terrain, deployments).ok_or(
            MapError::TerrainSizeMismatch {
                expected: shape.len(),
                found,
            },
        )?;

        Ok(ValidatedMapDocument {
            map,
            metadata: self.metadata,
        })
    }
}

impl ValidatedMapDocument {
    /// The board this document describes, with the units it starts.
    pub fn map(&self) -> &AwbwMap {
        &self.map
    }

    /// The board, taken out of the document.
    pub fn into_map(self) -> AwbwMap {
        self.map
    }

    pub fn metadata(&self) -> &AwbrnMapMetadata {
        &self.metadata
    }

    /// The document as its wire shape.
    pub fn to_document(&self) -> AwbrnMapDocument {
        AwbrnMapDocument::from_awbw_map(&self.map, self.metadata.clone())
    }

    /// Builds the content-hash preimage.
    ///
    /// `map_format` and `metadata` are excluded. Units need no sort: a map
    /// holds them keyed by tile, so they come out row-major already.
    pub fn content_preimage(&self) -> String {
        let document = self.to_document();
        let view = ContentView {
            width: document.width,
            height: document.height,
            terrain: &document.terrain,
            units: &document.units,
        };

        preimage(CONTENT_TAG, &view)
    }

    /// Builds the replay property-signature preimage.
    pub fn property_preimage(&self) -> String {
        let entries: Vec<PropertyEntry> = self
            .map
            .iter()
            .filter(|(_, terrain)| is_signature_tile(*terrain))
            .map(|(position, terrain)| PropertyEntry { position, terrain })
            .collect();

        preimage(PROPERTY_TAG, &entries)
    }

    /// Builds the replay unit-signature preimage.
    ///
    /// HP is excluded because replay matching does not include it.
    pub fn unit_preimage(&self) -> String {
        let entries: Vec<UnitEntry> = self
            .map
            .deployments()
            .iter()
            .map(|(position, deployment)| UnitEntry {
                position,
                unit: deployment.unit,
                faction: deployment.faction.into(),
            })
            .collect();

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
    units: &'a [AwbrnMapUnit],
}

#[derive(Serialize)]
struct PropertyEntry {
    position: Pos,
    terrain: AwbwTerrain,
}

#[derive(Serialize)]
struct UnitEntry {
    position: Pos,
    unit: Unit,
    faction: FactionCode,
}

impl TryFrom<&'_ AwbwMapData> for ValidatedMapDocument {
    type Error = MapError;

    fn try_from(data: &AwbwMapData) -> Result<Self, Self::Error> {
        // The map carries its own units, so there is nothing to reconcile here.
        let map = AwbwMap::try_from(data)?;

        Ok(ValidatedMapDocument {
            map,
            metadata: AwbrnMapMetadata {
                name: data.name.clone(),
                author: data.author.clone(),
                player_count: data.player_count,
            },
        })
    }
}

impl TryFrom<&'_ PredeployedUnit> for AwbrnMapUnit {
    type Error = MapError;

    fn try_from(unit: &PredeployedUnit) -> Result<Self, Self::Error> {
        let (position, deployment) = Deployment::from_predeployed(unit)?;

        Ok(AwbrnMapUnit {
            position,
            unit: deployment.unit,
            faction: deployment.faction.into(),
            hp: deployment.hp,
        })
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
