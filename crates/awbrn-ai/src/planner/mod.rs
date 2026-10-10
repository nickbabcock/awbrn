//! A turn planner that chooses whole own turns, not single orders.
//!
//! The greedy policy (`ai-hard-v2`) scores one legal order, plays it, and
//! scores again. It does not compare complete turns. The planner does:
//!
//! 1. It plays the Hard turn in a simulation. This is the seed plan, and the
//!    planner can always fall back to it.
//! 2. Generators propose other complete turns. Each one fixes some orders
//!    first, and the Hard policy then plays the rest of the turn.
//!    - Kill plans come from the combat solver ([`combat`]).
//!    - Safety plans keep a unit back when the seed plan leaves it where
//!      focused enemy fire can destroy it.
//!    - Block plans put a unit on one of our properties that an enemy
//!      capturer can reach.
//!    - Reroute plans give a different order to a unit that the seed plan
//!      leaves exposed to enemy fire. The planner plays each legal order of
//!      that unit alone, ends the turn, and scores the result with the reply
//!      estimate. The best orders become plans.
//!    - Power plans use a legal commander power before all other orders.
//! 3. Each complete turn gets a score: the position value from
//!    [`crate::eval`] at the start of the enemy turn, less a fast estimate of
//!    the enemy reply ([`reply`]). The best plans are then played against a
//!    simulated Hard reply. A plan can enter this check with a score below the
//!    seed plan when it is inside the reply window, because the fast estimate
//!    does not see how the enemy reply changes the board.
//! 4. The agent plays the best turn one order at a time. Before each order it
//!    compares the observed position with the predicted one. A combat roll or
//!    a fog reveal changes the position, and the agent then plans again from
//!    the new position.
//!
//! Simulation uses the middle of each luck range, so a plan is deterministic.

mod combat;
mod reply;

use awvm::commander::Domain;
use awvm::random::{Entropy, Luck, RandomError};
use awvm::ruleset::WeatherKind;
use awvm::semantic::{CellIdx, Location, Match, Observation, PlayerIdx, State, UnitId};
use awvm::session::{Order, OrderKind, Session};
use awvm::transition::Command;

use crate::agent::{Agent, NodeBudget, Play};
use crate::agents::GreedyAgent;
use crate::baseline::BaselineConfig;
use crate::eval::{EvalWeights, Evaluator};
use crate::rng::Rng;

pub use reply::{PropertyValues, ReplyEstimate};

use crate::replay_score::{ReplayReader, ReplayScore};

/// Everything that decides how the planner plays.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct PlannerConfig {
    /// A stable name for match records.
    pub identifier: &'static str,
    /// The greedy policy that makes the seed plan and fills each turn.
    pub baseline: BaselineConfig,
    /// The position value weights.
    pub eval_weights: EvalWeights,
    /// The values of the reply estimate.
    #[serde(skip)]
    pub property_values: PropertyValues,
    /// How much of the reply estimate the score subtracts.
    pub reply_weight: f64,
    /// The largest number of kill plans for one decision.
    pub kill_plans: usize,
    /// The largest number of safety plans for one decision.
    pub safety_plans: usize,
    /// The largest number of block plans for one decision.
    pub block_plans: usize,
    /// The largest number of plans for one turn, counting each replan.
    pub plans_per_turn: u32,
    /// The threshold for simulated greedy decisions in one own turn.
    ///
    /// Each greedy decision in a candidate turn or a simulated reply is
    /// one decision. When the turn has used this number, the planner starts
    /// no new candidate, and the agent plays Hard for the rest of the turn.
    /// A simulation that has started finishes and can exceed the threshold.
    /// Each combat search phase has a separate bound of 20,000 nodes.
    /// The threshold counts work and not time, so a turn is the same on every
    /// host. A Worker does not advance its clock during computation, so a
    /// wall-clock limit cannot stop a turn there.
    pub turn_work: Option<u64>,
    /// The score margin an alternative must have over the seed plan.
    pub seed_margin: f64,
    /// The number of best alternatives that a simulated Hard reply checks
    /// against the seed plan. Zero turns the check off.
    pub hard_reply_top: usize,
    /// The largest number of exposed units that get reroute plans in one
    /// decision.
    #[serde(skip_serializing_if = "is_zero")]
    pub reroute_units: usize,
    /// The largest number of reroute plans for one exposed unit.
    #[serde(skip_serializing_if = "is_zero")]
    pub reroute_orders: usize,
    /// How far below the seed score an alternative can be and still enter the
    /// simulated Hard reply check, in funds.
    ///
    /// Only the reply check can choose such an alternative. Without the
    /// check, an alternative must still be better than the seed plan.
    #[serde(skip_serializing_if = "is_zero_funds")]
    pub reply_window: f64,
    /// Whether the planner adds plans that use a commander power first.
    ///
    /// The Hard policy ranks a power below captures, so it can use a power
    /// after some of its attacks and moves. A power changes only the orders
    /// after it. For each legal power, the plan uses that power first and the
    /// Hard policy plays the rest of the turn.
    #[serde(skip_serializing_if = "is_false")]
    pub power_first: bool,
    /// The score of active duels without fog, fitted to human replays.
    ///
    /// When present, it replaces [`PlannerConfig::eval_weights`] at every
    /// search leaf and reroute screen of such a game. Terminal positions and
    /// other games keep the stock score.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_score: Option<ReplayScore>,
}

// The fields that later configurations add are left out of the fingerprint
// when they are zero, so the fingerprints of the earlier configurations do not
// change.
#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_zero(value: &usize) -> bool {
    *value == 0
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_false(value: &bool) -> bool {
    !*value
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero_funds(value: &f64) -> bool {
    *value == 0.0
}

impl PlannerConfig {
    /// The first planner configuration.
    pub const V1: Self = Self {
        identifier: "planner-v1",
        baseline: crate::profile::HARD_V2_CONFIG,
        eval_weights: EvalWeights {
            exposure: 0.0,
            front: 0.25,
            ..EvalWeights::STANDARD
        },
        property_values: PropertyValues::STANDARD,
        reply_weight: 1.0,
        kill_plans: 6,
        safety_plans: 3,
        block_plans: 3,
        plans_per_turn: 12,
        turn_work: None,
        seed_margin: 0.0,
        hard_reply_top: 0,
        reroute_units: 0,
        reroute_orders: 0,
        reply_window: 0.0,
        power_first: false,
        replay_score: None,
    };

    /// The first configuration with a simulated Hard reply check.
    pub const V2: Self = Self {
        identifier: "planner-v2",
        hard_reply_top: 2,
        ..Self::V1
    };

    /// The production configuration: planner-v2 with a work limit for each
    /// turn.
    pub const V3: Self = Self {
        identifier: "planner-v3",
        turn_work: Some(TURN_WORK),
        ..Self::V2
    };

    /// planner-v3 with reroute plans and a wider simulated Hard reply check.
    ///
    /// It evaluates more plans than planner-v3. Use a node budget of
    /// [`NodeBudget::THIRTY_TWO`] and the work threshold
    /// [`TURN_WORK_V4`].
    pub const V4: Self = Self {
        identifier: "planner-v4",
        turn_work: Some(TURN_WORK_V4),
        hard_reply_top: 4,
        reroute_units: 3,
        reroute_orders: 3,
        reply_window: 5_000.0,
        ..Self::V3
    };

    /// planner-v4 with power plans, on the scoring of `ai-hard-v3`, which
    /// adds a build floor.
    ///
    /// It uses the same node budget and work threshold as planner-v4.
    pub const V5: Self = Self {
        identifier: "planner-v5",
        baseline: crate::profile::HARD_V3_CONFIG,
        power_first: true,
        ..Self::V4
    };

    /// planner-v5 with the position score fitted to human replays.
    ///
    /// The plan generators, reply checks and work limits are those of
    /// planner-v5.
    pub const V6: Self = Self {
        identifier: "planner-v6",
        replay_score: Some(ReplayScore::CUP_3_4),
        ..Self::V5
    };

    /// Return a stable fingerprint of all configuration values.
    pub fn fingerprint(&self) -> String {
        let bytes = serde_json::to_vec(&(self, self.property_values_key()))
            .expect("planner configuration serializes");
        format!("{:016x}", crate::fingerprint::fnv1a(&bytes))
    }

    fn property_values_key(&self) -> [f64; 4] {
        let values = self.property_values;
        [
            values.income,
            values.production,
            values.headquarters,
            values.capture_share,
        ]
    }
}

/// The work threshold of [`PlannerConfig::V3`], in simulated greedy decisions.
pub const TURN_WORK: u64 = 1_000;

/// The work threshold of [`PlannerConfig::V4`], in simulated greedy decisions
/// and reroute screens.
pub const TURN_WORK_V4: u64 = 2_000;

/// Which generator made a plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Generator {
    Seed,
    Kill,
    Safety,
    Block,
    Reroute,
    Power,
}

/// Counters for one agent over a match.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PlannerStats {
    /// Plans built, counting each replan.
    pub plans: u64,
    /// Position evaluations, including simulated replies.
    pub evaluations: u64,
    /// Complete candidate turns scored.
    pub candidates: u64,
    /// Plans where the seed won.
    pub chose_seed: u64,
    /// Plans where a kill plan won.
    pub chose_kill: u64,
    /// Plans where a safety plan won.
    pub chose_safety: u64,
    /// Plans where a block plan won.
    pub chose_block: u64,
    /// Plans where a reroute plan won.
    #[serde(default)]
    pub chose_reroute: u64,
    /// Plans where a power plan won.
    #[serde(default)]
    pub chose_power: u64,
    /// Decisions where the observed position differed from the prediction.
    pub mismatches: u64,
    /// Decisions that the Hard policy made after the plan or work limit.
    pub fallbacks: u64,
    /// Rejected plays.
    pub rejections: u64,
    /// Simulated greedy decisions.
    pub work: u64,
    /// The most simulated greedy decisions in one own turn.
    pub max_turn_work: u64,
}

/// Luck at the middle of each range. Weather stays clear.
struct MeanLuck;

impl Entropy for MeanLuck {
    fn luck(&mut self, _polarity: Luck, domain: Domain) -> Result<i64, RandomError> {
        Ok(domain.minimum + (domain.maximum - domain.minimum) / 2)
    }

    fn weather(&mut self) -> Result<WeatherKind, RandomError> {
        Ok(WeatherKind::Clear)
    }
}

/// One complete simulated turn.
struct Line {
    generator: Generator,
    /// The plays before the end of the turn, in order.
    plays: Vec<Play>,
    score: f64,
    reply: ReplyEstimate,
}

/// The planner agent.
#[derive(Debug)]
pub struct PlannerAgent {
    config: PlannerConfig,
    seed: u64,
    fallback: GreedyAgent,
    evaluator: Evaluator,
    replay: Option<ReplayReader>,
    plan: Vec<Play>,
    positions: Vec<State>,
    next: usize,
    turn: Option<(awvm::semantic::PlayerId, u64)>,
    plans_this_turn: u32,
    work_this_turn: u64,
    stats: PlannerStats,
}

impl PlannerAgent {
    /// Build the planner with the first configuration.
    pub fn from_seed(seed: u64) -> Self {
        Self::with_config(seed, PlannerConfig::V1)
    }

    /// Build the planner with an explicit configuration.
    pub fn with_config(seed: u64, config: PlannerConfig) -> Self {
        Self {
            config,
            seed,
            fallback: config.baseline.build_greedy(seed),
            evaluator: Evaluator::new(config.eval_weights),
            replay: config.replay_score.map(ReplayReader::new),
            plan: Vec::new(),
            positions: Vec::new(),
            next: 0,
            turn: None,
            plans_this_turn: 0,
            work_this_turn: 0,
            stats: PlannerStats::default(),
        }
    }

    /// Return the configuration.
    pub const fn config(&self) -> &PlannerConfig {
        &self.config
    }

    /// Return the counters for the match so far.
    pub const fn stats(&self) -> &PlannerStats {
        &self.stats
    }

    fn begin_turn(&mut self, view: &Observation) {
        let turn = (view.turn.active_player.clone(), view.turn.day);
        if self.turn.as_ref() != Some(&turn) {
            self.turn = Some(turn);
            self.plans_this_turn = 0;
            self.work_this_turn = 0;
            self.plan.clear();
            self.positions.clear();
            self.next = 0;
        }
    }

    fn may_plan(&self) -> bool {
        if self.plans_this_turn >= self.config.plans_per_turn {
            return false;
        }
        self.config
            .turn_work
            .is_none_or(|limit| self.work_this_turn < limit)
    }

    /// Build a plan for the position in `session` and keep it.
    fn build(&mut self, session: &Session, budget: NodeBudget) -> Option<()> {
        let seat = session
            .state()
            .players
            .seat(&session.state().turn.active_player)?;
        let work_left = self
            .config
            .turn_work
            .map(|limit| limit.saturating_sub(self.work_this_turn));
        let mut planner = TurnPlanner {
            config: &self.config,
            seed: Rng::mix(self.seed ^ u64::from(self.plans_this_turn)),
            seat,
            evaluator: &mut self.evaluator,
            replay: self.replay.as_mut(),
            candidates: 0,
            work: 0,
            work_left,
            nodes_left: budget.get(),
        };
        let line = planner.plan(session);
        self.stats.evaluations += u64::from(budget.get() - planner.nodes_left);
        self.stats.candidates += planner.candidates;
        self.stats.work += planner.work;
        self.work_this_turn += planner.work;
        self.stats.max_turn_work = self.stats.max_turn_work.max(self.work_this_turn);
        self.plans_this_turn += 1;
        self.stats.plans += 1;
        let line = line?;
        let positions = line_positions(session, &line.plays)?;
        match line.generator {
            Generator::Seed => self.stats.chose_seed += 1,
            Generator::Kill => self.stats.chose_kill += 1,
            Generator::Safety => self.stats.chose_safety += 1,
            Generator::Block => self.stats.chose_block += 1,
            Generator::Reroute => self.stats.chose_reroute += 1,
            Generator::Power => self.stats.chose_power += 1,
        }
        self.plan = line.plays;
        self.positions = positions;
        self.next = 0;
        Some(())
    }
}

impl Agent for PlannerAgent {
    fn act(&mut self, view: &Observation, budget: NodeBudget) -> Option<Play> {
        let session = Session::from_observation(view).ok()?;
        if !session.is_commandable() {
            return None;
        }
        self.begin_turn(view);
        let on_plan = self.positions.get(self.next) == Some(session.state());
        if !on_plan {
            if !self.positions.is_empty() {
                self.stats.mismatches += 1;
            }
            self.plan.clear();
            self.positions.clear();
            self.next = 0;
            if !self.may_plan() || self.build(&session, budget).is_none() {
                self.stats.fallbacks += 1;
                return self.fallback.act_in_session(&session);
            }
        }
        let play = self.plan.get(self.next).copied();
        self.next += 1;
        play
    }

    fn planner_stats(&self) -> Option<PlannerStats> {
        Some(self.stats.clone())
    }

    fn start_match(&mut self) {
        let config = self.config;
        let seed = self.seed;
        *self = Self::with_config(seed, config);
    }

    fn start_turn(&mut self, view: &Observation) {
        self.begin_turn(view);
    }

    fn reject(&mut self, _view: &Observation) {
        self.stats.rejections += 1;
        self.plan.clear();
        self.positions.clear();
        self.next = 0;
        // A rejected plan must not be built again, so the rest of the turn
        // uses the Hard policy.
        self.plans_this_turn = self.config.plans_per_turn;
    }
}

/// The work of one planning call.
struct TurnPlanner<'a> {
    config: &'a PlannerConfig,
    seed: u64,
    seat: PlayerIdx,
    evaluator: &'a mut Evaluator,
    replay: Option<&'a mut ReplayReader>,
    candidates: u64,
    /// Simulated greedy decisions in this call.
    work: u64,
    /// The decisions this call may use before it starts no new candidate.
    work_left: Option<u64>,
    nodes_left: u32,
}

impl TurnPlanner<'_> {
    fn leaf_value(&mut self, session: &Session) -> f64 {
        match self.replay.as_deref_mut() {
            Some(replay) if ReplayScore::applies(session.state()) => {
                Evaluator::terminal_value(session.state(), self.seat)
                    .unwrap_or_else(|| replay.value_in(session, self.seat))
            }
            _ => self.evaluator.value_in(session, self.seat),
        }
    }

    fn plan(&mut self, root: &Session) -> Option<Line> {
        let mut session = Session::new(root.state().clone());
        let seed = self.line(&mut session, Generator::Seed, &[])?;
        let mut best = seed;
        let seed_score = best.score;

        let mut alternatives: Vec<(Generator, Vec<Play>)> = Vec::new();
        if !self.out_of_work() && self.config.kill_plans > 0 {
            let options = combat::attack_options(&session);
            let kills = combat::kills(&options);
            for plan in combat::plans(&kills, self.config.kill_plans) {
                let plays = plan
                    .orders()
                    .map(|attack| {
                        let target = target_cell(session.state(), attack.target)?;
                        Some(Play::new(
                            attack.attacker,
                            attack.destination,
                            OrderKind::Attack(target),
                        ))
                    })
                    .collect::<Option<Vec<_>>>();
                if let Some(plays) = plays {
                    alternatives.push((Generator::Kill, plays));
                }
            }
        }
        if self.out_of_work() {
            return Some(best);
        }
        if self.config.power_first {
            alternatives.extend(
                powers(&session)
                    .filter(|play| best.plays.first() != Some(play))
                    .map(|play| (Generator::Power, vec![play])),
            );
        }
        for unit in best.reply.destroyed.iter().take(self.config.safety_plans) {
            if let Some(play) = hold(&session, *unit) {
                alternatives.push((Generator::Safety, vec![play]));
            }
        }
        for cell in best.reply.threatened.iter().take(self.config.block_plans) {
            if let Some(play) = block(&session, self.seat, *cell) {
                alternatives.push((Generator::Block, vec![play]));
            }
        }
        // Reroutes come last: a block can save the headquarters, and a work
        // limit must not stop it.
        for play in self.reroutes(&mut session, &best) {
            alternatives.push((Generator::Reroute, vec![play]));
        }

        let better = seed_score + self.config.seed_margin;
        let mut contenders: Vec<Line> = Vec::new();
        for (generator, prefix) in alternatives {
            if self.out_of_work() {
                break;
            }
            let Some(line) = self.line(&mut session, generator, &prefix) else {
                continue;
            };
            if line.score > better - self.config.reply_window {
                contenders.push(line);
            }
        }
        contenders.sort_by(|left, right| right.score.total_cmp(&left.score));
        // Without the reply check, only a contender above the seed can win.
        let unchecked = |contenders: Vec<Line>, seed: Line| {
            Some(
                contenders
                    .into_iter()
                    .find(|line| line.score > better)
                    .unwrap_or(seed),
            )
        };
        if contenders.is_empty() {
            return Some(best);
        }
        if self.config.hard_reply_top == 0 {
            return unchecked(contenders, best);
        }

        // Play the Hard reply for the seed and the best contenders, and keep
        // the line with the best position after that reply.
        if self.out_of_work() || self.nodes_left < 2 {
            return unchecked(contenders, best);
        }
        contenders.truncate(self.config.hard_reply_top.min(self.nodes_left as usize - 1));
        let Some(mut best_value) = self.replied_value(&mut session, &best) else {
            return unchecked(contenders, best);
        };
        for line in contenders {
            if self.out_of_work() {
                break;
            }
            if let Some(value) = self.replied_value(&mut session, &line)
                && value > best_value
            {
                best_value = value;
                best = line;
            }
        }
        Some(best)
    }

    /// Other orders for the units that the seed plan leaves most exposed.
    ///
    /// For each exposed unit, the planner applies each legal wait, capture,
    /// and attack of that unit alone, ends the turn, and scores the position
    /// as a complete line is scored. Other units do not move in this screen.
    /// The screen keeps the best order of each kind: one wait, one capture,
    /// and one attack for each target. Many waits often get the same score,
    /// and the plans must not all be retreats. The best of these orders that
    /// are not in the seed plan are returned. Each screened order is one unit
    /// of work. The session is back at its start position when this returns.
    fn reroutes(&mut self, session: &mut Session, seed: &Line) -> Vec<Play> {
        if self.config.reroute_units == 0 || self.config.reroute_orders == 0 {
            return Vec::new();
        }
        let friendly = session.state().turn.active_player.clone();
        let mut orders = Vec::new();
        session.legal().orders(&mut orders);
        let mut plays = Vec::new();
        let mut units = 0;
        for unit in &seed.reply.exposed {
            if units >= self.config.reroute_units || self.out_of_work() {
                break;
            }
            let options: Vec<(Order, Play)> = orders
                .iter()
                .filter(|order| {
                    matches!(
                        order.kind(),
                        OrderKind::Wait | OrderKind::Capture | OrderKind::Attack(_)
                    )
                })
                .filter_map(|order| Some((*order, Play::from_order(session, *order)?)))
                .filter(|(_, play)| play.unit() == Some(*unit) && !seed.plays.contains(play))
                .collect();
            if options.is_empty() {
                continue;
            }
            units += 1;
            let mut scored: Vec<(f64, Play)> = Vec::new();
            for (order, play) in options {
                if self.work_left.is_some_and(|left| self.work >= left) {
                    break;
                }
                self.work += 1;
                if let Some(score) = self.screen(session, order, &friendly) {
                    scored.push((score, play));
                }
            }
            scored.sort_by(|left, right| right.0.total_cmp(&left.0));
            let mut kinds: Vec<OrderKind> = Vec::new();
            scored.retain(|(_, play)| {
                let kind = play.kind();
                if kinds.contains(&kind) {
                    return false;
                }
                kinds.push(kind);
                true
            });
            plays.extend(
                scored
                    .into_iter()
                    .take(self.config.reroute_orders)
                    .map(|(_, play)| play),
            );
        }
        plays
    }

    /// The score of a turn that plays only `order` and then ends.
    ///
    /// This does not spend a node, because it is not a complete line. The
    /// session is back at its start position when this returns.
    fn screen(
        &mut self,
        session: &mut Session,
        order: Order,
        friendly: &awvm::semantic::PlayerId,
    ) -> Option<f64> {
        let mut entropy = MeanLuck;
        let root = session.apply(order, &mut entropy, &mut ()).ok()?;
        let score = (|| {
            if matches!(session.state().match_state, Match::Active { .. }) {
                let end = session
                    .resolve(&Command::EndTurn {
                        player: friendly.clone(),
                    })
                    .ok()?;
                session.apply(end, &mut entropy, &mut ()).ok()?;
            }
            let value = self.leaf_value(session);
            let reply = if matches!(session.state().match_state, Match::Active { .. }) {
                reply::estimate(session, self.seat, self.config.property_values).total()
            } else {
                0.0
            };
            Some(value - self.config.reply_weight * reply)
        })();
        session.rewind(root);
        score
    }

    fn out_of_work(&self) -> bool {
        self.nodes_left == 0 || self.work_left.is_some_and(|left| self.work >= left)
    }

    /// Play `line`, then a Hard reply for the enemy, and value the result.
    ///
    /// The session is back at its start position when this returns.
    fn replied_value(&mut self, session: &mut Session, line: &Line) -> Option<f64> {
        let friendly = session.state().turn.active_player.clone();
        let mut entropy = MeanLuck;
        let mut root = None;
        let result = (|| {
            for play in &line.plays {
                let order = play_order(session, play)?;
                root.get_or_insert(session.apply(order, &mut entropy, &mut ()).ok()?);
            }
            if !matches!(session.state().match_state, Match::Active { .. }) {
                self.nodes_left -= 1;
                return Some(self.leaf_value(session));
            }
            let end = session
                .resolve(&Command::EndTurn {
                    player: friendly.clone(),
                })
                .ok()?;
            root.get_or_insert(session.apply(end, &mut entropy, &mut ()).ok()?);
            let enemy = session.state().turn.active_player.clone();
            let mut hard = self
                .config
                .baseline
                .build_greedy(Rng::mix(self.seed ^ 0x5eed));
            while session.state().turn.active_player == enemy
                && matches!(session.state().match_state, Match::Active { .. })
            {
                self.work += 1;
                let order = match hard.act_in_session(session) {
                    Some(play) => play_order(session, &play)?,
                    None => session
                        .resolve(&Command::EndTurn {
                            player: enemy.clone(),
                        })
                        .ok()?,
                };
                session.apply(order, &mut entropy, &mut ()).ok()?;
            }
            self.nodes_left -= 1;
            Some(self.leaf_value(session))
        })();
        if let Some(mark) = root {
            session.rewind(mark);
        }
        result
    }

    /// Play `prefix`, then let Hard finish the turn, and score the result.
    ///
    /// The session is back at its start position when this returns.
    fn line(
        &mut self,
        session: &mut Session,
        generator: Generator,
        prefix: &[Play],
    ) -> Option<Line> {
        let friendly = session.state().turn.active_player.clone();
        let mut entropy = MeanLuck;
        let mut hard = self.config.baseline.build_greedy(self.seed);
        let mut plays = Vec::new();
        let mut root = None;
        let mut prefix = prefix.iter();
        let mut work = 0;
        let decisions = &mut work;
        let result = (|| {
            loop {
                let state = session.state();
                if state.turn.active_player != friendly
                    || !matches!(state.match_state, Match::Active { .. })
                {
                    break;
                }
                let next = loop {
                    match prefix.next() {
                        Some(play) => {
                            // A prefix play can become illegal after an
                            // earlier one, for example when its target is
                            // already destroyed. Skip it.
                            if let Some(order) = play_order(session, play) {
                                break Some((*play, order));
                            }
                        }
                        None => break None,
                    }
                };
                if next.is_none() {
                    *decisions += 1;
                }
                let (play, order) = match next {
                    Some(next) => next,
                    None => match hard.act_in_session(session) {
                        Some(play) => {
                            let order = play_order(session, &play)?;
                            (play, order)
                        }
                        None => {
                            let order = session
                                .resolve(&Command::EndTurn {
                                    player: friendly.clone(),
                                })
                                .ok()?;
                            let mark = session.apply(order, &mut entropy, &mut ()).ok()?;
                            root.get_or_insert(mark);
                            break;
                        }
                    },
                };
                plays.push(play);
                let mark = session.apply(order, &mut entropy, &mut ()).ok()?;
                root.get_or_insert(mark);
            }
            Some(())
        })();
        self.work += work;
        let scored = result.map(|()| {
            self.nodes_left -= 1;
            self.candidates += 1;
            let value = self.leaf_value(session);
            let reply = if matches!(session.state().match_state, Match::Active { .. }) {
                reply::estimate(session, self.seat, self.config.property_values)
            } else {
                ReplyEstimate::default()
            };
            (value - self.config.reply_weight * reply.total(), reply)
        });
        if let Some(mark) = root {
            session.rewind(mark);
        }
        let (score, reply) = scored?;
        Some(Line {
            generator,
            plays,
            score,
            reply,
        })
    }
}

/// The order for `play` in `session`, if it is legal there.
fn play_order(session: &Session, play: &Play) -> Option<Order> {
    let command = play.command(session)?;
    session.resolve(&command).ok()
}

/// The cell where `unit` stands.
fn target_cell(state: &State, unit: UnitId) -> Option<CellIdx> {
    match state.units.get(unit)?.location {
        Location::Board { position } => state.board.dimensions().cell_index(position),
        Location::Cargo { .. } => None,
    }
}

/// The commander powers that are legal in `session`.
fn powers(session: &Session) -> impl Iterator<Item = Play> + '_ {
    let mut orders = Vec::new();
    session.legal().orders(&mut orders);
    orders
        .into_iter()
        .filter(|order| matches!(order.kind(), OrderKind::Power(_)))
        .filter_map(|order| Play::from_order(session, order))
}

/// A play that keeps `unit` where it stands.
fn hold(session: &Session, unit: UnitId) -> Option<Play> {
    let cell = target_cell(session.state(), unit)?;
    let play = Play::new(unit, cell, OrderKind::Wait);
    play_order(session, &play).map(|_| play)
}

/// A play that puts one of our units on `cell`.
///
/// The unit is the one with the lowest cost that can reach the cell, so the
/// block takes the least from the rest of the turn.
fn block(session: &Session, seat: PlayerIdx, cell: CellIdx) -> Option<Play> {
    let state = session.state();
    let mut candidates: Vec<(u64, UnitId)> = state
        .units
        .iter()
        .filter(|unit| unit.owner == seat)
        .map(|unit| (awvm::ruleset::profile(unit.kind).cost, unit.id))
        .collect();
    candidates.sort_unstable();
    candidates.into_iter().find_map(|(_, unit)| {
        let play = Play::new(unit, cell, OrderKind::Wait);
        play_order(session, &play).map(|_| play)
    })
}

/// The position before each play of a line, and one more before the end of
/// the turn if the turn is still active after the plays.
///
/// The planner scores many lines but keeps only one, so only that line keeps
/// its positions. The plays use mean luck, as in [`TurnPlanner::line`], so the
/// replay gets the same positions.
fn line_positions(root: &Session, plays: &[Play]) -> Option<Vec<State>> {
    let mut session = Session::new(root.state().clone());
    let friendly = session.state().turn.active_player.clone();
    let mut entropy = MeanLuck;
    let mut positions = Vec::with_capacity(plays.len() + 1);
    for play in plays {
        let order = play_order(&session, play)?;
        positions.push(session.state().clone());
        session.apply(order, &mut entropy, &mut ()).ok()?;
    }
    let state = session.state();
    if state.turn.active_player == friendly && matches!(state.match_state, Match::Active { .. }) {
        positions.push(state.clone());
    }
    Some(positions)
}

#[cfg(test)]
mod tests;
