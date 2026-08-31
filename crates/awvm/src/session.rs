//! One owner, three verbs, and an order that is `Copy`.
//!
//! [`crate::transition::execute`] answers about a state a caller holds. It
//! takes a wire [`Command`], returns a new [`State`], and keeps nothing
//! between calls. A server wants that: one command each second, and a value it
//! can send. It is the wrong shape for a search, which applies millions of
//! orders, keeps none of them, and rebuilds the same board tables after each
//! one.
//!
//! A [`Session`] owns the position instead of borrowing it. Everything derived
//! from the position is rebuilt through the session, so a caller never carries
//! a proof whose subject has moved on:
//!
//! ```text
//! legal   -> what the rules allow here
//! apply   -> put one order into effect, and name the position it left
//! rewind  -> go back to a position a mark names
//! ```
//!
//! Those verbs speak [`Order`]. It is eight bytes and `Copy`, and carries no
//! path and no lifetime: a unit, a destination, and what to do on arrival.
//! Enumerating a turn allocates nothing per candidate, and a search node holds
//! a move in a register rather than in a `Vec<Pos>`.
//!
//! [`Command`] stays the wire type. [`Session::resolve`] turns one into an
//! order and [`Session::spell`] turns an order back into one, and those two
//! are the only places a path is built or checked. A server pays that at its
//! edge. A search never calls either.
//!
//! Every question that would build a list takes the vector to build it in.
//! Each question has one form and no allocating twin, because the answer is
//! asked for thousands of times and the buffer is the difference between
//! asking cheaply and asking again. The session keeps the rest: the last unit
//! searched, the route walk's vectors, and the board-sized grid the search
//! writes into.

use std::cell::RefCell;

use crate::combat::Forecast;
use crate::commander::PowerLevel;
use crate::event::{AttackTarget, Event};
use crate::query::{
    self, MoveField, MoveScratch, PreparedMoveField, Sweep, Travel, TurnMaps, TurnTables,
    recipient_may_command,
};
use crate::random::Entropy;
use crate::ruleset::{self, UnitKind as UnitKindId};
use crate::semantic::{
    CellIdx, Location, Observation, PlayerIdx, Pos, State, Unit, UnitAction, UnitId,
};
use crate::transition::{
    ActiveTurn, Command, ExecuteError, ExecuteOutcome, PreparedDestination, PreparedProductionSite,
    execute_with,
};
use crate::violation::Violation;

/// A unit named by its index in [`State::units`], in two bytes.
///
/// A [`UnitId`] is four bytes and an order has room for two, which is the
/// whole reason this exists. The trade is that an index is a fact about one
/// roster. Applying a command that removes a unit shifts every later index, so
/// an order held across such a command names a different unit. That stays
/// safe, because the reducer still refuses anything the rules refuse, but it
/// is not the unit the caller meant.
///
/// A search is unaffected, because [`Session::rewind`] restores the roster
/// with the rest of the position. A caller that keeps orders across unrelated
/// positions, as a move-ordering table does, holds a hint and must treat it as
/// one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitIdx(u16);

impl UnitIdx {
    /// The index an order names when it is not about a unit at all.
    const NONE: Self = Self(u16::MAX);

    pub const fn get(self) -> u16 {
        self.0
    }

    pub const fn from_raw(index: u16) -> Self {
        Self(index)
    }
}

/// What a unit does when it arrives, and what the orders that move no unit do.
///
/// Every payload is a tile, a unit kind, or a cargo slot, so the widest
/// variant is two bytes and the whole enum is four with its tag. That holds
/// [`Order`] to eight.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OrderKind {
    /// Move there and end the unit's turn.
    Wait,
    /// Begin or continue capturing the property underfoot.
    Capture,
    /// Resupply the adjacent friendly units.
    Supply,
    /// Enter hidden state.
    Hide,
    /// Leave hidden state.
    Reveal,
    /// Self-destruct, damaging the surrounding area.
    Explode,
    /// Merge into the unit already standing on the destination.
    Join,
    /// Board the transport already standing on the destination.
    Load,
    /// Fire on the tile named here, which is a unit when one stands on it.
    Attack(CellIdx),
    /// Repair the friendly unit on the tile named here.
    Repair(CellIdx),
    /// Fire the silo underfoot at the tile named here.
    Launch(CellIdx),
    /// Remove the unit from play. The destination is where it stands.
    Delete,
    /// Build a unit of this kind at the destination, which is the site.
    Produce(UnitKindId),
    /// Put the cargo in this slot of the transport down on the destination.
    Unload(u8),
    /// Pass to the next player.
    EndTurn,
    /// Swap to the other commander of a tag pair.
    Tag,
    /// Leave the match.
    Resign,
    /// Leave the match because the clock ran out.
    Timeout,
    /// Activate a commander power.
    Power(PowerLevel),
}

/// One thing a player may do, in eight bytes.
///
/// The path is missing on purpose. A route follows from a destination rather
/// than being a separate choice, and building one for each of the hundreds of
/// destinations a turn reaches was most of what enumeration used to cost.
/// [`Session::spell`] rebuilds the route when a wire command needs one.
///
/// Nothing here proves the order is legal, and it carries no lifetime that
/// could. [`Session::apply`] decides that against the position it holds now,
/// which beats a borrow: a lifetime fixes the address of a state, not its
/// contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Order {
    unit: UnitIdx,
    dest: CellIdx,
    kind: OrderKind,
}

const _: () = assert!(size_of::<Order>() <= 8);

impl Order {
    /// An order for one unit.
    pub const fn new(unit: UnitIdx, dest: CellIdx, kind: OrderKind) -> Self {
        Self { unit, dest, kind }
    }

    /// An order that names no unit, such as a boundary or a production site.
    pub const fn unitless(dest: CellIdx, kind: OrderKind) -> Self {
        Self {
            unit: UnitIdx::NONE,
            dest,
            kind,
        }
    }

    /// The acting unit, or `None` for an order that moves nothing.
    pub const fn unit(self) -> Option<UnitIdx> {
        if self.unit.0 == UnitIdx::NONE.0 {
            None
        } else {
            Some(self.unit)
        }
    }

    /// Where the order takes effect: the arrival tile, the production site, or
    /// the tile the acting unit already stands on.
    pub const fn destination(self) -> CellIdx {
        self.dest
    }

    pub const fn kind(self) -> OrderKind {
        self.kind
    }
}

/// Which untargeted orders are available at a destination, and whether any
/// targeted one is.
///
/// [`query::ActionSet`] answers the same question in three `Vec`s and eleven
/// fields. A search discards all but one destination, and this answers those
/// in two bytes and no allocation. A caller that wants the targets asks
/// [`Legal::targets`] about the one destination it kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct OrderMask(u16);

/// Which of the three target lists [`Legal::targets`] should walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TargetKind {
    Attack,
    Repair,
    Launch,
}

impl OrderMask {
    const WAIT: u16 = 1 << 0;
    const CAPTURE: u16 = 1 << 1;
    const SUPPLY: u16 = 1 << 2;
    const HIDE: u16 = 1 << 3;
    const REVEAL: u16 = 1 << 4;
    const EXPLODE: u16 = 1 << 5;
    const JOIN: u16 = 1 << 6;
    const LOAD: u16 = 1 << 7;
    const ATTACK: u16 = 1 << 8;
    const REPAIR: u16 = 1 << 9;
    const LAUNCH: u16 = 1 << 10;

    /// The bit an untargeted order sits in, or `None` for one that has targets
    /// or does not belong to a destination at all.
    const fn bit(kind: OrderKind) -> Option<u16> {
        Some(match kind {
            OrderKind::Wait => Self::WAIT,
            OrderKind::Capture => Self::CAPTURE,
            OrderKind::Supply => Self::SUPPLY,
            OrderKind::Hide => Self::HIDE,
            OrderKind::Reveal => Self::REVEAL,
            OrderKind::Explode => Self::EXPLODE,
            OrderKind::Join => Self::JOIN,
            OrderKind::Load => Self::LOAD,
            _ => return None,
        })
    }

    /// Whether the destination admits no order at all, the usual answer for a
    /// tile a unit only passes through.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether an untargeted order is available here.
    ///
    /// A targeted kind, meaning attack, repair or launch, reports whether the
    /// destination has any target of that kind. That is all the mask holds.
    /// Ask [`Legal::targets`] which ones.
    pub const fn allows(self, kind: OrderKind) -> bool {
        match Self::bit(kind) {
            Some(bit) => self.0 & bit != 0,
            None => match kind {
                OrderKind::Attack(_) => self.has(TargetKind::Attack),
                OrderKind::Repair(_) => self.has(TargetKind::Repair),
                OrderKind::Launch(_) => self.has(TargetKind::Launch),
                _ => false,
            },
        }
    }

    /// Whether the destination has any target of this kind.
    pub const fn has(self, kind: TargetKind) -> bool {
        let bit = match kind {
            TargetKind::Attack => Self::ATTACK,
            TargetKind::Repair => Self::REPAIR,
            TargetKind::Launch => Self::LAUNCH,
        };
        self.0 & bit != 0
    }

    /// Every untargeted order this mask admits, in bit order.
    pub fn untargeted(self) -> impl Iterator<Item = OrderKind> {
        const UNTARGETED: [OrderKind; 8] = [
            OrderKind::Wait,
            OrderKind::Capture,
            OrderKind::Supply,
            OrderKind::Hide,
            OrderKind::Reveal,
            OrderKind::Explode,
            OrderKind::Join,
            OrderKind::Load,
        ];
        UNTARGETED
            .into_iter()
            .filter(move |kind| self.allows(*kind))
    }

    fn set(&mut self, bit: u16, present: bool) {
        if present {
            self.0 |= bit;
        }
    }
}

/// One row of a build menu.
///
/// A menu greys out what the player cannot yet pay for and hides what the site
/// cannot make at all. Only the reducer knows which is which, so this is its
/// answer rather than a second reading of the same rules. A kind it refuses
/// for any reason other than the price is not a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Production {
    pub kind: UnitKindId,
    /// What this site charges for it, under this player's commander.
    pub cost: u64,
    /// Whether the player can pay that now.
    pub affordable: bool,
}

/// One cargo a transport may put down, and the tile it lands on.
///
/// [`OrderKind::Unload`] names the slot, because a slot is one byte and an
/// order has room for one. A menu needs the unit itself, so this is the same
/// walk reported the way an interface reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unload {
    pub transport: UnitIdx,
    pub cargo: UnitId,
    pub cargo_kind: UnitKindId,
    pub slot: u8,
    pub destination: CellIdx,
}

/// Where the events of an applied order go.
///
/// A server keeps them, because they are what it sends each player and what a
/// replay reads back. A search discards them, and says so by passing `()`.
///
/// The event arrives owned. Nothing downstream of the reducer wants a copy of
/// an event the reducer is about to drop, so a sink that keeps events moves
/// them and a sink that does not costs nothing.
pub trait Sink {
    fn emit(&mut self, event: Event);
}

/// The sink a search passes. It does nothing, and the compiler removes the
/// call.
impl Sink for () {
    fn emit(&mut self, _event: Event) {}
}

/// The sink a server passes. The events it will send on, in order.
impl Sink for Vec<Event> {
    fn emit(&mut self, event: Event) {
        self.push(event);
    }
}

/// A position a session can be returned to.
///
/// Opaque, and only meaningful to the session that made it. The epoch makes a
/// stale mark loud rather than wrong. A search descends and unwinds on a
/// stack, so rewinding to a position the session has already left is a bug,
/// and a debug build reports it instead of restoring some other branch.
///
/// Do not keep a mark in a transposition table. That table holds an evaluation
/// keyed by a hash of the position, not a position to jump to, and a mark from
/// another branch names nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mark {
    depth: usize,
    epoch: u64,
}

/// Why a session refused an order.
///
/// The two halves differ, which is why they are not one variant. A
/// [`Violation`] is an answer. The rules refuse this order, and a caller
/// enumerating what is legal wants it back. An [`ExecuteError`] is a fault.
/// Nothing about the position answers the question.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The rules refuse this order.
    #[error("the order was rejected: {0:?}")]
    Rejected(Violation),
    /// The engine could not answer.
    #[error(transparent)]
    Failed(#[from] ExecuteError),
    /// A derived table could not be built for this position.
    #[error(transparent)]
    Query(#[from] query::QueryError),
}

/// The buffers a walk fills and refills, between questions.
///
/// None of it survives a question and none of it needs dirty tracking, because
/// every answer is written from nothing. What it saves is the allocator
/// traffic of writing from nothing, which enumeration pays hundreds of times
/// per turn.
#[derive(Debug, Default)]
struct Routes {
    /// Plausible attacks for one unit, keyed by its reachable firing tiles.
    attack_index: query::AttackIndex,
    /// What a target walk reports, before an order renames it by tile.
    attacks: Vec<AttackTarget>,
    repairs: Vec<UnitId>,
    launches: Vec<Pos>,
    /// The scratch the attack walk sorts its units in, lent to every call so
    /// that a walk per destination does not allocate a pair of its own.
    attack_units: Vec<UnitId>,
    attack_tiles: Vec<Pos>,
}

/// One unit's searched reach, and the pool the search draws from.
///
/// A shortest-path search over the board is the dearest thing a question about
/// one unit needs, and every question about that unit needs the same one: each
/// destination's mask, each destination's targets, and the route
/// [`Session::spell`] puts in a wire command. Holding the last one searched
/// stops a session from paying for it again. The epoch stops it from answering
/// out of a position it has left.
#[derive(Debug, Default)]
struct Reach {
    /// The index searched, the epoch it was searched at, and the search.
    held: Option<(UnitIdx, u64, MoveField)>,
    /// Board-sized allocations spent fields have handed back.
    scratch: MoveScratch,
    /// Each seat's board tables, in seat order, and the current epoch.
    ///
    /// Opening a turn costs an entry-cost grid per movement class and a
    /// blocking grid, rebuilt over every tile. A single order opens two turns
    /// on one position, [`Session::legal`] to offer it and
    /// [`Session::route_to`] to spell its route, and answering a mask and then
    /// a route used to pay for the same grids twice. Holding them here makes
    /// the second turn free. The epoch stops them from answering out of a
    /// position the session has left. Entry-cost grids have narrower inputs
    /// than blocking, so they can continue across an epoch when only units
    /// moved.
    ///
    /// One row per seat rather than one row, because a reader of the position
    /// asks about the seats it is not playing: what the enemy threatens is a
    /// search of the enemy's units, under the enemy's commander, and those are
    /// the enemy's tables. A row is built when a seat is first asked about, so
    /// a session nobody asks that of still holds one.
    tables: (u64, Vec<TurnTables>),
}

/// One position, and the memory that answering about it needs.
///
/// A session is the unit of ownership a search wants: one per thread, each
/// holding its own position, undo stack and scratch. Size them before starting
/// one per core. The scratch is board-sized, and a design that wins on
/// instruction count and loses on cache misses has not won.
#[derive(Debug)]
pub struct Session {
    state: State,
    /// The position each live mark names, oldest first, with the epoch it was
    /// pushed at. A `State` is a handful of `Vec`s over `Copy` cells, so
    /// keeping one is mostly a memcpy. It is 0.19% of the search node that
    /// follows it, against a journal's cost of routing every write in the
    /// reducer through one path.
    prior: Vec<(State, u64)>,
    routes: RefCell<Routes>,
    /// Held apart from [`Session::routes`] because a field walk and an action
    /// query need both scratch stores at the same time.
    reach: RefCell<Reach>,
    /// How many times this session has changed, in either direction. Every
    /// derived thing the session caches is stamped with this, so changing the
    /// position invalidates all of it without a traversal.
    epoch: u64,
    /// Whether the holder of this session may act on the position at all. Only
    /// [`Session::from_observation`] can set this false. See it for why.
    commandable: bool,
    /// Tiles holding a unit whose exact health this session's holder does not
    /// know, sorted. Empty for an authoritative session, which knows all of it.
    ///
    /// A forecast against a guessed health is a lie, so [`Legal::forecast`]
    /// refuses on these tiles.
    uncertain: Vec<CellIdx>,
}

impl Session {
    /// Open a session on a position.
    pub fn new(state: State) -> Self {
        Self {
            state,
            prior: Vec::new(),
            routes: RefCell::new(Routes::default()),
            reach: RefCell::new(Reach::default()),
            epoch: 0,
            commandable: true,
            uncertain: Vec::new(),
        }
    }

    /// Open a session on what one recipient can see.
    ///
    /// A client holds a fog-limited [`Observation`], not a [`State`]. Reifying
    /// one gives a provisional state that the reducer can answer about, with
    /// everything the recipient cannot see filled in at its most conservative
    /// reading, so the reducer is never told a fact the projection withheld.
    ///
    /// The answers are advisory, and no amount of care changes that. A hidden
    /// blocker can make an offered order illegal, and a hidden target can make
    /// a legal one missing. Executing against the authoritative state is still
    /// the validation that counts.
    ///
    /// A recipient who may not command at all, because it is not their turn,
    /// the phase is wrong or the match is over, gets a session whose
    /// [`Session::legal`] offers nothing rather than an error.
    pub fn from_observation(observation: &Observation) -> Result<Self, Error> {
        let mut session = Self::new(query::reify(observation)?);
        session.commandable = recipient_may_command(observation);
        let dimensions = session.state.board.dimensions();
        session.uncertain = observation
            .units
            .iter()
            .filter(|unit| unit.hp.exact().is_none())
            .filter_map(|unit| match unit.location {
                Location::Board { position } => dimensions.cell_index(position),
                Location::Cargo { .. } => None,
            })
            .collect();
        session.uncertain.sort_unstable();
        Ok(session)
    }

    /// The position as it stands.
    pub const fn state(&self) -> &State {
        &self.state
    }

    /// Whether this session's holder may issue orders at all.
    ///
    /// Always true for an authoritative session. False for one built from an
    /// observation the recipient may not act on.
    pub const fn is_commandable(&self) -> bool {
        self.commandable
    }

    /// How deep the undo stack is, which is how many applied orders a rewind
    /// to the opening position would undo.
    pub fn depth(&self) -> usize {
        self.prior.len()
    }

    /// Put a new game in this session and keep the memory it holds.
    ///
    /// This is how a self-play run starts its next game without handing the
    /// allocator back the board-sized tables it will immediately ask for. The
    /// new position is authoritative, so a session reset from one built by
    /// [`Session::from_observation`] may command again.
    pub fn reset(&mut self, state: State) {
        self.state = state;
        self.prior.clear();
        self.commandable = true;
        self.uncertain.clear();
        self.epoch += 1;
    }

    /// Report what the rules allow in this position.
    pub fn legal(&self) -> Legal<'_> {
        Legal {
            session: self,
            turn: self
                .commandable
                .then(|| {
                    ActiveTurn::opened_with(
                        &self.state,
                        &self.state.turn.active_player,
                        self.turn_tables(),
                    )
                    .ok()
                })
                .flatten(),
        }
    }

    /// Apply one order, and name the position it left.
    ///
    /// The order is validated against the position the session holds now, not
    /// against the one that offered it. Rewinding to the returned mark undoes
    /// this order and everything applied after it.
    pub fn apply<S: Sink>(
        &mut self,
        order: Order,
        entropy: &mut impl Entropy,
        sink: &mut S,
    ) -> Result<Mark, Error> {
        let command = self.command_for(order)?;
        self.apply_command(command, entropy, sink)
    }

    /// Apply one wire command, keeping the route it names.
    ///
    /// An [`Order`] names a destination, and [`Session::spell`] answers it
    /// with the field's own route. Two routes to one tile are not always the
    /// same command, because they spend different fuel and meet different
    /// hidden units. An authority replaying a recorded game, where the route
    /// is evidence, applies the command it was given rather than resolving it
    /// first.
    pub fn apply_command<S: Sink>(
        &mut self,
        command: Command,
        entropy: &mut impl Entropy,
        sink: &mut S,
    ) -> Result<Mark, Error> {
        match execute_with(&self.state, command, entropy)? {
            ExecuteOutcome::Rejected(violation) => Err(Error::Rejected(violation)),
            ExecuteOutcome::Accepted(mut execution) => {
                for event in execution.events.drain(..) {
                    sink.emit(event);
                }
                let mark = Mark {
                    depth: self.prior.len(),
                    epoch: self.epoch,
                };
                let previous = std::mem::replace(&mut self.state, execution.state);
                self.prior.push((previous, self.epoch));
                self.epoch += 1;
                Ok(mark)
            }
        }
    }

    /// Undo each order back to `mark`.
    ///
    /// Panics in a debug build when `mark` names a position this session has
    /// already left. A search descends and unwinds on a stack, so that cannot
    /// happen by accident. The check is here because it costs one comparison.
    pub fn rewind(&mut self, mark: Mark) {
        debug_assert!(
            mark.depth < self.prior.len() && self.prior[mark.depth].1 == mark.epoch,
            "a mark from a position this session has already left"
        );
        if mark.depth >= self.prior.len() {
            return;
        }
        self.prior.truncate(mark.depth + 1);
        let (state, _) = self.prior.pop().expect("the mark's own position");
        self.state = state;
        self.epoch += 1;
    }

    /// The board tables of the active seat, for the position the session holds
    /// now.
    fn turn_tables(&self) -> TurnTables {
        self.state
            .players
            .seat(&self.state.turn.active_player)
            .map(|seat| self.turn_tables_for(seat))
            .unwrap_or_default()
    }

    /// The board tables of one seat, for the position the session holds now.
    ///
    /// Blocking tables are dropped after every position change. Entry-cost
    /// tables continue when their terrain, weather, and commander inputs are
    /// unchanged. See [`TurnTables`].
    fn turn_tables_for(&self, seat: PlayerIdx) -> TurnTables {
        let mut reach = self.reach.borrow_mut();
        if reach.tables.0 != self.epoch {
            for (other, _) in self.state.players.seats() {
                if let Some(tables) = reach.tables.1.get_mut(other.get()) {
                    tables.advance(&self.state, other);
                }
            }
            reach.tables.0 = self.epoch;
        }
        let row = seat.get();
        if reach.tables.1.len() <= row {
            reach.tables.1.resize_with(row + 1, TurnTables::default);
        }
        // A row nobody has shared yet holds nothing at all, which is what a
        // turn that builds and drops its own tables wants. Sharing it is what
        // this call is asking for.
        if reach.tables.1[row].is_empty() {
            reach.tables.1[row].clear();
            reach.tables.1[row].advance(&self.state, seat);
        }
        reach.tables.1[row].clone()
    }

    /// One seat's board tables, to search unit after unit of that seat with.
    ///
    /// An agent that measures what a player can reach searches every unit of
    /// that player, and [`query::reachable`] would open the same tables once
    /// per unit. A sweep opens them once, and the session keeps them for as
    /// long as the position stands. `None` when the roster has no such seat.
    ///
    /// The tables belong to the position the session holds now. Applying or
    /// rewinding an order ends that position, and the borrow ends with it.
    pub fn sweep(&self, seat: PlayerIdx) -> Option<Sweep<'_>> {
        TurnMaps::with_tables(&self.state, seat, self.turn_tables_for(seat)).map(Sweep::with_maps)
    }

    /// One seat's movement-point distances, over the same board tables.
    ///
    /// [`Travel`] and [`Session::sweep`] read the same entry-cost grids, so a
    /// caller that asks both questions of one seat builds each grid once.
    /// `None` when the roster has no such seat.
    pub fn travel(&self, seat: PlayerIdx) -> Option<Travel<'_>> {
        TurnMaps::with_tables(&self.state, seat, self.turn_tables_for(seat)).map(Travel::with_maps)
    }

    /// Run `read` against one index's movement field.
    ///
    /// The search runs only when the session is not already holding this
    /// unit's reach for this position. Otherwise the held geometry is rebound
    /// to a fresh proof through [`PreparedMoveField::from_parts`]. Every
    /// question about a unit comes through here, whether it is the unit's
    /// destinations, one destination's targets, or the route a wire command
    /// needs, so asking all three costs one search rather than three.
    ///
    /// `None` means the reducer would not let this index move at all, which is
    /// an answer and not a fault.
    fn with_field<'a, R>(
        &'a self,
        turn: &ActiveTurn<'a>,
        unit: UnitIdx,
        read: impl FnOnce(&PreparedMoveField<'a, &TurnMaps<'a>>) -> R,
    ) -> Option<R> {
        let subject = self.state.units.at(usize::from(unit.get()))?;
        let mut reach = self.reach.borrow_mut();
        let epoch = self.epoch;
        let held = match reach.held.take() {
            Some((index, at, geometry)) if index == unit && at == epoch => Some(geometry),
            // Another unit's, or another position's. The next search wants
            // the board-sized half of it. The rest is stale.
            Some((_, _, geometry)) => {
                geometry.recycle(&mut reach.scratch);
                None
            }
            None => None,
        };
        let field = match held {
            Some(geometry) => {
                let active = turn.unit(subject.id).ok()?.ok()?;
                PreparedMoveField::from_parts(active, geometry, turn.maps())
            }
            None => turn
                .move_field(subject.id, &mut reach.scratch)
                .ok()
                .flatten()?,
        };
        // The read can use action scratch, which lives in the other cell.
        drop(reach);
        let answer = read(&field);
        let (_, geometry) = field.into_parts();
        self.reach.borrow_mut().held = Some((unit, epoch, geometry));
        Some(answer)
    }
}

/// The wire form and the internal form.
impl Session {
    /// The wire form to the internal form. The server front door.
    ///
    /// The route a command names is checked here and then dropped. An order
    /// carries a destination, and [`Session::spell`] answers it with the
    /// field's own route. Where the route itself is evidence, as in a replay
    /// or an authority recording a match, apply the command directly through
    /// [`Session::apply_command`] instead.
    pub fn resolve(&self, command: &Command) -> Result<Order, Error> {
        let dimensions = self.state.board.dimensions();
        let cell = |position: Pos| {
            dimensions
                .cell_index(position)
                .ok_or(Error::Rejected(Violation::PathOutOfBounds {
                    index: 0,
                    position,
                }))
        };
        let arrival = |path: &[Pos]| match path.last() {
            Some(position) => cell(*position),
            None => Err(Error::Rejected(Violation::InvalidTarget { target: None })),
        };
        let index = |unit: UnitId| {
            self.unit_index(unit)
                .ok_or(Error::Rejected(Violation::UnitNotFound { unit }))
        };
        let moved = |unit: &UnitId, path: &[Pos], kind: OrderKind| {
            Ok(Order::new(index(*unit)?, arrival(path)?, kind))
        };
        match command {
            Command::MoveWait { unit, path, .. } => moved(unit, path, OrderKind::Wait),
            Command::MoveCapture { unit, path, .. } => moved(unit, path, OrderKind::Capture),
            Command::MoveSupply { unit, path, .. } => moved(unit, path, OrderKind::Supply),
            Command::MoveHide { unit, path, .. } => moved(unit, path, OrderKind::Hide),
            Command::MoveReveal { unit, path, .. } => moved(unit, path, OrderKind::Reveal),
            Command::MoveExplode { unit, path, .. } => moved(unit, path, OrderKind::Explode),
            Command::MoveJoin { unit, path, .. } => moved(unit, path, OrderKind::Join),
            Command::MoveLoad { unit, path, .. } => moved(unit, path, OrderKind::Load),
            Command::MoveRepair {
                unit, path, target, ..
            } => {
                let at =
                    self.position_of(*target)
                        .ok_or(Error::Rejected(Violation::InvalidTarget {
                            target: Some((*target).into()),
                        }))?;
                moved(unit, path, OrderKind::Repair(cell(at)?))
            }
            Command::MoveAttack {
                unit, path, target, ..
            } => {
                let at = match target {
                    AttackTarget::Tile { position } => *position,
                    AttackTarget::Unit { unit } => self.position_of(*unit).ok_or(
                        Error::Rejected(Violation::InvalidTarget {
                            target: Some((*unit).into()),
                        }),
                    )?,
                };
                moved(unit, path, OrderKind::Attack(cell(at)?))
            }
            Command::MoveLaunch {
                unit, path, target, ..
            } => moved(unit, path, OrderKind::Launch(cell(*target)?)),
            Command::DeleteUnit { unit, .. } => {
                let at = self
                    .position_of(*unit)
                    .ok_or(Error::Rejected(Violation::UnitNotOnBoard { unit: *unit }))?;
                Ok(Order::new(index(*unit)?, cell(at)?, OrderKind::Delete))
            }
            Command::ProduceUnit { position, kind, .. } => {
                Ok(Order::unitless(cell(*position)?, OrderKind::Produce(*kind)))
            }
            Command::Unload {
                transport,
                cargo,
                destination,
                ..
            } => {
                let slot = self.cargo_slot(*transport, *cargo).ok_or(Error::Rejected(
                    Violation::InvalidTarget {
                        target: Some((*cargo).into()),
                    },
                ))?;
                Ok(Order::new(
                    index(*transport)?,
                    cell(*destination)?,
                    OrderKind::Unload(slot),
                ))
            }
            Command::EndTurn { .. } => Ok(Order::unitless(ORIGIN, OrderKind::EndTurn)),
            Command::Resign { .. } => Ok(Order::unitless(ORIGIN, OrderKind::Resign)),
            Command::Timeout { .. } => Ok(Order::unitless(ORIGIN, OrderKind::Timeout)),
            Command::Tag { .. } => Ok(Order::unitless(ORIGIN, OrderKind::Tag)),
            Command::ActivatePower { level, .. } => {
                Ok(Order::unitless(ORIGIN, OrderKind::Power(*level)))
            }
            Command::Unsupported => Err(Error::Failed(ExecuteError::UnsupportedCommand)),
        }
    }

    /// The internal form to the wire form. For events, replay and the protocol.
    ///
    /// `None` when the order names nothing this position holds, such as an
    /// index past the roster, a tile off the board, or a destination the unit
    /// cannot reach. The route is the movement field's own, the cheapest one
    /// to the destination and not necessarily the one a player drew.
    pub fn spell(&self, order: Order) -> Option<Command> {
        self.command_for(order).ok()
    }

    fn command_for(&self, order: Order) -> Result<Command, Error> {
        let player = self.state.turn.active_player.clone();
        let dimensions = self.state.board.dimensions();
        let reject = |violation: Violation| Error::Rejected(violation);
        let destination = dimensions
            .position_of(order.dest)
            .ok_or_else(|| reject(Violation::InvalidTarget { target: None }))?;

        match order.kind {
            OrderKind::EndTurn => return Ok(Command::EndTurn { player }),
            OrderKind::Resign => return Ok(Command::Resign { player }),
            OrderKind::Timeout => return Ok(Command::Timeout { player }),
            OrderKind::Tag => return Ok(Command::Tag { player }),
            OrderKind::Power(level) => return Ok(Command::ActivatePower { player, level }),
            OrderKind::Produce(kind) => {
                return Ok(Command::ProduceUnit {
                    player,
                    position: destination,
                    kind,
                });
            }
            _ => {}
        }

        let index = order
            .unit()
            .ok_or_else(|| reject(Violation::InvalidTarget { target: None }))?;
        let subject = self
            .state
            .units
            .at(usize::from(index.get()))
            .ok_or_else(|| reject(Violation::InvalidTarget { target: None }))?;
        let unit = subject.id;

        match order.kind {
            OrderKind::Delete => return Ok(Command::DeleteUnit { player, unit }),
            OrderKind::Unload(slot) => {
                let cargo = self
                    .cargo_in_slot(unit, slot)
                    .ok_or_else(|| reject(Violation::InvalidTarget { target: None }))?;
                return Ok(Command::Unload {
                    player,
                    transport: unit,
                    cargo,
                    destination,
                });
            }
            _ => {}
        }

        let path = self.route_to(index, unit, destination)?;
        let occupant = |position: Pos| {
            self.state
                .units
                .iter()
                .find(|candidate| candidate.location == Location::Board { position })
                .map(|candidate| candidate.id)
        };
        let target_unit = |cell: CellIdx| {
            let position = dimensions
                .position_of(cell)
                .ok_or_else(|| reject(Violation::InvalidTarget { target: None }))?;
            occupant(position).ok_or_else(|| {
                reject(Violation::InvalidTarget {
                    target: Some(position.into()),
                })
            })
        };

        Ok(match order.kind {
            OrderKind::Wait => Command::MoveWait { player, unit, path },
            OrderKind::Capture => Command::MoveCapture { player, unit, path },
            OrderKind::Supply => Command::MoveSupply { player, unit, path },
            OrderKind::Hide => Command::MoveHide { player, unit, path },
            OrderKind::Reveal => Command::MoveReveal { player, unit, path },
            OrderKind::Explode => Command::MoveExplode { player, unit, path },
            OrderKind::Join => Command::MoveJoin {
                player,
                unit,
                path,
                target: target_unit(order.dest)?,
            },
            OrderKind::Load => Command::MoveLoad {
                player,
                unit,
                path,
                transport: target_unit(order.dest)?,
            },
            OrderKind::Repair(cell) => Command::MoveRepair {
                player,
                unit,
                path,
                target: target_unit(cell)?,
            },
            OrderKind::Launch(cell) => Command::MoveLaunch {
                player,
                unit,
                path,
                target: dimensions
                    .position_of(cell)
                    .ok_or_else(|| reject(Violation::InvalidTarget { target: None }))?,
            },
            // A tile a unit stands on is attacked as that unit. Destructible
            // terrain, such as a pipe seam, admits no occupant, so the two
            // never name the same tile.
            OrderKind::Attack(cell) => {
                let position = dimensions
                    .position_of(cell)
                    .ok_or_else(|| reject(Violation::InvalidTarget { target: None }))?;
                Command::MoveAttack {
                    player,
                    unit,
                    path,
                    target: match occupant(position) {
                        Some(unit) => AttackTarget::Unit { unit },
                        None => AttackTarget::Tile { position },
                    },
                }
            }
            kind => unreachable!("{kind:?} was answered before a route was built"),
        })
    }

    /// The field's route from `index` to `destination`.
    ///
    /// A refusal here is the refusal the reducer would give, because the field
    /// is built from the rules the reducer enforces, and an unreachable tile
    /// has no route through it at all. The field is the session's own, so
    /// spelling an order the session has just offered searches nothing.
    fn route_to(&self, index: UnitIdx, unit: UnitId, destination: Pos) -> Result<Vec<Pos>, Error> {
        let turn = ActiveTurn::open(
            &self.state,
            &self.state.turn.active_player,
            self.turn_tables(),
        )?
        .map_err(Error::Rejected)?;
        turn.unit(unit)?.map_err(Error::Rejected)?;
        self.with_field(&turn, index, |field| field.path_to(destination))
            .flatten()
            .ok_or(Error::Rejected(Violation::InvalidTarget {
                target: Some(destination.into()),
            }))
    }

    /// The index a unit occupies in this position, which is how an order names
    /// it.
    ///
    /// A caller holding a [`UnitId`], such as an interface or a protocol
    /// message, comes through here once and speaks in indices after.
    pub fn index_of(&self, unit: UnitId) -> Option<UnitIdx> {
        self.unit_index(unit)
    }

    /// The unit an order in this position names, or `None` for one that moves
    /// nothing.
    pub fn unit_of(&self, order: Order) -> Option<UnitId> {
        let index = order.unit()?;
        Some(self.state.units.at(usize::from(index.get()))?.id)
    }

    /// The cargo an [`OrderKind::Unload`] order names.
    ///
    /// The order names a slot. This is the unit in it. `None` for any other
    /// kind, or when the slot is empty in this position.
    pub fn cargo_of(&self, order: Order) -> Option<UnitId> {
        let OrderKind::Unload(slot) = order.kind() else {
            return None;
        };
        self.cargo_in_slot(self.unit_of(order)?, slot)
    }

    fn unit_index(&self, unit: UnitId) -> Option<UnitIdx> {
        let index = self.state.units.index_of(unit)?;
        u16::try_from(index).ok().map(UnitIdx)
    }

    fn position_of(&self, unit: UnitId) -> Option<Pos> {
        match self.state.units.get(unit)?.location {
            Location::Board { position } => Some(position),
            Location::Cargo { .. } => None,
        }
    }

    fn cargo_slot(&self, transport: UnitId, cargo: UnitId) -> Option<u8> {
        match self.state.units.get(cargo)?.location {
            Location::Cargo {
                transport: carrier,
                slot,
            } if carrier == transport => u8::try_from(slot).ok(),
            _ => None,
        }
    }

    fn cargo_in_slot(&self, transport: UnitId, slot: u8) -> Option<UnitId> {
        self.state
            .units
            .iter()
            .find(|unit| {
                unit.location
                    == Location::Cargo {
                        transport,
                        slot: usize::from(slot),
                    }
            })
            .map(|unit| unit.id)
    }
}

/// The tile every order that names no tile points at. A board is never empty,
/// so index zero always exists.
const ORIGIN: CellIdx = CellIdx::from_raw(0);

/// What the rules allow in one position.
///
/// Every answer comes from the session's state while this value lives, and the
/// state cannot change while it does, so nothing here can go stale.
///
/// Every method that would build a list appends into a vector the caller owns.
/// None of them has an allocating twin. Enumeration is the hot path and gets
/// asked the same question thousands of times, so the buffer is the interface:
/// clear it, pass it, read it, pass it again. A caller that wants a fresh list
/// each time passes a fresh vector and pays for it.
///
/// The questions come at the three grains a caller actually has.
/// [`Legal::destinations`] answers about a whole unit in bits.
/// [`Legal::unit_orders`] names every order one unit has.
/// [`Legal::orders_at`] and [`Legal::targets`] answer about the one
/// destination a search kept.
///
/// A fault in the position, meaning a state the reducer cannot answer about at
/// all, reads here as "not legal". Enumeration is an offer, and an offer that
/// cannot be validated is not one. [`Session::apply`] is where a fault is
/// reported.
#[derive(Debug)]
pub struct Legal<'a> {
    session: &'a Session,
    /// `None` when the position admits nothing, because the match is over, the
    /// phase is not unit action, or the roster disagrees with the turn.
    turn: Option<ActiveTurn<'a>>,
}

/// One attack while its legal-enumeration context is still available.
#[derive(Debug)]
pub struct AttackCandidate<'a> {
    pub order: Order,
    pub attacker: &'a Unit,
    pub target: AttackTarget,
    pub target_unit: Option<&'a Unit>,
    pub forecast: Option<Forecast>,
}

/// A typed consumer of legal orders.
///
/// Attack visitors receive the facts that otherwise have to be recovered
/// after enumeration. Other orders carry no prepared data that current
/// consumers use.
///
/// A callback must not ask the [`Legal`] it is being visited by anything.
/// Enumeration holds that instance's search memory for as long as it runs, so
/// a query made from inside [`LegalVisitor::order`] or
/// [`LegalVisitor::attack`] panics. Collect what the visit emits, and make the
/// queries after it returns.
pub trait LegalVisitor {
    /// Whether attack forecasts and unit references are required.
    const ATTACK_CONTEXT: bool = false;

    /// Take one order. Ask the visiting [`Legal`] nothing from here.
    fn order(&mut self, order: Order);

    /// Take one attack with its context. Ask the visiting [`Legal`] nothing
    /// from here either.
    fn attack(&mut self, candidate: AttackCandidate<'_>) {
        self.order(candidate.order);
    }
}

/// The part of a legal action space that a policy wants to visit.
#[derive(Clone, Copy, Debug)]
pub struct LegalScope<'a> {
    /// Units whose orders are included.
    pub units: &'a [UnitId],
    /// Whether actions that do not belong to a unit are included.
    pub unitless: bool,
}

struct OrderCollector<'a>(&'a mut Vec<Order>);

impl LegalVisitor for OrderCollector<'_> {
    fn order(&mut self, order: Order) {
        self.0.push(order);
    }
}

struct TargetCollector<'a>(&'a mut Vec<CellIdx>);

impl LegalVisitor for TargetCollector<'_> {
    fn order(&mut self, order: Order) {
        match order.kind() {
            OrderKind::Attack(target) | OrderKind::Repair(target) | OrderKind::Launch(target) => {
                self.0.push(target);
            }
            _ => {}
        }
    }
}

impl<'a> Legal<'a> {
    /// The position these answers are about.
    pub const fn state(&self) -> &'a State {
        &self.session.state
    }

    /// Every unit that may still act this turn, appended to `out`.
    pub fn units(&self, out: &mut Vec<UnitIdx>) {
        out.extend(self.ready_units());
    }

    /// Where `unit` may stop, and what it may do on arrival, appended to `out`.
    ///
    /// A tile the unit can only pass through is not reported, and neither is
    /// one it can stop on with nothing to do there. Each mask reports whether
    /// the destination has any attack, repair or launch target, not which one.
    /// Those walks stop at their first hit here, and [`Legal::targets`] is
    /// where a caller pays to learn the rest.
    ///
    /// This is the cheap grain. A search that keeps one destination out of
    /// hundreds asks this first, then asks [`Legal::orders_at`] about the one
    /// it kept. A caller that wants every order of the unit anyway wants
    /// [`Legal::unit_orders`], which visits each destination once instead of
    /// twice.
    pub fn destinations(&self, unit: UnitIdx, out: &mut Vec<(CellIdx, OrderMask)>) {
        let Some(turn) = self.turn.as_ref() else {
            return;
        };
        let dimensions = self.session.state.board.dimensions();
        self.session.with_field(turn, unit, |field| {
            let mut routes = self.session.routes.borrow_mut();
            for (position, _) in field.geometry().reach() {
                let Some(cell) = dimensions.cell_index(position) else {
                    continue;
                };
                let Some(destination) = field.query_destination(position) else {
                    continue;
                };
                let mask = mask_at(&mut routes, &destination, position);
                if !mask.is_empty() {
                    out.push((cell, mask));
                }
            }
        });
    }

    /// Every order `unit` may give, appended to `out`.
    ///
    /// One visit to each destination answers all of it: deletion, the
    /// untargeted orders and each target list. Ask this for the unit's whole
    /// action space. Deciding the masks first and the orders after visits
    /// every destination twice.
    pub fn unit_orders(&self, unit: UnitIdx, out: &mut Vec<Order>) {
        let mut collector = OrderCollector(out);
        self.visit_unit_orders(unit, &mut collector);
    }

    fn visit_unit_orders<V: LegalVisitor>(&self, unit: UnitIdx, visitor: &mut V) {
        let Some(turn) = self.turn.as_ref() else {
            return;
        };
        let state = &self.session.state;
        let dimensions = state.board.dimensions();
        let Some(subject) = state.units.at(usize::from(unit.get())) else {
            return;
        };
        let Location::Board { position: origin } = subject.location else {
            return;
        };
        if let Some(cell) = dimensions.cell_index(origin)
            && let Ok(Ok(active)) = turn.unit(subject.id)
            && active.can_delete().unwrap_or(false)
        {
            visitor.order(Order::new(unit, cell, OrderKind::Delete));
        }
        self.session.with_field(turn, unit, |field| {
            let mut routes = self.session.routes.borrow_mut();
            let mut attack_index = std::mem::take(&mut routes.attack_index);
            field.prepare_attack_index(&mut attack_index);
            for (position, _) in field.geometry().reach() {
                let Some(cell) = dimensions.cell_index(position) else {
                    continue;
                };
                push_orders_at(
                    self,
                    &mut routes,
                    field,
                    unit,
                    cell,
                    Some(attack_index.targets(cell)),
                    visitor,
                );
            }
            routes.attack_index = attack_index;
        });
    }

    /// Every order `unit` may give that ends at `dest`, appended to `out`.
    ///
    /// This is [`Legal::unit_orders`] for the one destination a search kept.
    /// It excludes deletion, which belongs to no destination.
    pub fn orders_at(&self, unit: UnitIdx, dest: CellIdx, out: &mut Vec<Order>) {
        let Some(turn) = self.turn.as_ref() else {
            return;
        };
        self.session.with_field(turn, unit, |field| {
            let mut routes = self.session.routes.borrow_mut();
            let mut collector = OrderCollector(out);
            push_orders_at(self, &mut routes, field, unit, dest, None, &mut collector);
        });
    }

    /// Everything of one kind that `unit` may target from `dest`, appended to
    /// `out` as the tiles the targets stand on.
    ///
    /// This is the walk a mask reports as a single bit. A search that keeps
    /// one destination out of hundreds pays for the walk once.
    pub fn targets(&self, unit: UnitIdx, dest: CellIdx, kind: TargetKind, out: &mut Vec<CellIdx>) {
        let Some(turn) = self.turn.as_ref() else {
            return;
        };
        let state = &self.session.state;
        let Some(position) = state.board.dimensions().position_of(dest) else {
            return;
        };
        self.session.with_field(turn, unit, |field| {
            let mut routes = self.session.routes.borrow_mut();
            let Some(destination) = field.query_destination(position) else {
                return;
            };
            let mut collector = TargetCollector(out);
            let wrap = match kind {
                TargetKind::Attack => OrderKind::Attack as fn(CellIdx) -> OrderKind,
                TargetKind::Repair => OrderKind::Repair,
                TargetKind::Launch => OrderKind::Launch,
            };
            walk_targets(
                state,
                &mut routes,
                &destination,
                dest,
                kind,
                &mut collector,
                |cell, target| Order::new(unit, cell, wrap(target)),
            );
        });
    }

    /// The movement geometry for one unit.
    ///
    /// This is the search itself, not what may be done at the end of it. It
    /// covers every tile the unit can reach, what entering each one costs,
    /// which tiles it may come to rest on, and the routes between. An
    /// interface drawing a movement range wants all of that, and a player who
    /// traces a route of their own wants [`MoveField::route_cost`] to price
    /// it.
    ///
    /// The field is lent rather than given, because the session owns it.
    /// Asking again for the same unit in the same position reads the search
    /// that already happened. Clone it out if it has to outlive the position,
    /// and remember that a cloned field describes the position it was taken
    /// from.
    ///
    /// `None` when the reducer would not let this unit move at all.
    pub fn field<R>(&self, unit: UnitIdx, read: impl FnOnce(&MoveField) -> R) -> Option<R> {
        let turn = self.turn.as_ref()?;
        self.session
            .with_field(turn, unit, |field| read(field.geometry()))
    }

    /// Whether the reducer would accept removing this unit.
    ///
    /// Deletion belongs to no destination, so this is the one order a sweep of
    /// destinations cannot answer. [`Legal::unit_orders`] offers it too. This
    /// is for a caller that wants the answer without the sweep.
    pub fn can_delete(&self, unit: UnitIdx) -> bool {
        let Some(turn) = self.turn.as_ref() else {
            return false;
        };
        let Some(subject) = self.session.state.units.at(usize::from(unit.get())) else {
            return false;
        };
        let Ok(Ok(active)) = turn.unit(subject.id) else {
            return false;
        };
        active.can_delete().unwrap_or(false)
    }

    /// Every cargo `unit` may put down and where, appended to `out`.
    ///
    /// The same walk [`Legal::orders`] reports as [`OrderKind::Unload`],
    /// naming the cargo rather than its slot.
    pub fn unloads(&self, unit: UnitIdx, out: &mut Vec<Unload>) {
        let Some(turn) = self.turn.as_ref() else {
            return;
        };
        self.walk_unloads(
            turn,
            |candidate, _| candidate == unit,
            |unload| out.push(unload),
        );
    }

    /// What the facility on `site` may build, appended to `out`.
    ///
    /// Ordered by base cost and then by kind, the order a build menu reads in.
    /// Commander cost effects change the price rather than the ordering, so
    /// the order is stable under them.
    ///
    /// A row the player cannot afford is still a row. Everything else the
    /// reducer refuses is not: a tile that is not theirs, a banned unit, a lab
    /// unit without a lab, an occupied site, the unit limit.
    pub fn production_options(&self, site: CellIdx, out: &mut Vec<Production>) {
        let Some(turn) = self.turn.as_ref() else {
            return;
        };
        let state = &self.session.state;
        let Some(position) = state.board.dimensions().position_of(site) else {
            return;
        };
        let Ok(Ok(bound)) = turn.production_site(position) else {
            return;
        };
        rows_at(&bound, out);
    }

    /// What a strike from `from` onto `target` would do.
    ///
    /// `None` when nothing can be forecast, either because the mover cannot
    /// fire on that tile from there, or because the target's exact health is
    /// hidden from a session opened on an observation. A forecast against a
    /// guessed health would be a lie rather than an estimate.
    ///
    /// This is a preview and not an offer. Ask [`Legal::targets`] what may be
    /// fired on. This says what would happen if it were.
    pub fn forecast(&self, unit: UnitIdx, from: CellIdx, target: CellIdx) -> Option<Forecast> {
        let turn = self.turn.as_ref()?;
        if self.session.uncertain.binary_search(&target).is_ok() {
            return None;
        }
        crate::benchmark::record_forecast_calculated();
        let state = &self.session.state;
        let dimensions = state.board.dimensions();
        let (from, target) = (
            dimensions.position_of(from)?,
            dimensions.position_of(target)?,
        );
        let index = usize::from(unit.get());
        let subject = state.units.at(index)?;
        let player = state.try_player_id(subject.owner)?;
        query::forecast_at(
            state,
            turn.maps().holdings(),
            player,
            index,
            subject.id,
            from,
            target,
        )
    }

    /// Every legal order in this position, appended to `out`.
    ///
    /// This is the complete action space, boundary orders and resignation
    /// included. A rollout policy wants a draw from it rather than the whole
    /// of it. Building it is most of what a search node costs, and a playout
    /// keeps one order and discards the rest.
    pub fn orders(&self, out: &mut Vec<Order>) {
        let mut collector = OrderCollector(out);
        self.visit_orders(&mut collector);
    }

    /// Visit the complete legal action space in the same stable order as
    /// [`Legal::orders`].
    ///
    /// The visitor must not query this [`Legal`] from its callbacks. See
    /// [`LegalVisitor`].
    pub fn visit_orders<V: LegalVisitor>(&self, visitor: &mut V) {
        let Some(turn) = self.turn.as_ref() else {
            return;
        };
        self.visit_boundary_orders(visitor);
        self.visit_production_orders(turn, visitor);
        for unit in self.ready_units() {
            self.visit_unit_orders(unit, visitor);
        }
        self.visit_unload_orders(turn, visitor);
    }

    /// Visit legal orders for selected units and optional unitless actions.
    ///
    /// The unit restriction is applied before movement fields and targets are
    /// calculated. Orders keep the same relative order as [`Legal::orders`].
    /// The visitor must not query this [`Legal`] from its callbacks. See
    /// [`LegalVisitor`].
    pub fn visit_scoped<V: LegalVisitor>(&self, scope: LegalScope<'_>, visitor: &mut V) {
        let Some(turn) = self.turn.as_ref() else {
            return;
        };
        if scope.unitless {
            self.visit_boundary_orders(visitor);
            self.visit_production_orders(turn, visitor);
        }
        for unit in self.ready_units() {
            let Some(subject) = self.session.state.units.at(usize::from(unit.get())) else {
                continue;
            };
            if scope.units.contains(&subject.id) {
                self.visit_unit_orders(unit, visitor);
            }
        }
        self.visit_unload_orders_scoped(turn, scope.units, visitor);
    }

    /// Every index that may still act, without collecting them.
    ///
    /// [`Legal::units`] is this appended to a caller's vector. The loops below
    /// want it without one.
    fn ready_units(&self) -> impl Iterator<Item = UnitIdx> + '_ {
        let seat = self.turn.as_ref().map(ActiveTurn::seat);
        self.session
            .state
            .units
            .as_slice()
            .iter()
            .enumerate()
            .filter(move |(_, unit)| {
                Some(unit.owner) == seat
                    && unit.action == UnitAction::Ready
                    && matches!(unit.location, Location::Board { .. })
            })
            .filter_map(|(index, _)| u16::try_from(index).ok().map(UnitIdx))
    }

    /// End of turn, resignation, the tag swap and both power levels.
    fn visit_boundary_orders<V: LegalVisitor>(&self, visitor: &mut V) {
        let state = &self.session.state;
        let player = &state.turn.active_player;
        let candidates = [
            (
                OrderKind::EndTurn,
                Command::EndTurn {
                    player: player.clone(),
                },
            ),
            (
                OrderKind::Resign,
                Command::Resign {
                    player: player.clone(),
                },
            ),
            (
                OrderKind::Tag,
                Command::Tag {
                    player: player.clone(),
                },
            ),
            (
                OrderKind::Power(PowerLevel::Cop),
                Command::ActivatePower {
                    player: player.clone(),
                    level: PowerLevel::Cop,
                },
            ),
            (
                OrderKind::Power(PowerLevel::Scop),
                Command::ActivatePower {
                    player: player.clone(),
                    level: PowerLevel::Scop,
                },
            ),
        ];
        for (kind, command) in candidates {
            if crate::transition::accepts(state, command).unwrap_or(false) {
                visitor.order(Order::unitless(ORIGIN, kind));
            }
        }
    }

    /// What each production site the active player owns may build.
    ///
    /// The ownership test comes first because binding a site counts the
    /// player's army, and a board has far more tiles than facilities.
    fn visit_production_orders<V: LegalVisitor>(&self, turn: &ActiveTurn<'_>, visitor: &mut V) {
        let state = &self.session.state;
        let seat = turn.seat();
        let dimensions = state.board.dimensions();
        let mut rows = Vec::new();
        for position in dimensions.positions() {
            if state
                .board
                .get(position)
                .is_none_or(|tile| !tile.owner.is_owned_by(seat))
            {
                continue;
            }
            let Some(cell) = dimensions.cell_index(position) else {
                continue;
            };
            let Ok(Ok(site)) = turn.production_site(position) else {
                continue;
            };
            // The same listing a build menu reads. Stating "what this site
            // may build" twice is how the menu and the action space would come
            // to disagree.
            //
            // The action space asks the stricter question. A menu row says the
            // request is legal. An order must also be one the reducer can
            // carry out, which needs a state that can name the unit it
            // creates.
            rows.clear();
            rows_at(&site, &mut rows);
            for row in rows
                .iter()
                .filter(|row| row.affordable)
                .filter(|row| site.can_produce(row.kind).unwrap_or(false))
            {
                visitor.order(Order::unitless(cell, OrderKind::Produce(row.kind)));
            }
        }
    }

    /// Every cargo a transport of the active player may put down, and where.
    fn visit_unload_orders<V: LegalVisitor>(&self, turn: &ActiveTurn<'_>, visitor: &mut V) {
        self.walk_unloads(
            turn,
            |_, _| true,
            |unload| {
                visitor.order(Order::new(
                    unload.transport,
                    unload.destination,
                    OrderKind::Unload(unload.slot),
                ));
            },
        );
    }

    fn visit_unload_orders_scoped<V: LegalVisitor>(
        &self,
        turn: &ActiveTurn<'_>,
        units: &[UnitId],
        visitor: &mut V,
    ) {
        self.walk_unloads(
            turn,
            |_, transport| units.contains(&transport.id),
            |unload| {
                visitor.order(Order::new(
                    unload.transport,
                    unload.destination,
                    OrderKind::Unload(unload.slot),
                ));
            },
        );
    }

    /// Every unload the active player may give, named the way a menu reads
    /// them.
    ///
    /// One walk serves this and [`Legal::orders`]. An unload is a rule about a
    /// transport, its cargo and a neighbouring tile, and stating that rule
    /// twice is how the two would come to disagree.
    fn walk_unloads(
        &self,
        turn: &ActiveTurn<'_>,
        mut includes: impl FnMut(UnitIdx, &Unit) -> bool,
        mut found: impl FnMut(Unload),
    ) {
        let state = &self.session.state;
        let seat = turn.seat();
        let dimensions = state.board.dimensions();
        for (index, transport) in state.units.as_slice().iter().enumerate() {
            if transport.owner != seat {
                continue;
            }
            let Location::Board { position } = transport.location else {
                continue;
            };
            let Some(transport_index) = u16::try_from(index).ok().map(UnitIdx) else {
                continue;
            };
            if !includes(transport_index, transport) {
                continue;
            }
            let Ok(Ok(bound)) = turn.unload(transport.id) else {
                continue;
            };
            for cargo in state.units.iter() {
                let Location::Cargo {
                    transport: carrier,
                    slot,
                } = cargo.location
                else {
                    continue;
                };
                if carrier != transport.id {
                    continue;
                }
                let Ok(slot) = u8::try_from(slot) else {
                    continue;
                };
                let Ok(Ok(loaded)) = bound.cargo(cargo.id) else {
                    continue;
                };
                for target in position.orthogonal() {
                    let Some(cell) = dimensions.cell_index(target) else {
                        continue;
                    };
                    if loaded.can_unload(target).unwrap_or(false) {
                        found(Unload {
                            transport: transport_index,
                            cargo: cargo.id,
                            cargo_kind: cargo.kind,
                            slot,
                            destination: cell,
                        });
                    }
                }
            }
        }
    }
}

/// Every order that ends at one arrival tile, appended to `out`.
///
/// One preparation answers all of them. Splitting the mask from the orders
/// would prepare the destination twice, and preparing it is thousands of
/// instructions against the board sweep's few hundred.
fn push_orders_at<'a, M, V: LegalVisitor>(
    legal: &Legal<'a>,
    routes: &mut Routes,
    field: &PreparedMoveField<'a, M>,
    unit: UnitIdx,
    dest: CellIdx,
    prepared_attacks: Option<&[AttackTarget]>,
    visitor: &mut V,
) where
    M: std::borrow::Borrow<TurnMaps<'a>>,
{
    let state = legal.state();
    let Some(position) = state.board.dimensions().position_of(dest) else {
        return;
    };
    let Some(destination) = field.query_destination(position) else {
        return;
    };
    for kind in untargeted_mask(&destination, position).untargeted() {
        visitor.order(Order::new(unit, dest, kind));
    }
    // No probe first. A list walked in full says by its own length whether
    // the destination had a target of that kind, and each walk refuses early
    // for a unit that cannot do it at all.
    routes.attacks.clear();
    let attacks = match prepared_attacks {
        Some(candidates) => {
            let mut valid = true;
            for target in candidates.iter().copied() {
                crate::benchmark::record_destination_inspected();
                match destination.can_attack(target) {
                    Ok(true) => {
                        routes.attacks.push(target);
                        match target {
                            AttackTarget::Unit { .. } => {
                                crate::benchmark::record_unit_target_found();
                            }
                            AttackTarget::Tile { .. } => {
                                crate::benchmark::record_tile_target_found();
                            }
                        }
                    }
                    Ok(false) => {}
                    Err(_) => {
                        valid = false;
                        break;
                    }
                }
            }
            valid
        }
        None => query::attack_targets_into::<_, { usize::MAX }>(
            &destination,
            &mut routes.attacks,
            &mut routes.attack_units,
            &mut routes.attack_tiles,
        )
        .is_ok(),
    };
    if attacks {
        for target in routes.attacks.drain(..) {
            let Some(target_cell) = target_cell(state, target) else {
                continue;
            };
            let order = Order::new(unit, dest, OrderKind::Attack(target_cell));
            if V::ATTACK_CONTEXT {
                let attacker = state.units.at(usize::from(unit.get()));
                let target_unit = match target {
                    AttackTarget::Unit { unit } => state.units.get(unit),
                    AttackTarget::Tile { .. } => None,
                };
                if let Some(attacker) = attacker {
                    visitor.attack(AttackCandidate {
                        order,
                        attacker,
                        target,
                        target_unit,
                        forecast: legal.forecast(unit, dest, target_cell),
                    });
                }
            } else {
                visitor.order(order);
            }
        }
    }
    for (kind, wrap) in [
        (
            TargetKind::Repair,
            OrderKind::Repair as fn(CellIdx) -> OrderKind,
        ),
        (TargetKind::Launch, OrderKind::Launch),
    ] {
        walk_targets(
            state,
            routes,
            &destination,
            dest,
            kind,
            visitor,
            |cell, target| Order::new(unit, cell, wrap(target)),
        );
    }
}

/// Walk one target list of a prepared destination, appending what `name` makes
/// of each target.
///
/// The walk fills a pooled buffer with whatever the reducer reports, a unit or
/// a tile, and this is where that becomes the tile an order names. A fault
/// reads as no targets. Enumeration is an offer, and an offer that cannot be
/// validated is not one.
fn walk_targets<'a, M, V: LegalVisitor>(
    state: &State,
    routes: &mut Routes,
    destination: &PreparedDestination<'a, M>,
    dest: CellIdx,
    kind: TargetKind,
    visitor: &mut V,
    name: impl Fn(CellIdx, CellIdx) -> Order,
) where
    M: std::borrow::Borrow<TurnMaps<'a>>,
{
    let dimensions = state.board.dimensions();
    match kind {
        TargetKind::Attack => {
            routes.attacks.clear();
            if query::attack_targets_into::<M, { usize::MAX }>(
                destination,
                &mut routes.attacks,
                &mut routes.attack_units,
                &mut routes.attack_tiles,
            )
            .is_err()
            {
                return;
            }
            for order in routes
                .attacks
                .drain(..)
                .filter_map(|target| target_cell(state, target))
                .map(|cell| name(dest, cell))
            {
                visitor.order(order);
            }
        }
        TargetKind::Repair => {
            routes.repairs.clear();
            if query::repair_targets_into::<M, { usize::MAX }>(destination, &mut routes.repairs)
                .is_err()
            {
                return;
            }
            for order in routes
                .repairs
                .drain(..)
                .filter_map(|unit| target_cell(state, AttackTarget::Unit { unit }))
                .map(|cell| name(dest, cell))
            {
                visitor.order(order);
            }
        }
        TargetKind::Launch => {
            routes.launches.clear();
            if query::launch_targets_into::<M, { usize::MAX }>(destination, &mut routes.launches)
                .is_err()
            {
                return;
            }
            for order in routes
                .launches
                .drain(..)
                .filter_map(|position| dimensions.cell_index(position))
                .map(|cell| name(dest, cell))
            {
                visitor.order(order);
            }
        }
    }
}

/// The tile a target stands on, which is how an order names it.
fn target_cell(state: &State, target: AttackTarget) -> Option<CellIdx> {
    let position = match target {
        AttackTarget::Tile { position } => position,
        AttackTarget::Unit { unit } => match state.units.get(unit)?.location {
            Location::Board { position } => position,
            Location::Cargo { .. } => return None,
        },
    };
    state.board.dimensions().cell_index(position)
}

/// What one bound site may build, appended to `out`, in menu order.
///
/// The one statement of the rule. [`Legal::production_options`] hands it to a
/// menu, and [`Legal::orders`] narrows it to the rows a search may take.
fn rows_at(site: &PreparedProductionSite<'_>, out: &mut Vec<Production>) {
    let start = out.len();
    for kind in UnitKindId::ALL.iter().copied() {
        // The price comes back with the answer, so nothing here restates what
        // a commander charges.
        let (cost, affordable) = match site.produce_cost(kind) {
            Ok(Ok(cost)) => (cost, true),
            Ok(Err(Violation::InsufficientFunds { required, .. })) => (required, false),
            _ => continue,
        };
        out.push(Production {
            kind,
            cost,
            affordable,
        });
    }
    // Base cost, then the identifier, so two units priced the same always
    // come back in the same order. `UnitKindId::ALL` is alphabetical, which is
    // not an order any player reads a build menu in.
    out[start..].sort_by(|left, right| {
        ruleset::profile(left.kind)
            .cost
            .cmp(&ruleset::profile(right.kind).cost)
            .then(left.kind.cmp(&right.kind))
    });
}

/// The eight orders that name nothing beyond the destination itself.
fn untargeted_mask<'a, M>(destination: &PreparedDestination<'a, M>, position: Pos) -> OrderMask
where
    M: std::borrow::Borrow<TurnMaps<'a>>,
{
    let mut mask = OrderMask::default();
    mask.set(OrderMask::WAIT, destination.can_wait().unwrap_or(false));
    mask.set(
        OrderMask::CAPTURE,
        destination.can_capture().unwrap_or(false),
    );
    mask.set(OrderMask::SUPPLY, destination.can_supply().unwrap_or(false));
    mask.set(OrderMask::HIDE, destination.can_hide().unwrap_or(false));
    mask.set(OrderMask::REVEAL, destination.can_reveal().unwrap_or(false));
    mask.set(
        OrderMask::EXPLODE,
        destination.can_explode().unwrap_or(false),
    );
    if let Some(occupant) = destination.view().occupant(position) {
        mask.set(
            OrderMask::JOIN,
            destination.can_join(occupant).unwrap_or(false),
        );
        mask.set(
            OrderMask::LOAD,
            destination.can_load(occupant).unwrap_or(false),
        );
    }
    mask
}

/// Which orders a prepared destination admits, as bits.
///
/// The three targeted kinds stop their walk at the first hit. The mask says
/// only that a target exists, and [`Legal::targets`] is where a caller pays to
/// learn which. A probe fills the same buffers a full walk would, so it
/// allocates nothing either.
fn mask_at<'a, M>(
    routes: &mut Routes,
    destination: &PreparedDestination<'a, M>,
    position: Pos,
) -> OrderMask
where
    M: std::borrow::Borrow<TurnMaps<'a>>,
{
    let mut mask = untargeted_mask(destination, position);
    routes.attacks.clear();
    routes.repairs.clear();
    routes.launches.clear();
    mask.set(
        OrderMask::ATTACK,
        query::attack_targets_into::<M, 1>(
            destination,
            &mut routes.attacks,
            &mut routes.attack_units,
            &mut routes.attack_tiles,
        )
        .is_ok()
            && !routes.attacks.is_empty(),
    );
    mask.set(
        OrderMask::REPAIR,
        query::repair_targets_into::<M, 1>(destination, &mut routes.repairs).is_ok()
            && !routes.repairs.is_empty(),
    );
    mask.set(
        OrderMask::LAUNCH,
        query::launch_targets_into::<M, 1>(destination, &mut routes.launches).is_ok()
            && !routes.launches.is_empty(),
    );
    mask
}

#[cfg(test)]
mod tests {
    use super::Session;

    /// A session is the unit of ownership a parallel search wants: one per
    /// core, each holding its own position and scratch. That works only if a
    /// session can cross a thread boundary, which is why the board tables a
    /// session shares between its turns sit in [`std::sync::OnceLock`] rather
    /// than the cheaper [`std::cell::OnceCell`].
    #[test]
    fn a_session_can_be_sent_to_another_thread() {
        const fn assert_send<T: Send>() {}
        assert_send::<Session>();
    }
}
