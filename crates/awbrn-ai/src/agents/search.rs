//! One-pass improvement of a complete greedy turn.
//!
//! The greedy turn is the seed. The search changes one unit or production
//! order at a time, repairs the rest of the friendly turn greedily, plays a
//! fresh greedy reply, and evaluates the resulting position. One complete
//! evaluated candidate is one node.

use awvm::random::Entropy;
use awvm::semantic::{
    AwbwVisibility, Match, Observation, PlayerId, PlayerIdx, State, UnitId, observe_into,
};
use awvm::session::{LegalScope, LegalVisitor, Mark, Order, OrderKind, Session, UnitIdx};
use awvm::transition::Command;

use crate::agent::{Agent, NodeBudget, Play, SearchCoordinateCoverage, SearchStats};
use crate::agents::{GreedyAgent, Weights, order_candidate_id};
use crate::eval::{EvalBreakdown, EvalWeights, Evaluator};
use crate::rng::Rng;

/// Allocation strategy for coordinate search.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SearchAllocator {
    /// Allocate a sequential quota to each remaining coordinate.
    #[default]
    SequentialQuota,
    /// Allocate one candidate to each coordinate before a second pass.
    RoundRobin,
}

/// A greedy agent with one coordinate-descent pass over its complete turn.
#[derive(Debug)]
pub struct SearchAgent {
    seed: u64,
    weights: Weights,
    search_eval_weights: EvalWeights,
    node_budget: Option<NodeBudget>,
    allocator: SearchAllocator,
    marginal_cap: Option<f64>,
    fallback: GreedyAgent,
    plan: Vec<Play>,
    next: usize,
    turn: Option<(PlayerId, u64)>,
    stats: SearchStats,
    timing: crate::agent::AgentTiming,
    decision_times_nanos: Vec<u64>,
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
    /// Complete candidates that reached the evaluator.
    pub evaluated_candidates: Vec<SearchCandidateEvaluation>,
    /// Search coverage for this decision.
    pub coverage: SearchDecisionCoverage,
}

/// One complete candidate that reached the search evaluator.
#[derive(Clone, Debug)]
pub struct SearchCandidateEvaluation {
    /// Complete candidate plan.
    pub plan: Vec<Order>,
    /// Candidate score.
    pub score: f64,
    /// Candidate score breakdown.
    pub breakdown: EvalBreakdown,
}

/// Coverage and visit order for one search decision.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct SearchDecisionCoverage {
    /// Allocator used by the search.
    pub allocator: SearchAllocator,
    /// Requested node budget.
    pub nodes_requested: u32,
    /// Nodes used by the search.
    pub nodes_used: u32,
    /// Searchable coordinates in seed-plan order.
    pub searchable_coordinates: Vec<usize>,
    /// Coordinates that received a visit.
    pub visited_coordinates: Vec<usize>,
    /// Coordinate visits grouped by pass.
    pub coordinate_visits_by_pass: Vec<Vec<usize>>,
    /// Alternative visits grouped by pass.
    pub alternative_visits_by_pass: Vec<Vec<SearchAlternativeVisit>>,
    /// True when the node limit stopped the search before its last coordinate.
    pub exhausted_before_final_coordinate: bool,
    /// Alternative counters for visited coordinates.
    pub coordinates: Vec<SearchCoordinateCoverage>,
}

/// One alternative attempted at a coordinate during a search pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct SearchAlternativeVisit {
    /// Coordinate that supplied the alternative.
    pub coordinate: usize,
    /// Zero-based position in the coordinate's filtered alternatives.
    pub alternative_index: usize,
    /// Stable identity of the alternative order.
    pub candidate_id: u64,
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
    audit_with_allocator(
        view,
        seed,
        weights,
        search_eval_weights,
        budget,
        SearchAllocator::SequentialQuota,
    )
}

/// Audit one search decision with an explicit allocator.
pub fn audit_with_allocator(
    view: &Observation,
    seed: u64,
    weights: Weights,
    search_eval_weights: EvalWeights,
    budget: NodeBudget,
    allocator: SearchAllocator,
) -> Option<SearchAudit> {
    let mut search =
        TurnSearch::new_with_allocator(view, seed, weights, search_eval_weights, None, allocator)?;
    search.capture_evaluated_candidates = true;
    let root = search.session.state().clone();
    let result = search.search_orders(budget)?;
    let evaluated_candidates = search
        .evaluated_candidates
        .clone()
        .into_iter()
        .filter_map(|candidate| {
            let state = search.line_state(&candidate.plan)?;
            let breakdown =
                TurnSearch::state_breakdown(&state, &search.friendly, search_eval_weights)?;
            Some(SearchCandidateEvaluation {
                plan: candidate.plan,
                score: candidate.score,
                breakdown,
            })
        })
        .collect();
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
        evaluated_candidates,
        coverage: search.last_decision_coverage.clone()?,
    })
}

impl SearchAgent {
    pub fn from_seed(seed: u64) -> Self {
        Self {
            seed,
            weights: Weights::DEFAULT,
            search_eval_weights: EvalWeights::STANDARD,
            node_budget: None,
            allocator: SearchAllocator::SequentialQuota,
            marginal_cap: None,
            fallback: GreedyAgent::with_weights(seed, Weights::DEFAULT),
            plan: Vec::new(),
            next: 0,
            turn: None,
            stats: SearchStats::default(),
            timing: crate::agent::AgentTiming::default(),
            decision_times_nanos: Vec::new(),
        }
    }

    /// Use the specified policy weights.
    pub fn with_weights(mut self, weights: Weights) -> Self {
        self.weights = weights;
        self.fallback = GreedyAgent::with_weights(self.seed, weights);
        self
    }

    /// Use the specified position evaluator weights.
    pub const fn with_evaluator_weights(mut self, weights: EvalWeights) -> Self {
        self.search_eval_weights = weights;
        self
    }

    /// Limit the marginal score that can select a changed plan.
    pub const fn with_marginal_cap(mut self, marginal_cap: f64) -> Self {
        self.marginal_cap = Some(marginal_cap);
        self
    }

    /// Use a fixed node budget instead of the caller's budget.
    pub const fn with_node_budget(mut self, node_budget: NodeBudget) -> Self {
        self.node_budget = Some(node_budget);
        self
    }

    /// Use a deterministic coordinate allocator.
    pub const fn with_allocator(mut self, allocator: SearchAllocator) -> Self {
        self.allocator = allocator;
        self
    }

    fn build_plan(&mut self, view: &Observation, budget: NodeBudget) -> Option<()> {
        let started = std::time::Instant::now();
        let mut search = TurnSearch::new_with_allocator(
            view,
            self.seed,
            self.weights,
            self.search_eval_weights,
            self.marginal_cap,
            self.allocator,
        )?;
        let plan = search.improve(budget);
        let elapsed = started.elapsed().as_nanos().try_into().unwrap_or(u64::MAX);
        self.timing.plan_construction_nanos =
            self.timing.plan_construction_nanos.saturating_add(elapsed);
        self.decision_times_nanos.push(elapsed);
        self.stats.add(search.stats);
        self.plan = plan?;
        self.next = 0;
        self.turn = Some((view.turn.active_player.clone(), view.turn.day));
        Some(())
    }
}

impl Agent for SearchAgent {
    fn act(&mut self, view: &Observation, budget: NodeBudget) -> Option<Play> {
        let budget = self.node_budget.unwrap_or(budget);
        let turn = (view.turn.active_player.clone(), view.turn.day);
        if self.turn.as_ref() == Some(&turn) && self.next >= self.plan.len() {
            return None;
        }
        if self.turn.as_ref() != Some(&turn) && self.build_plan(view, budget).is_none() {
            return self.fallback.act(view, budget);
        }
        let play = self.plan.get(self.next).copied()?;
        self.next += 1;
        Some(play)
    }

    fn search_stats(&self) -> Option<SearchStats> {
        Some(self.stats.clone())
    }

    fn search_decision_times_nanos(&self) -> Option<Vec<u64>> {
        Some(self.decision_times_nanos.clone())
    }

    fn timing(&self) -> Option<crate::agent::AgentTiming> {
        Some(self.timing)
    }

    fn start_match(&mut self) {
        self.timing = crate::agent::AgentTiming::default();
        self.stats = SearchStats::default();
        self.decision_times_nanos.clear();
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
    allocator: SearchAllocator,
    stats: SearchStats,
    last_decision_coverage: Option<SearchDecisionCoverage>,
    capture_evaluated_candidates: bool,
    evaluated_candidates: Vec<SearchLeaf>,
}

struct SearchResult {
    seed: Vec<Order>,
    selected: Vec<Order>,
    seed_score: f64,
    selected_score: f64,
}

#[derive(Clone)]
struct SearchLeaf {
    score: f64,
    breakdown: Option<EvalBreakdown>,
    plan: Vec<Order>,
}

struct CoordinateEvaluation {
    leaves: Vec<SearchLeaf>,
    rejected: u64,
    generated: u64,
    consumed: usize,
    attempted_alternatives: Vec<(usize, u64)>,
}

struct CoordinateSearchResult {
    consumed: usize,
    attempted_alternatives: Vec<(usize, u64)>,
    best_changed: bool,
}

impl TurnSearch {
    #[cfg(test)]
    fn new(
        view: &Observation,
        seed: u64,
        weights: Weights,
        search_eval_weights: EvalWeights,
        marginal_cap: Option<f64>,
    ) -> Option<Self> {
        Self::new_with_allocator(
            view,
            seed,
            weights,
            search_eval_weights,
            marginal_cap,
            SearchAllocator::SequentialQuota,
        )
    }

    fn new_with_allocator(
        view: &Observation,
        seed: u64,
        weights: Weights,
        search_eval_weights: EvalWeights,
        marginal_cap: Option<f64>,
        allocator: SearchAllocator,
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
            allocator,
            stats: SearchStats::default(),
            last_decision_coverage: None,
            capture_evaluated_candidates: false,
            evaluated_candidates: Vec::new(),
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
        if self.capture_evaluated_candidates {
            self.evaluated_candidates.push(seed_leaf.clone());
        }
        self.stats.nodes_evaluated += 1;
        let seed_breakdown = seed_leaf.breakdown;
        let seed_score = self.rank_score(seed_leaf.score, seed_breakdown, seed_breakdown)?;
        let mut best_value = seed_score;
        let mut best = seed.clone();
        let mut nodes = 1;
        let searchable_coordinates = best
            .iter()
            .enumerate()
            .filter_map(|(coordinate, order)| search_coordinate(*order).then_some(coordinate))
            .collect::<Vec<_>>();
        let final_quartile_start = final_quartile_start(searchable_coordinates.len());
        let mut decision_coverage = SearchDecisionCoverage {
            allocator: self.allocator,
            nodes_requested: budget.get(),
            nodes_used: 1,
            searchable_coordinates: searchable_coordinates.clone(),
            visited_coordinates: Vec::new(),
            coordinate_visits_by_pass: Vec::new(),
            alternative_visits_by_pass: Vec::new(),
            exhausted_before_final_coordinate: false,
            coordinates: Vec::new(),
        };
        self.record_search_start(&searchable_coordinates, final_quartile_start, budget);
        match self.allocator {
            SearchAllocator::SequentialQuota => self.search_front_loaded(
                budget,
                &mut nodes,
                &mut best,
                &mut best_value,
                seed_breakdown,
                &mut decision_coverage,
            ),
            SearchAllocator::RoundRobin => self.search_round_robin(
                budget,
                &mut nodes,
                &mut best,
                &mut best_value,
                seed_breakdown,
                &mut decision_coverage,
            ),
        }
        decision_coverage.nodes_used = nodes;
        decision_coverage.exhausted_before_final_coordinate = nodes >= budget.get()
            && searchable_coordinates
                .last()
                .is_some_and(|last| !decision_coverage.visited_coordinates.contains(last));
        self.record_search_end(&decision_coverage);
        self.last_decision_coverage = Some(decision_coverage);

        if best != seed {
            self.stats.changed_seed_plans += 1;
            self.stats.coverage.changed_seed_plans += 1;
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

    fn record_search_start(
        &mut self,
        searchable: &[usize],
        final_quartile_start: usize,
        budget: NodeBudget,
    ) {
        self.stats.coverage.decisions += 1;
        self.stats.coverage.seed_plans += 1;
        self.stats.coverage.searchable_coordinates += searchable.len() as u64;
        self.stats.coverage.final_quartile_searchable_coordinates +=
            searchable.len().saturating_sub(final_quartile_start) as u64;
        self.stats.coverage.nodes_requested += u64::from(budget.get());
    }

    fn record_search_end(&mut self, coverage: &SearchDecisionCoverage) {
        self.stats.coverage.nodes_used += u64::from(coverage.nodes_used);
        if coverage.exhausted_before_final_coordinate {
            self.stats
                .coverage
                .decisions_exhausted_before_final_coordinate += 1;
        }
        for (index, coordinate) in coverage.searchable_coordinates.iter().enumerate() {
            let visited = coverage.visited_coordinates.contains(coordinate);
            let is_final_quartile =
                index >= final_quartile_start(coverage.searchable_coordinates.len());
            {
                let entry = self.coordinate_coverage(*coordinate);
                entry.searchable_decisions += 1;
                if visited {
                    entry.visited_decisions += 1;
                }
            }
            if visited {
                self.stats.coverage.visited_searchable_coordinates += 1;
                if is_final_quartile {
                    self.stats.coverage.visited_final_quartile_coordinates += 1;
                }
            }
        }
        if let Some(first) = coverage.visited_coordinates.first().copied() {
            if self.stats.coverage.first_visited_coordinate.is_none() {
                self.stats.coverage.first_visited_coordinate = Some(first);
            }
            self.stats.coverage.last_visited_coordinate = Some(
                coverage
                    .visited_coordinates
                    .last()
                    .copied()
                    .unwrap_or(first),
            );
        }
        if self.stats.coverage.coordinate_visits_by_pass.len()
            < coverage.coordinate_visits_by_pass.len()
        {
            self.stats
                .coverage
                .coordinate_visits_by_pass
                .resize(coverage.coordinate_visits_by_pass.len(), 0);
        }
        for (total, visits) in self
            .stats
            .coverage
            .coordinate_visits_by_pass
            .iter_mut()
            .zip(&coverage.coordinate_visits_by_pass)
        {
            *total += visits.len() as u64;
        }
    }

    fn coordinate_coverage(&mut self, coordinate: usize) -> &mut SearchCoordinateCoverage {
        if let Some(index) = self
            .stats
            .coverage
            .coordinates
            .iter()
            .position(|entry| entry.coordinate == coordinate)
        {
            return &mut self.stats.coverage.coordinates[index];
        }
        self.stats
            .coverage
            .coordinates
            .push(SearchCoordinateCoverage {
                coordinate,
                ..SearchCoordinateCoverage::default()
            });
        let index = self.stats.coverage.coordinates.len() - 1;
        &mut self.stats.coverage.coordinates[index]
    }

    fn search_front_loaded(
        &mut self,
        budget: NodeBudget,
        nodes: &mut u32,
        best: &mut Vec<Order>,
        best_value: &mut f64,
        seed_breakdown: Option<EvalBreakdown>,
        coverage: &mut SearchDecisionCoverage,
    ) {
        let mut coordinate = 0;
        while coordinate < best.len() {
            let coordinate_plan = best.clone();
            if !search_coordinate(coordinate_plan[coordinate]) {
                coordinate += 1;
                continue;
            }
            let remaining = budget.get().saturating_sub(*nodes);
            let coordinates_left = u32::try_from(
                coordinate_plan[coordinate..]
                    .iter()
                    .filter(|order| search_coordinate(**order))
                    .count(),
            )
            .unwrap_or(u32::MAX)
            .max(1);
            let coordinate_limit = remaining.div_ceil(coordinates_left).max(1);
            if remaining > 0 {
                self.visit_coordinate(coverage, coordinate, 0);
            }
            self.evaluate_and_select(
                &coordinate_plan[..coordinate],
                coordinate_plan[coordinate],
                coordinate_plan[coordinate].unit(),
                coordinate_limit.min(remaining),
                0,
                nodes,
                best,
                best_value,
                seed_breakdown,
                coverage,
            );
            coordinate += 1;
        }
    }

    fn search_round_robin(
        &mut self,
        budget: NodeBudget,
        nodes: &mut u32,
        best: &mut Vec<Order>,
        best_value: &mut f64,
        seed_breakdown: Option<EvalBreakdown>,
        coverage: &mut SearchDecisionCoverage,
    ) {
        let mut cursors = Vec::new();
        let mut pass = 0;
        while *nodes < budget.get() {
            let mut visited = false;
            for coordinate in coverage.searchable_coordinates.clone() {
                if *nodes >= budget.get() || coordinate >= best.len() {
                    break;
                }
                let coordinate_plan = best.clone();
                if !search_coordinate(coordinate_plan[coordinate]) {
                    continue;
                }
                let cursor = cursors.get(coordinate).copied().unwrap_or_default();
                let result = self.evaluate_and_select(
                    &coordinate_plan[..coordinate],
                    coordinate_plan[coordinate],
                    coordinate_plan[coordinate].unit(),
                    1,
                    cursor,
                    nodes,
                    best,
                    best_value,
                    seed_breakdown,
                    coverage,
                );
                if result.consumed == 0 {
                    continue;
                }
                if result.best_changed {
                    for cursor in cursors.iter_mut().skip(coordinate.saturating_add(1)) {
                        *cursor = 0;
                    }
                }
                visited = true;
                if cursors.len() <= coordinate {
                    cursors.resize(coordinate + 1, 0);
                }
                cursors[coordinate] = cursor.saturating_add(result.consumed);
                self.visit_coordinate(coverage, coordinate, pass);
                if coverage.alternative_visits_by_pass.len() <= pass {
                    coverage
                        .alternative_visits_by_pass
                        .resize_with(pass + 1, Vec::new);
                }
                coverage.alternative_visits_by_pass[pass].extend(
                    result.attempted_alternatives.into_iter().map(
                        |(alternative_index, candidate_id)| SearchAlternativeVisit {
                            coordinate,
                            alternative_index,
                            candidate_id,
                        },
                    ),
                );
            }
            if !visited {
                break;
            }
            pass += 1;
        }
    }

    fn visit_coordinate(
        &self,
        coverage: &mut SearchDecisionCoverage,
        coordinate: usize,
        pass: usize,
    ) {
        coverage.visited_coordinates.push(coordinate);
        if coverage.coordinate_visits_by_pass.len() <= pass {
            coverage
                .coordinate_visits_by_pass
                .resize_with(pass + 1, Vec::new);
        }
        coverage.coordinate_visits_by_pass[pass].push(coordinate);
    }

    #[allow(clippy::too_many_arguments)]
    fn evaluate_and_select(
        &mut self,
        prefix: &[Order],
        current: Order,
        unit: Option<UnitIdx>,
        limit: u32,
        cursor: usize,
        nodes: &mut u32,
        best: &mut Vec<Order>,
        best_value: &mut f64,
        seed_breakdown: Option<EvalBreakdown>,
        decision_coverage: &mut SearchDecisionCoverage,
    ) -> CoordinateSearchResult {
        let result = self.evaluate_coordinate(prefix, current, unit, cursor, limit);
        self.stats.legal_candidates_rejected += result.rejected;
        let coordinate = prefix.len();
        let aggregate = self.coordinate_coverage(coordinate);
        if cursor == 0 {
            aggregate.alternatives_generated += result.generated;
        }
        aggregate.alternatives_rejected += result.rejected;
        aggregate.alternatives_evaluated += result.leaves.len() as u64;
        if let Some(existing) = decision_coverage
            .coordinates
            .iter_mut()
            .find(|entry| entry.coordinate == coordinate)
        {
            if cursor == 0 {
                existing.alternatives_generated += result.generated;
            }
            existing.alternatives_rejected += result.rejected;
            existing.alternatives_evaluated += result.leaves.len() as u64;
        } else {
            decision_coverage
                .coordinates
                .push(SearchCoordinateCoverage {
                    coordinate,
                    alternatives_generated: if cursor == 0 { result.generated } else { 0 },
                    alternatives_rejected: result.rejected,
                    alternatives_evaluated: result.leaves.len() as u64,
                    ..SearchCoordinateCoverage::default()
                });
        }
        let mut best_changed = false;
        for candidate_leaf in result.leaves {
            if self.capture_evaluated_candidates {
                self.evaluated_candidates.push(candidate_leaf.clone());
            }
            self.stats.nodes_evaluated += 1;
            *nodes += 1;
            let Some(value) = self.rank_score(
                candidate_leaf.score,
                candidate_leaf.breakdown,
                seed_breakdown,
            ) else {
                self.stats.legal_candidates_rejected += 1;
                continue;
            };
            if value > *best_value {
                *best_value = value;
                *best = candidate_leaf.plan;
                best_changed = true;
            }
        }
        CoordinateSearchResult {
            consumed: result.consumed,
            attempted_alternatives: result.attempted_alternatives,
            best_changed,
        }
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

    #[cfg(test)]
    fn alternatives(
        &mut self,
        seed: &[Order],
        coordinate: usize,
        unit: Option<UnitIdx>,
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
        let mut alternatives = self.coordinate_orders(unit);
        alternatives.retain(|order| {
            !matches!(order.kind(), OrderKind::Delete | OrderKind::Resign)
                && (unit.is_none() || order.kind() != OrderKind::EndTurn)
        });
        if let Some(mark) = root {
            self.session.rewind(mark);
        }
        alternatives
    }

    /// Evaluate one coordinate while retaining its applied prefix.
    fn evaluate_coordinate(
        &mut self,
        prefix: &[Order],
        current: Order,
        unit: Option<UnitIdx>,
        cursor: usize,
        limit: u32,
    ) -> CoordinateEvaluation {
        if limit == 0 {
            return CoordinateEvaluation {
                leaves: Vec::new(),
                rejected: 0,
                generated: 0,
                consumed: 0,
                attempted_alternatives: Vec::new(),
            };
        }
        let original = self.session.state().clone();
        let mut entropy = Rng::from_seed(self.entropy_seed);
        let mut root = None;
        for order in prefix.iter().copied() {
            let Ok(mark) = self.session.apply(order, &mut entropy, &mut ()) else {
                if let Some(mark) = root {
                    self.session.rewind(mark);
                }
                return CoordinateEvaluation {
                    leaves: Vec::new(),
                    rejected: 1,
                    generated: 0,
                    consumed: 0,
                    attempted_alternatives: Vec::new(),
                };
            };
            root.get_or_insert(mark);
        }
        let prefix_entropy = entropy.clone();
        let mut alternatives = self.coordinate_orders(unit);
        let mut rejected = 0;
        alternatives.retain(|order| {
            *order != current
                && !matches!(order.kind(), OrderKind::Delete | OrderKind::Resign)
                && (unit.is_none() || order.kind() != OrderKind::EndTurn)
        });
        let generated = alternatives.len() as u64;

        let mut leaves = Vec::new();
        let mut consumed = 0;
        let mut attempted_alternatives = Vec::new();
        for (alternative_index, alternative) in alternatives.into_iter().enumerate().skip(cursor) {
            if u32::try_from(consumed).unwrap_or(u32::MAX) >= limit {
                break;
            }
            consumed += 1;
            attempted_alternatives.push((alternative_index, order_candidate_id(alternative)));
            entropy = prefix_entropy.clone();
            let Ok(branch) = self.session.apply(alternative, &mut entropy, &mut ()) else {
                rejected += 1;
                continue;
            };
            let mut branch_root = Some(branch);
            let mut agent = GreedyAgent::with_weights(self.friendly_seed, self.weights);
            let leaf = self
                .greedy_turn(&mut agent, &mut entropy, &mut branch_root)
                .and_then(|suffix| {
                    let mut plan = prefix.to_vec();
                    plan.push(alternative);
                    plan.extend(suffix);
                    self.greedy_reply(&mut entropy).and_then(|reply| {
                        let _ = reply;
                        self.leaf(plan)
                    })
                });
            self.session.rewind(branch);
            match leaf {
                Some(leaf) => leaves.push(leaf),
                None => rejected += 1,
            }
        }
        if let Some(mark) = root {
            self.session.rewind(mark);
        }
        debug_assert_eq!(self.session.state(), &original);
        CoordinateEvaluation {
            leaves,
            rejected,
            generated,
            consumed,
            attempted_alternatives,
        }
    }

    /// Orders that can replace one coordinate of a turn plan.
    ///
    /// A unit coordinate keeps ownership with that unit. A unitless
    /// coordinate can change a production order or end the turn. The explicit
    /// end-turn alternative is the price of saving funds for a later turn.
    fn coordinate_orders(&self, unit: Option<UnitIdx>) -> Vec<Order> {
        if let Some(unit) = unit {
            let mut alternatives = Vec::new();
            self.session.legal().unit_orders(unit, &mut alternatives);
            return alternatives;
        }

        struct UnitlessCollector<'a>(&'a mut Vec<Order>);

        impl LegalVisitor for UnitlessCollector<'_> {
            fn order(&mut self, order: Order) {
                if matches!(order.kind(), OrderKind::Produce(_) | OrderKind::EndTurn) {
                    self.0.push(order);
                }
            }
        }

        let mut alternatives = Vec::new();
        self.session.legal().visit_scoped(
            LegalScope {
                units: &[],
                unitless: true,
            },
            &mut UnitlessCollector(&mut alternatives),
        );
        if !alternatives
            .iter()
            .any(|order| matches!(order.kind(), OrderKind::EndTurn))
        {
            alternatives.push(Order::unitless(
                awvm::semantic::CellIdx::from_raw(0),
                OrderKind::EndTurn,
            ));
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
            let _ = reply;
            self.leaf(plan.to_vec())
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

fn search_coordinate(order: Order) -> bool {
    order.unit().is_some() || matches!(order.kind(), OrderKind::Produce(_))
}

fn final_quartile_start(coordinate_count: usize) -> usize {
    coordinate_count.saturating_sub(coordinate_count.div_ceil(4))
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
    fn round_robin_visits_each_available_coordinate_once_before_a_second_pass() {
        let view = view();
        let audit = audit_with_allocator(
            &view,
            23,
            Weights::THREAT,
            EvalWeights::STANDARD,
            NodeBudget::SIXTEEN,
            SearchAllocator::RoundRobin,
        )
        .expect("the search audit is available");
        assert_eq!(audit.coverage.allocator, SearchAllocator::RoundRobin);
        assert!(audit.coverage.nodes_used <= NodeBudget::SIXTEEN.get());
        if let Some(first_pass) = audit.coverage.coordinate_visits_by_pass.first() {
            let mut unique = first_pass.clone();
            unique.sort_unstable();
            unique.dedup();
            assert_eq!(unique, *first_pass);
            assert_eq!(unique, audit.coverage.searchable_coordinates);
        }
        let serialized = serde_json::to_vec(&audit.coverage).expect("coverage serializes");
        let deserialized: SearchDecisionCoverage =
            serde_json::from_slice(&serialized).expect("coverage deserializes");
        assert_eq!(deserialized, audit.coverage);
    }

    #[test]
    fn round_robin_evaluates_distinct_alternatives_on_later_passes() {
        let view = view();
        let audit = audit_with_allocator(
            &view,
            23,
            Weights::THREAT,
            EvalWeights::STANDARD,
            NodeBudget::new(64).expect("the node budget is valid"),
            SearchAllocator::RoundRobin,
        )
        .expect("the search audit is available");
        let mut candidates = std::collections::BTreeSet::new();
        let mut later_pass = false;
        for (pass, visits) in audit.coverage.alternative_visits_by_pass.iter().enumerate() {
            later_pass |= pass > 0 && !visits.is_empty();
            for visit in visits {
                assert!(
                    candidates.insert((visit.coordinate, visit.candidate_id)),
                    "round-robin repeated candidate {visit:?}"
                );
            }
        }
        assert!(
            later_pass,
            "the fixture must reach a later round-robin pass"
        );
    }

    #[test]
    fn search_stats_report_the_requested_and_used_nodes() {
        let view = view();
        let mut agent = SearchAgent::from_seed(31)
            .with_allocator(SearchAllocator::RoundRobin)
            .with_node_budget(NodeBudget::FOUR);
        let _ = agent.act(&view, NodeBudget::SIXTEEN);
        let stats = agent.search_stats().expect("search stats are available");
        assert_eq!(stats.coverage.nodes_requested, 4);
        assert!(stats.coverage.nodes_used <= 4);
        assert_eq!(stats.coverage.decisions, 1);
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
            for alternative in search.alternatives(&seed, coordinate, seed[coordinate].unit()) {
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
            let result = search.evaluate_coordinate(
                &seed[..coordinate],
                seed[coordinate],
                seed[coordinate].unit(),
                0,
                4,
            );
            // Every branch of a coordinate rewinds to the position the search
            // came in at, whether or not the coordinate offered a branch.
            assert_eq!(search.session.state(), &original);
            let Some(leaf) = result.leaves.first() else {
                continue;
            };
            assert_eq!(&leaf.plan[..coordinate], &seed[..coordinate]);
            assert_ne!(leaf.plan[coordinate], seed[coordinate]);
            assert!(leaf.score.is_finite(), "each leaf has a finite score");
            return;
        }
        panic!("the fixture offered no repairable alternative");
    }

    #[test]
    fn production_coordinates_can_build_another_kind_or_save() {
        let view = view();
        let mut search = TurnSearch::new(
            &view,
            19,
            Weights::CAPTURER_SHORTFALL_50,
            EvalWeights::STANDARD,
            None,
        )
        .expect("search opens");
        let seed = search.greedy_seed().expect("greedy makes a turn");
        let coordinate = seed
            .iter()
            .position(|order| matches!(order.kind(), OrderKind::Produce(_)))
            .expect("the seed builds a unit");
        let alternatives = search.alternatives(&seed, coordinate, None);

        assert!(
            alternatives
                .iter()
                .any(|order| matches!(order.kind(), OrderKind::EndTurn)),
            "production must be compared with saving the funds: {alternatives:?}"
        );
        assert!(
            alternatives.iter().any(|order| {
                matches!(order.kind(), OrderKind::Produce(_)) && *order != seed[coordinate]
            }),
            "production must be compared with another legal build"
        );
    }

    #[test]
    fn fog_uses_the_promoted_greedy_policy() {
        let state = arena(true, 1);
        let view = observe(&AwbwVisibility, &state, &state.turn.active_player)
            .expect("the active player observes the arena");
        let mut search = SearchAgent::from_seed(29).with_weights(Weights::CAPTURER_SHORTFALL_50);
        let mut greedy = GreedyAgent::with_weights(29, Weights::CAPTURER_SHORTFALL_50);

        assert_eq!(
            search.act(&view, NodeBudget::SIXTEEN),
            greedy.act(&view, NodeBudget::SIXTEEN)
        );
    }
}
