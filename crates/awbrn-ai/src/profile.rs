//! The opponents a match may seat, named so a record can say which one played.
//!
//! A difficulty is a word on a screen. What a seat records has to be more than
//! that: "hard" means one thing this month and another after the weights move,
//! and a finished match that says only "hard" no longer says who it was
//! against. So a profile carries a versioned identifier, and retuning a tier
//! mints the next version rather than changing what an old record means. This
//! is the discipline [`BaselineConfig`](crate::BaselineConfig) already keeps
//! for the comparison baseline, applied to the opponents players meet.
//!
//! The tier is what a player chooses. The identifier is what the match stores.

use crate::agent::{Agent, NodeBudget};
use crate::agents::{GreedyAgent, RandomAgent, StrategicAgent};
use crate::baseline::BaselineConfig;
use crate::rng::Rng;
use serde::{Deserialize, Serialize};

/// How hard an opponent is meant to be, as a player reads it.
///
/// One tier holds one profile at a time. A retuned tier keeps its name and
/// takes a new profile, so this is what a screen offers and never what a
/// record stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiTier {
    Easy,
    Standard,
    Hard,
}

/// Which implementation a profile seats.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiImplementation {
    /// Draws a legal play uniformly.
    Random,
    /// Scores every legal play and takes the best.
    Greedy,
    /// Uses the configured strategic baseline.
    Strategic,
}

/// One named opponent, and everything that decides how it plays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AiProfile {
    /// What a match record stores. Stable for the life of the profile.
    pub id: &'static str,
    /// The difficulty this profile currently fills.
    pub tier: AiTier,
    /// What a player is shown.
    pub label: &'static str,
    /// One line on how this opponent plays.
    pub blurb: &'static str,
    /// The implementation seated.
    pub implementation: AiImplementation,
    /// The scoring configuration the implementation reads.
    pub config: BaselineConfig,
}

/// The opponent seated for [`AiTier::Easy`].
pub const EASY: AiProfile = AiProfile {
    id: "ai-easy-v1",
    tier: AiTier::Easy,
    label: "Easy",
    blurb: "Moves at random. It will take a property it stumbles onto and little else.",
    implementation: AiImplementation::Random,
    config: BaselineConfig::LOCKED,
};

/// The opponent seated for [`AiTier::Standard`].
pub const STANDARD: AiProfile = AiProfile {
    id: "ai-standard-v1",
    tier: AiTier::Standard,
    label: "Standard",
    blurb: "Scores every play and takes the best one. It captures, builds, and trades.",
    implementation: AiImplementation::Greedy,
    config: BaselineConfig::LOCKED,
};

/// The opponent seated for [`AiTier::Hard`].
pub const HARD: AiProfile = AiProfile {
    id: "ai-hard-v1",
    tier: AiTier::Hard,
    label: "Hard",
    blurb: "Scores the promoted weighting and punishes a thin front.",
    implementation: AiImplementation::Strategic,
    config: BaselineConfig::PRODUCTION,
};

/// Every profile a stored match may name, retired versions included.
///
/// A retuned tier appends its new version here and keeps the old one, so this
/// list only grows and is not what a tier is chosen from.
pub const PROFILES: [AiProfile; 3] = [EASY, STANDARD, HARD];

/// The profile each tier seats now, easiest first.
///
/// One entry per tier. Retiring a profile means replacing the entry, not
/// removing it from [`PROFILES`].
pub const CURRENT_PROFILES: [AiProfile; 3] = [EASY, STANDARD, HARD];

/// The profile with this identifier.
///
/// A retired identifier still resolves for as long as a stored match names it,
/// which is what lets an old match reconstruct.
pub fn profile(id: &str) -> Option<&'static AiProfile> {
    PROFILES.iter().find(|profile| profile.id == id)
}

/// The profile a tier currently seats.
///
/// Named one by one rather than searched, so a retired profile that still
/// carries its tier can never be the one a new match seats.
pub fn profile_for_tier(tier: AiTier) -> &'static AiProfile {
    match tier {
        AiTier::Easy => &EASY,
        AiTier::Standard => &STANDARD,
        AiTier::Hard => &HARD,
    }
}

impl AiProfile {
    /// Build the agent this profile names.
    pub fn agent(&self, seed: u64) -> Box<dyn Agent> {
        match self.implementation {
            AiImplementation::Random => Box::new(RandomAgent::from_seed(seed)),
            AiImplementation::Greedy => {
                Box::new(GreedyAgent::with_weights(seed, self.config.weights))
            }
            AiImplementation::Strategic => Box::new(StrategicAgent::with_config(seed, self.config)),
        }
    }

    /// How many candidate turn plans this profile may evaluate.
    pub const fn node_budget(&self) -> NodeBudget {
        self.config.node_budget
    }

    /// The seed for one turn of one seat.
    ///
    /// Derived from the match's own seed rather than kept anywhere, so a seat
    /// decides the same way whether the match has been running for an hour or
    /// was rebuilt from its log a moment ago.
    pub const fn turn_seed(&self, match_seed: u64, slot: usize, day: u64) -> u64 {
        Rng::mix(match_seed ^ ((slot as u64) << 32) ^ day.wrapping_mul(0x9e37_79b9))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_unique() {
        for (index, profile) in PROFILES.iter().enumerate() {
            assert!(
                PROFILES[..index].iter().all(|other| other.id != profile.id),
                "{} is used twice",
                profile.id
            );
        }
    }

    #[test]
    fn every_tier_seats_exactly_one_current_profile() {
        for tier in [AiTier::Easy, AiTier::Standard, AiTier::Hard] {
            let seated = CURRENT_PROFILES.iter().filter(|p| p.tier == tier).count();
            assert_eq!(seated, 1, "{tier:?} seats {seated} current profiles");
            assert_eq!(profile_for_tier(tier).tier, tier);
            assert!(
                CURRENT_PROFILES.contains(profile_for_tier(tier)),
                "{tier:?} seats a profile that is not current"
            );
        }
    }

    /// Every current profile is also a profile a stored match can name.
    #[test]
    fn a_current_profile_resolves_by_identifier() {
        for current in CURRENT_PROFILES {
            assert!(profile(current.id).is_some_and(|found| *found == current));
        }
    }

    #[test]
    fn a_stored_identifier_resolves() {
        assert_eq!(profile("ai-hard-v1"), Some(&HARD));
        assert_eq!(profile("ai-nonesuch"), None);
    }

    /// The identifiers are what finished matches store, so a rename is a
    /// migration and not an edit.
    #[test]
    fn identifiers_are_locked() {
        assert_eq!(
            PROFILES.map(|profile| profile.id),
            ["ai-easy-v1", "ai-standard-v1", "ai-hard-v1"]
        );
    }

    #[test]
    fn a_seat_seeds_the_same_way_twice() {
        assert_eq!(HARD.turn_seed(7, 1, 3), HARD.turn_seed(7, 1, 3));
        assert_ne!(HARD.turn_seed(7, 1, 3), HARD.turn_seed(7, 1, 4));
        assert_ne!(HARD.turn_seed(7, 0, 3), HARD.turn_seed(7, 1, 3));
    }
}
