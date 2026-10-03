//! The opponents a match may seat, named so a record can say which one played.
//!
//! A difficulty is a word on a screen. A profile names the agent a seat uses.
//! Keep the identifier stable while a profile is in development. After release,
//! give a behavior change a new identifier so old match records keep their
//! meaning.
//!
//! The tier is what a player chooses. The identifier is what the match stores.

use crate::agent::{Agent, NodeBudget};
use crate::agents::{GreedyAgent, RandomAgent, StrategicAgent, Weights};
use crate::baseline::BaselineConfig;
use crate::fingerprint::fnv1a;
use crate::planner::{PlannerAgent, PlannerConfig};
use crate::rng::Rng;
use serde::{Deserialize, Serialize};

/// The scoring configuration of `ai-hard-v2`.
///
/// The planner of `ai-hard-v3` uses it for its seed plan and its fallback.
pub const HARD_V2_CONFIG: BaselineConfig = BaselineConfig {
    identifier: "greedy-capturer-shortfall-50-generic-tactical-v2-conceal",
    weights: Weights {
        conceal: 2.0,
        ..BaselineConfig::PRODUCTION.weights
    },
    ..BaselineConfig::PRODUCTION
};

/// How hard an opponent is meant to be, as a player reads it.
///
/// One tier holds one profile at a time. The tier is for selection. A match
/// record stores the profile identifier.
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
    /// Compares complete own turns with the planner of
    /// [`PlannerConfig::V4`].
    Planner,
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

/// The retired Hard opponent. It scores one order at a time.
///
/// It stays here because it is the fixed comparison opponent of the AI
/// diagnostics, and the planner of [`HARD`] starts from its turn.
pub const HARD_V2: AiProfile = AiProfile {
    id: "ai-hard-v2",
    tier: AiTier::Hard,
    label: "Hard",
    blurb: "Uses production scoring. In fog, values positions hidden from enemy sight.",
    implementation: AiImplementation::Strategic,
    config: HARD_V2_CONFIG,
};

/// The opponent seated for [`AiTier::Hard`].
pub const HARD: AiProfile = AiProfile {
    id: "ai-hard-v3",
    tier: AiTier::Hard,
    label: "Hard",
    blurb: "Plans its whole turn. It sets up kills, guards its bases, and keeps units out of focused fire.",
    implementation: AiImplementation::Planner,
    config: HARD_V2_CONFIG,
};

/// Every profile a stored match may name, including retired profiles.
///
/// Before release, development may replace a profile under its identifier.
/// After release, add a new identifier and keep the old one here.
pub const PROFILES: [AiProfile; 4] = [EASY, STANDARD, HARD_V2, HARD];

/// The profile that each tier seats now, easiest first.
///
/// Keep one entry per tier. Retire a profile by changing this list and keep its
/// definition in [`PROFILES`] while stored matches may name it.
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
            AiImplementation::Planner => Box::new(PlannerAgent::with_config(seed, self.planner())),
        }
    }

    /// The planner configuration that a [`AiImplementation::Planner`]
    /// profile seats.
    ///
    /// The planner starts from the greedy turn of this profile's scoring
    /// configuration.
    pub const fn planner(&self) -> PlannerConfig {
        PlannerConfig {
            baseline: self.config,
            ..PlannerConfig::V4
        }
    }

    /// Return a fingerprint for the profile and its scoring configuration.
    pub fn configuration_fingerprint(&self) -> String {
        let planner = matches!(self.implementation, AiImplementation::Planner)
            .then(|| self.planner().fingerprint());
        let bytes = match planner {
            None => serde_json::to_vec(&(
                self.id,
                self.implementation,
                self.config,
                "seeded-reservoir",
            )),
            Some(planner) => serde_json::to_vec(&(
                self.id,
                self.implementation,
                self.config,
                "seeded-reservoir",
                planner,
                self.node_budget(),
            )),
        }
        .expect("AI profile configuration serializes");
        format!("{:016x}", fnv1a(&bytes))
    }

    /// How many candidate turn plans this profile may evaluate.
    pub const fn node_budget(&self) -> NodeBudget {
        match self.implementation {
            AiImplementation::Planner => NodeBudget::THIRTY_TWO,
            _ => self.config.node_budget,
        }
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
    use crate::board::arena;
    use crate::harness::run_agent_turn_unmeasured;

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
        assert_eq!(profile("ai-hard-v2"), Some(&HARD_V2));
        assert_eq!(profile("ai-hard-v3"), Some(&HARD));
        assert_eq!(profile("ai-nonesuch"), None);
    }

    /// The identifiers are what finished matches store, so a rename is a
    /// migration and not an edit.
    #[test]
    fn identifiers_are_locked() {
        assert_eq!(
            PROFILES.map(|profile| profile.id),
            ["ai-easy-v1", "ai-standard-v1", "ai-hard-v2", "ai-hard-v3"]
        );
    }

    #[test]
    fn hard_v2_uses_a_distinct_config_with_only_concealment_changed() {
        assert_eq!(profile("ai-hard-v2"), Some(&HARD_V2));
        assert_eq!(HARD_V2.implementation, AiImplementation::Strategic);
        assert_ne!(
            HARD_V2.config.identifier,
            BaselineConfig::PRODUCTION.identifier
        );
        assert_eq!(HARD_V2.config.agent, BaselineConfig::PRODUCTION.agent);
        assert_eq!(
            HARD_V2.config.node_budget,
            BaselineConfig::PRODUCTION.node_budget
        );
        assert_eq!(
            HARD_V2.config.tie_break,
            BaselineConfig::PRODUCTION.tie_break
        );
        assert_eq!(
            HARD_V2.config.weights,
            Weights {
                conceal: 2.0,
                ..BaselineConfig::PRODUCTION.weights
            }
        );
    }

    #[test]
    fn hard_v2_keeps_the_production_standard_turn() {
        let seed = 18;
        let state = arena(false, seed);
        let mut current = HARD_V2.agent(73);
        let mut old_config = StrategicAgent::with_config(73, BaselineConfig::PRODUCTION);
        current.start_match();
        old_config.start_match();
        let mut current_entropy = Rng::from_seed(seed);
        let mut old_config_entropy = Rng::from_seed(seed);
        let current_turn = run_agent_turn_unmeasured(
            state.clone(),
            &mut *current,
            &mut current_entropy,
            HARD_V2.node_budget(),
        )
        .expect("the current standard turn executes");
        let old_config_turn = run_agent_turn_unmeasured(
            state,
            &mut old_config,
            &mut old_config_entropy,
            HARD_V2.node_budget(),
        )
        .expect("the old production turn executes");

        assert_eq!(
            current_turn.commands, old_config_turn.commands,
            "standard commands must match the old production config"
        );
        assert_eq!(current_turn.state, old_config_turn.state);
        assert_eq!(current_turn.rejected_commands, 0);
        assert_eq!(current_turn.preflight_rejections, 0);
        assert_eq!(current_turn.unrealizable_plays, 0);
    }

    #[test]
    fn every_hard_profile_runs_a_legal_fog_turn() {
        for hard in [HARD_V2, HARD] {
            let seed = 91;
            let state = arena(true, seed);
            let mut agent = hard.agent(101);
            agent.start_match();
            let mut entropy = Rng::from_seed(seed);
            let result =
                run_agent_turn_unmeasured(state, &mut *agent, &mut entropy, hard.node_budget())
                    .expect("the fog turn executes");

            assert!(result.completed, "{}", hard.id);
            assert_eq!(result.rejected_commands, 0, "{}", hard.id);
            assert_eq!(result.preflight_rejections, 0, "{}", hard.id);
            assert_eq!(result.unrealizable_plays, 0, "{}", hard.id);
            assert!(!result.commands.is_empty(), "{}", hard.id);
        }
    }

    #[test]
    fn hard_v2_profile_fingerprint_is_locked() {
        assert_eq!(HARD_V2.configuration_fingerprint(), "d5e39223474b7cd3");
    }

    #[test]
    fn hard_tier_seats_the_v4_planner_on_the_v2_scoring() {
        assert_eq!(profile_for_tier(AiTier::Hard), &HARD);
        assert_eq!(HARD.id, "ai-hard-v3");
        assert_eq!(HARD.implementation, AiImplementation::Planner);
        assert_eq!(HARD.config, HARD_V2.config);
        assert_eq!(HARD.planner(), PlannerConfig::V4);
        assert_eq!(HARD.node_budget(), NodeBudget::THIRTY_TWO);
        assert!(HARD.planner().turn_work.is_some());
    }

    #[test]
    fn hard_v3_profile_fingerprint_is_locked() {
        assert_eq!(HARD.configuration_fingerprint(), "ecc243a219620587");
    }

    #[test]
    fn a_seat_seeds_the_same_way_twice() {
        assert_eq!(HARD.turn_seed(7, 1, 3), HARD.turn_seed(7, 1, 3));
        assert_ne!(HARD.turn_seed(7, 1, 3), HARD.turn_seed(7, 1, 4));
        assert_ne!(HARD.turn_seed(7, 0, 3), HARD.turn_seed(7, 1, 3));
    }
}
