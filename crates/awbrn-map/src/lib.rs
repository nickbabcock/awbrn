mod awbrn_map;
mod awbw_map;
mod deployment;
mod map_document;
mod map_error;
mod terrain_knowledge;
pub mod xy;

pub use awbrn_map::AwbrnMap;
pub use awbw_map::{AwbwMap, AwbwMapData, AwbwSymbols, Legend, LosslessSymbols, PredeployedUnit};
pub use deployment::{Deployment, Deployments};
pub use map_document::{
    AwbrnMapDocument, AwbrnMapMetadata, AwbrnMapUnit, MAP_FORMAT, MAX_DIMENSION, MapDigest,
    MapDigests, ValidatedMapDocument,
};
pub use map_error::MapError;
pub use terrain_knowledge::TerrainKnowledge;

/// The board coordinate, the board shape, and the table keyed by them.
///
/// These are the VM's own types, not copies of them. A map and the VM that
/// runs it describe the same board, so a tile on one is a tile on the other,
/// and nothing converts between two spellings of the same pair. See
/// [`xy`] for the object spelling the browser wire uses.
pub use awvm::semantic::{Dimensions, Grid, Pos};
