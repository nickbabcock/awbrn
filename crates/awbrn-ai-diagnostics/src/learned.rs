//! Offline learned-policy experiment support.

use awbrn_ai::Rng;
use awbrn_ai::agent::{Agent, AgentTiming, NodeBudget, Play, SearchStats};
use awbrn_ai::agents::{GreedyAgent, ScoredOrder};
use awbrn_ai::baseline::BaselineConfig;
use awbrn_ai_diagnostic_types::AgentIdentity;
use awvm::semantic::{Observation, ObservedEvent};
use awvm::session::Session;
use awvm::transition::Command;

use crate::feature_analysis::{
    FeatureMode, ModeAnalysisReport, ReducedEvaluator, observable_features,
};
use crate::tournament::AgentFactory;

/// Stable executable identity for the offline learned-policy experiment.
pub const LEARNED_EXECUTABLE_FINGERPRINT: &str = "awbrn-ai-learned-observable-v1";

/// A factory for a learned rerank over the locked greedy candidate set.
#[derive(Clone, Debug)]
pub struct LearnedFactory {
    identity: AgentIdentity,
    evaluator: ReducedEvaluator,
    baseline: BaselineConfig,
    top_k: usize,
}

impl LearnedFactory {
    /// Build a learned factory from an offline fog-visible model report.
    pub fn from_report(
        report: &ModeAnalysisReport,
        baseline: BaselineConfig,
        top_k: usize,
    ) -> Result<Self, String> {
        let bytes = serde_json::to_vec(&report.model).map_err(|error| error.to_string())?;
        Self::from_report_with_content_fingerprint(
            report,
            baseline,
            top_k,
            awbrn_ai_diagnostic_types::fingerprint_bytes(&bytes),
        )
    }

    /// Build a learned factory with the fingerprint of the source model file.
    pub fn from_report_with_content_fingerprint(
        report: &ModeAnalysisReport,
        baseline: BaselineConfig,
        top_k: usize,
        model_fingerprint: String,
    ) -> Result<Self, String> {
        if report.mode != FeatureMode::FogVisible {
            return Err("the live experiment requires the fog-visible model".into());
        }
        if top_k == 0 {
            return Err("learned top-k must be nonzero".into());
        }
        if model_fingerprint.is_empty() {
            return Err("learned model fingerprint must not be empty".into());
        }
        let evaluator = ReducedEvaluator::from_report(report);
        let bytes = serde_json::to_vec(&(
            model_fingerprint,
            report.model.feature_names.as_slice(),
            report.model.reduced_weights.as_slice(),
            report.model.reduced_intercept,
            baseline,
            top_k,
        ))
        .map_err(|error| error.to_string())?;
        let fingerprint = format!(
            "{}-k{top_k}",
            awbrn_ai_diagnostic_types::fingerprint_bytes(&bytes)
        );
        Ok(Self {
            identity: AgentIdentity {
                identifier: "learned-observable-v1".into(),
                configuration_fingerprint: fingerprint,
                executable_fingerprint: LEARNED_EXECUTABLE_FINGERPRINT.into(),
            },
            evaluator,
            baseline,
            top_k,
        })
    }
}

impl AgentFactory for LearnedFactory {
    fn identity(&self) -> &AgentIdentity {
        &self.identity
    }

    fn create(&self, seed: u64) -> Box<dyn Agent> {
        Box::new(LearnedAgent {
            greedy: self.baseline.build_greedy(seed),
            evaluator: self.evaluator.clone(),
            top_k: self.top_k,
            seed,
        })
    }
}

struct LearnedAgent {
    greedy: GreedyAgent,
    evaluator: ReducedEvaluator,
    top_k: usize,
    seed: u64,
}

impl Agent for LearnedAgent {
    fn act(&mut self, view: &Observation, budget: NodeBudget) -> Option<Play> {
        let session = Session::from_observation(view).ok()?;
        let scored = self.greedy.scored_orders(view)?;
        let mut candidates = scored.candidates().to_vec();
        candidates.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then(left.candidate_id.cmp(&right.candidate_id))
        });

        let mut best: Option<(f64, ScoredOrder, Play)> = None;
        for candidate in candidates.into_iter().take(self.top_k) {
            let Some(play) = Play::from_order(&session, candidate.order) else {
                continue;
            };
            let Some(command) = play.command(&session) else {
                continue;
            };
            let mut simulated = Session::new(session.state().clone());
            let mut entropy = Rng::from_seed(Rng::mix(self.seed ^ candidate.candidate_id));
            if simulated
                .apply_command(command, &mut entropy, &mut ())
                .is_err()
            {
                continue;
            }
            let Some(perspective) = simulated.state().players.seat(&view.recipient) else {
                continue;
            };
            let Some(features) = observable_features(
                simulated.state(),
                perspective,
                live_turn_index(simulated.state()),
            ) else {
                continue;
            };
            let probability = self.evaluator.probability(features);
            let replace = best
                .as_ref()
                .is_none_or(|(best_probability, best_candidate, _)| {
                    probability > *best_probability
                        || (probability == *best_probability
                            && (candidate.score > best_candidate.score
                                || (candidate.score == best_candidate.score
                                    && candidate.candidate_id < best_candidate.candidate_id)))
                });
            if replace {
                best = Some((probability, candidate, play));
            }
        }
        best.map(|(_, _, play)| play)
            .or_else(|| self.greedy.act(view, budget))
    }

    fn observe(&mut self, events: &[ObservedEvent]) {
        self.greedy.observe(events);
    }

    fn start_match(&mut self) {
        self.greedy.start_match();
    }

    fn start_turn(&mut self, view: &Observation) {
        self.greedy.start_turn(view);
    }

    fn refresh(&mut self, view: &Observation) {
        self.greedy.refresh(view);
    }

    fn classify_command(&mut self, view: &Observation, command: &Command) {
        self.greedy.classify_command(view, command);
    }

    fn finalize_trace(
        &mut self,
        reason: awbrn_ai::TurnEndReason,
    ) -> Result<(), awbrn_ai::TraceError> {
        self.greedy.finalize_trace(reason)
    }

    fn trace(&self) -> Option<&awbrn_ai::DecisionTrace> {
        self.greedy.trace()
    }

    fn clear_trace(&mut self) {
        self.greedy.clear_trace();
    }

    fn timing(&self) -> Option<AgentTiming> {
        self.greedy.timing()
    }

    fn search_stats(&self) -> Option<SearchStats> {
        self.greedy.search_stats()
    }
}

fn live_turn_index(state: &awvm::semantic::State) -> u32 {
    let players = u64::try_from(state.players.len()).unwrap_or(1);
    state
        .turn
        .day
        .saturating_sub(1)
        .saturating_mul(players)
        .saturating_add(state.turn.position as u64)
        .saturating_add(1)
        .try_into()
        .unwrap_or(u32::MAX)
}
