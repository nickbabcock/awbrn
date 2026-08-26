//! One-pass improvement of a complete greedy turn.
//!
//! The greedy turn is the seed. The search changes one unit order at a time,
//! keeps the rest of the turn fixed, plays a fresh greedy reply, and evaluates
//! the resulting position. One complete evaluated candidate is one node.

use awvm::random::Entropy;
use awvm::semantic::{AwbwVisibility, Match, Observation, PlayerId, observe_into};
use awvm::session::{Order, OrderKind, Session};
use awvm::transition::Command;

use crate::agent::{Agent, NodeBudget, Play};
use crate::agents::{GreedyAgent, Weights};
use crate::eval::{EvalWeights, Evaluator};
use crate::rng::Rng;

/// A greedy agent with one coordinate-descent pass over its complete turn.
#[derive(Debug)]
pub struct SearchAgent {
    seed: u64,
    weights: Weights,
    plan: Vec<Play>,
    next: usize,
    turn: Option<(PlayerId, u64)>,
}

impl SearchAgent {
    pub const fn from_seed(seed: u64) -> Self {
        Self::with_weights(seed, Weights::DEFAULT)
    }

    pub const fn with_weights(seed: u64, weights: Weights) -> Self {
        Self {
            seed,
            weights,
            plan: Vec::new(),
            next: 0,
            turn: None,
        }
    }

    fn build_plan(&mut self, view: &Observation, budget: NodeBudget) -> Option<()> {
        let mut search = TurnSearch::new(view, self.seed, self.weights)?;
        self.plan = search.improve(budget)?;
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
}

impl TurnSearch {
    fn new(view: &Observation, seed: u64, weights: Weights) -> Option<Self> {
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
            evaluator: Evaluator::new(EvalWeights::STANDARD),
        })
    }

    fn improve(&mut self, budget: NodeBudget) -> Option<Vec<Play>> {
        let seed = self.greedy_seed()?;
        let (mut best_value, _) = self.evaluate(&seed)?;
        let mut best = seed.clone();
        let mut nodes = 1;

        'coordinates: for coordinate in 0..seed.len() {
            let Some(unit) = seed[coordinate].unit() else {
                continue;
            };
            let coordinate_plan = best.clone();
            for alternative in self.alternatives(&coordinate_plan, coordinate, unit) {
                if nodes >= budget.get() {
                    break 'coordinates;
                }
                if alternative == coordinate_plan[coordinate] {
                    continue;
                }
                let mut candidate = coordinate_plan.clone();
                candidate[coordinate] = alternative;
                let Some((value, _reply)) = self.evaluate(&candidate) else {
                    continue;
                };
                nodes += 1;
                if value > best_value {
                    best_value = value;
                    best = candidate;
                }
            }
        }

        self.plays(&best)
    }

    fn greedy_seed(&mut self) -> Option<Vec<Order>> {
        let mut agent = GreedyAgent::with_weights(self.friendly_seed, self.weights);
        let mut entropy = Rng::from_seed(self.entropy_seed);
        let mut plan = Vec::new();
        let mut root = None;
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
            let mark = self.session.apply(order, &mut entropy, &mut ()).ok()?;
            root.get_or_insert(mark);
            plan.push(order);
        }
        self.session.rewind(root?);
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

    fn evaluate(&mut self, plan: &[Order]) -> Option<(f64, Vec<Order>)> {
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
            let seat = self.session.state().players.seat(&self.friendly)?;
            let value = self.evaluator.value_in(&self.session, seat);
            Some((value, reply))
        });
        self.session.rewind(root?);
        debug_assert_eq!(self.session.state(), &original);
        result
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
        let mut expected = TurnSearch::new(&view, 7, Weights::THREAT).expect("search opens");
        let seed = expected.greedy_seed().expect("greedy makes a turn");
        let seed = expected.plays(&seed).expect("the seed is legal");

        let mut actual = TurnSearch::new(&view, 7, Weights::THREAT).expect("search opens");
        assert_eq!(actual.improve(NodeBudget::ONE), Some(seed));
    }

    #[test]
    fn fixed_state_and_seed_make_the_same_plan() {
        let view = view();
        let plan = || {
            TurnSearch::new(&view, 11, Weights::THREAT)
                .expect("search opens")
                .improve(NodeBudget::FOUR)
        };
        assert_eq!(plan(), plan());
    }

    #[test]
    fn evaluated_candidates_are_legal_and_rewind_exactly() {
        let view = view();
        let mut search = TurnSearch::new(&view, 13, Weights::THREAT).expect("search opens");
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
                if let Some((_value, reply)) = search.evaluate(&candidate) {
                    assert!(!reply.is_empty(), "each leaf has an opponent reply");
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
}
