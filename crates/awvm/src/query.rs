//! The geometry an interface draws, and the oracle that tests the session.
//!
//! [`crate::transition::execute`] answers a question the caller already knows
//! how to ask. A user interface has the opposite problem: before it can offer a
//! command it must know which commands exist here, and the only way to find out
//! from the reducer alone is to guess and be told no. So interfaces compute
//! their own move ranges — and a range computed beside the rules is a range
//! that disagrees with them, silently, wherever weather, a commander effect, or
//! a hidden blocker was left out.
//!
//! What is legal is now [`crate::session`]'s question. A consumer holding a
//! [`State`] or an [`Observation`] opens a [`crate::session::Session`] on it
//! and asks there, so the rules are stated once for both. What stays here is
//! the part that is not a rule:
//!
//! * [`MoveField`] is the movement geometry: every tile the unit reaches, what
//!   entering it costs, which tiles it may rest on, and the routes between. An
//!   interface draws a range with it, and prices a route the player traced with
//!   [`MoveField::route_cost`], because in this game the route is the player's
//!   choice and not a detail derived from the destination.
//!   [`crate::session::Legal::field`] hands one out.
//! * [`reify`] rebuilds a projection into a state the reducer can answer about.
//!   Opening a session on an observation does this once.
//! * [`ActionSet`], [`actions_at`], [`actions_for_path`] and [`by_position`]
//!   are the reference reading of what one destination allows. Nothing in the
//!   tree consumes them. `tests/session.rs` checks the session's answers
//!   against them over the whole corpus. Both walk the same reducer
//!   preparation, so the pair cannot drift on a rule. What the oracle catches
//!   is the session losing an answer while reshaping it.
//!
//! [`reachable`] is the one thing here written beside the rules rather than
//! derived from them. A probe per tile would answer whether a path is legal but
//! not produce one, and a caller needs the path to build the command, so the
//! search is written out. `tests/query.rs` holds it to `execute`'s verdict for
//! every unit and every tile in the fixture corpus, which is what keeps the
//! exception honest.
//!
//! None of this is authoritative. A server still executes the command it
//! receives; this exists so a client can offer commands the server will take.

use std::borrow::Borrow;
use std::cell::OnceCell;
use std::collections::HashSet;
use std::sync::{Arc, OnceLock};

use crate::combat::Forecast;
use crate::commander::{self, Holdings};
use crate::event::AttackTarget;
use crate::ruleset::{self, FireMode, MovementClass, TerrainTrait};
use crate::semantic::{
    AwbwView, Grid, Location, Observation, ObservedMatch, ObservedPlayer, PlayerId, PlayerIdx, Pos,
    State, Unit, UnitId, WeatherKind,
};
use crate::transition::{
    ActiveTurn, ExecuteError, PreparedActiveUnit, PreparedDestination, board_position,
    forecast_tile_attack, forecast_unit_attack, prepare_active_unit, prepare_movement,
};
use crate::violation::Violation;

/// Why a question could not be answered at all.
///
/// This is not "the command would be rejected" — that is a [`Violation`], and
/// the point of this module is to report those before they happen. These are
/// questions that do not parse against the state they were asked about.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum QueryError {
    #[error("unit {0} is not in play")]
    UnitNotFound(UnitId),
    #[error("unit {0} is cargo, so it has no board position to reason from")]
    UnitNotOnBoard(UnitId),
    #[error("unit {unit} is held by seat {}, which the roster does not have", seat.get())]
    UnknownOwner { unit: UnitId, seat: PlayerIdx },
    #[error("this observation does not describe a whole board: {0}")]
    Unprojectable(&'static str),
    #[error(transparent)]
    Transition(#[from] ExecuteError),
}

/// Why the reducer would refuse to act with this unit at all, if it would.
///
/// A greyed-out unit in an interface is a question — *why* — and this answers
/// it with the violation the reducer would produce, without needing a
/// destination or an action to ask about. `Ok(Ok(()))` means some command with
/// this unit is worth offering.
pub fn can_act(state: &State, unit: UnitId) -> Result<Result<(), Violation>, QueryError> {
    let subject = lookup(state, unit)?;
    let Location::Board { .. } = subject.location else {
        return Err(QueryError::UnitNotOnBoard(unit));
    };
    let owner = state
        .try_player_id(subject.owner)
        .ok_or(QueryError::UnknownOwner {
            unit,
            seat: subject.owner,
        })?;
    match prepare_active_unit(state, owner, unit) {
        Ok(prepared) => Ok(prepared.map(|_| ())),
        Err(_) => Err(QueryError::UnknownOwner {
            unit,
            seat: subject.owner,
        }),
    }
}

/// One tile a unit can reach, and how.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    /// Total movement points, and therefore fuel, spent arriving here.
    pub cost: u64,
    /// Whether a `move-*` command may name this as its destination. False for
    /// a teleporter, which is crossed but never held, and for a tile whose
    /// occupant is disclosed to the moving team — those stay in
    /// [`MoveField::reach`] because join, load and a moving attack reach them.
    pub can_stop: bool,
    /// The previous tile on the cheapest route here, absent at the origin.
    previous: Option<Pos>,
}

/// Everywhere a unit can go, with the path to each.
///
/// The paths are the point. A command carries the complete intended route
/// (`spec/semantics/movement.md`), not a destination, so an interface that
/// knows only *which* tiles are reachable still cannot build the command;
/// [`MoveField::path_to`] closes that gap with a route the reducer will accept.
#[derive(Clone, Debug)]
pub struct MoveField {
    unit: UnitId,
    origin: Pos,
    /// What each tile costs this unit's movement class to enter, shared with
    /// every other unit of that class this turn.
    entry: Arc<Grid<EntryCost>>,
    /// What each tile denies a mover, shared with every unit of the team.
    blocking: Arc<Grid<Blocking>>,
    /// This unit's own search result.
    arrivals: Grid<Option<Arrival>>,
    budget: u64,
}

/// The board-sized memory a repeated movement search reuses.
///
/// A search owns two things: the arrival grid, which is board-sized, and
/// Dial's buckets, one small vector per point of the allowance. Both have the
/// same shape for every unit of a turn, so a caller that hands the same
/// scratch back to each search allocates once instead of once per unit.
///
/// Nothing here survives a search. A field takes a grid out of the pool and
/// [`MoveField::recycle`] puts it back. A grid that is never given back is
/// dropped, and a caller with no pool always gets that.
#[derive(Debug, Default)]
pub(crate) struct MoveScratch {
    /// Arrival grids handed back by spent fields.
    grids: Vec<Grid<Option<Arrival>>>,
    /// Dial's buckets, cleared and resized rather than rebuilt.
    buckets: Vec<Vec<Pos>>,
}

/// The cheapest route the search found into one tile.
///
/// The search once held five board-sized maps — entry cost, two blocking
/// flags, the settled cost, and the route back — and rebuilt all five for
/// every unit. Three of them say nothing about the unit that asked, so they
/// are [`TurnMaps`] and are worked out once a turn; what is left is this, and
/// it is the only board-sized thing a search allocates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Arrival {
    /// Movement points spent getting here by that route.
    cost: u16,
    /// How many tiles the route holds, the origin included. The search knows
    /// it, and a caller walking the route back can size its vectors from it
    /// instead of growing them from nothing per destination.
    depth: u8,
    /// Whether a `move-*` command may name this tile as its destination.
    can_stop: bool,
    /// Which neighbour the route arrived from, absent at the origin.
    from: Option<Approach>,
}

/// The arrival grid holds one of these per tile, so its width is the search's
/// only board-sized allocation. Adding `depth` fit in the padding the other
/// fields already left.
const _: () = assert!(std::mem::size_of::<Option<Arrival>>() == 6);

/// Which neighbour a route arrived from.
///
/// A route is remembered as the direction it came from rather than as the
/// coordinate it came from, because the coordinate is one subtraction away and
/// the board holds one of these per tile per unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Approach {
    West,
    East,
    North,
    South,
}

impl Approach {
    /// Which neighbour of `position` `previous` is.
    fn of(position: Pos, previous: Pos) -> Self {
        if previous.x < position.x {
            Self::West
        } else if previous.x > position.x {
            Self::East
        } else if previous.y < position.y {
            Self::North
        } else {
            Self::South
        }
    }

    /// The tile a route arriving this way came from.
    fn previous(self, position: Pos) -> Pos {
        let (x, y) = (position.x, position.y);
        match self {
            Self::West => Pos::new(x - 1, y),
            Self::East => Pos::new(x + 1, y),
            Self::North => Pos::new(x, y - 1),
            Self::South => Pos::new(x, y + 1),
        }
    }
}

impl Arrival {
    /// The public reading of a settled tile.
    fn step(self, position: Pos) -> Step {
        Step {
            cost: u64::from(self.cost),
            can_stop: self.can_stop,
            previous: self.from.map(|from| from.previous(position)),
        }
    }
}

/// What a tile denies whoever moves, whoever that is.
///
/// Neither answer depends on the moving unit, apart from the tile the unit
/// itself stands on: that tile is marked as blocked here, by its own occupant.
/// A search settles its origin before it reads this table, so the reading
/// never reaches it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Blocking {
    /// Nothing may come to rest here: a unit the moving team sees stands on
    /// the tile, or it is a teleporter, which is crossed but never held.
    stop: bool,
    /// No route may pass through, though one may end here: an enemy the moving
    /// team sees stands in the way.
    route: bool,
}

/// What one tile costs to enter, absent where it cannot be entered at all.
///
/// The width is what the table holds, not a rule. A terrain cost this large is
/// orders of magnitude beyond anything AWBW defines, and one larger still is
/// held as unenterable, which against any allowance this crate can spend it
/// effectively is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct EntryCost(Option<u16>);

impl EntryCost {
    fn new(cost: Option<u64>) -> Self {
        Self(cost.and_then(|cost| u16::try_from(cost).ok()))
    }

    /// Movement points to enter, or `None` where the mover cannot.
    const fn points(self) -> Option<u16> {
        self.0
    }
}

/// The largest allowance a search can spend, and so the largest arrival cost.
const MAXIMUM_BUDGET: u64 = u16::MAX as u64;

/// The board-sized tables of one turn, held apart from the turn that built them.
///
/// [`TurnMaps`] borrows the state, so nothing that names it can outlive the
/// position. These two tables do not name it. An entry-cost map and a blocking
/// map are grids of plain cells, decided by the position but not borrowing it.
/// Lifting them out lets a caller who opens a second turn on the same position
/// pay for them once instead of twice. [`crate::session::Session`] does that,
/// once to offer an order and again to spell its route. Rebuilding these
/// tables is most of what opening a turn costs.
///
/// A handle is shared, so a table filled through one copy is visible through
/// every other. Keeping one is a promise. The tables answer for one position
/// and one seat, and a holder that reuses them against a different position
/// hands out answers about a board that is gone. [`Session`] keys its copy by
/// epoch for that reason: applying an order is the only thing that changes the
/// active seat, and it advances the epoch too, so no handle survives into
/// another position or another seat. Anything else that keeps one owes the same
/// guard.
///
/// [`Session`]: crate::session::Session
#[derive(Debug, Clone, Default)]
pub(crate) struct TurnTables {
    /// `None` when nobody outside the turn wants the tables, which is every
    /// command the reducer runs. The turn then fills the cells it carries
    /// itself and drops them with the rest of its maps, so a share nobody
    /// asked for costs no allocation.
    shared: Option<Arc<SharedTables>>,
}

/// The cells a [`TurnTables`] handle shares. [`OnceLock`] rather than the
/// cheaper [`OnceCell`] so that a session stays `Send`, because one search per
/// core is the point of a session. No table is ever filled from two threads.
#[derive(Debug, Default)]
struct SharedTables {
    blocking: OnceLock<Arc<Grid<Blocking>>>,
    /// Entry costs, one map per movement class, each built when a unit of that
    /// class first asks.
    entries: [OnceLock<Arc<Grid<EntryCost>>>; MovementClass::COUNT],
}

impl TurnTables {
    /// Whether this handle shares anything at all.
    pub(crate) const fn is_empty(&self) -> bool {
        self.shared.is_none()
    }

    /// Forget everything, so the next turn to ask rebuilds it.
    ///
    /// A holder calls this when the position it kept the tables for is gone.
    /// Every other handle keeps the old tables, so this is safe to call while
    /// a turn is open.
    pub(crate) fn clear(&mut self) {
        self.shared = Some(Arc::default());
    }
}

/// The tables every action of one turn shares.
///
/// A movement search asks two things of every tile — what it costs to enter,
/// and what it denies — and neither answer is about the unit that asked. Entry
/// cost follows the mover's movement class, its owner's commander and the
/// weather; blocking follows what the moving team can see. A turn holds dozens
/// of units drawn from eight movement classes, so a search that answered for
/// itself rebuilt the same few tables once per unit.
///
/// Every table here is bound to one `&State` and to one player of it, exactly
/// as [`AwbwView`] is bound to one team, so none of them can outlive the state
/// they describe or answer for another one.
#[derive(Debug)]
pub(crate) struct TurnMaps<'a> {
    state: &'a State,
    /// The seat these maps answer for. Entry costs follow this player's
    /// commander, so a unit of any other player must not be searched with
    /// them.
    seat: PlayerIdx,
    /// The weather this player's units move through.
    weather: WeatherKind,
    view: AwbwView<'a>,
    /// The board-sized tables, when the opener kept a handle on them.
    tables: TurnTables,
    /// The same tables when it did not. Empty cells cost nothing, so a turn
    /// nobody asks for a board table never allocates one.
    owned: SharedTables,
    /// What every player holds on the board. Commander combat rules read the
    /// tower and property counts of both sides of every strike, and scoring
    /// one attack candidate used to count them from the board twice.
    holdings: OnceCell<Holdings<'a>>,
}

impl<'a> TurnMaps<'a> {
    /// Open the tables a player's units share, for a seat a unit already
    /// names. `None` when the seat is not on the roster.
    pub(crate) fn for_seat(state: &'a State, seat: PlayerIdx) -> Option<Self> {
        Self::with_tables(state, seat, TurnTables::default())
    }

    /// The same tables again, reusing board tables the caller kept.
    ///
    /// The caller vouches that `tables` was filled against this position and
    /// this seat. See [`TurnTables`].
    pub(crate) fn with_tables(
        state: &'a State,
        seat: PlayerIdx,
        tables: TurnTables,
    ) -> Option<Self> {
        let player = state.players.get(seat.get())?;
        Some(Self {
            state,
            seat,
            weather: commander::player_weather(state, seat),
            view: AwbwView::new(state, &player.team),
            tables,
            owned: SharedTables::default(),
            holdings: OnceCell::new(),
        })
    }

    /// The cells the board tables live in: the opener's when it kept a handle,
    /// otherwise this turn's own.
    fn tables(&self) -> &SharedTables {
        self.tables.shared.as_deref().unwrap_or(&self.owned)
    }

    /// The moving team's view of the state.
    pub fn view(&self) -> &AwbwView<'a> {
        &self.view
    }

    /// What every player holds, counted once for the whole turn.
    pub(crate) fn holdings(&self) -> &Holdings<'a> {
        self.holdings.get_or_init(|| Holdings::tally(self.state))
    }

    /// What each tile denies, worked out once for the whole turn.
    fn blocking(&self) -> &Arc<Grid<Blocking>> {
        // The table asks about every tile, and every destination queried
        // through a field asks again. This sits outside the cell on purpose.
        // The table may already be built, by a turn opened earlier on this
        // same position, but the index belongs to this turn's view, and a view
        // without one answers an occupancy question by walking every unit of
        // the state for every tile.
        self.view.index_occupancy();
        self.tables().blocking.get_or_init(|| {
            Arc::new(Grid::from_fn(self.state.board.dimensions(), |position| {
                let occupied = self.view.occupant_disclosed(position);
                Blocking {
                    stop: occupied || is_teleporter(self.state, position),
                    route: occupied && self.view.occupant_obstructs(position),
                }
            }))
        })
    }

    /// What each tile costs a unit of `class` to enter.
    ///
    /// The answer depends on the mover's movement class, its owner's commander
    /// and the weather, and on nothing else about it, so one map serves every
    /// unit of a class.
    fn entry_costs(&self, class: MovementClass) -> &Arc<Grid<EntryCost>> {
        self.tables().entries[class.index()].get_or_init(|| {
            Arc::new(Grid::from_fn(self.state.board.dimensions(), |position| {
                EntryCost::new(entry_cost(
                    self.state,
                    self.seat,
                    class,
                    position,
                    self.weather,
                ))
            }))
        })
    }
}

impl MoveField {
    /// Give the arrival grid back to the pool it came from.
    ///
    /// The grid is the only board-sized thing a search owns. A caller done
    /// with a field that will search again hands it back here, and the next
    /// search writes into this allocation instead of asking for one.
    pub(crate) fn recycle(self, scratch: &mut MoveScratch) {
        scratch.grids.push(self.arrivals);
    }

    /// The unit this field was computed for.
    pub const fn unit(&self) -> UnitId {
        self.unit
    }

    /// Where it started.
    pub const fn origin(&self) -> Pos {
        self.origin
    }

    /// Movement points available: the commander-effective allowance, capped by
    /// fuel, since `spec/semantics/movement.md` spends both at the same rate.
    pub const fn budget(&self) -> u64 {
        self.budget
    }

    /// What it costs to arrive at `position`, if the unit can arrive at all.
    pub fn step(&self, position: Pos) -> Option<Step> {
        Some((*self.arrivals.get(position)?)?.step(position))
    }

    /// Whether a `move-*` command may end here.
    pub fn can_stop_at(&self, position: Pos) -> bool {
        self.arrivals
            .get(position)
            .is_some_and(|arrival| arrival.is_some_and(|arrival| arrival.can_stop))
    }

    /// Every tile the unit can end its move on, with the cost of getting there.
    ///
    /// This is the set an interface highlights.
    pub fn destinations(&self) -> impl Iterator<Item = (Pos, u64)> + '_ {
        self.arrivals.iter().filter_map(|(position, arrival)| {
            let arrival = (*arrival)?;
            arrival
                .can_stop
                .then(|| (position, u64::from(arrival.cost)))
        })
    }

    /// Every tile the unit can arrive at, including those it cannot stop on.
    ///
    /// Join, load and a moving attack all name a destination the mover does not
    /// come to rest on alone, so they are asked about against this rather than
    /// against [`MoveField::destinations`].
    pub fn reach(&self) -> impl Iterator<Item = (Pos, u64)> + '_ {
        self.arrivals
            .iter()
            .filter_map(|(position, arrival)| Some((position, u64::from((*arrival)?.cost))))
    }

    /// The route to `position`, origin first, ready to be a command's `path`.
    ///
    /// One of the cheapest routes; where several cost the same the choice is
    /// arbitrary but stable. A caller wanting a particular route may build its
    /// own — the reducer validates whatever it is sent — but this one is known
    /// to be within the movement and fuel allowance.
    pub fn path_to(&self, position: Pos) -> Option<Vec<Pos>> {
        self.step(position)?;
        let mut path = vec![position];
        let mut cursor = position;
        // Bounded by the board for the same reason as
        // `PreparedMoveField::prepare_destination`: a chain that pointed back
        // into itself would walk forever.
        let mut remaining = self.entry.dimensions().len();
        while let Some(previous) = self.step(cursor).and_then(|step| step.previous) {
            remaining = remaining.checked_sub(1)?;
            path.push(previous);
            cursor = previous;
        }
        path.reverse();
        Some(path)
    }

    /// Validate and price a caller-chosen route through this field.
    ///
    /// This preserves deliberate routes drawn by an interface without making
    /// that interface restate weather, movement, occupancy, or teleporter
    /// rules. The route must begin at this field's origin; a blocking tile may
    /// be named only as its final destination (for attack, join, or load).
    pub fn route_cost(&self, path: &[Pos]) -> Option<u64> {
        if path.first() != Some(&self.origin) {
            return None;
        }
        let mut total = 0_u64;
        for (edge_index, edge) in path.windows(2).enumerate() {
            if !edge[0].orthogonal().any(|position| position == edge[1]) {
                return None;
            }
            let is_last = edge_index + 2 == path.len();
            if self.blocking.get(edge[1])?.route && !is_last {
                return None;
            }
            total = total.checked_add(u64::from(self.entry.get(edge[1])?.points()?))?;
            if total > self.budget {
                return None;
            }
        }
        Some(total)
    }
}

/// A movement field bound to the active-unit proof that produced it.
///
/// The state borrow makes paths from this field current for as long as the
/// field exists. This lets the field prepare destinations without repeating
/// movement validation. All destinations borrow one view from the field's
/// maps.
#[derive(Debug)]
pub(crate) struct PreparedMoveField<'a, M = TurnMaps<'a>> {
    active: PreparedActiveUnit<'a>,
    field: MoveField,
    maps: M,
}

impl<'a> PreparedMoveField<'a, TurnMaps<'a>> {
    /// Compute a movement field for one prepared active unit.
    ///
    /// This form owns its maps. A caller enumerating several units of one turn
    /// wants [`ActiveTurn::move_field`], which shares the turn's maps instead
    /// of rebuilding the same board tables once per unit.
    pub fn new(
        active: PreparedActiveUnit<'a>,
        scratch: &mut MoveScratch,
    ) -> Result<Self, QueryError> {
        let state = active.state();
        let subject = lookup(state, active.unit())?;
        let maps = TurnMaps::for_seat(state, subject.owner).ok_or(QueryError::UnknownOwner {
            unit: active.unit(),
            seat: subject.owner,
        })?;
        Self::with_maps(active, maps, scratch)
    }
}

impl<'a, M> PreparedMoveField<'a, M>
where
    M: Borrow<TurnMaps<'a>>,
{
    /// Compute a movement field against maps the caller already holds.
    fn with_maps(
        active: PreparedActiveUnit<'a>,
        maps: M,
        scratch: &mut MoveScratch,
    ) -> Result<Self, QueryError> {
        let field = reachable_with(active.state(), active.unit(), maps.borrow(), scratch)?;
        Ok(Self::from_parts(active, field, maps))
    }

    /// Rebind a field that was already searched for this unit.
    ///
    /// The search is the expensive half, and the geometry it produces borrows
    /// nothing, so a caller that asks about one unit several times searches
    /// once and rebinds after. The geometry describes one `&State`, and this
    /// signature requires the new proof to come from that same state.
    pub(crate) const fn from_parts(
        active: PreparedActiveUnit<'a>,
        field: MoveField,
        maps: M,
    ) -> Self {
        Self {
            active,
            field,
            maps,
        }
    }

    /// Give the proof and the searched geometry back, for rebinding later.
    pub(crate) fn into_parts(self) -> (PreparedActiveUnit<'a>, MoveField) {
        (self.active, self.field)
    }

    /// The searched geometry this field wraps.
    pub(crate) const fn geometry(&self) -> &MoveField {
        &self.field
    }

    /// Return the field's route to `position`.
    pub fn path_to(&self, position: Pos) -> Option<Vec<Pos>> {
        self.field.path_to(position)
    }

    /// Bind one reachable destination to its prepared movement.
    ///
    /// Transit-only teleporter tiles do not produce destinations. Occupied
    /// destinations remain available for join, load, and attack queries.
    pub fn prepare_destination<'field>(
        &'field self,
        position: Pos,
    ) -> Option<PreparedDestination<'a, &'field TurnMaps<'a>>> {
        self.prepare_destination_into(position, Vec::new(), Vec::new())
    }

    /// [`PreparedMoveField::prepare_destination`], writing the route into
    /// vectors the caller supplies.
    ///
    /// Enumerating a turn walks a route for every tile a unit can reach, and
    /// these two vectors are all that walk allocates. Take them back from a
    /// spent destination with [`PreparedDestination::recycle`] and a pass
    /// allocates once instead of once per candidate.
    ///
    /// They are passed by value, not lent. A `&mut` pair costs 4% of a
    /// complete enumeration, because the walk stops reading as two independent
    /// locals and the walk is most of the pass.
    ///
    /// `None` drops them. It means the tile has no route through it, either a
    /// teleporter or a tile outside the field, and a caller reusing buffers
    /// starts the next candidate with empty ones.
    #[inline(always)]
    pub(crate) fn prepare_destination_into<'field>(
        &'field self,
        position: Pos,
        mut path: Vec<Pos>,
        mut entry_costs: Vec<u64>,
    ) -> Option<PreparedDestination<'a, &'field TurnMaps<'a>>> {
        if is_teleporter(self.active.state(), position) {
            return None;
        }
        let maps = self.maps.borrow();
        // Walk the predecessor chain once, collecting the route and what each
        // step costs together. Building the route first and pricing it after
        // grew two vectors per candidate destination, and enumeration asks
        // about every tile the unit can reach.
        let dimensions = self.field.arrivals.dimensions();
        let mut cell = dimensions.cell(position)?;
        let depth = usize::from((*self.field.arrivals.at(cell))?.depth);
        // The vectors arrive empty, so growing one is replacing it. Asking
        // `Vec::reserve` instead costs 5% of a complete enumeration, because
        // it cannot assume an empty vector and takes the amortized-growth path
        // on every candidate.
        if path.capacity() < depth {
            path = Vec::with_capacity(depth);
        }
        if entry_costs.capacity() < depth {
            entry_costs = Vec::with_capacity(depth);
        }
        // A route visits a tile at most once, so the board's tile count bounds
        // the walk. A predecessor chain that closed on itself would otherwise
        // never end.
        let mut remaining = dimensions.len();
        loop {
            remaining = remaining.checked_sub(1)?;
            let cursor = cell.position();
            let arrival = (*self.field.arrivals.at(cell))?;
            path.push(cursor);
            match arrival.from {
                Some(from) => {
                    entry_costs.push(u64::from(self.field.entry.at(cell).points()?));
                    cell = dimensions.cell(from.previous(cursor))?;
                }
                None => {
                    entry_costs.push(0);
                    break;
                }
            }
        }
        path.reverse();
        entry_costs.reverse();
        Some(
            self.active
                .movement_from_field(path, entry_costs)
                .prepare_destination_with(maps),
        )
    }

    /// Enumerate actions at one destination without validating its path again.
    pub fn actions_at(&self, position: Pos) -> Result<ActionSet, QueryError> {
        self.prepare_destination(position)
            .map_or_else(|| Ok(ActionSet::default()), actions_for_destination)
    }
}

impl<'a> ActiveTurn<'a> {
    /// The movement field for one of this turn's units.
    ///
    /// `Ok(None)` means the reducer would refuse to move the unit at all — it
    /// is not this player's, not on the board, or has already acted — which is
    /// an answer, not a fault.
    ///
    /// The field and every destination queried through it borrow the turn's
    /// maps, so a caller walking a whole turn resolves the acting team's
    /// sightings, unit positions and entry costs once rather than once per
    /// unit.
    pub fn move_field<'turn>(
        &'turn self,
        unit: UnitId,
        scratch: &mut MoveScratch,
    ) -> Result<Option<PreparedMoveField<'a, &'turn TurnMaps<'a>>>, QueryError> {
        let Ok(active) = self.unit(unit)? else {
            return Ok(None);
        };
        PreparedMoveField::with_maps(active, self.maps(), scratch).map(Some)
    }
}

/// Everywhere `unit` can move, under the rules the reducer would apply.
///
/// The geometry is computed for the unit's own owner and team, so this answers
/// for an enemy unit too — an interface showing threat ranges wants that. It is
/// silent about whether the unit may act *now*: a spent unit still has a
/// reachable set, and [`can_act`] is what says a command would be refused.
///
/// Fog cuts both ways here, deliberately. A tile held by a unit the moving team
/// cannot see stays in the field, because `spec/semantics/movement.md` keeps
/// hidden occupancy out of validation and resolves it as a trap during
/// execution instead. Removing it would leak the hidden unit.
pub fn reachable(state: &State, unit: UnitId) -> Result<MoveField, QueryError> {
    let subject = lookup(state, unit)?;
    let maps = TurnMaps::for_seat(state, subject.owner).ok_or(QueryError::UnknownOwner {
        unit,
        seat: subject.owner,
    })?;
    reachable_with(state, unit, &maps, &mut MoveScratch::default())
}

/// [`reachable`] against maps the caller already holds.
///
/// The search asks one question of every tile — what it costs to enter, and
/// what it blocks — which is exactly what the maps hold. Passing them in is
/// what stops a caller enumerating a whole turn from rebuilding the same
/// tables once per unit.
fn reachable_with(
    state: &State,
    unit: UnitId,
    maps: &TurnMaps<'_>,
    scratch: &mut MoveScratch,
) -> Result<MoveField, QueryError> {
    let subject = lookup(state, unit)?;
    let Location::Board { position: origin } = subject.location else {
        return Err(QueryError::UnitNotOnBoard(unit));
    };

    debug_assert_eq!(
        subject.owner, maps.seat,
        "a search must use the maps opened for the mover's own player"
    );

    let profile = ruleset::profile(subject.kind);
    let allowance = commander::effective_move(state, subject, profile.movement, profile.domain);
    let budget = allowance.min(subject.fuel).min(MAXIMUM_BUDGET);
    let entry = Arc::clone(maps.entry_costs(profile.movement_class));
    let blocking = Arc::clone(maps.blocking());
    let dimensions = state.board.dimensions();

    // The only board-sized thing a search owns. Everything else it needs is
    // shared with the rest of the turn. Taken from the pool when one is warm,
    // the ordinary case for a caller searching unit after unit.
    let mut arrivals = match scratch.grids.pop() {
        Some(mut grid) => {
            grid.refill(dimensions, None);
            grid
        }
        None => Grid::filled(dimensions, None),
    };
    arrivals[origin] = Some(Arrival {
        cost: 0,
        depth: 1,
        // A predeployed unit standing on a teleporter may leave but may not
        // wait in place: the tile is traversable and cannot hold a unit at
        // rest. Nothing else can block the tile the mover already stands on.
        can_stop: !is_teleporter(state, origin),
        from: None,
    });

    // Dial's algorithm uses the small integer movement allowance as its bucket
    // range. Zero-cost teleporter edges return to the current bucket and are
    // exhausted before the search advances.
    let bucket_count = usize::try_from(budget)
        .ok()
        .and_then(|budget| budget.checked_add(1))
        .expect("the ruleset movement allowance and zero bucket fit usize");
    if scratch.buckets.len() < bucket_count {
        scratch.buckets.resize_with(bucket_count, Vec::new);
    }
    let buckets = &mut scratch.buckets[..bucket_count];
    for bucket in buckets.iter_mut() {
        bucket.clear();
    }
    buckets[0].push(origin);
    for current_cost in 0..bucket_count {
        while let Some(position) = buckets[current_cost].pop() {
            let settled = arrivals[position].expect("a bucket only holds a settled tile");
            if usize::from(settled.cost) != current_cost {
                continue;
            }
            // A disclosed enemy blocks the route through its tile. Allied
            // units may be crossed but remain invalid destinations.
            if position != origin && blocking[position].route {
                continue;
            }
            for next in position.orthogonal() {
                // One coordinate, three tables: what the tile costs to enter,
                // whether it stops a route, and how the search arrived.
                let Some(cell) = dimensions.cell(next) else {
                    continue;
                };
                let Some(cost) = entry.at(cell).points() else {
                    continue;
                };
                let Some(total) = u64::from(settled.cost)
                    .checked_add(u64::from(cost))
                    .filter(|total| *total <= budget)
                else {
                    continue;
                };
                let total = total as u16;
                let arrival = arrivals.at_mut(cell);
                if arrival.is_some_and(|arrival| arrival.cost <= total) {
                    continue;
                }
                *arrival = Some(Arrival {
                    cost: total,
                    // This is a capacity hint and nothing reads it as a
                    // length: a later cheaper route to the predecessor leaves
                    // this stale, and a route longer than a byte saturates.
                    // Either way the vector it sizes still grows correctly.
                    depth: settled.depth.saturating_add(1),
                    can_stop: !blocking.at(cell).stop,
                    from: Some(Approach::of(next, position)),
                });
                buckets[usize::from(total)].push(next);
            }
        }
    }

    Ok(MoveField {
        unit,
        origin,
        entry,
        blocking,
        arrivals,
        budget,
    })
}

/// What `unit` may do if it moves to `destination`.
///
/// Every field is the reducer's own verdict on the corresponding command, so an
/// interface can enable exactly the buttons that will work. `destination` is
/// normally one of [`MoveField::reach`]; an unreachable one yields an empty
/// set, because the shared movement prefix rejects it first.
///
/// Every field uses preparation. The query does not clone or change the state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActionSet {
    /// Move there and end the unit's turn.
    pub wait: bool,
    /// Begin or continue capturing the property underfoot.
    pub capture: bool,
    /// Merge into the unit already standing there.
    pub join: bool,
    /// Board the transport already standing there.
    pub load: bool,
    /// Resupply adjacent friendly units from there.
    pub supply: bool,
    /// Enter hidden state.
    pub hide: bool,
    /// Leave hidden state.
    pub reveal: bool,
    /// Self-destruct, damaging the surrounding area.
    pub explode: bool,
    /// Everything the unit may attack from there, unit and tile alike.
    pub attack: Vec<AttackTarget>,
    /// Friendly units it may repair from there.
    pub repair: Vec<UnitId>,
    /// Tiles a missile silo underfoot may be fired at. Empty when the unit
    /// cannot launch, which is the common case.
    pub launch: Vec<Pos>,
}

impl ActionSet {
    /// Whether any command at all is available at this destination.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Whether the recipient is the one on turn, in the phase where units take
/// orders, in a match that is still running under a ruleset this crate models.
///
/// Every observed-side query answers with nothing when this is false: an
/// observation the recipient cannot act on offers no commands at all.
pub(crate) fn recipient_may_command(observation: &Observation) -> bool {
    ruleset::supports(&observation.ruleset)
        && observation.turn.active_player == observation.recipient
        && observation.turn.phase == crate::semantic::Phase::UnitAction
        && matches!(observation.match_state, ObservedMatch::Active { .. })
}

/// One target's forecast, dispatched on what is standing there.
pub(crate) fn forecast_at(
    state: &State,
    holdings: &Holdings<'_>,
    player: &PlayerId,
    index: usize,
    unit: UnitId,
    from: Pos,
    target: Pos,
) -> Option<Forecast> {
    let occupant = state
        .units
        .iter()
        .find(|candidate| candidate.id != unit && board_position(candidate) == Some(target));
    match occupant {
        Some(defender) => {
            forecast_unit_attack(state, holdings, player, index, from, defender.id).ok()
        }
        None => forecast_tile_attack(state, holdings, player, &state.units[index], from, target)
            .ok()
            .flatten(),
    }
}

/// An [`ActionSet`] whose targets are named by the tile they stand on.
///
/// A projection carries no identifier for a unit its holder cannot see, so a
/// target it may legally fire on has no id to name it by. The tile always
/// exists, so this shape and [`crate::session::Order`] both name a target that
/// way.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObservedActionSet {
    pub wait: bool,
    pub capture: bool,
    pub join: bool,
    pub load: bool,
    pub supply: bool,
    pub hide: bool,
    pub reveal: bool,
    pub explode: bool,
    pub attack: Vec<Pos>,
    pub repair: Vec<Pos>,
    pub launch: Vec<Pos>,
}

impl ObservedActionSet {
    /// Whether any command at all is available at this destination.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Restate an [`ActionSet`]'s targets as the positions its units occupy.
///
/// A unit named by an action is on the board by construction — the reducer
/// accepted a command against it — so an id that resolves to nothing is
/// dropped rather than guessed at.
///
/// Targets come back in map order. The reducer reports them in unit order,
/// which a projection is free to permute, and a menu whose entries move between
/// two readings of the same board is a menu that cannot be trusted.
pub fn by_position(state: &State, actions: ActionSet) -> ObservedActionSet {
    let ActionSet {
        wait,
        capture,
        join,
        load,
        supply,
        hide,
        reveal,
        explode,
        attack,
        repair,
        launch,
    } = actions;
    let position_of = |unit: UnitId| match state.units.get(unit).map(|unit| &unit.location) {
        Some(Location::Board { position }) => Some(*position),
        _ => None,
    };

    let in_map_order = |mut positions: Vec<Pos>| {
        positions.sort();
        positions.dedup();
        positions
    };

    ObservedActionSet {
        wait,
        capture,
        join,
        load,
        supply,
        hide,
        reveal,
        explode,
        attack: in_map_order(
            attack
                .into_iter()
                .filter_map(|target| match target {
                    AttackTarget::Unit { unit } => position_of(unit),
                    AttackTarget::Tile { position } => Some(position),
                })
                .collect(),
        ),
        repair: in_map_order(repair.into_iter().filter_map(position_of).collect()),
        launch: in_map_order(launch),
    }
}

/// Rebuild a provisional [`State`] from one recipient's [`Observation`].
///
/// Every censored fact is replaced by the reading that cannot invent a
/// capability: an opponent's treasury becomes zero, because funds only ever
/// unlock commands, and an enemy the projection did not report simply is not
/// there. Enemy units carry no identity in a projection, so they are given
/// synthetic ids above every real one, which keeps them distinguishable to the
/// reducer without colliding with a friendly unit's id.
pub fn reify(observation: &Observation) -> Result<State, QueryError> {
    // The roster is built first: the board's tiles name a seat in it.
    let players =
        crate::semantic::Roster::new(observation.players.iter().map(reified_player).collect())
            .map_err(|_| {
                QueryError::Unprojectable("its roster holds more players than a seat can name")
            })?;
    let mut board = crate::semantic::Board::new(
        observation.board.width(),
        observation.board.height(),
        board_tiles(observation, &players)?,
    )
    .map_err(|_| QueryError::Unprojectable("its board is not a whole rectangle"))?;
    board.set_rare_states(
        observation
            .board
            .iter()
            .filter_map(|(position, observed)| {
                let state = crate::semantic::RareTileState {
                    destructible_hp: observed.destructible_hp(),
                    teleporter: observed.teleporter().cloned(),
                    // A projection carries no trait state, so a reified board
                    // has none either.
                    trait_state: None,
                };
                (!state.is_empty()).then_some((position, state))
            })
            .collect(),
    );

    let units = crate::semantic::UnitStore::new(reified_units(observation, &players)?)
        .map_err(|_| QueryError::Unprojectable("it names one unit twice"))?;

    // An observation carries no identifier counter, and production is
    // inadmissible without one (`spec/semantics/production.md`), so a
    // projection with no counter offers no build at all. One past the highest
    // reified unit satisfies the freshness the state invariant asks for
    // (`spec/model/state.md`), which is enough for the projection to answer
    // what a player may build. It is a guess, like every enemy identifier a
    // projection holds: the identifier the produced unit really gets comes
    // from the authoritative state when the command executes there.
    let next_unit_id = units
        .iter()
        .map(|unit| unit.id.get())
        .max()
        .map_or(Some(1), |highest| highest.checked_add(1));

    Ok(State {
        ruleset: observation.ruleset.clone(),
        settings: observation.settings.clone(),
        board,
        teams: observation.teams.clone(),
        players,
        turn: observation.turn.clone(),
        weather: observation.weather.clone(),
        units,
        next_unit_id,
        match_state: match &observation.match_state {
            ObservedMatch::Active { own_team_offers } => crate::semantic::Match::Active {
                draw_offers: own_team_offers.clone(),
            },
            ObservedMatch::Finished { outcome } => crate::semantic::Match::Finished {
                outcome: outcome.clone(),
            },
        },
    })
}

fn board_tiles(
    observation: &Observation,
    players: &crate::semantic::Roster,
) -> Result<Vec<crate::semantic::Tile>, QueryError> {
    let mut tiles = Vec::with_capacity(
        usize::from(observation.board.width()) * usize::from(observation.board.height()),
    );
    for y in 0..observation.board.height() {
        for x in 0..observation.board.width() {
            let observed = observation.board.tile(Pos::new(x, y));
            let mut tile = crate::semantic::Tile::new(observed.terrain);
            // The projection names its holder; the state it is reified into
            // stores the seat that name sits in.
            tile.owner = match observed.owner.player() {
                Some(name) => {
                    let seat = players.seat(name).ok_or(QueryError::Unprojectable(
                        "a tile names a holder its roster does not hold",
                    ))?;
                    crate::semantic::TileOwner::Owned(seat)
                }
                None if observed.owner.is_ownable() => crate::semantic::TileOwner::Neutral,
                None => crate::semantic::TileOwner::NotOwnable,
            };
            tile.capture_points = observed.capture_points;
            tile.silo = observed.silo;
            tiles.push(tile);
        }
    }
    Ok(tiles)
}

/// An opponent's private state is unknown, so it is filled with the reading
/// that grants nothing: no funds, and their powers described only as far as the
/// projection describes them.
fn reified_player(player: &ObservedPlayer) -> crate::semantic::Player {
    match player {
        ObservedPlayer::Private {
            id,
            team,
            funds,
            status,
            commanders,
            power_state,
            ..
        } => crate::semantic::Player::new(id.clone(), team.clone())
            .with_funds(*funds)
            .with_status(*status)
            .with_commanders(commanders.clone())
            .with_power_state(power_state.clone()),
        ObservedPlayer::Public {
            id,
            team,
            status,
            commanders,
            power_state,
            ..
        } => crate::semantic::Player::new(id.clone(), team.clone())
            .with_status(*status)
            .with_commanders(
                commanders
                    .iter()
                    .map(|commander| crate::semantic::Commander {
                        id: commander.id,
                        active: commander.active,
                        power_charge: commander.power_charge,
                        power_uses: commander.power_uses,
                    })
                    .collect(),
            )
            .with_power_state(power_state.clone()),
    }
}

fn reified_units(
    observation: &Observation,
    players: &crate::semantic::Roster,
) -> Result<Vec<Unit>, QueryError> {
    let mut next_synthetic = observation
        .units
        .iter()
        .filter_map(|unit| match unit.reference {
            crate::semantic::ObservedUnitRef::Friendly { unit } => Some(unit.get()),
            crate::semantic::ObservedUnitRef::Enemy { .. } => None,
        })
        .max()
        .map_or(1, |highest| highest.saturating_add(1));

    let mut ids = Vec::with_capacity(observation.units.len());
    let mut known_ids = HashSet::with_capacity(observation.units.len());
    for observed in &observation.units {
        let id = match observed.reference {
            crate::semantic::ObservedUnitRef::Friendly { unit } => unit,
            crate::semantic::ObservedUnitRef::Enemy { .. } => {
                let synthetic = UnitId::new(next_synthetic);
                next_synthetic = next_synthetic.saturating_add(1);
                synthetic
            }
        };
        ids.push(id);
        known_ids.insert(id);
    }

    observation
        .units
        .iter()
        .zip(&ids)
        .filter(|(observed, _)| {
            // A projection never names an enemy transport, so an enemy's cargo
            // has no id to be held by. Dropping it is the conservative reading:
            // cargo influences no command issued from outside its transport.
            match observed.location {
                Location::Cargo { transport, .. } => known_ids.contains(&transport),
                Location::Board { .. } => true,
            }
        })
        .map(|(observed, id)| {
            let owner = players
                .seat(&observed.owner)
                .ok_or(QueryError::Unprojectable(
                    "a unit names an owner its roster does not hold",
                ))?;
            Ok(Unit {
                id: *id,
                kind: observed.kind,
                owner,
                // Hidden enemy HP does not affect movement or whether it can be
                // targeted. Forecasts for these synthetic values are suppressed.
                hp: observed.hp.exact().unwrap_or(100),
                fuel: observed.fuel,
                ammo: observed.ammo,
                action: observed.action,
                concealment: observed.concealment,
                location: observed.location,
            })
        })
        .collect()
}

/// Enumerate every command `unit` could issue ending at `destination`.
///
/// Call [`actions_for_path`] when the caller already has a [`MoveField`]. This
/// convenience form computes a field to obtain the path.
pub fn actions_at(state: &State, unit: UnitId, destination: Pos) -> Result<ActionSet, QueryError> {
    prepared_move_field(state, unit)?.map_or_else(
        || Ok(ActionSet::default()),
        |field| field.actions_at(destination),
    )
}

fn prepared_move_field(
    state: &State,
    unit: UnitId,
) -> Result<Option<PreparedMoveField<'_>>, QueryError> {
    let subject = lookup(state, unit)?;
    if !matches!(subject.location, Location::Board { .. }) {
        return Err(QueryError::UnitNotOnBoard(unit));
    }
    let owner = state
        .try_player_id(subject.owner)
        .ok_or(QueryError::UnknownOwner {
            unit,
            seat: subject.owner,
        })?;
    let Ok(active) = prepare_active_unit(state, owner, unit)? else {
        return Ok(None);
    };
    // A one-shot caller keeps no pool, so the search allocates and frees the
    // one grid it needs. Repeated searching wants `MoveScratch`.
    PreparedMoveField::new(active, &mut MoveScratch::default()).map(Some)
}

/// Enumerate actions for a path without computing a movement field.
///
/// The path is validated against `state` before any action is offered. This
/// makes a path from an older movement field safe to submit: a state change
/// produces an empty set instead of bypassing current movement rules.
pub fn actions_for_path(
    state: &State,
    unit: UnitId,
    path: Vec<Pos>,
) -> Result<ActionSet, QueryError> {
    let subject = lookup(state, unit)?;
    if !matches!(subject.location, Location::Board { .. }) {
        return Err(QueryError::UnitNotOnBoard(unit));
    }
    let player = state
        .try_player_id(subject.owner)
        .ok_or(QueryError::UnknownOwner {
            unit,
            seat: subject.owner,
        })?
        .clone();
    let Ok(movement) = prepare_movement(state, &player, unit, path)? else {
        return Ok(ActionSet::default());
    };
    actions_for_movement(movement)
}

fn actions_for_movement(
    movement: crate::transition::PreparedMovement<'_>,
) -> Result<ActionSet, QueryError> {
    actions_for_destination(movement.prepare_destination())
}

fn actions_for_destination<'a, M>(
    destination: PreparedDestination<'a, M>,
) -> Result<ActionSet, QueryError>
where
    M: Borrow<TurnMaps<'a>>,
{
    let movement = destination.movement();
    let position = movement.plan().destination();
    let occupant = destination.view().occupant(position);
    Ok(ActionSet {
        wait: destination.can_wait()?,
        capture: destination.can_capture()?,
        supply: destination.can_supply()?,
        hide: destination.can_hide()?,
        reveal: destination.can_reveal()?,
        explode: destination.can_explode()?,
        join: match occupant {
            Some(target) => destination.can_join(target)?,
            None => false,
        },
        load: match occupant {
            Some(transport) => destination.can_load(transport)?,
            None => false,
        },
        attack: {
            let mut targets = Vec::new();
            let (mut units, mut tiles) = (Vec::new(), Vec::new());
            attack_targets_into::<_, { usize::MAX }>(
                &destination,
                &mut targets,
                &mut units,
                &mut tiles,
            )?;
            targets
        },
        repair: {
            let mut targets = Vec::new();
            repair_targets_into::<_, { usize::MAX }>(&destination, &mut targets)?;
            targets
        },
        launch: {
            let mut targets = Vec::new();
            launch_targets_into::<_, { usize::MAX }>(&destination, &mut targets)?;
            targets
        },
    })
}

/// Everything the mover may fire on from here, appended to `out`.
///
/// The walk stops after `LIMIT` targets. A caller that only needs to know
/// whether the destination admits an attack asks for one and stops the range
/// walk at the first hit. A caller that wants the list asks for
/// `{ usize::MAX }`. Enumeration asks for the bit hundreds of times per list,
/// so the two are one function and not two.
///
/// The limit is a constant rather than an argument so that the unlimited walk,
/// which is every complete enumeration, compiles to what it did before the
/// limited one existed. Passed as a value it cost 4% of a turn.
///
/// This appends to `out` and never clears it. A caller collecting several
/// kinds into one buffer wants that, and a caller that wants only this walk's
/// answer reads from the length it noted before the call.
///
/// The walk sorts the units it finds, so it needs somewhere to hold them until
/// it has them all. `units` and `tiles` are that scratch: they are cleared on
/// entry and left full on exit, so a caller that asks once per destination
/// lends the same two buffers to every call and pays for them once.
pub(crate) fn attack_targets_into<'a, M, const LIMIT: usize>(
    destination: &PreparedDestination<'a, M>,
    out: &mut Vec<AttackTarget>,
    units: &mut Vec<UnitId>,
    tiles: &mut Vec<Pos>,
) -> Result<(), QueryError>
where
    M: Borrow<TurnMaps<'a>>,
{
    let movement = destination.movement();
    let state = movement.state();
    let unit = movement.unit();
    let subject = lookup(state, unit)?;
    let profile = ruleset::profile(subject.kind);
    if profile.fire_mode == FireMode::None {
        return Ok(());
    }
    let from = movement.plan().destination();

    // Range bounds the search; everything else is the reducer's to decide.
    let (minimum, maximum) = match profile.indirect_range {
        Some(range) => (
            range.minimum,
            commander::effective_attack_range(
                state,
                subject,
                range.maximum,
                profile.domain,
                FireMode::Indirect,
            ),
        ),
        None => (1, 1),
    };
    let in_range = |position: Pos| {
        let distance = from.distance(position);
        distance >= minimum && distance <= maximum
    };
    // Walk the tiles the range covers, not the roster: the occupancy index
    // names whoever stands on each one, so the cost follows the weapon range
    // instead of the size of the army.
    let radius = u8::try_from(maximum).unwrap_or(u8::MAX);
    let minimum_x = from.x.saturating_sub(radius);
    let maximum_x = from.x.saturating_add(radius).min(state.board.width() - 1);
    let minimum_y = from.y.saturating_sub(radius);
    let maximum_y = from.y.saturating_add(radius).min(state.board.height() - 1);
    units.clear();
    tiles.clear();
    let dimensions = state.board.dimensions();
    'walk: for y in minimum_y..=maximum_y {
        for x in minimum_x..=maximum_x {
            let position = Pos::new(x, y);
            if !in_range(position) {
                continue;
            }
            // The box is clamped to the board, so every tile in it has a cell.
            // Both questions below are about that one tile, so the coordinate
            // is resolved once and each table read with the answer.
            let Some(cell) = dimensions.cell(position) else {
                continue;
            };
            // The index names the occupant whether or not this team sees it,
            // which is what the roster walk did; `can_attack` refuses the
            // ones the team may not fire at.
            if let Some(candidate) = destination.view().occupant_at(cell)
                && candidate != unit
                && destination.can_attack(AttackTarget::Unit { unit: candidate })?
            {
                units.push(candidate);
                if units.len() + tiles.len() >= LIMIT {
                    break 'walk;
                }
            }
            if ruleset::terrain(state.board.at(cell).terrain)
                .destructible
                .is_some()
                && destination.can_attack(AttackTarget::Tile { position })?
            {
                tiles.push(position);
                if units.len() + tiles.len() >= LIMIT {
                    break 'walk;
                }
            }
        }
    }
    // The walk finds units in board order; report them by id, so the list does
    // not depend on where the mover stopped.
    units.sort_unstable();
    out.extend(units.iter().map(|unit| AttackTarget::Unit { unit: *unit }));
    out.extend(tiles.iter().map(|position| AttackTarget::Tile {
        position: *position,
    }));
    Ok(())
}

/// Friendly units the mover may repair from here, appended to `out`.
///
/// [`attack_targets_into`] explains `LIMIT` and the append.
pub(crate) fn repair_targets_into<'a, M, const LIMIT: usize>(
    destination: &PreparedDestination<'a, M>,
    out: &mut Vec<UnitId>,
) -> Result<(), QueryError>
where
    M: Borrow<TurnMaps<'a>>,
{
    let movement = destination.movement();
    let state = movement.state();
    let unit = movement.unit();
    let Some(repair) = state
        .units
        .get(unit)
        .and_then(|unit| ruleset::profile(unit.kind).repair)
    else {
        return Ok(());
    };
    if repair.relation != ruleset::Relation::Adjacent {
        return Ok(());
    }
    let from = movement.plan().destination();
    let found = out.len();
    for candidate in from
        .orthogonal()
        .filter_map(|position| destination.view().occupant(position))
        .filter(|candidate| *candidate != unit)
    {
        if destination.can_repair(candidate)? {
            out.push(candidate);
            if out.len() - found >= LIMIT {
                break;
            }
        }
    }
    Ok(())
}

/// Every tile a silo under the mover may be fired at, appended to `out`.
///
/// [`attack_targets_into`] explains `LIMIT` and the append.
pub(crate) fn launch_targets_into<'a, M, const LIMIT: usize>(
    destination: &PreparedDestination<'a, M>,
    out: &mut Vec<Pos>,
) -> Result<(), QueryError>
where
    M: Borrow<TurnMaps<'a>>,
{
    let movement = destination.movement();
    let state = movement.state();
    let position = movement.plan().destination();
    // Launching is rare and the scan is over the whole board, so refuse early
    // unless the tile underfoot actually carries a silo.
    if state
        .board
        .get(position)
        .is_none_or(|tile| tile.silo.is_none())
    {
        return Ok(());
    }
    let found = out.len();
    for target in state.board.positions() {
        if destination.can_launch(target)? {
            out.push(target);
            if out.len() - found >= LIMIT {
                break;
            }
        }
    }
    Ok(())
}

/// What entering `position` costs this unit, or `None` when it cannot.
fn entry_cost(
    state: &State,
    owner: PlayerIdx,
    class: MovementClass,
    position: Pos,
    weather: WeatherKind,
) -> Option<u64> {
    let terrain = state.board.tile(position).terrain;
    let base = ruleset::movement_cost(terrain, weather, class);
    // A teleporter's zero is terrain behaviour, not a finite cost for the
    // commander cost-set operators to replace (`spec/semantics/movement.md`).
    if ruleset::terrain_has(terrain, TerrainTrait::Teleporter) {
        base
    } else {
        commander::player_movement_cost(state, owner, base)
    }
}

fn is_teleporter(state: &State, position: Pos) -> bool {
    ruleset::terrain_has(state.board.tile(position).terrain, TerrainTrait::Teleporter)
}

fn lookup(state: &State, unit: UnitId) -> Result<&Unit, QueryError> {
    state.units.get(unit).ok_or(QueryError::UnitNotFound(unit))
}
