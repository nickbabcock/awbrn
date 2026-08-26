//! The agents, weakest first.
//!
//! Each one implements [`Agent`](crate::Agent) and nothing else, which is what
//! lets an arena play any of them against any other.
//!
//! [`random`] draws a legal play uniformly. It is the floor: an agent that
//! cannot beat it is not an agent yet. [`greedy`] scores every legal play and
//! takes the best one, against a weighting that puts capture first. [`search`]
//! improves one complete greedy turn with a deterministic node budget.

pub mod random;
pub mod search;

pub mod greedy;

pub use greedy::{GreedyAgent, Weights};
pub use random::RandomAgent;
pub use search::SearchAgent;
