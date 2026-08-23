//! From a map to a starting position.
//!
//! A map is terrain. A match needs more: a roster, teams, settings, a turn,
//! and an owner for each property. [`state_from_setup`](crate::state_from_setup)
//! adds those and gives an [`awvm::semantic::State`] the reducer accepts.
//!
//! Nothing here does server work. It computes no fog and records no entropy,
//! which is what lets a headless agent build a position without a renderer.

use std::num::NonZeroU8;

use awbrn_types::{Co, PlayerFaction};

use crate::AwbrnMap;

/// Configuration for a single player joining a game.
#[derive(Debug, Clone)]
pub struct PlayerSetup {
    pub faction: PlayerFaction,
    /// Team identifier. `None` means FFA (no team).
    pub team: Option<NonZeroU8>,
    pub starting_funds: u32,
    pub co: Co,
}

/// Configuration for creating a new game.
#[derive(Debug, Clone)]
pub struct GameSetup {
    pub map: AwbrnMap,
    pub players: Vec<PlayerSetup>,
    pub fog_enabled: bool,
    pub rng_seed: u64,
}

/// Error returned when a game cannot be initialized from the provided setup.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SetupError {
    InvalidPlayers { reason: String },
    InvalidMap { reason: String },
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPlayers { reason } => write!(f, "invalid game setup: {reason}"),
            Self::InvalidMap { reason } => write!(f, "invalid game map: {reason}"),
        }
    }
}

impl std::error::Error for SetupError {}
