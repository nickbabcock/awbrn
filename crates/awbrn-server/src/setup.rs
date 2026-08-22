use std::num::NonZeroU8;

use awbrn_map::AwbrnMap;
use awbrn_types::{Co, PlayerFaction};

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
///
/// The map carries the units it starts, so there is no second field here that
/// could disagree with it.
#[derive(Debug, Clone)]
pub struct GameSetup {
    pub map: AwbrnMap,
    pub players: Vec<PlayerSetup>,
    pub fog_enabled: bool,
    pub rng_seed: u64,
}

#[derive(Clone)]
pub struct GameRng {
    state: u64,
}

impl GameRng {
    pub fn from_seed(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9e37_79b9_7f4a_7c15
            } else {
                seed
            },
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    /// Returns a uniformly distributed value in `0..=max`.
    pub fn roll(&mut self, max: u8) -> u8 {
        if max == 0 {
            return 0;
        }

        let range = u64::from(max) + 1;
        let max_usable = u64::MAX - (u64::MAX % range);

        loop {
            let sample = self.next_u64();
            if sample < max_usable {
                return (sample % range) as u8;
            }
        }
    }
}

impl awvm::random::Entropy for GameRng {
    fn luck(
        &mut self,
        _polarity: awvm::random::Luck,
        domain: awvm::commander::Domain,
    ) -> Result<i64, awvm::random::RandomError> {
        let width = u64::try_from(domain.maximum - domain.minimum)
            .expect("commander luck domains are ordered");
        let offset = if width == 0 {
            0
        } else {
            let range = width + 1;
            let max_usable = u64::MAX - (u64::MAX % range);
            loop {
                let sample = self.next_u64();
                if sample < max_usable {
                    break sample % range;
                }
            }
        };
        Ok(domain.minimum + offset as i64)
    }

    fn weather(&mut self) -> Result<awvm::ruleset::WeatherKind, awvm::random::RandomError> {
        Ok(match self.roll(2) {
            0 => awvm::ruleset::WeatherKind::Clear,
            1 => awvm::ruleset::WeatherKind::Rain,
            _ => awvm::ruleset::WeatherKind::Snow,
        })
    }
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
