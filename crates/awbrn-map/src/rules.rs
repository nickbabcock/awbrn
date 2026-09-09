//! The AWVM facts a map has to state about itself.
//!
//! A map document holds AWBW terrain, because that is the vocabulary the
//! catalog and the replay archives are written in. Anything a map says about
//! *rules* — what a tile pays, what a foot soldier can walk over — is AWVM's
//! answer, not a second table kept beside it. This module is the one place the
//! two vocabularies meet.

use awbrn_types::{AwbwTerrain, Property};
use awvm::ruleset::{self, Terrain, TerrainTrait};

/// What one property pays its owner each turn, before a commander changes it.
///
/// A match carries this as a setting, and the editor has no match to read it
/// from. Keeping the number here rather than in each caller is what lets the
/// muster and a real game agree on what a board is worth.
pub const DEFAULT_INCOME_PER_PROPERTY: u64 = 1_000;

/// The AWVM terrain an AWBW terrain becomes.
pub fn semantic_terrain(terrain: AwbwTerrain) -> Terrain {
    match terrain {
        AwbwTerrain::Plain | AwbwTerrain::PipeRubble(_) => Terrain::Plain,
        AwbwTerrain::Mountain => Terrain::Mountain,
        AwbwTerrain::Wood => Terrain::Wood,
        AwbwTerrain::River(_) => Terrain::River,
        AwbwTerrain::Road(_) => Terrain::Road,
        AwbwTerrain::Bridge(_) => Terrain::Bridge,
        AwbwTerrain::Sea => Terrain::Sea,
        AwbwTerrain::Shoal(_) => Terrain::Shoal,
        AwbwTerrain::Reef => Terrain::Reef,
        AwbwTerrain::Property(property) => match property {
            Property::City(_) => Terrain::City,
            Property::Base(_) => Terrain::Base,
            Property::Airport(_) => Terrain::Airport,
            Property::Port(_) => Terrain::Port,
            Property::ComTower(_) => Terrain::ComTower,
            Property::Lab(_) => Terrain::Lab,
            Property::HQ(_) => Terrain::Hq,
        },
        AwbwTerrain::Pipe(_) => Terrain::Pipe,
        AwbwTerrain::MissileSilo(_) => Terrain::MissileSilo,
        AwbwTerrain::PipeSeam(_) => Terrain::PipeSeam,
        AwbwTerrain::Teleporter => Terrain::Teleporter,
    }
}

/// Whether a tile pays its owner an income.
///
/// A com tower and a lab are held like any other building and pay nothing, so
/// counting buildings and counting money give different answers.
pub fn pays_income(terrain: AwbwTerrain) -> bool {
    ruleset::terrain_has(semantic_terrain(terrain), TerrainTrait::Income)
}
