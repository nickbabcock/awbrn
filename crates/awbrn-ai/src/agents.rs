//! The agents, weakest first.
//!
//! Each one implements [`Agent`](crate::Agent) and nothing else, which is what
//! lets an arena play any of them against any other.
//!
//! [`random`] draws a legal play uniformly. It is the floor: an agent that
//! cannot beat it is not an agent yet. [`greedy`] scores every legal play and
//! takes the best one, against a weighting that puts capture first. [`search`]
//! improves one complete greedy turn with a deterministic node budget.

pub mod classifier;
pub mod portfolio;
pub mod random;
pub mod search;
pub mod stratified;

pub mod greedy;

pub use classifier::{
    CaptureMission, CaptureMissionState, MissionBook, RoleAssignment, UnitRole, classify,
    classify_with_missions,
};
pub use greedy::{GreedyAgent, Weights};
pub use portfolio::{Script, ScriptPlan, generate_plan, generate_plans};
pub use random::RandomAgent;
pub use search::{SearchAgent, SearchAudit, audit};
pub use stratified::{StratifiedScripts, Stratum, generate_stratified_plan};
