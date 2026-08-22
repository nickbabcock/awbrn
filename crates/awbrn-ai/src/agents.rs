//! The agents, weakest first.
//!
//! Each one implements [`Agent`](crate::Agent) and nothing else, which is what
//! lets an arena play any of them against any other.

pub mod random;

pub use random::RandomAgent;
