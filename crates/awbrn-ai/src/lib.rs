//! The opponent, over `awvm` and nothing else.
//!
//! An agent reads an [`Observation`](awvm::semantic::Observation) and names one
//! thing to do. The driver holds the authoritative state, turns that into a
//! [`Command`](awvm::transition::Command), executes it, observes again, and
//! asks once more. The agent never holds the state, which is what keeps it from
//! seeing through fog.
//!
//! [`agent`] holds the interface: the [`Agent`] trait and the [`Play`] an agent
//! returns. [`agents`] holds the implementations. [`rng`] is the seeded
//! generator that makes a game repeatable, which every measurement of one agent
//! against another needs.
//!
//! This crate is a sibling of `awbrn-client` and `awbrn-server`, not a layer
//! under either. All three consume the same core.

pub mod agent;
pub mod agents;
pub mod board;
pub mod harness;
pub mod rng;
pub mod threat;

pub use agent::{Agent, Play};
pub use harness::{Limits, Record, play};
pub use rng::Rng;
pub use threat::ThreatMap;
