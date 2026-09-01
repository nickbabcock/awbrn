//! Diagnostics-only tactical reranking.
//!
//! This policy is an experiment. It lives in diagnostics so player-facing
//! profiles cannot enable it by accident.

use awbrn_ai::agent::{Agent, AgentTiming, NodeBudget, Play, SearchStats};
use awbrn_ai::agents::GreedyAgent;
use awbrn_ai::baseline::BaselineConfig;
use awbrn_ai::rng::Rng;
use awbrn_ai::threat::ThreatMap;
use awbrn_ai_diagnostic_types::{AgentIdentity, fingerprint_bytes};
use awvm::semantic::{Location, Observation, ObservedEvent, PlayerIdx};
use awvm::session::{OrderKind, Session};
use awvm::transition::Command;
use serde::Serialize;

use crate::tournament::AgentFactory;

/// The part of the tactical policy that is exposed to the experiment plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum TacticalRerankMode {
    Collateral,
    CaptureOnly,
}

/// Configuration for the diagnostics-only tactical reranker.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct TacticalRerank {
    top_k: usize,
    mode: TacticalRerankMode,
    penalty_percent: u16,
}

impl TacticalRerank {
    /// The former experiment default.
    pub const TOP_THREE: Self = Self {
        top_k: 3,
        mode: TacticalRerankMode::Collateral,
        penalty_percent: 100,
    };

    /// Build a validated rerank configuration.
    pub const fn configured(
        top_k: usize,
        mode: TacticalRerankMode,
        penalty_percent: u16,
    ) -> Option<Self> {
        if top_k == 0 {
            None
        } else {
            Some(Self {
                top_k,
                mode,
                penalty_percent,
            })
        }
    }

    pub const fn top_k(self) -> usize {
        self.top_k
    }

    pub const fn mode(self) -> TacticalRerankMode {
        self.mode
    }

    pub const fn penalty_percent(self) -> u16 {
        self.penalty_percent
    }
}

/// Stable executable identity for the tactical experiment.
pub const TACTICAL_EXECUTABLE_FINGERPRINT: &str = "awbrn-ai-diagnostics-tactical-rerank-v1";

/// A factory for the diagnostics-only tactical policy.
#[derive(Clone, Debug)]
pub struct TacticalFactory {
    identity: AgentIdentity,
    config: BaselineConfig,
    rerank: TacticalRerank,
}

impl TacticalFactory {
    /// Create a factory for one tactical experiment configuration.
    pub fn new(identifier: &str, config: BaselineConfig, rerank: TacticalRerank) -> Self {
        let fingerprint = fingerprint_bytes(
            &serde_json::to_vec(&(identifier, config, rerank, TACTICAL_EXECUTABLE_FINGERPRINT))
                .expect("tactical configuration serializes"),
        );
        Self {
            identity: AgentIdentity {
                identifier: identifier.to_owned(),
                configuration_fingerprint: fingerprint,
                executable_fingerprint: TACTICAL_EXECUTABLE_FINGERPRINT.into(),
            },
            config,
            rerank,
        }
    }
}

impl AgentFactory for TacticalFactory {
    fn identity(&self) -> &AgentIdentity {
        &self.identity
    }

    fn create(&self, seed: u64) -> Box<dyn Agent> {
        Box::new(TacticalAgent {
            greedy: self.config.build_greedy(seed),
            rerank: self.rerank,
            seed,
        })
    }
}

struct TacticalAgent {
    greedy: GreedyAgent,
    rerank: TacticalRerank,
    seed: u64,
}

impl Agent for TacticalAgent {
    fn act(&mut self, view: &Observation, budget: NodeBudget) -> Option<Play> {
        let session = Session::from_observation(view).ok()?;
        let baseline = self.greedy.act(view, budget)?;
        let scored = self.greedy.scored_orders(view)?;
        let seat = session.state().players.seat(&view.recipient)?;
        let baseline_exposure = exposure(&session, seat);
        let context = ScoreContext {
            session: &session,
            seat,
            baseline_exposure,
            seed: self.seed,
            penalty_percent: self.rerank.penalty_percent,
        };
        let Some((baseline_raw_score, baseline_id)) = score_for_play(&session, &scored, baseline)
        else {
            return Some(baseline);
        };
        let Some(baseline_score) =
            adjusted_score(&context, baseline, baseline_raw_score, baseline_id)
        else {
            return Some(baseline);
        };
        let mut candidates = scored
            .candidates()
            .iter()
            .copied()
            .filter(|candidate| {
                candidate.score.is_finite()
                    && candidate.score > 0.0
                    && is_tactical(candidate.order.kind(), self.rerank.mode)
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then(left.candidate_id.cmp(&right.candidate_id))
        });

        let mut best = (baseline, baseline_score);
        for candidate in candidates.into_iter().take(self.rerank.top_k) {
            let Some(play) = Play::from_order(&session, candidate.order) else {
                continue;
            };
            let Some(adjusted) =
                adjusted_score(&context, play, candidate.score, candidate.candidate_id)
            else {
                continue;
            };
            if adjusted > best.1 {
                best = (play, adjusted);
            }
        }
        Some(best.0)
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

fn score_for_play(
    session: &Session,
    scored: &awbrn_ai::agents::ScoredOrders,
    play: Play,
) -> Option<(f64, u64)> {
    scored.candidates().iter().find_map(|candidate| {
        (Play::from_order(session, candidate.order) == Some(play) && candidate.score.is_finite())
            .then_some((candidate.score, candidate.candidate_id))
    })
}

struct ScoreContext<'a> {
    session: &'a Session,
    seat: PlayerIdx,
    baseline_exposure: f64,
    seed: u64,
    penalty_percent: u16,
}

fn adjusted_score(
    context: &ScoreContext<'_>,
    play: Play,
    raw_score: f64,
    candidate_id: u64,
) -> Option<f64> {
    let command = play.command(context.session)?;
    let mut simulated = Session::new(context.session.state().clone());
    let mut entropy = Rng::from_seed(Rng::mix(context.seed ^ candidate_id));
    simulated
        .apply_command::<()>(command, &mut entropy, &mut ())
        .ok()?;
    let increase = (exposure(&simulated, context.seat) - context.baseline_exposure).max(0.0);
    let penalty = increase * f64::from(context.penalty_percent) / 100.0;
    Some(raw_score - penalty)
}

fn is_tactical(kind: OrderKind, mode: TacticalRerankMode) -> bool {
    match mode {
        TacticalRerankMode::Collateral => matches!(
            kind,
            OrderKind::Wait | OrderKind::Capture | OrderKind::Attack(_) | OrderKind::Launch(_)
        ),
        TacticalRerankMode::CaptureOnly => matches!(kind, OrderKind::Capture),
    }
}

fn exposure(session: &Session, seat: PlayerIdx) -> f64 {
    let mut threat = ThreatMap::new();
    threat.build(session, seat);
    let state = session.state();
    state
        .units
        .iter()
        .filter(|unit| unit.owner == seat)
        .filter_map(|unit| {
            let Location::Board { position } = unit.location else {
                return None;
            };
            let cell = state.board.dimensions().cell_index(position)?;
            Some(threat.immediate(cell, unit.kind) + threat.deferred(cell, unit.kind) * 0.5)
        })
        .sum()
}
