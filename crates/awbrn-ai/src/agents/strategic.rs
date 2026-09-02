//! The baseline-backed strategic agent.
//!
//! This agent has a separate identity and configuration. It delegates choices
//! to the greedy baseline and supports the common agent lifecycle.

use awvm::semantic::{Observation, ObservedEvent};

use crate::agent::{Agent, AgentTiming, NodeBudget, Play, SearchStats};
use crate::baseline::BaselineConfig;
use crate::mission::{DecisionTrace, TraceError, TurnEndReason};

use super::GreedyAgent;

/// A strategic agent backed by a greedy baseline.
#[derive(Debug)]
pub struct StrategicAgent {
    config: BaselineConfig,
    baseline: GreedyAgent,
    trace: Option<DecisionTrace>,
}

impl StrategicAgent {
    /// Build the agent from the locked baseline configuration.
    pub const fn from_seed(seed: u64) -> Self {
        Self::with_config(seed, BaselineConfig::LOCKED)
    }

    /// Build the agent with an explicit configuration.
    pub const fn with_config(seed: u64, config: BaselineConfig) -> Self {
        Self {
            config,
            baseline: config.build_greedy(seed),
            trace: None,
        }
    }

    /// Build the agent with an enabled decision trace.
    ///
    /// The agent does not generate objectives or missions. The trace remains
    /// empty unless a caller records typed advisory events.
    pub fn with_trace(seed: u64, config: BaselineConfig) -> Self {
        let mut agent = Self::with_config(seed, config);
        agent.trace = Some(DecisionTrace::new());
        agent
    }

    /// Build the locked agent with an enabled decision trace.
    pub fn from_seed_with_trace(seed: u64) -> Self {
        Self::with_trace(seed, BaselineConfig::LOCKED)
    }

    /// Return the configuration used by this agent.
    pub const fn config(&self) -> BaselineConfig {
        self.config
    }

    /// Return whether advisory tracing is enabled.
    pub const fn trace_enabled(&self) -> bool {
        self.trace.is_some()
    }

    /// Borrow the advisory trace, when tracing is enabled.
    pub const fn trace(&self) -> Option<&DecisionTrace> {
        self.trace.as_ref()
    }

    /// Borrow the advisory trace for a caller that records typed events.
    pub fn trace_mut(&mut self) -> Option<&mut DecisionTrace> {
        self.trace.as_mut()
    }

    /// Clear the advisory trace and retain its allocations.
    pub fn clear_trace(&mut self) {
        if let Some(trace) = self.trace.as_mut() {
            trace.clear();
        }
    }

    /// Take the advisory trace from the agent.
    pub fn take_trace(&mut self) -> Option<DecisionTrace> {
        self.trace.take()
    }
}

impl Agent for StrategicAgent {
    fn act(&mut self, view: &Observation, budget: NodeBudget) -> Option<Play> {
        self.baseline.act(view, budget)
    }

    fn observe(&mut self, events: &[ObservedEvent]) {
        self.baseline.observe(events);
    }

    fn start_match(&mut self) {
        self.clear_trace();
    }

    fn finalize_trace(&mut self, reason: TurnEndReason) -> Result<(), TraceError> {
        self.trace
            .as_mut()
            .map_or(Ok(()), |trace| trace.finalize(reason))
    }

    fn trace(&self) -> Option<&DecisionTrace> {
        self.trace.as_ref().filter(|trace| trace.is_finalized())
    }

    fn clear_trace(&mut self) {
        StrategicAgent::clear_trace(self);
    }

    fn timing(&self) -> Option<AgentTiming> {
        Some(AgentTiming::default())
    }

    fn search_stats(&self) -> Option<SearchStats> {
        self.baseline.search_stats()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::arena;
    use crate::rng::Rng;
    use awvm::semantic::{AwbwVisibility, observe};

    #[test]
    fn delegating_agent_matches_baseline_for_one_decision() {
        let state = arena(false, 1);
        let view = observe(&AwbwVisibility, &state, &state.turn.active_player)
            .expect("the baseline test position is observable");
        let seed = Rng::mix(7);
        let mut strategic = StrategicAgent::from_seed(seed);
        let mut baseline = BaselineConfig::LOCKED.build_greedy(seed);

        assert_eq!(
            strategic.act(&view, BaselineConfig::LOCKED.node_budget),
            baseline.act(&view, BaselineConfig::LOCKED.node_budget)
        );
        assert_eq!(
            strategic.config().identifier,
            BaselineConfig::LOCKED.identifier
        );
    }

    #[test]
    fn tracing_is_opt_in_and_does_not_change_the_baseline_play() {
        let state = arena(false, 1);
        let view = observe(&AwbwVisibility, &state, &state.turn.active_player)
            .expect("the baseline test position is observable");
        let seed = Rng::mix(11);
        let mut traced = StrategicAgent::from_seed_with_trace(seed);
        let mut baseline = BaselineConfig::LOCKED.build_greedy(seed);

        assert!(!StrategicAgent::from_seed(seed).trace_enabled());
        assert_eq!(
            traced.act(&view, BaselineConfig::LOCKED.node_budget),
            baseline.act(&view, BaselineConfig::LOCKED.node_budget)
        );
        assert!(traced.trace_enabled());
        assert!(
            traced
                .trace()
                .expect("trace is enabled")
                .records()
                .is_empty()
        );
    }
}
