//! What an agent is asked, and how its answer becomes a command.

use awvm::semantic::{CellIdx, Observation, ObservedEvent, UnitId};
use awvm::session::{Order, OrderKind, Session};
use awvm::transition::Command;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::eval::EvalBreakdown;
use crate::mission::{DecisionTrace, TraceError, TurnEndReason};

/// One decision, from a position the agent can see.
///
/// The interface steps. It gives one play, takes a fresh observation, and gives
/// the next one. A batch interface cannot work here: moving a recon reveals
/// enemy units, and a plan made before that move is a plan about a board that
/// no longer exists. This shape is baked into every agent written against it,
/// so it is the expensive thing to get wrong.
pub trait Agent {
    /// The next play, or `None` to end the turn.
    ///
    /// `view` is what this player knows. It is the only board the agent gets.
    fn act(&mut self, view: &Observation, budget: NodeBudget) -> Option<Play>;

    /// What the agent saw since the last call.
    ///
    /// An agent that keeps a belief about hidden units updates it here rather
    /// than deriving it again from each observation. An agent that keeps
    /// nothing ignores this.
    fn observe(&mut self, _events: &[ObservedEvent]) {}

    /// Reset state at the start of a match.
    fn start_match(&mut self) {}

    /// Start or reconcile the active turn.
    fn start_turn(&mut self, _view: &Observation) {}

    /// Refresh state after an accepted command.
    fn refresh(&mut self, _view: &Observation) {}

    /// Classify the selected command.
    fn classify_command(&mut self, _view: &Observation, _command: &Command) {}

    /// Finalize the current decision trace.
    fn finalize_trace(&mut self, _reason: TurnEndReason) -> Result<(), TraceError> {
        Ok(())
    }

    /// Return the finalized decision trace, if any.
    fn trace(&self) -> Option<&DecisionTrace> {
        None
    }

    /// Clear turn-local state.
    fn clear_trace(&mut self) {}

    /// Return lifecycle timing counters.
    fn timing(&self) -> Option<AgentTiming> {
        None
    }

    /// Return search counters when this agent has them.
    fn search_stats(&self) -> Option<SearchStats> {
        None
    }

    /// Return wall-clock decision samples when the agent records them.
    fn search_decision_times_nanos(&self) -> Option<Vec<u64>> {
        None
    }
}

/// Timing counters for lifecycle work.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AgentTiming {
    /// Time spent constructing a turn plan.
    pub plan_construction_nanos: u64,
    /// Time spent refreshing a turn plan.
    pub plan_refresh_nanos: u64,
    /// Time spent selecting a baseline command.
    pub baseline_selection_nanos: u64,
    /// Time spent recording trace data.
    pub trace_recording_nanos: u64,
}

impl AgentTiming {
    /// Return the counters added since `before`.
    pub const fn since(self, before: Self) -> Self {
        Self {
            plan_construction_nanos: self
                .plan_construction_nanos
                .saturating_sub(before.plan_construction_nanos),
            plan_refresh_nanos: self
                .plan_refresh_nanos
                .saturating_sub(before.plan_refresh_nanos),
            baseline_selection_nanos: self
                .baseline_selection_nanos
                .saturating_sub(before.baseline_selection_nanos),
            trace_recording_nanos: self
                .trace_recording_nanos
                .saturating_sub(before.trace_recording_nanos),
        }
    }
}

/// Counters from the one-pass search.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct SearchStats {
    /// Complete candidate plans that reached the evaluator.
    pub nodes_evaluated: u64,
    /// Legal alternatives that did not reach the evaluator.
    pub legal_candidates_rejected: u64,
    /// Greedy seed plans that the search built.
    pub seed_plans: u64,
    /// Seed plans that the search changed.
    pub changed_seed_plans: u64,
    /// Changed plans with two non-terminal leaves that were broken down.
    pub changed_leaf_breakdowns: u64,
    /// Sum of selected-minus-seed evaluator terms for changed plans.
    pub changed_leaf_deltas: EvalBreakdown,
    /// Sum of the same changes under the standard evaluator.
    pub standard_leaf_deltas: EvalBreakdown,
    /// Distribution of standard front deltas.
    pub standard_front_deltas: MarginalDistribution,
    /// Distribution of standard exposure deltas.
    pub standard_exposure_deltas: MarginalDistribution,
    /// Search coverage measurements.
    pub coverage: SearchCoverage,
}

impl SearchStats {
    /// Add counters from one search run.
    pub fn add(&mut self, other: Self) {
        self.nodes_evaluated += other.nodes_evaluated;
        self.legal_candidates_rejected += other.legal_candidates_rejected;
        self.seed_plans += other.seed_plans;
        self.changed_seed_plans += other.changed_seed_plans;
        self.changed_leaf_breakdowns += other.changed_leaf_breakdowns;
        self.changed_leaf_deltas.score += other.changed_leaf_deltas.score;
        self.changed_leaf_deltas.army += other.changed_leaf_deltas.army;
        self.changed_leaf_deltas.income += other.changed_leaf_deltas.income;
        self.changed_leaf_deltas.exposure += other.changed_leaf_deltas.exposure;
        self.changed_leaf_deltas.contest += other.changed_leaf_deltas.contest;
        self.changed_leaf_deltas.front += other.changed_leaf_deltas.front;
        self.changed_leaf_deltas.other += other.changed_leaf_deltas.other;
        self.standard_leaf_deltas.score += other.standard_leaf_deltas.score;
        self.standard_leaf_deltas.army += other.standard_leaf_deltas.army;
        self.standard_leaf_deltas.income += other.standard_leaf_deltas.income;
        self.standard_leaf_deltas.exposure += other.standard_leaf_deltas.exposure;
        self.standard_leaf_deltas.contest += other.standard_leaf_deltas.contest;
        self.standard_leaf_deltas.front += other.standard_leaf_deltas.front;
        self.standard_leaf_deltas.other += other.standard_leaf_deltas.other;
        self.standard_front_deltas.add(other.standard_front_deltas);
        self.standard_exposure_deltas
            .add(other.standard_exposure_deltas);
        self.coverage.add(other.coverage);
    }
}

/// Coverage for one searchable coordinate, aggregated over decisions.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct SearchCoordinateCoverage {
    /// The order position in the complete turn plan.
    pub coordinate: usize,
    /// Decisions that had this searchable coordinate.
    pub searchable_decisions: u64,
    /// Decisions that visited this coordinate.
    pub visited_decisions: u64,
    /// Legal alternatives enumerated for this coordinate.
    pub alternatives_generated: u64,
    /// Alternatives that did not reach the evaluator.
    pub alternatives_rejected: u64,
    /// Complete alternatives that reached the evaluator.
    pub alternatives_evaluated: u64,
}

/// Deterministic coverage measurements for the one-pass search.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct SearchCoverage {
    /// Search decisions with a complete greedy seed.
    pub decisions: u64,
    /// Searchable coordinate occurrences in those decisions.
    pub searchable_coordinates: u64,
    /// Searchable coordinate occurrences that received a visit.
    pub visited_searchable_coordinates: u64,
    /// Searchable coordinate occurrences in the final quartile.
    pub final_quartile_searchable_coordinates: u64,
    /// Final-quartile coordinate occurrences that received a visit.
    pub visited_final_quartile_coordinates: u64,
    /// Decisions that used all nodes before their final searchable coordinate.
    pub decisions_exhausted_before_final_coordinate: u64,
    /// Seed plans made by the search.
    pub seed_plans: u64,
    /// Seed plans changed by the search.
    pub changed_seed_plans: u64,
    /// Nodes requested by the search.
    pub nodes_requested: u64,
    /// Nodes used by the search.
    pub nodes_used: u64,
    /// First visited coordinate in this aggregate.
    pub first_visited_coordinate: Option<usize>,
    /// Last visited coordinate in this aggregate.
    pub last_visited_coordinate: Option<usize>,
    /// Coordinate visits grouped by round-robin pass.
    pub coordinate_visits_by_pass: Vec<u64>,
    /// Per-coordinate counters.
    pub coordinates: Vec<SearchCoordinateCoverage>,
}

impl SearchCoverage {
    /// Add one search coverage aggregate.
    pub fn add(&mut self, other: Self) {
        self.decisions += other.decisions;
        self.searchable_coordinates += other.searchable_coordinates;
        self.visited_searchable_coordinates += other.visited_searchable_coordinates;
        self.final_quartile_searchable_coordinates += other.final_quartile_searchable_coordinates;
        self.visited_final_quartile_coordinates += other.visited_final_quartile_coordinates;
        self.decisions_exhausted_before_final_coordinate +=
            other.decisions_exhausted_before_final_coordinate;
        self.seed_plans += other.seed_plans;
        self.changed_seed_plans += other.changed_seed_plans;
        self.nodes_requested += other.nodes_requested;
        self.nodes_used += other.nodes_used;
        if self.first_visited_coordinate.is_none() {
            self.first_visited_coordinate = other.first_visited_coordinate;
        }
        self.last_visited_coordinate = other
            .last_visited_coordinate
            .or(self.last_visited_coordinate);
        if self.coordinate_visits_by_pass.len() < other.coordinate_visits_by_pass.len() {
            self.coordinate_visits_by_pass
                .resize(other.coordinate_visits_by_pass.len(), 0);
        }
        for (left, right) in self
            .coordinate_visits_by_pass
            .iter_mut()
            .zip(other.coordinate_visits_by_pass)
        {
            *left += right;
        }
        for coordinate in other.coordinates {
            let Some(existing) = self
                .coordinates
                .iter_mut()
                .find(|existing| existing.coordinate == coordinate.coordinate)
            else {
                self.coordinates.push(coordinate);
                continue;
            };
            existing.searchable_decisions += coordinate.searchable_decisions;
            existing.visited_decisions += coordinate.visited_decisions;
            existing.alternatives_generated += coordinate.alternatives_generated;
            existing.alternatives_rejected += coordinate.alternatives_rejected;
            existing.alternatives_evaluated += coordinate.alternatives_evaluated;
        }
        self.coordinates
            .sort_by_key(|coordinate| coordinate.coordinate);
    }
}

/// A compact distribution of one marginal evaluator term, in funds.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct MarginalDistribution {
    /// Number of recorded changes.
    pub count: u64,
    /// Sum of the changes.
    pub sum: f64,
    /// Sum of squared changes.
    pub sum_squared: f64,
    /// Smallest change.
    pub min: f64,
    /// Largest change.
    pub max: f64,
    /// Counts in fixed funds ranges from below -50k to at least 20k.
    pub buckets: [u64; 11],
}

impl MarginalDistribution {
    const EDGES: [f64; 10] = [
        -50_000.0, -20_000.0, -10_000.0, -5_000.0, -2_000.0, 0.0, 2_000.0, 5_000.0, 10_000.0,
        20_000.0,
    ];

    /// Make an empty distribution.
    pub const fn new() -> Self {
        Self {
            count: 0,
            sum: 0.0,
            sum_squared: 0.0,
            min: 0.0,
            max: 0.0,
            buckets: [0; 11],
        }
    }

    /// Record one marginal change.
    pub fn record(&mut self, value: f64) {
        let bucket = Self::EDGES
            .iter()
            .position(|edge| value < *edge)
            .unwrap_or(Self::EDGES.len());
        self.count += 1;
        self.sum += value;
        self.sum_squared += value * value;
        if self.count == 1 {
            self.min = value;
            self.max = value;
        } else {
            self.min = self.min.min(value);
            self.max = self.max.max(value);
        }
        self.buckets[bucket] += 1;
    }

    /// Add another distribution.
    pub fn add(&mut self, other: Self) {
        if other.count == 0 {
            return;
        }
        if self.count == 0 {
            self.min = other.min;
            self.max = other.max;
        } else {
            self.min = self.min.min(other.min);
            self.max = self.max.max(other.max);
        }
        self.count += other.count;
        self.sum += other.sum;
        self.sum_squared += other.sum_squared;
        for (left, right) in self.buckets.iter_mut().zip(other.buckets) {
            *left += right;
        }
    }
}

/// The maximum number of candidate turn plans an agent may evaluate.
///
/// One node is one evaluated leaf. Applying the individual orders in a turn
/// plan does not spend more nodes. This definition makes a search repeatable
/// across machines: the same position and budget examine the same number of
/// candidates, independent of clock speed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct NodeBudget(u32);

impl NodeBudget {
    pub const ONE: Self = Self(1);
    pub const FOUR: Self = Self(4);
    pub const EIGHT: Self = Self(8);
    pub const SIXTEEN: Self = Self(16);

    /// Make a nonzero node budget.
    pub const fn new(nodes: u32) -> Option<Self> {
        if nodes == 0 { None } else { Some(Self(nodes)) }
    }

    /// The number of candidate turn plans the agent may evaluate.
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl Default for NodeBudget {
    fn default() -> Self {
        Self::FOUR
    }
}

impl<'de> Deserialize<'de> for NodeBudget {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let nodes = u32::deserialize(deserializer)?;
        Self::new(nodes).ok_or_else(|| D::Error::custom("node budget must be nonzero"))
    }
}

#[cfg(test)]
mod tests {
    use super::NodeBudget;

    #[test]
    fn node_budget_is_nonzero() {
        assert_eq!(NodeBudget::new(0), None);
        assert_eq!(NodeBudget::new(16), Some(NodeBudget::SIXTEEN));
    }

    #[test]
    fn node_budget_deserialization_rejects_zero() {
        serde_json::from_str::<NodeBudget>("0").expect_err("zero budget must fail");
        assert_eq!(
            serde_json::from_str::<NodeBudget>("16").expect("nonzero budget"),
            NodeBudget::SIXTEEN
        );
    }
}

/// One play, named the way a player can name it.
///
/// This is an [`Order`] that names its unit by id instead of by roster index,
/// because the agent and the authority do not agree on indices: a fogged
/// projection drops the units the player cannot see, so a seat in the
/// projection is not the same seat in the true state.
///
/// It is also the reason an agent does not return a [`Command`] directly. A
/// command names an attack target by unit id, and the agent has no true id for
/// an enemy — [`awvm::query::reify`] invents one to fill the projection. A play
/// names the target tile instead, and [`Play::command`] resolves the tile
/// against the true state. This is the same split the client and the server
/// already run on, and it is what stops an agent cheating through fog by
/// accident.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Play {
    unit: Option<UnitId>,
    cargo: Option<UnitId>,
    dest: CellIdx,
    kind: OrderKind,
}

impl Play {
    /// A play by one owned unit.
    pub const fn new(unit: UnitId, dest: CellIdx, kind: OrderKind) -> Self {
        Self {
            unit: Some(unit),
            cargo: None,
            dest,
            kind,
        }
    }

    /// A play no unit performs: production, a power, the turn boundary.
    pub const fn unitless(dest: CellIdx, kind: OrderKind) -> Self {
        Self {
            unit: None,
            cargo: None,
            dest,
            kind,
        }
    }

    /// The play an order names, read in the session that offered it.
    ///
    /// `None` when the order names a unit the session does not hold, or an
    /// unload whose slot is empty. Both mean the order came from another
    /// position.
    pub fn from_order(session: &Session, order: Order) -> Option<Self> {
        let unit = match order.unit() {
            Some(_) => Some(session.unit_of(order)?),
            None => None,
        };
        let cargo = match order.kind() {
            OrderKind::Unload(_) => Some(session.cargo_of(order)?),
            _ => None,
        };
        Some(Self {
            unit,
            cargo,
            dest: order.destination(),
            kind: order.kind(),
        })
    }

    /// The acting unit, or `None` for a play that moves nothing.
    pub const fn unit(&self) -> Option<UnitId> {
        self.unit
    }

    /// The cargo an unload puts down.
    pub const fn cargo(&self) -> Option<UnitId> {
        self.cargo
    }

    /// Where the play takes effect: the arrival tile, or the production site.
    pub const fn destination(&self) -> CellIdx {
        self.dest
    }

    pub const fn kind(&self) -> OrderKind {
        self.kind
    }

    /// The command this play is, in the position the authority holds.
    ///
    /// `authority` is a session on the true state, so the route and the attack
    /// target come from the board the reducer will validate against. A play
    /// built from a projection can still be refused here — a hidden unit can
    /// block the route the agent counted on — and a refusal is the answer, not
    /// a fault.
    ///
    /// `None` when the true state holds no such unit or no such route.
    pub fn command(&self, authority: &Session) -> Option<Command> {
        // Unload names two friendly units, and the agent knows the real id of
        // both. Naming them is safer than sending the slot the projection
        // reported, because a slot is a position in a transport and this play
        // may arrive several commands after it was chosen.
        if matches!(self.kind, OrderKind::Unload(_)) {
            let state = authority.state();
            return Some(Command::Unload {
                player: state.turn.active_player.clone(),
                transport: self.unit?,
                cargo: self.cargo?,
                destination: state.board.dimensions().position_of(self.dest)?,
            });
        }

        let order = match self.unit {
            Some(unit) => Order::new(authority.index_of(unit)?, self.dest, self.kind),
            None => Order::unitless(self.dest, self.kind),
        };
        authority.spell(order)
    }
}
