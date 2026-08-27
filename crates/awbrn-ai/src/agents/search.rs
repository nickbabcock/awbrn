//! One-pass improvement of a complete greedy turn.
//!
//! The greedy turn is the seed. The search changes one unit order at a time,
//! repairs the rest of the friendly turn greedily, plays a fresh greedy reply,
//! and evaluates the resulting position. One complete evaluated candidate is
//! one node.

use awvm::random::Entropy;
use awvm::semantic::{
    AwbwVisibility, Match, Observation, PlayerId, PlayerIdx, State, UnitId, observe_into,
};
use awvm::session::{Mark, Order, OrderKind, Session};
use awvm::transition::Command;

use crate::agent::{Agent, MarginalDistribution, NodeBudget, Play, SearchStats};
use crate::agents::{GreedyAgent, Weights};
use crate::eval::{EvalBreakdown, EvalWeights, Evaluator};
use crate::rng::Rng;

/// A greedy agent with one coordinate-descent pass over its complete turn.
#[derive(Debug)]
pub struct SearchAgent {
    seed: u64,
    weights: Weights,
    search_eval_weights: EvalWeights,
    marginal_cap: Option<f64>,
    plan: Vec<Play>,
    next: usize,
    turn: Option<(PlayerId, u64)>,
    stats: SearchStats,
}

/// One search choice and the two leaf positions it compared.
#[derive(Clone, Debug)]
pub struct SearchAudit {
    /// The root position.
    pub root: State,
    /// The seat that made the choice.
    pub friendly: PlayerId,
    /// The seat index that made the choice.
    pub friendly_seat: PlayerIdx,
    /// The seed used by the friendly greedy policy.
    pub friendly_seed: u64,
    /// The seed used by the greedy reply.
    pub reply_seed: u64,
    /// The seed used for speculative command entropy.
    pub entropy_seed: u64,
    /// The weighting used by the greedy policy.
    pub weights: Weights,
    /// The complete greedy seed plan.
    pub seed_plan: Vec<Order>,
    /// The selected plan.
    pub selected_plan: Vec<Order>,
    /// The leaf after the seed and its greedy reply.
    pub seed_state: State,
    /// The leaf after the selected plan and its greedy reply.
    pub selected_state: State,
    /// The evaluator score of the seed leaf.
    pub seed_score: f64,
    /// The evaluator score of the selected leaf.
    pub selected_score: f64,
    /// The seed leaf breakdown.
    pub seed_breakdown: EvalBreakdown,
    /// The selected leaf breakdown.
    pub selected_breakdown: EvalBreakdown,
    /// Every order that changed between the two plans.
    pub changes: Vec<OrderChange>,
}

/// One unit order that changed in a search choice.
#[derive(Clone, Copy, Debug)]
pub struct OrderChange {
    /// The order position in the turn plan.
    pub coordinate: usize,
    /// The unit id, when the order names a unit.
    pub unit: Option<UnitId>,
    /// The order in the seed plan.
    pub seed: Order,
    /// The selected order.
    pub selected: Order,
}

/// Audit one search decision without changing the supplied observation.
pub fn audit(
    view: &Observation,
    seed: u64,
    weights: Weights,
    search_eval_weights: EvalWeights,
    budget: NodeBudget,
) -> Option<SearchAudit> {
    let mut search = TurnSearch::new(view, seed, weights, search_eval_weights, None)?;
    let root = search.session.state().clone();
    let result = search.search_orders(budget)?;
    let friendly = search.friendly.clone();
    let friendly_seat = search.session.state().players.seat(&friendly)?;
    let seed_state = search.line_state(&result.seed)?;
    let selected_state = search.line_state(&result.selected)?;
    let mut evaluator = Evaluator::new(search_eval_weights);
    let seed_session = Session::new(seed_state.clone());
    let selected_session = Session::new(selected_state.clone());
    let seed_breakdown = evaluator.breakdown_in(&seed_session, friendly_seat);
    let selected_breakdown = evaluator.breakdown_in(&selected_session, friendly_seat);
    let changes = result
        .seed
        .iter()
        .zip(&result.selected)
        .enumerate()
        .filter(|(_, (seed_order, selected_order))| seed_order != selected_order)
        .map(|(coordinate, (seed_order, selected_order))| OrderChange {
            coordinate,
            unit: search.session.unit_of(*seed_order),
            seed: *seed_order,
            selected: *selected_order,
        })
        .collect();

    Some(SearchAudit {
        root,
        friendly,
        friendly_seat,
        friendly_seed: search.friendly_seed,
        reply_seed: search.reply_seed,
        entropy_seed: search.entropy_seed,
        weights,
        seed_plan: result.seed,
        selected_plan: result.selected,
        seed_state,
        selected_state,
        seed_score: result.seed_score,
        selected_score: result.selected_score,
        seed_breakdown,
        selected_breakdown,
        changes,
    })
}

impl SearchAgent {
    pub const fn from_seed(seed: u64) -> Self {
        Self::with_weights(seed, Weights::DEFAULT)
    }

    pub const fn with_weights(seed: u64, weights: Weights) -> Self {
        Self::with_weights_and_evaluator(seed, weights, EvalWeights::STANDARD)
    }

    pub const fn with_weights_and_evaluator(
        seed: u64,
        weights: Weights,
        search_eval_weights: EvalWeights,
    ) -> Self {
        Self::with_weights_evaluator_and_cap(seed, weights, search_eval_weights, None)
    }

    pub const fn with_weights_evaluator_and_cap(
        seed: u64,
        weights: Weights,
        search_eval_weights: EvalWeights,
        marginal_cap: Option<f64>,
    ) -> Self {
        Self {
            seed,
            weights,
            search_eval_weights,
            marginal_cap,
            plan: Vec::new(),
            next: 0,
            turn: None,
            stats: SearchStats {
                nodes_evaluated: 0,
                legal_candidates_rejected: 0,
                seed_plans: 0,
                changed_seed_plans: 0,
                changed_leaf_breakdowns: 0,
                changed_leaf_deltas: EvalBreakdown {
                    score: 0.0,
                    army: 0.0,
                    income: 0.0,
                    exposure: 0.0,
                    contest: 0.0,
                    front: 0.0,
                    other: 0.0,
                },
                standard_leaf_deltas: EvalBreakdown {
                    score: 0.0,
                    army: 0.0,
                    income: 0.0,
                    exposure: 0.0,
                    contest: 0.0,
                    front: 0.0,
                    other: 0.0,
                },
                standard_front_deltas: MarginalDistribution::new(),
                standard_exposure_deltas: MarginalDistribution::new(),
            },
        }
    }

    fn build_plan(&mut self, view: &Observation, budget: NodeBudget) -> Option<()> {
        let mut search = TurnSearch::new(
            view,
            self.seed,
            self.weights,
            self.search_eval_weights,
            self.marginal_cap,
        )?;
        let plan = search.improve(budget);
        self.stats.add(search.stats);
        self.plan = plan?;
        self.next = 0;
        self.turn = Some((view.turn.active_player.clone(), view.turn.day));
        Some(())
    }
}

impl Agent for SearchAgent {
    fn act(&mut self, view: &Observation, budget: NodeBudget) -> Option<Play> {
        let turn = (view.turn.active_player.clone(), view.turn.day);
        if self.turn.as_ref() == Some(&turn) && self.next >= self.plan.len() {
            return None;
        }
        if self.turn.as_ref() != Some(&turn) {
            self.build_plan(view, budget)?;
        }
        let play = self.plan.get(self.next).copied()?;
        self.next += 1;
        Some(play)
    }

    fn search_stats(&self) -> Option<SearchStats> {
        Some(self.stats)
    }
}

struct TurnSearch {
    session: Session,
    view: Observation,
    friendly: PlayerId,
    friendly_seed: u64,
    reply_seed: u64,
    entropy_seed: u64,
    weights: Weights,
    evaluator: Evaluator,
    marginal_cap: Option<f64>,
    stats: SearchStats,
}

struct SearchResult {
    seed: Vec<Order>,
    selected: Vec<Order>,
    seed_score: f64,
    selected_score: f64,
}

struct SearchLeaf {
    score: f64,
    breakdown: Option<EvalBreakdown>,
    plan: Vec<Order>,
}

impl TurnSearch {
    fn new(
        view: &Observation,
        seed: u64,
        weights: Weights,
        search_eval_weights: EvalWeights,
        marginal_cap: Option<f64>,
    ) -> Option<Self> {
        let session = Session::from_observation(view).ok()?;
        if !session.is_commandable() || session.state().settings.fog {
            return None;
        }
        Some(Self {
            friendly: session.state().turn.active_player.clone(),
            view: view.clone(),
            session,
            friendly_seed: seed,
            reply_seed: Rng::mix(seed ^ 0x2),
            entropy_seed: Rng::mix(seed ^ 0x3),
            weights,
            evaluator: Evaluator::new(search_eval_weights),
            marginal_cap,
            stats: SearchStats::default(),
        })
    }

    fn improve(&mut self, budget: NodeBudget) -> Option<Vec<Play>> {
        let result = self.search_orders(budget)?;
        self.plays(&result.selected)
    }

    fn search_orders(&mut self, budget: NodeBudget) -> Option<SearchResult> {
        let seed = self.greedy_seed()?;
        self.stats.seed_plans += 1;
        let seed_leaf = self.evaluate(&seed)?;
        self.stats.nodes_evaluated += 1;
        let seed_breakdown = seed_leaf.breakdown;
        let seed_score = self.rank_score(seed_leaf.score, seed_breakdown, seed_breakdown)?;
        let mut best_value = seed_score;
        let mut best = seed.clone();
        let mut nodes = 1;

        let mut coordinate = 0;
        'coordinates: while coordinate < best.len() {
            let coordinate_plan = best.clone();
            let Some(unit) = coordinate_plan[coordinate].unit() else {
                coordinate += 1;
                continue;
            };
            for alternative in self.alternatives(&coordinate_plan, coordinate, unit) {
                if nodes >= budget.get() {
                    break 'coordinates;
                }
                if alternative == coordinate_plan[coordinate] {
                    self.stats.legal_candidates_rejected += 1;
                    continue;
                }
                let Some(candidate_leaf) =
                    self.evaluate_repaired(&coordinate_plan[..coordinate], alternative)
                else {
                    self.stats.legal_candidates_rejected += 1;
                    continue;
                };
                let Some(value) = self.rank_score(
                    candidate_leaf.score,
                    candidate_leaf.breakdown,
                    seed_breakdown,
                ) else {
                    self.stats.legal_candidates_rejected += 1;
                    continue;
                };
                self.stats.nodes_evaluated += 1;
                nodes += 1;
                if value > best_value {
                    best_value = value;
                    best = candidate_leaf.plan;
                }
            }
            coordinate += 1;
        }

        if best != seed {
            self.stats.changed_seed_plans += 1;
            let search_eval_weights = self.search_eval_weights();
            if let (Some(seed_state), Some(selected_state)) =
                (self.line_state(&seed), self.line_state(&best))
            {
                let seed_breakdown =
                    Self::state_breakdown(&seed_state, &self.friendly, search_eval_weights);
                let selected_breakdown =
                    Self::state_breakdown(&selected_state, &self.friendly, search_eval_weights);
                if let (Some(seed_breakdown), Some(selected_breakdown)) =
                    (seed_breakdown, selected_breakdown)
                {
                    self.stats.changed_leaf_breakdowns += 1;
                    add_breakdown_delta(
                        &mut self.stats.changed_leaf_deltas,
                        seed_breakdown,
                        selected_breakdown,
                    );
                }
                let standard = if search_eval_weights == EvalWeights::STANDARD {
                    (seed_breakdown, selected_breakdown)
                } else {
                    (
                        Self::state_breakdown(&seed_state, &self.friendly, EvalWeights::STANDARD),
                        Self::state_breakdown(
                            &selected_state,
                            &self.friendly,
                            EvalWeights::STANDARD,
                        ),
                    )
                };
                if let (Some(seed_breakdown), Some(selected_breakdown)) = standard {
                    add_breakdown_delta(
                        &mut self.stats.standard_leaf_deltas,
                        seed_breakdown,
                        selected_breakdown,
                    );
                    self.stats
                        .standard_front_deltas
                        .record(selected_breakdown.front - seed_breakdown.front);
                    self.stats
                        .standard_exposure_deltas
                        .record(selected_breakdown.exposure - seed_breakdown.exposure);
                }
            }
        }
        Some(SearchResult {
            seed,
            selected: best,
            seed_score,
            selected_score: best_value,
        })
    }

    fn greedy_seed(&mut self) -> Option<Vec<Order>> {
        let mut agent = GreedyAgent::with_weights(self.friendly_seed, self.weights);
        let mut entropy = Rng::from_seed(self.entropy_seed);
        let mut root = None;
        let plan = match self.greedy_turn(&mut agent, &mut entropy, &mut root) {
            Some(plan) => plan,
            None => {
                if let Some(mark) = root {
                    self.session.rewind(mark);
                }
                return None;
            }
        };
        self.session.rewind(root?);
        Some(plan)
    }

    fn greedy_turn(
        &mut self,
        agent: &mut GreedyAgent,
        entropy: &mut impl Entropy,
        root: &mut Option<Mark>,
    ) -> Option<Vec<Order>> {
        let mut plan = Vec::new();
        while self.session.state().turn.active_player == self.friendly
            && matches!(self.session.state().match_state, Match::Active { .. })
        {
            self.observe(&self.friendly.clone())?;
            let command = agent
                .act(&self.view, NodeBudget::ONE)
                .and_then(|play| play.command(&self.session))
                .unwrap_or_else(|| Command::EndTurn {
                    player: self.friendly.clone(),
                });
            let order = self.session.resolve(&command).ok()?;
            let mark = self.session.apply(order, entropy, &mut ()).ok()?;
            root.get_or_insert(mark);
            plan.push(order);
        }
        Some(plan)
    }

    fn alternatives(
        &mut self,
        seed: &[Order],
        coordinate: usize,
        unit: awvm::session::UnitIdx,
    ) -> Vec<Order> {
        let mut entropy = Rng::from_seed(self.entropy_seed);
        let mut root = None;
        for order in seed[..coordinate].iter().copied() {
            let Ok(mark) = self.session.apply(order, &mut entropy, &mut ()) else {
                if let Some(mark) = root {
                    self.session.rewind(mark);
                }
                return Vec::new();
            };
            root.get_or_insert(mark);
        }
        let mut alternatives = Vec::new();
        self.session.legal().unit_orders(unit, &mut alternatives);
        alternatives.retain(|order| {
            !matches!(
                order.kind(),
                OrderKind::Delete | OrderKind::Resign | OrderKind::EndTurn
            )
        });
        if let Some(mark) = root {
            self.session.rewind(mark);
        }
        alternatives
    }

    fn evaluate(&mut self, plan: &[Order]) -> Option<SearchLeaf> {
        let original = self.session.state().clone();
        let mut entropy = Rng::from_seed(self.entropy_seed);
        let mut root = None;
        for order in plan.iter().copied() {
            match self.session.apply(order, &mut entropy, &mut ()) {
                Ok(mark) => {
                    root.get_or_insert(mark);
                }
                Err(_) => {
                    if let Some(mark) = root {
                        self.session.rewind(mark);
                    }
                    debug_assert_eq!(self.session.state(), &original);
                    return None;
                }
            }
        }

        let result = self.greedy_reply(&mut entropy).and_then(|reply| {
            debug_assert!(!reply.is_empty(), "each leaf has an opponent reply");
            self.leaf(plan.to_vec())
        });
        self.session.rewind(root?);
        debug_assert_eq!(self.session.state(), &original);
        result
    }

    fn evaluate_repaired(&mut self, prefix: &[Order], alternative: Order) -> Option<SearchLeaf> {
        let original = self.session.state().clone();
        let mut entropy = Rng::from_seed(self.entropy_seed);
        let mut root = None;
        for order in prefix.iter().copied().chain(std::iter::once(alternative)) {
            match self.session.apply(order, &mut entropy, &mut ()) {
                Ok(mark) => {
                    root.get_or_insert(mark);
                }
                Err(_) => {
                    if let Some(mark) = root {
                        self.session.rewind(mark);
                    }
                    debug_assert_eq!(self.session.state(), &original);
                    return None;
                }
            }
        }

        let mut agent = GreedyAgent::with_weights(self.friendly_seed, self.weights);
        let suffix = match self.greedy_turn(&mut agent, &mut entropy, &mut root) {
            Some(suffix) => suffix,
            None => {
                if let Some(mark) = root {
                    self.session.rewind(mark);
                }
                debug_assert_eq!(self.session.state(), &original);
                return None;
            }
        };
        let mut plan = prefix.to_vec();
        plan.push(alternative);
        plan.extend(suffix);
        let result = self.greedy_reply(&mut entropy).and_then(|reply| {
            debug_assert!(!reply.is_empty(), "each leaf has an opponent reply");
            self.leaf(plan)
        });
        self.session.rewind(root?);
        debug_assert_eq!(self.session.state(), &original);
        result
    }

    fn leaf(&mut self, plan: Vec<Order>) -> Option<SearchLeaf> {
        let seat = self.session.state().players.seat(&self.friendly)?;
        let value = self.evaluator.value_in(&self.session, seat);
        let breakdown = self
            .marginal_cap
            .map(|_| self.evaluator.breakdown_in(&self.session, seat));
        Some(SearchLeaf {
            score: value,
            breakdown,
            plan,
        })
    }

    fn line_state(&mut self, plan: &[Order]) -> Option<State> {
        let original = self.session.state().clone();
        let mut entropy = Rng::from_seed(self.entropy_seed);
        let mut root = None;
        for order in plan.iter().copied() {
            let mark = match self.session.apply(order, &mut entropy, &mut ()) {
                Ok(mark) => mark,
                Err(_) => {
                    if let Some(mark) = root {
                        self.session.rewind(mark);
                    }
                    debug_assert_eq!(self.session.state(), &original);
                    return None;
                }
            };
            root.get_or_insert(mark);
        }
        if self.greedy_reply(&mut entropy).is_none() {
            if let Some(mark) = root {
                self.session.rewind(mark);
            }
            debug_assert_eq!(self.session.state(), &original);
            return None;
        }
        let state = self.session.state().clone();
        let Some(root) = root else {
            debug_assert_eq!(self.session.state(), &original);
            return None;
        };
        self.session.rewind(root);
        debug_assert_eq!(self.session.state(), &original);
        Some(state)
    }

    fn search_eval_weights(&self) -> EvalWeights {
        *self.evaluator.weights()
    }

    fn rank_score(
        &self,
        score: f64,
        candidate: Option<EvalBreakdown>,
        seed: Option<EvalBreakdown>,
    ) -> Option<f64> {
        let Some(cap) = self.marginal_cap else {
            return Some(score);
        };
        let candidate = candidate?;
        let seed = seed?;
        let front = candidate.front - seed.front;
        let exposure = candidate.exposure - seed.exposure;
        Some(score - front - exposure + front.clamp(-cap, cap) + exposure.clamp(-cap, cap))
    }

    fn state_breakdown(
        state: &State,
        friendly: &PlayerId,
        eval_weights: EvalWeights,
    ) -> Option<EvalBreakdown> {
        let seat = state.players.seat(friendly)?;
        let session = Session::new(state.clone());
        let mut evaluator = Evaluator::new(eval_weights);
        matches!(state.match_state, Match::Active { .. })
            .then(|| evaluator.breakdown_in(&session, seat))
    }

    fn greedy_reply(&mut self, entropy: &mut impl Entropy) -> Option<Vec<Order>> {
        let opponent = self.session.state().turn.active_player.clone();
        let mut agent = GreedyAgent::with_weights(self.reply_seed, self.weights);
        let mut reply = Vec::new();
        while self.session.state().turn.active_player == opponent
            && matches!(self.session.state().match_state, Match::Active { .. })
        {
            self.observe(&opponent)?;
            let command = agent
                .act(&self.view, NodeBudget::ONE)
                .and_then(|play| play.command(&self.session))
                .unwrap_or_else(|| Command::EndTurn {
                    player: opponent.clone(),
                });
            let order = self.session.resolve(&command).ok()?;
            self.session.apply(order, entropy, &mut ()).ok()?;
            reply.push(order);
        }
        Some(reply)
    }

    fn plays(&mut self, plan: &[Order]) -> Option<Vec<Play>> {
        let mut entropy = Rng::from_seed(self.entropy_seed);
        let mut root = None;
        let mut plays = Vec::new();
        for order in plan.iter().copied() {
            if order.kind() != OrderKind::EndTurn {
                plays.push(Play::from_order(&self.session, order)?);
            }
            let mark = self.session.apply(order, &mut entropy, &mut ()).ok()?;
            root.get_or_insert(mark);
        }
        self.session.rewind(root?);
        Some(plays)
    }

    fn observe(&mut self, player: &PlayerId) -> Option<()> {
        observe_into(
            &AwbwVisibility,
            self.session.state(),
            player,
            &mut self.view,
        )
        .ok()
    }
}

fn add_breakdown_delta(total: &mut EvalBreakdown, seed: EvalBreakdown, selected: EvalBreakdown) {
    total.score += selected.score - seed.score;
    total.army += selected.army - seed.army;
    total.income += selected.income - seed.income;
    total.exposure += selected.exposure - seed.exposure;
    total.contest += selected.contest - seed.contest;
    total.front += selected.front - seed.front;
    total.other += selected.other - seed.other;
}

#[cfg(test)]
mod tests {
    use awvm::semantic::observe;
    use awvm::transition::{ExecuteOutcome, execute};

    use super::*;
    use crate::board::arena;

    fn view() -> Observation {
        let mut state = arena(false, 1);
        for _ in 0..1 {
            let player = state.turn.active_player.clone();
            state = match execute(&state, Command::EndTurn { player }, &[]) {
                Ok(ExecuteOutcome::Accepted(execution)) => execution.state,
                other => panic!("end turn did not execute: {other:?}"),
            };
        }
        observe(&AwbwVisibility, &state, &state.turn.active_player)
            .expect("the active player observes the arena")
    }

    #[test]
    fn one_node_is_the_greedy_seed() {
        let view = view();
        let mut expected = TurnSearch::new(&view, 7, Weights::THREAT, EvalWeights::STANDARD, None)
            .expect("search opens");
        let seed = expected.greedy_seed().expect("greedy makes a turn");
        let seed = expected.plays(&seed).expect("the seed is legal");

        let mut actual = TurnSearch::new(&view, 7, Weights::THREAT, EvalWeights::STANDARD, None)
            .expect("search opens");
        assert_eq!(actual.improve(NodeBudget::ONE), Some(seed));
    }

    #[test]
    fn fixed_state_and_seed_make_the_same_plan() {
        let view = view();
        let plan = || {
            TurnSearch::new(&view, 11, Weights::THREAT, EvalWeights::STANDARD, None)
                .expect("search opens")
                .improve(NodeBudget::FOUR)
        };
        assert_eq!(plan(), plan());
    }

    #[test]
    fn evaluated_candidates_are_legal_and_rewind_exactly() {
        let view = view();
        let mut search = TurnSearch::new(&view, 13, Weights::THREAT, EvalWeights::STANDARD, None)
            .expect("search opens");
        let seed = search.greedy_seed().expect("greedy makes a turn");
        let original = search.session.state().clone();
        let mut evaluated = 0;
        let mut alternatives = 0;

        for coordinate in 0..seed.len() {
            let Some(unit) = seed[coordinate].unit() else {
                continue;
            };
            for alternative in search.alternatives(&seed, coordinate, unit) {
                alternatives += 1;
                if alternative == seed[coordinate] {
                    continue;
                }
                let mut candidate = seed.clone();
                candidate[coordinate] = alternative;
                if let Some(leaf) = search.evaluate(&candidate) {
                    assert!(leaf.score.is_finite(), "each leaf has a finite score");
                    assert_eq!(search.session.state(), &original);
                    evaluated += 1;
                    if evaluated == 4 {
                        return;
                    }
                }
            }
        }
        panic!("the fixture offered {alternatives} alternatives and {evaluated} legal candidates");
    }

    #[test]
    fn repaired_candidates_keep_the_prefix_and_rewind_exactly() {
        let view = view();
        let mut search = TurnSearch::new(&view, 17, Weights::THREAT, EvalWeights::STANDARD, None)
            .expect("search opens");
        let seed = search.greedy_seed().expect("greedy makes a turn");
        let original = search.session.state().clone();

        for coordinate in 0..seed.len() {
            let Some(unit) = seed[coordinate].unit() else {
                continue;
            };
            for alternative in search.alternatives(&seed, coordinate, unit) {
                if alternative == seed[coordinate] {
                    continue;
                }
                let Some(leaf) = search.evaluate_repaired(&seed[..coordinate], alternative) else {
                    continue;
                };
                assert_eq!(&leaf.plan[..coordinate], &seed[..coordinate]);
                assert_eq!(leaf.plan[coordinate], alternative);
                assert_eq!(search.session.state(), &original);
                return;
            }
        }
        panic!("the fixture offered no repairable alternative");
    }
}
