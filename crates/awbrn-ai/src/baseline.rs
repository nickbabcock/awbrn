//! The locked comparison baseline.

use crate::agent::NodeBudget;
use crate::agents::{GreedyAgent, StrategicAgent, Weights};
use crate::fingerprint::fnv1a;
use crate::rng::Rng;

/// The implementation selected by the locked baseline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum BaselineAgent {
    /// Score one legal play at a time.
    Greedy,
}

/// The tie rule used by the greedy baseline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum TieBreak {
    /// Use seeded reservoir selection over equal scores.
    SeededReservoir,
}

/// One named baseline configuration.
///
/// Change [`BaselineConfig::LOCKED`] and [`IDENTIFIER`] together when the
/// baseline behavior changes.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct BaselineConfig {
    /// Stable name for match records.
    pub identifier: &'static str,
    /// Agent implementation used by the baseline.
    pub agent: BaselineAgent,
    /// Greedy scoring weights.
    pub weights: Weights,
    /// Node budget used by baseline match drivers.
    pub node_budget: NodeBudget,
    /// Equal-score tie rule.
    pub tie_break: TieBreak,
}

/// Stable baseline identifier.
pub const IDENTIFIER: &str = "greedy-baseline-v1";

impl BaselineConfig {
    /// The only baseline configuration used for comparisons.
    pub const LOCKED: Self = Self {
        identifier: IDENTIFIER,
        agent: BaselineAgent::Greedy,
        weights: Weights::BASELINE,
        node_budget: NodeBudget::FOUR,
        tie_break: TieBreak::SeededReservoir,
    };

    /// Return a stable fingerprint of all baseline inputs.
    pub fn fingerprint(&self) -> String {
        let bytes = serde_json::to_vec(self).expect("baseline configuration serializes");
        format!("{:016x}", fnv1a(&bytes))
    }

    /// Build the locked greedy implementation for one seed.
    pub const fn build_greedy(self, seed: u64) -> GreedyAgent {
        match self.agent {
            BaselineAgent::Greedy => GreedyAgent::with_weights(seed, self.weights),
        }
    }

    /// Derive the seed for one paired game.
    pub const fn game_seed(self, run_seed: u64, pair: usize) -> u64 {
        let _ = self;
        Rng::mix(run_seed ^ ((pair as u64) << 32))
    }

    /// Derive the reducer entropy seed for one paired game.
    pub const fn entropy_seed(self, game_seed: u64) -> u64 {
        let _ = self;
        Rng::mix(game_seed ^ 0x1)
    }

    /// Derive an agent seed from a paired game seed and seat slot.
    ///
    /// Slot zero and slot one preserve the existing `^ 0x2` and `^ 0x3`
    /// streams used by the arena.
    pub const fn agent_seed(self, game_seed: u64, slot: usize) -> u64 {
        let _ = self;
        Rng::mix(game_seed ^ (slot as u64 + 0x2))
    }
}

impl Default for BaselineConfig {
    fn default() -> Self {
        Self::LOCKED
    }
}

/// Build the baseline-backed strategic agent.
///
/// The strategic agent delegates to the locked baseline. The separate public
/// name identifies the strategic interface while the behavior remains equal
/// to the baseline.
pub const fn production_agent(seed: u64) -> StrategicAgent {
    StrategicAgent::from_seed(seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_locked_configuration_has_a_stable_fingerprint() {
        assert_eq!(BaselineConfig::LOCKED.fingerprint(), "79aa8a6e0491065f");
        assert_eq!(
            BaselineConfig::LOCKED.fingerprint(),
            BaselineConfig::LOCKED.fingerprint()
        );
    }

    #[test]
    fn a_weight_change_changes_the_fingerprint() {
        let changed = BaselineConfig {
            weights: Weights {
                funds: BaselineConfig::LOCKED.weights.funds + 1.0,
                ..BaselineConfig::LOCKED.weights
            },
            ..BaselineConfig::LOCKED
        };
        assert_ne!(BaselineConfig::LOCKED.fingerprint(), changed.fingerprint());
    }
}
