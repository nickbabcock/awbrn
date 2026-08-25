//! An agent that draws one legal play at a time, uniformly.
//!
//! It plays badly on purpose. Its job is the plumbing: it opens a session on
//! its own observation, asks what is legal, chooses, and hands back a play that
//! the driver has to turn into a command against the true state. Every fault in
//! that path shows up here, where a wrong answer costs nothing.
//!
//! It is also the floor of the ladder. An agent that cannot beat this one is
//! not an agent yet.

use awvm::semantic::Observation;
use awvm::session::{Order, OrderKind, Session};

use crate::agent::{Agent, Play};
use crate::rng::Rng;

#[derive(Debug)]
pub struct RandomAgent {
    rng: Rng,
    /// Held across calls so that a turn's enumeration reuses one allocation.
    orders: Vec<Order>,
}

impl RandomAgent {
    pub const fn from_seed(seed: u64) -> Self {
        Self {
            rng: Rng::from_seed(seed),
            orders: Vec::new(),
        }
    }
}

impl Agent for RandomAgent {
    fn act(&mut self, view: &Observation) -> Option<Play> {
        // A projection this player may not act on reports nothing legal, so
        // the session answers the question rather than an error path.
        let session = Session::from_observation(view).ok()?;
        if !session.is_commandable() {
            return None;
        }

        self.orders.clear();
        session.legal().orders(&mut self.orders);

        // A reservoir draw, so that the choice is uniform over the plays this
        // agent will consider without a second list to hold them.
        let Self { rng, orders } = self;
        let mut seen = 0u64;
        let mut chosen = None;
        for order in orders.iter().copied().filter(|order| considered(*order)) {
            seen += 1;
            if rng.below(seen) == 0 {
                chosen = Some(order);
            }
        }

        Play::from_order(&session, chosen?)
    }
}

/// Whether a random agent may draw this order.
///
/// The three it may not are the three that decide a game for a reason no policy
/// holds. Resignation ends the match, deletion throws a unit away, and both
/// would make a game's length a property of the agent's readiness to give up
/// rather than of the game. Ending the turn is left out for a different reason:
/// it is what `None` means, so drawing it would end a turn twice.
const fn considered(order: Order) -> bool {
    !matches!(
        order.kind(),
        OrderKind::EndTurn | OrderKind::Resign | OrderKind::Delete
    )
}
