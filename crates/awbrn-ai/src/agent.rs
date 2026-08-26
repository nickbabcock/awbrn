//! What an agent is asked, and how its answer becomes a command.

use awvm::semantic::{CellIdx, Observation, ObservedEvent, UnitId};
use awvm::session::{Order, OrderKind, Session};
use awvm::transition::Command;

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
}

/// The maximum number of candidate turn plans an agent may evaluate.
///
/// One node is one evaluated leaf. Applying the individual orders in a turn
/// plan does not spend more nodes. This definition makes a search repeatable
/// across machines: the same position and budget examine the same number of
/// candidates, independent of clock speed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
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

#[cfg(test)]
mod tests {
    use super::NodeBudget;

    #[test]
    fn node_budget_is_nonzero() {
        assert_eq!(NodeBudget::new(0), None);
        assert_eq!(NodeBudget::new(16), Some(NodeBudget::SIXTEEN));
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
