//! Checked-in map loading, normalization, and fingerprint validation.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use awbrn_map::{AwbrnMap, AwbwMap};
use awbrn_types::{AwbwTerrain, Faction, PlayerFaction, UnitExt};
use awvm::semantic::{Pos, State};
use serde::{Deserialize, Serialize};

/// The canonical seats used by all registered two-player maps.
pub const CANONICAL_SEATS: [PlayerFaction; 2] =
    [PlayerFaction::OrangeStar, PlayerFaction::BlueMoon];

/// One source map named by a manifest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapManifestEntry {
    pub awbw_id: u32,
    pub name: String,
    pub source: String,
    pub original_factions: [String; 2],
    #[serde(default)]
    pub source_fingerprint: Option<String>,
    #[serde(default)]
    pub normalized_fingerprint: Option<String>,
}

/// A versioned set of checked-in maps.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapManifest {
    pub schema_version: u16,
    pub maps: Vec<MapManifestEntry>,
    #[serde(skip)]
    source_root: PathBuf,
}

impl MapManifest {
    /// Parse a manifest and use the workspace map directory for relative sources.
    pub fn from_json(data: &[u8]) -> Result<Self, MapRegistryError> {
        let mut manifest: Self = serde_json::from_slice(data)?;
        if manifest.source_root.as_os_str().is_empty() {
            manifest.source_root = default_source_root();
        }
        Ok(manifest)
    }

    /// Read a manifest. Its source paths are relative to the manifest directory.
    pub fn read(path: impl AsRef<Path>) -> Result<Self, MapRegistryError> {
        let path = path.as_ref();
        let mut manifest = Self::from_json(&fs::read(path)?)?;
        manifest.source_root = path.parent().map(Path::to_owned).unwrap_or_default();
        Ok(manifest)
    }

    /// Set the directory used to resolve source map paths.
    pub fn with_source_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.source_root = root.into();
        self
    }

    fn source_root(&self) -> PathBuf {
        if self.source_root.as_os_str().is_empty() {
            default_source_root()
        } else {
            self.source_root.clone()
        }
    }
}

/// One property in a source and normalized map.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapPropertyRecord {
    pub position: [u8; 2],
    pub property_type: String,
    pub original_faction: String,
    pub owner_slot: u8,
    pub normalized_faction: String,
    pub normalized_owner_slot: u8,
}

/// One deployment in a source and normalized map.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapDeploymentRecord {
    pub position: [u8; 2],
    pub unit: String,
    pub hp: u8,
    pub original_faction: String,
    pub owner_slot: u8,
    pub normalized_faction: String,
    pub normalized_owner_slot: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct MapFingerprintInput {
    awbw_id: u32,
    width: u8,
    height: u8,
    terrain: Vec<String>,
    original_factions: [String; 2],
    canonical_seats: [String; 2],
    properties: Vec<MapPropertyRecord>,
    deployments: Vec<MapDeploymentRecord>,
    canonical_first_actor: u8,
}

/// A validated source map and its normalized two-seat form.
#[derive(Clone, Debug, PartialEq)]
pub struct RegisteredMap {
    pub id: u32,
    pub name: String,
    pub source_path: String,
    pub original_factions: [PlayerFaction; 2],
    pub source: AwbwMap,
    pub normalized: AwbwMap,
    pub properties: Vec<MapPropertyRecord>,
    pub deployments: Vec<MapDeploymentRecord>,
    pub source_fingerprint: String,
    pub normalized_fingerprint: String,
}

impl RegisteredMap {
    fn load(entry: &MapManifestEntry, source_data: &[u8]) -> Result<Self, MapRegistryError> {
        let original_factions = [
            PlayerFaction::from_country_code(&entry.original_factions[0]),
            PlayerFaction::from_country_code(&entry.original_factions[1]),
        ];
        let [Some(first), Some(second)] = original_factions else {
            return Err(MapRegistryError::Map(format!(
                "map {} has an unknown original faction",
                entry.awbw_id
            )));
        };
        if first == second {
            return Err(MapRegistryError::Map(format!(
                "map {} repeats its original faction",
                entry.awbw_id
            )));
        }
        if source_data.is_empty() {
            return Err(MapRegistryError::Map(format!(
                "map {} has empty source data",
                entry.awbw_id
            )));
        }
        let source = AwbwMap::parse_json(source_data)
            .map_err(|error| MapRegistryError::Map(error.to_string()))?;
        let original_factions = [first, second];
        for (_, terrain) in source.iter() {
            if let AwbwTerrain::Property(property) = terrain
                && let Faction::Player(faction) = property.faction()
                && !original_factions.contains(&faction)
            {
                return Err(MapRegistryError::Map(format!(
                    "map {} has property faction {} outside its manifest",
                    entry.awbw_id,
                    faction.country_code()
                )));
            }
        }
        for (_, deployment) in source.deployments().iter() {
            if !original_factions.contains(&deployment.faction) {
                return Err(MapRegistryError::Map(format!(
                    "map {} has deployment faction {} outside its manifest",
                    entry.awbw_id,
                    deployment.faction.country_code()
                )));
            }
        }
        let normalized = source.map_factions(|faction| {
            if faction == original_factions[0] {
                CANONICAL_SEATS[0]
            } else {
                CANONICAL_SEATS[1]
            }
        });
        let properties = property_records(&source, &normalized, original_factions, entry.awbw_id)?;
        let deployments =
            deployment_records(&source, &normalized, original_factions, entry.awbw_id)?;
        let source_fingerprint = fingerprint(&fingerprint_input(
            entry.awbw_id,
            entry.original_factions.clone(),
            &source,
            &properties,
            &deployments,
        ));
        let normalized_fingerprint = fingerprint(&fingerprint_input(
            entry.awbw_id,
            entry.original_factions.clone(),
            &normalized,
            &properties,
            &deployments,
        ));
        let map = Self {
            id: entry.awbw_id,
            name: entry.name.clone(),
            source_path: entry.source.clone(),
            original_factions,
            source,
            normalized,
            properties,
            deployments,
            source_fingerprint,
            normalized_fingerprint,
        };
        map.validate_setup()?;
        Ok(map)
    }

    /// Build a canonical two-seat state for a match seed.
    pub fn state(&self, seed: u64) -> Result<State, MapRegistryError> {
        let state = awbrn_ai::board::try_state_from_map(
            AwbrnMap::from_map(&self.normalized),
            &CANONICAL_SEATS,
            false,
            seed,
        )
        .map_err(|error| MapRegistryError::Map(error.to_string()))?;
        let first = state
            .player_index(&state.turn.active_player)
            .ok_or_else(|| MapRegistryError::Map("the active player has no seat".into()))?;
        if first.get() != 0 {
            return Err(MapRegistryError::Map(format!(
                "map {} does not start with canonical seat zero",
                self.id
            )));
        }
        Ok(state)
    }

    /// Validate every recorded source and normalized setup fact.
    pub fn validate_setup(&self) -> Result<(), MapRegistryError> {
        if self.original_factions[0] == self.original_factions[1]
            || self
                .properties
                .iter()
                .any(|property| property.owner_slot > 1 || property.normalized_owner_slot > 1)
            || self
                .deployments
                .iter()
                .any(|deployment| deployment.owner_slot > 1 || deployment.normalized_owner_slot > 1)
        {
            return Err(MapRegistryError::Map(format!(
                "map {} has invalid seat setup",
                self.id
            )));
        }
        for property in &self.properties {
            let position = Pos::new(property.position[0], property.position[1]);
            let source = self.source.terrain_at(position);
            let normalized = self.normalized.terrain_at(position);
            let (Some(AwbwTerrain::Property(source)), Some(AwbwTerrain::Property(normalized))) =
                (source, normalized)
            else {
                return Err(MapRegistryError::Map(format!(
                    "map {} changed property position ({}, {})",
                    self.id, property.position[0], property.position[1]
                )));
            };
            if source.kind().name() != property.property_type
                || normalized.kind().name() != property.property_type
                || faction_code(source.faction()) != property.original_faction
                || faction_code(normalized.faction()) != property.normalized_faction
            {
                return Err(MapRegistryError::Map(format!(
                    "map {} changed property at ({}, {})",
                    self.id, property.position[0], property.position[1]
                )));
            }
        }
        for deployment in &self.deployments {
            let position = Pos::new(deployment.position[0], deployment.position[1]);
            let source = self.source.deployments().get(position);
            let normalized = self.normalized.deployments().get(position);
            let (Some(source), Some(normalized)) = (source, normalized) else {
                return Err(MapRegistryError::Map(format!(
                    "map {} changed deployment position ({}, {})",
                    self.id, deployment.position[0], deployment.position[1]
                )));
            };
            if source.unit.name() != deployment.unit
                || normalized.unit.name() != deployment.unit
                || source.hp.get() != deployment.hp
                || normalized.hp.get() != deployment.hp
                || source.faction.country_code() != deployment.original_faction
                || normalized.faction.country_code() != deployment.normalized_faction
            {
                return Err(MapRegistryError::Map(format!(
                    "map {} changed deployment at ({}, {})",
                    self.id, deployment.position[0], deployment.position[1]
                )));
            }
        }
        Ok(())
    }
}

/// A deterministic registry loaded from a manifest.
#[derive(Clone, Debug, Default)]
pub struct MapRegistry {
    maps: BTreeMap<u32, RegisteredMap>,
    order: Vec<u32>,
}

impl MapRegistry {
    /// Load every map named by `manifest`.
    pub fn load(manifest: &MapManifest) -> Result<Self, MapRegistryError> {
        if manifest.schema_version != 1 {
            return Err(MapRegistryError::Manifest(format!(
                "unsupported map manifest schema {}",
                manifest.schema_version
            )));
        }
        if manifest.maps.is_empty() {
            return Err(MapRegistryError::Manifest("manifest has no maps".into()));
        }
        let mut maps = BTreeMap::new();
        let mut order = Vec::with_capacity(manifest.maps.len());
        for entry in &manifest.maps {
            if entry.name.is_empty() || !safe_source_name(&entry.source) {
                return Err(MapRegistryError::Manifest(format!(
                    "map {} has an invalid name or source",
                    entry.awbw_id
                )));
            }
            let path = manifest.source_root().join(&entry.source);
            let source = fs::read(&path).map_err(|error| {
                MapRegistryError::Manifest(format!(
                    "map {} source {} cannot be read: {error}",
                    entry.awbw_id,
                    path.display()
                ))
            })?;
            let map = RegisteredMap::load(entry, &source)?;
            if entry
                .source_fingerprint
                .as_deref()
                .is_some_and(|expected| expected != map.source_fingerprint)
            {
                return Err(MapRegistryError::Manifest(format!(
                    "map {} source fingerprint differs from the manifest",
                    entry.awbw_id
                )));
            }
            if entry
                .normalized_fingerprint
                .as_deref()
                .is_some_and(|expected| expected != map.normalized_fingerprint)
            {
                return Err(MapRegistryError::Manifest(format!(
                    "map {} normalized fingerprint differs from the manifest",
                    entry.awbw_id
                )));
            }
            if maps.insert(map.id, map).is_some() {
                return Err(MapRegistryError::Manifest(format!(
                    "manifest repeats map {}",
                    entry.awbw_id
                )));
            }
            order.push(entry.awbw_id);
        }
        Ok(Self { maps, order })
    }

    /// Load a checked-in map manifest from disk.
    pub fn load_path(path: impl AsRef<Path>) -> Result<Self, MapRegistryError> {
        let manifest = MapManifest::read(path)?;
        Self::load(&manifest)
    }

    /// Load the checked-in diagnostic map registry.
    pub fn load_checked_in() -> Result<Self, MapRegistryError> {
        let manifest =
            MapManifest::from_json(include_bytes!("../../../assets/ai-diagnostics/maps.json"))?
                .with_source_root(default_source_root());
        Self::load(&manifest)
    }

    /// Read one registered map.
    pub fn get(&self, id: u32) -> Option<&RegisteredMap> {
        self.maps.get(&id)
    }

    /// Read maps in manifest order.
    pub fn iter(&self) -> impl Iterator<Item = &RegisteredMap> {
        self.order.iter().filter_map(|id| self.get(*id))
    }

    /// Return the number of registered maps.
    pub fn len(&self) -> usize {
        self.maps.len()
    }

    /// Return whether the registry has no maps.
    pub fn is_empty(&self) -> bool {
        self.maps.is_empty()
    }

    /// Return map identities for a run manifest.
    pub fn identities(&self) -> impl Iterator<Item = (u32, &str, &str, &str)> {
        self.iter().map(|map| {
            (
                map.id,
                map.name.as_str(),
                map.source_fingerprint.as_str(),
                map.normalized_fingerprint.as_str(),
            )
        })
    }
}

/// Errors from map manifest loading or setup validation.
#[derive(Debug, thiserror::Error)]
pub enum MapRegistryError {
    #[error("map manifest error: {0}")]
    Manifest(String),
    #[error("map error: {0}")]
    Map(String),
    #[error("map manifest JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("map manifest I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

fn property_records(
    source: &AwbwMap,
    normalized: &AwbwMap,
    original: [PlayerFaction; 2],
    map_id: u32,
) -> Result<Vec<MapPropertyRecord>, MapRegistryError> {
    let mut records = Vec::new();
    for (position, terrain) in source.iter() {
        let AwbwTerrain::Property(property) = terrain else {
            continue;
        };
        let Some(owner_slot) = source_faction_slot(property.faction(), original) else {
            continue;
        };
        let Some(AwbwTerrain::Property(normalized_property)) = normalized.terrain_at(position)
        else {
            return Err(MapRegistryError::Map(format!(
                "map {map_id} changed property at {position:?}"
            )));
        };
        let Some(normalized_owner_slot) =
            source_faction_slot(normalized_property.faction(), CANONICAL_SEATS)
        else {
            return Err(MapRegistryError::Map(format!(
                "map {map_id} has an unowned normalized property"
            )));
        };
        records.push(MapPropertyRecord {
            position: [position.x, position.y],
            property_type: property.kind().name().into(),
            original_faction: faction_code(property.faction()),
            owner_slot,
            normalized_faction: faction_code(normalized_property.faction()),
            normalized_owner_slot,
        });
    }
    Ok(records)
}

fn deployment_records(
    source: &AwbwMap,
    normalized: &AwbwMap,
    original: [PlayerFaction; 2],
    map_id: u32,
) -> Result<Vec<MapDeploymentRecord>, MapRegistryError> {
    let mut records = Vec::new();
    for (position, deployment) in source.deployments().iter() {
        let Some(owner_slot) = source_faction_slot(Faction::Player(deployment.faction), original)
        else {
            return Err(MapRegistryError::Map(format!(
                "map {map_id} has an unowned source deployment"
            )));
        };
        let Some(normalized_deployment) = normalized.deployments().get(position) else {
            return Err(MapRegistryError::Map(format!(
                "map {map_id} changed deployment at {position:?}"
            )));
        };
        let Some(normalized_owner_slot) = source_faction_slot(
            Faction::Player(normalized_deployment.faction),
            CANONICAL_SEATS,
        ) else {
            return Err(MapRegistryError::Map(format!(
                "map {map_id} has an unowned normalized deployment"
            )));
        };
        records.push(MapDeploymentRecord {
            position: [position.x, position.y],
            unit: deployment.unit.name().into(),
            hp: deployment.hp.get(),
            original_faction: deployment.faction.country_code().into(),
            owner_slot,
            normalized_faction: normalized_deployment.faction.country_code().into(),
            normalized_owner_slot,
        });
    }
    Ok(records)
}

fn source_faction_slot(faction: Faction, original: [PlayerFaction; 2]) -> Option<u8> {
    match faction {
        Faction::Neutral => None,
        Faction::Player(faction) if faction == original[0] => Some(0),
        Faction::Player(faction) if faction == original[1] => Some(1),
        Faction::Player(_) => None,
    }
}

fn faction_code(faction: Faction) -> String {
    match faction {
        Faction::Neutral => "neutral".into(),
        Faction::Player(faction) => faction.country_code().into(),
    }
}

fn fingerprint_input(
    awbw_id: u32,
    original_factions: [String; 2],
    map: &AwbwMap,
    properties: &[MapPropertyRecord],
    deployments: &[MapDeploymentRecord],
) -> MapFingerprintInput {
    MapFingerprintInput {
        awbw_id,
        width: map.width(),
        height: map.height(),
        terrain: map
            .iter()
            .map(|(_, terrain)| terrain.name().into())
            .collect(),
        original_factions,
        canonical_seats: CANONICAL_SEATS.map(|faction| faction.country_code().into()),
        properties: properties.to_vec(),
        deployments: deployments.to_vec(),
        canonical_first_actor: 0,
    }
}

fn fingerprint(value: &MapFingerprintInput) -> String {
    let bytes = serde_json::to_vec(value).expect("map fingerprint input serializes");
    let mut hash = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{hash:016x}")
}

fn safe_source_name(source: &str) -> bool {
    let path = Path::new(source);
    !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(value) if !value.is_empty()))
}

fn default_source_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the AI crate is inside the workspace")
        .join("assets/maps")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_registry_loads_in_manifest_order() {
        let registry = MapRegistry::load_checked_in().expect("the checked-in maps load");
        assert_eq!(registry.len(), 4);
        let ids = registry.iter().map(|map| map.id).collect::<Vec<_>>();
        assert_eq!(ids, vec![61748, 67945, 67073, 73021]);
        let fingerprints = registry
            .iter()
            .map(|map| map.normalized_fingerprint.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            fingerprints,
            vec![
                "6f2236eae04d7b52",
                "f811fa06a6f19b1e",
                "340849e43090fda2",
                "1cdb3c3c9594d2d9"
            ]
        );
        assert!(registry.iter().all(|map| map.validate_setup().is_ok()));
    }

    #[test]
    fn source_paths_cannot_escape_the_manifest_directory() {
        assert!(!safe_source_name("../outside.json"));
        assert!(!safe_source_name("/tmp/map.json"));
        assert!(safe_source_name("61748.json"));
    }
}
