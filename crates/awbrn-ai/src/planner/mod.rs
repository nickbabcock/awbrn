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
//! 3. Each complete turn gets a score: the position value from
//!    [`crate::eval`] at the start of the enemy turn, less a fast estimate of
//!    the enemy reply ([`reply`]).
//! 4. The agent plays the best turn one order at a time. Before each order it
//!    compares the observed position with the predicted one. A combat roll or
//!    a fog reveal changes the position, and the agent then plans again from
//!    the new position.
//!
//! Simulation uses the middle of each luck range, so a plan is deterministic.

mod combat;
mod reply;

use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::io::{self, Write};

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

/// Which generator made a plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Generator {
    Seed,
    Kill,
    Safety,
    Block,
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
    /// The digest of the position before each play, and one more at the end.
    digests: Vec<u64>,
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
    plan: Vec<Play>,
    digests: Vec<u64>,
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
            plan: Vec::new(),
            digests: Vec::new(),
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
            self.digests.clear();
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
        match line.generator {
            Generator::Seed => self.stats.chose_seed += 1,
            Generator::Kill => self.stats.chose_kill += 1,
            Generator::Safety => self.stats.chose_safety += 1,
            Generator::Block => self.stats.chose_block += 1,
        }
        self.plan = line.plays;
        self.digests = line.digests;
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
        let digest = digest(session.state());
        let on_plan = !self.digests.is_empty() && self.digests.get(self.next) == Some(&digest);
        if !on_plan {
            if !self.digests.is_empty() {
                self.stats.mismatches += 1;
            }
            self.plan.clear();
            self.digests.clear();
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
        self.digests.clear();
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
    candidates: u64,
    /// Simulated greedy decisions in this call.
    work: u64,
    /// The decisions this call may use before it starts no new candidate.
    work_left: Option<u64>,
    nodes_left: u32,
}

impl TurnPlanner<'_> {
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

        let mut contenders: Vec<Line> = Vec::new();
        for (generator, prefix) in alternatives {
            if self.out_of_work() {
                break;
            }
            let Some(line) = self.line(&mut session, generator, &prefix) else {
                continue;
            };
            if line.score > seed_score + self.config.seed_margin {
                contenders.push(line);
            }
        }
        contenders.sort_by(|left, right| right.score.total_cmp(&left.score));
        if contenders.is_empty() {
            return Some(best);
        }
        if self.config.hard_reply_top == 0 {
            return contenders.into_iter().next();
        }

        // Play the Hard reply for the seed and the best contenders, and keep
        // the line with the best position after that reply.
        if self.out_of_work() || self.nodes_left < 2 {
            return contenders.into_iter().next();
        }
        contenders.truncate(self.config.hard_reply_top.min(self.nodes_left as usize - 1));
        let Some(mut best_value) = self.replied_value(&mut session, &best) else {
            return contenders.into_iter().next();
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
                return Some(self.evaluator.value_in(session, self.seat));
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
            Some(self.evaluator.value_in(session, self.seat))
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
        let mut digests = Vec::new();
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
                            digests.push(digest(session.state()));
                            let mark = session.apply(order, &mut entropy, &mut ()).ok()?;
                            root.get_or_insert(mark);
                            break;
                        }
                    },
                };
                digests.push(digest(session.state()));
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
            let value = self.evaluator.value_in(session, self.seat);
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
            digests,
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

/// A digest of the position used by the plan.
///
/// Keep exact health and all mutable command inputs. Write the fields into
/// the hasher without an intermediate string or byte buffer.
fn digest(state: &State) -> u64 {
    struct HashWriter(DefaultHasher);
    impl Write for HashWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.write(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut writer = HashWriter(DefaultHasher::new());
    serde_json::to_writer(&mut writer, state).expect("the position serializes");
    writer.0.finish()
}

#[cfg(test)]
mod tests;
