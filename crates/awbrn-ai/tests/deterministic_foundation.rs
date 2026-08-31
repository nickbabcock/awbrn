use awbrn_ai::agent::{Agent, NodeBudget, Play};
use awbrn_ai::agents::{GreedyAgent, StrategicAgent};
use awbrn_ai::baseline::BaselineConfig;
use awbrn_ai::board::arena;
use awbrn_ai::harness::{Limits, Record, play_measured, run_agent_turn};
use awbrn_ai::mission::TurnEndReason;
use awbrn_ai::rng::Rng;
use awbrn_ai::{BaselineAgent, TieBreak};
use awvm::semantic::{Observation, ObservedEvent};
use awvm::session::Session;

const LIMITS: Limits = Limits {
    days: 4,
    ..Limits::DEFAULT
};

fn run_baseline(strategic: bool) -> Record {
    let state = arena(false, 11);
    let mut session = Session::new(state.clone());
    let mut entropy = Rng::from_seed(Rng::mix(12));
    if strategic {
        let mut first = StrategicAgent::from_seed(Rng::mix(13));
        let mut second = StrategicAgent::from_seed(Rng::mix(14));
        let mut agents: [&mut dyn Agent; 2] = [&mut first, &mut second];
        play_measured(state, &mut session, &mut agents, &mut entropy, LIMITS)
    } else {
        let mut first = BaselineConfig::LOCKED.build_greedy(Rng::mix(13));
        let mut second = BaselineConfig::LOCKED.build_greedy(Rng::mix(14));
        let mut agents: [&mut dyn Agent; 2] = [&mut first, &mut second];
        play_measured(state, &mut session, &mut agents, &mut entropy, LIMITS)
    }
}

#[test]
fn strategic_agent_preserves_locked_baseline_match_parity() {
    let strategic = run_baseline(true);
    let baseline = run_baseline(false);

    assert_eq!(strategic.outcome, baseline.outcome);
    assert_eq!(strategic.turns, baseline.turns);
    assert_eq!(strategic.days, baseline.days);
    assert_eq!(strategic.commands, baseline.commands);
    assert_eq!(strategic.refusals, baseline.refusals);
    assert_eq!(strategic.units, baseline.units);
    assert_eq!(
        strategic.command_fingerprints,
        baseline.command_fingerprints
    );
}

#[test]
fn a_repeated_seed_reproduces_the_command_fingerprint() {
    let first = run_baseline(false);
    let second = run_baseline(false);

    assert_eq!(first.outcome, second.outcome);
    assert_eq!(first.commands, second.commands);
    assert_eq!(first.refusals, second.refusals);
    assert_eq!(first.command_fingerprints, second.command_fingerprints);
}

#[test]
fn typed_scores_keep_the_production_total_and_tie_stream() {
    let state = arena(false, 17);
    let view = awvm::semantic::observe(
        &awvm::semantic::AwbwVisibility,
        &state,
        &state.turn.active_player,
    )
    .expect("the active player can observe the arena");
    let mut scored_agent = GreedyAgent::from_seed(19);
    let initial_tie_state = scored_agent.tie_break_state();
    let scored = scored_agent
        .scored_orders(&view)
        .expect("the active player has a legal-order session");

    assert_eq!(scored_agent.tie_break_state(), initial_tie_state);
    assert!(!scored.candidates().is_empty());
    for candidate in scored.candidates() {
        assert_eq!(candidate.score, candidate.breakdown.total);
        assert_eq!(candidate.capture, candidate.breakdown.capture);
        assert_eq!(
            GreedyAgent::order_candidate_id(candidate.order),
            candidate.candidate_id
        );
        if let Some(attack) = candidate.breakdown.attack {
            let subtotal = attack.exchange
                + attack.guaranteed_removal
                + attack.attacker_loss
                + attack.capture_denial
                + attack.pull
                + attack.sight;
            assert_eq!(attack.total, subtotal);
        }
    }

    let mut selecting_agent = GreedyAgent::from_seed(19);
    let selected = selecting_agent
        .act(&view, NodeBudget::FOUR)
        .expect("the greedy agent selects a legal play");
    let projection = Session::from_observation(&view).expect("the observation reifies");
    let selected_candidate = scored
        .candidates()
        .iter()
        .find(|candidate| {
            Play::from_order(&projection, candidate.order)
                .is_some_and(|candidate_play| candidate_play == selected)
        })
        .expect("the selected play is in the scored candidates");
    let best_score = scored
        .candidates()
        .iter()
        .map(|candidate| candidate.score)
        .fold(f64::NEG_INFINITY, f64::max);
    assert_eq!(selected_candidate.score, best_score);
}

#[derive(Default)]
struct LifecycleRecorder {
    starts: u32,
    turns: u32,
    refreshes: u32,
    classifications: u32,
    observations: u32,
    finalizations: Vec<TurnEndReason>,
    clears: u32,
}

impl Agent for LifecycleRecorder {
    fn act(&mut self, _view: &Observation, _budget: NodeBudget) -> Option<Play> {
        None
    }

    fn start_match(&mut self) {
        self.starts += 1;
    }

    fn start_turn(&mut self, _view: &Observation) {
        self.turns += 1;
    }

    fn refresh(&mut self, _view: &Observation) {
        self.refreshes += 1;
    }

    fn classify_command(&mut self, _view: &Observation, _command: &awvm::transition::Command) {
        self.classifications += 1;
    }

    fn observe(&mut self, _events: &[ObservedEvent]) {
        self.observations += 1;
    }

    fn finalize_trace(
        &mut self,
        reason: TurnEndReason,
    ) -> Result<(), awbrn_ai::mission::TraceError> {
        self.finalizations.push(reason);
        Ok(())
    }

    fn clear_trace(&mut self) {
        self.clears += 1;
    }
}

#[test]
fn one_turn_harness_preserves_match_scoped_agent_state() {
    let mut agent = LifecycleRecorder::default();
    let mut entropy = Rng::from_seed(23);
    let result = run_agent_turn(arena(false, 29), &mut agent, &mut entropy, NodeBudget::FOUR);

    assert!(result.completed);
    assert_eq!(result.commands.len(), 1);
    assert_eq!(agent.starts, 0);
    assert_eq!(agent.turns, 1);
    assert_eq!(agent.refreshes, 0);
    assert_eq!(agent.classifications, 1);
    assert_eq!(agent.observations, 1);
    assert_eq!(agent.finalizations.as_slice(), [TurnEndReason::AgentPass]);
    assert_eq!(agent.clears, 1);
    assert_ne!(
        result.command_fingerprint, 0xcbf2_9ce4_8422_2325,
        "the accepted end-turn command is part of the fingerprint"
    );
}

#[test]
fn locked_baseline_identity_is_complete() {
    assert_eq!(BaselineConfig::LOCKED.identifier, "greedy-baseline-v1");
    assert_eq!(BaselineConfig::LOCKED.agent, BaselineAgent::Greedy);
    assert_eq!(BaselineConfig::LOCKED.tie_break, TieBreak::SeededReservoir);
    assert_eq!(BaselineConfig::LOCKED.fingerprint(), "79aa8a6e0491065f");
}
