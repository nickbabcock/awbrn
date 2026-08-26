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

pub mod agent;
pub mod agents;
pub mod board;
pub mod calibration;
pub mod eval;
pub mod harness;
pub mod map;
pub mod probe;
pub mod rng;
pub mod shape;
pub mod threat;
pub mod vision;

pub use agent::{Agent, MarginalDistribution, NodeBudget, Play, SearchStats};
pub use calibration::Calibration;
pub use eval::{EvalBreakdown, EvalWeights, Evaluator};
pub use harness::{Limits, Record, play, play_measured};
pub use map::ContestMap;
pub use probe::Probe;
pub use rng::Rng;
pub use shape::{SeatShape, Shape};
pub use threat::ThreatMap;
pub use vision::VisionMap;
