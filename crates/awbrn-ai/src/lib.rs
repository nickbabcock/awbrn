//! The opponent, over `awvm` and nothing else.
//!
//! An agent reads an [`Observation`](awvm::semantic::Observation) and names one
//! thing to do. The driver holds the authoritative state, turns that into a
//! [`Command`](awvm::transition::Command), executes it, observes again, and
//! asks once more. The agent never holds the state, which is what keeps it from
//! seeing through fog.
//!
//! [`agent`] holds the interface: the [`Agent`] trait and the [`Play`] an agent
//! returns. [`agents`] holds the implementations. [`map`] is what the board
//! says before anybody plays on it. [`probe`] is what a commander is worth,
//! measured off the ruleset's own calculator rather than restated as a rule.
//! [`profile`] is the opponent a match seats: a versioned name for one
//! configuration, so a finished match records which opponent it was against
//! rather than a difficulty word whose meaning moves.
//! [`eval`] scores a position rather than a play, which is what a search needs
//! to stop on a board it did not play to the end; [`calibration`] is how that
//! scoring is proved to know anything, by predicting the result of games the
//! arena already plays. [`agents::search`] uses that value for a one-pass
//! improvement of a complete greedy turn. [`vision`] is the one map
//! that reads fog as fog: what this player can see now, and what a play would
//! light. [`rng`] is the seeded
//! generator that makes a game repeatable, which every measurement of one agent
//! against another needs. [`shape`] holds what a game was made of, which a
//! score does not say.
//!
//! This crate is a sibling of `awbrn-client` and `awbrn-server`, not a layer
//! under either. All three consume the same core.

pub mod adaptive;
pub mod agent;
pub mod agents;
pub mod baseline;
pub mod board;
pub mod calibration;
pub mod diagnostic;
pub mod eval;
mod fingerprint;
pub mod harness;
pub mod map;
pub mod mission;
pub mod probe;
pub mod profile;
pub mod rng;
pub mod shape;
pub mod threat;
pub mod vision;

pub use agent::{
    Agent, AgentTiming, MarginalDistribution, NodeBudget, Play, SearchCoordinateCoverage,
    SearchCoverage, SearchStats,
};
pub use agents::{
    GreedyAttackBreakdown, GreedyScoreBreakdown, ScoredOrder, ScoredOrders, StrategicAgent,
    order_candidate_id,
};
pub use baseline::{
    BaselineAgent, BaselineConfig, IDENTIFIER as BASELINE_IDENTIFIER, PRODUCTION_IDENTIFIER,
    TieBreak, production_agent, production_configuration_fingerprint,
};
pub use calibration::Calibration;
pub use eval::{
    EVAL_WEIGHT_TERMS, EvalArmyRelativeShares, EvalBreakdown, EvalBreakdownContext, EvalPreset,
    EvalRawFeatures, EvalScoreDelta, EvalTerminalReason, EvalTerms, EvalWeightKind, EvalWeightTerm,
    EvalWeights, Evaluator, MIRROR_TOLERANCE, RESIDUAL_TOLERANCE,
};
pub use fingerprint::FNV1A_OFFSET_BASIS;
pub use harness::{
    Limits, Record, TurnResult, next_command_fingerprint, play, play_measured, play_observed,
    play_observed_fallible, run_agent_turn, run_agent_turn_unmeasured,
};
pub use map::ContestMap;
pub use mission::{DecisionTrace, TraceError, TurnEndReason};
pub use probe::Probe;
pub use profile::{
    AiImplementation, AiProfile, AiTier, CURRENT_PROFILES as AI_CURRENT_PROFILES, EASY, HARD,
    PROFILES as AI_PROFILES, STANDARD, profile, profile_for_tier,
};
pub use rng::Rng;
pub use shape::{SeatShape, Shape};
pub use threat::ThreatMap;
pub use vision::VisionMap;
