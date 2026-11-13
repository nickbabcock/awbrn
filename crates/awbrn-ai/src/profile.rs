//! The opponents a match may seat, named so a record can say which one played.
//!
//! A difficulty is a word on a screen. A profile names the agent a seat uses.
//! Keep the identifier stable while a profile is in development. After release,
//! give a behavior change a new identifier so old match records keep their
//! meaning.
//!
//! The tier is what a player chooses. The identifier is what the match stores.

use crate::agent::{Agent, NodeBudget};
use crate::agents::{StrategicAgent, Weights};
use crate::baseline::BaselineConfig;
use crate::fingerprint::fnv1a;
use crate::planner::{PlannerAgent, PlannerConfig};
use crate::rng::Rng;
use serde::{Deserialize, Serialize};

/// The scoring configuration of `ai-easy-v2` and of the retired `ai-hard-v2`.
///
/// Planner configurations up to `planner-v4` use it for their seed plan and
/// their fallback.
pub const HARD_V2_CONFIG: BaselineConfig = BaselineConfig {
    identifier: "greedy-capturer-shortfall-50-generic-tactical-v2-conceal",
    weights: Weights {
        conceal: 2.0,
        ..BaselineConfig::PRODUCTION.weights
    },
    ..BaselineConfig::PRODUCTION
};

/// The `ai-hard-v2` scoring with a build floor, so a factory is not left
/// empty while the funds can buy a unit.
///
/// Planner configurations from `planner-v5` to `planner-v7-wide` use it for
/// their seed plan and their fallback.
pub const HARD_V3_CONFIG: BaselineConfig = BaselineConfig {
    identifier: "greedy-capturer-shortfall-50-generic-tactical-v2-conceal-build-floor",
    weights: Weights {
        build_floor: 1.0,
        ..HARD_V2_CONFIG.weights
    },
    ..HARD_V2_CONFIG
};

/// The scoring configuration of `ai-medium-v1`, the greedy policy of
/// `planner-v8`: the [`HARD_V3_CONFIG`] scoring with the build model and its
/// range term, the commander effects, the power policy and the firing
/// positions of indirect units.
///
/// The planner of `ai-medium-v1` uses it for its seed plan and its fallback.
pub const V8_GREEDY_CONFIG: BaselineConfig = BaselineConfig {
    identifier: "greedy-capturer-shortfall-50-generic-tactical-v2-conceal-build-floor-v8",
    weights: Weights {
        build_model: true,
        build_range: 2.5,
        commander_effects: true,
        power_policy: true,
        firing_positions: 0.01,
        ..HARD_V3_CONFIG.weights
    },
    ..HARD_V3_CONFIG
};

/// The [`V8_GREEDY_CONFIG`] scoring with the move model fitted to human unit
/// orders at strength 0.5 ([`crate::agents::Weights::move_policy`]). In fog
/// games, the policy uses the fog model with the known map.
///
/// [`PlannerConfig::V8_MOVE`], the planner of `ai-hard-v4`, uses it for its
/// seed plan, its fallback and its simulated replies.
pub const V8_MOVE_GREEDY_CONFIG: BaselineConfig = BaselineConfig {
    identifier: "greedy-capturer-shortfall-50-generic-tactical-v2-conceal-build-floor-v8-move-v2",
    weights: Weights {
        move_policy: 0.5,
        ..V8_GREEDY_CONFIG.weights
    },
    ..V8_GREEDY_CONFIG
};

/// How hard an opponent is meant to be, as a player reads it.
///
/// One tier holds one profile at a time. The tier is for selection. A match
/// record stores the profile identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiTier {
    Easy,
    Medium,
    Hard,
}

/// Which implementation a profile seats.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiImplementation {
    /// Uses the configured strategic baseline.
    Strategic,
    /// Compares complete own turns with the planner of the profile
    /// ([`AiProfile::planner`]).
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
    /// The planner of an [`AiImplementation::Planner`] profile, and `None`
    /// for every other implementation. Its greedy policy is
    /// [`AiProfile::config`].
    pub planner: Option<PlannerConfig>,
}

/// The opponent seated for [`AiTier::Easy`]. It scores one order at a time
/// with the scoring of [`HARD_V2`].
pub const EASY: AiProfile = AiProfile {
    id: "ai-easy-v2",
    tier: AiTier::Easy,
    label: "Easy",
    blurb: "Scores one order at a time. In fog, values positions hidden from enemy sight.",
    ..HARD_V2
};

/// The opponent seated for [`AiTier::Medium`]: the planner of
/// [`PlannerConfig::V8`].
pub const MEDIUM: AiProfile = AiProfile {
    id: "ai-medium-v1",
    tier: AiTier::Medium,
    label: "Medium",
    blurb: "Plans its whole turn. It sets up kills, guards its bases, and keeps units out of focused fire.",
    implementation: AiImplementation::Planner,
    config: V8_GREEDY_CONFIG,
    planner: Some(PlannerConfig::V8),
};

/// The opponent seated for [`AiTier::Hard`]: the planner of
/// [`PlannerConfig::V8_MOVE`].
pub const HARD: AiProfile = AiProfile {
    id: "ai-hard-v4",
    tier: AiTier::Hard,
    label: "Hard",
    blurb: "Plans its whole turn, and moves its units the way strong human players do.",
    implementation: AiImplementation::Planner,
    config: V8_MOVE_GREEDY_CONFIG,
    planner: Some(PlannerConfig::V8_MOVE),
};

/// A retired Hard opponent. It scores one order at a time.
///
/// It stays here because it is the fixed comparison opponent of the AI
/// diagnostics. [`EASY`] seats the same agent under its own identifier.
pub const HARD_V2: AiProfile = AiProfile {
    id: "ai-hard-v2",
    tier: AiTier::Hard,
    label: "Hard",
    blurb: "Uses production scoring. In fog, values positions hidden from enemy sight.",
    implementation: AiImplementation::Strategic,
    config: HARD_V2_CONFIG,
    planner: None,
};

/// Every profile a stored match may name, including retired profiles.
///
/// Before release, development may replace a profile under its identifier.
/// After release, add a new identifier and keep the old one here.
pub const PROFILES: [AiProfile; 4] = [EASY, MEDIUM, HARD, HARD_V2];

/// The profile that each tier seats now, easiest first.
///
/// Keep one entry per tier. Retire a profile by changing this list and keep its
/// definition in [`PROFILES`] while stored matches may name it.
pub const CURRENT_PROFILES: [AiProfile; 3] = [EASY, MEDIUM, HARD];

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
        AiTier::Medium => &MEDIUM,
        AiTier::Hard => &HARD,
    }
}

impl AiProfile {
    /// Build the agent this profile names.
    pub fn agent(&self, seed: u64) -> Box<dyn Agent> {
        match self.implementation {
            AiImplementation::Strategic => Box::new(StrategicAgent::with_config(seed, self.config)),
            AiImplementation::Planner => Box::new(PlannerAgent::with_config(
                seed,
                self.planner.expect("a planner profile names its planner"),
            )),
        }
    }

    /// Return a fingerprint for the profile and its scoring configuration.
    pub fn configuration_fingerprint(&self) -> String {
        let planner = self.planner.map(|planner| planner.fingerprint());
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
            AiImplementation::Planner => NodeBudget::TWO_HUNDRED_FIFTY_SIX,
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
        for tier in [AiTier::Easy, AiTier::Medium, AiTier::Hard] {
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
        assert_eq!(profile("ai-hard-v4"), Some(&HARD));
        assert_eq!(profile("ai-nonesuch"), None);
    }

    /// The identifiers are what finished matches store, so a rename is a
    /// migration and not an edit.
    #[test]
    fn identifiers_are_locked() {
        assert_eq!(
            PROFILES.map(|profile| profile.id),
            ["ai-easy-v2", "ai-medium-v1", "ai-hard-v4", "ai-hard-v2"]
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

    /// The planner of a profile is there exactly when the profile seats a
    /// planner, and plays the greedy policy of the profile.
    #[test]
    fn a_planner_profile_names_its_planner() {
        for profile in PROFILES {
            match profile.implementation {
                AiImplementation::Planner => {
                    let planner = profile.planner.expect(profile.id);
                    assert_eq!(planner.baseline, profile.config, "{}", profile.id);
                }
                _ => assert_eq!(profile.planner, None, "{}", profile.id),
            }
        }
    }

    #[test]
    fn easy_seats_the_hard_v2_agent() {
        assert_eq!(profile_for_tier(AiTier::Easy), &EASY);
        assert_eq!(EASY.implementation, HARD_V2.implementation);
        assert_eq!(EASY.config, HARD_V2.config);
        assert_eq!(EASY.planner, None);
        assert_eq!(EASY.node_budget(), HARD_V2.node_budget());
    }

    #[test]
    fn every_profile_runs_a_legal_fog_turn() {
        for profile in PROFILES {
            let seed = 91;
            let state = arena(true, seed);
            let mut agent = profile.agent(101);
            agent.start_match();
            let mut entropy = Rng::from_seed(seed);
            let result =
                run_agent_turn_unmeasured(state, &mut *agent, &mut entropy, profile.node_budget())
                    .expect("the fog turn executes");

            assert!(result.completed, "{}", profile.id);
            assert_eq!(result.rejected_commands, 0, "{}", profile.id);
            assert_eq!(result.preflight_rejections, 0, "{}", profile.id);
            assert_eq!(result.unrealizable_plays, 0, "{}", profile.id);
            assert!(!result.commands.is_empty(), "{}", profile.id);
        }
    }

    #[test]
    fn hard_v2_profile_fingerprint_is_locked() {
        assert_eq!(HARD_V2.configuration_fingerprint(), "d5e39223474b7cd3");
    }

    #[test]
    fn medium_tier_seats_the_v8_planner() {
        assert_eq!(profile_for_tier(AiTier::Medium), &MEDIUM);
        assert_eq!(MEDIUM.id, "ai-medium-v1");
        assert_eq!(MEDIUM.implementation, AiImplementation::Planner);
        assert_eq!(MEDIUM.config, V8_GREEDY_CONFIG);
        assert_eq!(MEDIUM.planner, Some(PlannerConfig::V8));
        assert_eq!(MEDIUM.node_budget(), NodeBudget::TWO_HUNDRED_FIFTY_SIX);
    }

    #[test]
    fn hard_tier_seats_the_v8_move_planner() {
        assert_eq!(profile_for_tier(AiTier::Hard), &HARD);
        assert_eq!(HARD.id, "ai-hard-v4");
        assert_eq!(HARD.implementation, AiImplementation::Planner);
        assert_eq!(HARD.config, V8_MOVE_GREEDY_CONFIG);
        assert_eq!(HARD.planner, Some(PlannerConfig::V8_MOVE));
        assert_eq!(HARD.node_budget(), NodeBudget::TWO_HUNDRED_FIFTY_SIX);
    }

    /// `planner-v8-move` is `planner-v8` with the move model in its greedy
    /// policy, and nothing else: the two configurations are the same except
    /// for the identifiers and the move model strength.
    #[test]
    fn the_v8_move_planner_adds_only_the_move_model() {
        let strip = |config: PlannerConfig| {
            let mut value = serde_json::to_value(config).unwrap();
            let fields = value.as_object_mut().unwrap();
            fields.remove("identifier");
            let baseline = fields["baseline"].as_object_mut().unwrap();
            baseline.remove("identifier");
            baseline["weights"]
                .as_object_mut()
                .unwrap()
                .remove("move_policy");
            value
        };
        assert_eq!(strip(PlannerConfig::V8_MOVE), strip(PlannerConfig::V8));
        assert_eq!(PlannerConfig::V8.baseline.weights.move_policy, 0.0);
        assert_eq!(PlannerConfig::V8_MOVE.baseline.weights.move_policy, 0.5);
    }

    #[test]
    fn current_profile_fingerprints_are_locked() {
        assert_eq!(EASY.configuration_fingerprint(), "1f129fffc6556ade");
        assert_eq!(MEDIUM.configuration_fingerprint(), "bcd58e5b9055256f");
        assert_eq!(HARD.configuration_fingerprint(), "59e0ddef0dc4cfcb");
    }

    #[test]
    fn a_seat_seeds_the_same_way_twice() {
        assert_eq!(HARD.turn_seed(7, 1, 3), HARD.turn_seed(7, 1, 3));
        assert_ne!(HARD.turn_seed(7, 1, 3), HARD.turn_seed(7, 1, 4));
        assert_ne!(HARD.turn_seed(7, 0, 3), HARD.turn_seed(7, 1, 3));
    }
}
