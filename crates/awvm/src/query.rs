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

use crate::semantic;
use std::borrow::Borrow;
use std::cell::OnceCell;
use std::collections::HashSet;
use std::sync::{Arc, OnceLock};

use crate::combat::{self, Forecast};
use crate::commander::{self, Holdings};
use crate::event::AttackTarget;
use crate::ruleset::{self, CommanderKind, FireMode, MovementClass, TerrainTrait};
use crate::semantic::{
    AwbwView, Grid, Location, Observation, ObservedMatch, ObservedPlayer, PlayerId, PlayerIdx, Pos,
    PowerState, State, TerrainId, Unit, UnitId, Viewpoint, WeatherKind,
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
    #[error("unit {unit} is held by seat {}, but these maps answer for seat {}", owner.get(), seat.get())]
    WrongSeat {
        unit: UnitId,
        owner: PlayerIdx,
        seat: PlayerIdx,
    },
    #[error("this observation does not describe a whole board: {0}")]
    Unprojectable(&'static str),
    #[error(transparent)]
    Transition(#[from] ExecuteError),
}

/// The funds `seat` collects when its turn starts.
///
/// `spec/semantics/turn.md` names this `income(S, p)`: the count of the tiles
/// the seat owns whose terrain carries the `income` trait, times the seat's
/// effective income per property. `com-tower` and `lab` are ownable and carry
/// no such trait, so they never pay.
///
/// The turn boundary pays this, and `spec/model/phases.md` pays it once more at
/// match initialization, where the first player's day-one turn starts without a
/// boundary to run. Both read it here so the two cannot disagree.
///
/// `None` reports an income too large for the funds field, which only a
/// malformed state produces.
pub fn income(state: &State, seat: PlayerIdx) -> Option<u64> {
    let properties = state
        .board
        .tiles()
        .filter(|tile| {
            tile.owner.is_owned_by(seat) && ruleset::terrain_has(tile.terrain, TerrainTrait::Income)
        })
        .count();
    u64::try_from(properties)
        .ok()?
        .checked_mul(commander::effective_income_per_property(state, seat))
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
pub struct MoveScratch {
    /// Arrival grids handed back by spent fields.
    grids: Vec<Grid<Option<Arrival>>>,
    /// Dial's buckets, cleared and resized rather than rebuilt.
    buckets: Vec<Vec<Pos>>,
}

impl MoveScratch {
    /// An empty pool. The first search fills it.
    pub const fn new() -> Self {
        Self {
            grids: Vec::new(),
            buckets: Vec::new(),
        }
    }
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
    /// About how many tiles the route holds, the origin included. This is a
    /// capacity hint and not an exact length: a later cheaper route to the
    /// predecessor leaves it stale, and a route longer than a byte saturates.
    /// A caller walking the route back sizes its vectors from it instead of
    /// growing them from nothing per destination, and `query_destination`
    /// hands it to a summarized movement, whose only reader asks whether the
    /// route is longer than the origin alone. Anything that needs the exact
    /// length materializes the route.
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
/// every other. Blocking answers for one position and cannot cross an epoch.
/// Entry costs depend on fewer inputs and can cross an epoch through
/// [`TurnTables::advance`]. A holder must use that operation before it binds a
/// kept handle to another position.
///
/// [`Session`]: crate::session::Session
#[derive(Debug, Clone, Default)]
pub(crate) struct TurnTables {
    /// `None` when nobody outside the turn wants the tables, which is every
    /// command the reducer runs. The turn then fills the cells it carries
    /// itself and drops them with the rest of its maps, so a share nobody
    /// asked for costs no allocation.
    shared: Option<Arc<SharedTables>>,
    entries_for: Option<EntryContext>,
}

/// The cells a [`TurnTables`] handle shares. [`OnceLock`] rather than the
/// cheaper [`OnceCell`] so that a session stays `Send`, because one search per
/// core is the point of a session. No table is ever filled from two threads.
#[derive(Debug, Default)]
struct SharedTables {
    blocking: OnceLock<Arc<Grid<Blocking>>>,
    hidden_board_unit: OnceLock<bool>,
    entries: Arc<EntryTables>,
}

#[derive(Debug, Default)]
struct EntryTables {
    /// Entry costs, one map per movement class, each built when a unit of that
    /// class first asks.
    entries: [OnceLock<Arc<Grid<EntryCost>>>; MovementClass::COUNT],
}

/// The state inputs that can change a player's terrain entry costs.
#[derive(Clone, Debug, PartialEq, Eq)]
struct EntryContext {
    terrain: Vec<TerrainId>,
    weather: WeatherKind,
    commanders: Vec<(CommanderKind, bool)>,
    power: PowerState,
}

impl EntryContext {
    fn of(state: &State, seat: PlayerIdx) -> Option<Self> {
        let player = state.players.get(seat.get())?;
        Some(Self {
            terrain: state.board.iter().map(|(_, tile)| tile.terrain).collect(),
            weather: state.weather.kind,
            commanders: player
                .commanders
                .iter()
                .map(|commander| (commander.id, commander.active))
                .collect(),
            power: player.power_state.clone(),
        })
    }
}

impl TurnTables {
    /// Whether this handle shares anything at all.
    pub(crate) const fn is_empty(&self) -> bool {
        self.shared.is_none()
    }

    /// Forget the tables, so the next query rebuilds them.
    pub(crate) fn clear(&mut self) {
        self.shared = Some(Arc::default());
        self.entries_for = None;
    }

    /// Rebind the tables to a changed position.
    ///
    /// Occupancy always invalidates the blocking grid. Terrain entry costs
    /// survive when terrain, weather, and the player's movement rules did not
    /// change.
    pub(crate) fn advance(&mut self, state: &State, seat: PlayerIdx) {
        let entries_for = EntryContext::of(state, seat);
        let entries = if self.entries_for == entries_for {
            self.shared
                .as_ref()
                .map_or_else(Arc::default, |tables| Arc::clone(&tables.entries))
        } else {
            Arc::default()
        };
        self.shared = Some(Arc::new(SharedTables {
            blocking: OnceLock::new(),
            hidden_board_unit: OnceLock::new(),
            entries,
        }));
        self.entries_for = entries_for;
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
/// Each map view is bound to one `&State` and one player. The shared entry
/// grids can come from an earlier state only when [`EntryContext`] proves that
/// all inputs to those grids are unchanged.
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
    /// Visible board units in stable identifier order.
    attack_units: OnceCell<Vec<(Unit, Pos)>>,
    /// Destructible board tiles in board order.
    attack_tiles: OnceCell<Vec<(Pos, crate::ruleset::UnitKind)>>,
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
            attack_units: OnceCell::new(),
            attack_tiles: OnceCell::new(),
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

    /// Whether this view hides a unit that can interrupt a route.
    fn has_hidden_board_unit(&self) -> bool {
        *self
            .tables()
            .hidden_board_unit
            .get_or_init(|| self.view.has_hidden_board_unit())
    }

    /// What every player holds, counted once for the whole turn.
    pub(crate) fn holdings(&self) -> &Holdings<'a> {
        self.holdings.get_or_init(|| Holdings::tally(self.state))
    }

    fn attack_units(&self) -> &[(Unit, Pos)] {
        self.attack_units.get_or_init(|| {
            let mut units: Vec<_> = self
                .state
                .units
                .iter()
                .filter_map(|unit| {
                    let Location::Board { position } = unit.location else {
                        return None;
                    };
                    self.view.unit(unit).then_some((*unit, position))
                })
                .collect();
            units.sort_unstable_by_key(|(unit, _)| unit.id);
            units
        })
    }

    fn attack_tiles(&self) -> &[(Pos, crate::ruleset::UnitKind)] {
        self.attack_tiles.get_or_init(|| {
            self.state
                .board
                .dimensions()
                .positions()
                .filter_map(|position| {
                    ruleset::terrain(self.state.board.tile(position).terrain)
                        .destructible
                        .map(|destructible| (position, destructible.target_kind))
                })
                .collect()
        })
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
        self.tables().entries.entries[class.index()].get_or_init(|| {
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
    pub fn recycle(self, scratch: &mut MoveScratch) {
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

    /// Prepare plausible attack targets for every reachable firing tile.
    ///
    /// Targets drive this index. Direct weapons mark reachable neighbours of
    /// each target, while indirect weapons mark only their stationary origin.
    /// The destination validator still makes the authoritative legal decision.
    pub(crate) fn prepare_attack_index(&self, index: &mut AttackIndex) {
        let state = self.active.state();
        let dimensions = state.board.dimensions();
        index.refill(dimensions.len());
        crate::benchmark::record_attack_target_call();
        let Some(attacker) = state.units.get(self.active.unit()) else {
            return;
        };
        let profile = ruleset::profile(attacker.kind);
        if profile.fire_mode == FireMode::None {
            crate::benchmark::record_empty_target_search();
            return;
        }
        let origin = self.active.origin();
        let range = profile.indirect_range.map(|range| {
            (
                range.minimum,
                commander::effective_attack_range(
                    state,
                    attacker,
                    range.maximum,
                    profile.domain,
                    FireMode::Indirect,
                ),
            )
        });
        let mut mark = |target_position: Pos, target: AttackTarget| match range {
            Some((minimum, maximum)) => {
                let distance = origin.distance(target_position);
                if distance >= minimum
                    && distance <= maximum
                    && let Some(cell) = dimensions.cell_index(origin)
                {
                    index.push(cell, target);
                }
            }
            None => {
                for position in target_position.orthogonal() {
                    if self
                        .field
                        .arrivals
                        .get(position)
                        .is_some_and(Option::is_some)
                        && let Some(cell) = dimensions.cell_index(position)
                    {
                        index.push(cell, target);
                    }
                }
            }
        };

        let maps = self.maps.borrow();
        for (unit, position) in maps.attack_units() {
            if unit.id != attacker.id
                && combat::select_weapon(attacker.kind, unit.kind, attacker.ammo).is_some()
            {
                mark(*position, AttackTarget::Unit { unit: unit.id });
            }
        }
        for (position, target_kind) in maps.attack_tiles() {
            if combat::select_weapon(attacker.kind, *target_kind, attacker.ammo).is_some() {
                mark(
                    *position,
                    AttackTarget::Tile {
                        position: *position,
                    },
                );
            }
        }
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
    /// These vectors let a caller reuse storage when it must materialize many
    /// routes. Legal enumeration uses route summaries and does not call this
    /// operation.
    ///
    /// `None` drops them. It means that the tile has no route through it. The
    /// tile is a teleporter or is outside the field.
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

    /// Bind one destination for legal queries without building its route.
    ///
    /// The movement field already proves reachability and records path depth.
    /// Enumeration needs only that depth and the destination. Command spelling
    /// materializes the route for the selected order.
    ///
    /// [`Arrival::depth`] is a capacity hint and not an exact route length, so
    /// the summarized movement this hands back carries the same approximation.
    /// `path_len() > 1`, all the enumeration asks of it, still holds, because
    /// only the origin arrives at depth one. A caller that needs the route's
    /// true length must materialize the route.
    pub(crate) fn query_destination<'field>(
        &'field self,
        position: Pos,
    ) -> Option<PreparedDestination<'a, &'field TurnMaps<'a>>> {
        if self.maps.borrow().has_hidden_board_unit() {
            return self.prepare_destination(position);
        }
        if is_teleporter(self.active.state(), position) {
            return None;
        }
        let arrival = (*self.field.arrivals.get(position)?)?;
        Some(
            self.active
                .movement_summary(position, u16::from(arrival.depth))
                .prepare_destination_with(self.maps.borrow()),
        )
    }

    /// Enumerate actions at one destination without validating its path again.
    pub fn actions_at(&self, position: Pos) -> Result<ActionSet, QueryError> {
        self.prepare_destination(position)
            .map_or_else(|| Ok(ActionSet::default()), actions_for_destination)
    }
}

/// Plausible attack targets keyed by firing tile.
#[derive(Debug, Default)]
pub(crate) struct AttackIndex {
    rows: Vec<Vec<AttackTarget>>,
}

impl AttackIndex {
    fn refill(&mut self, cells: usize) {
        self.rows.resize_with(cells, Vec::new);
        for row in &mut self.rows {
            row.clear();
        }
    }

    fn push(&mut self, cell: crate::semantic::CellIdx, target: AttackTarget) {
        if let Some(row) = self.rows.get_mut(usize::from(cell.get())) {
            row.push(target);
        }
    }

    pub(crate) fn targets(&self, cell: crate::semantic::CellIdx) -> &[AttackTarget] {
        self.rows
            .get(usize::from(cell.get()))
            .map(Vec::as_slice)
            .unwrap_or(&[])
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
/// What one unit sees, and what stays hidden inside its reach.
///
/// [`reachable`] answers where a unit can go; this answers what it can watch.
/// The two together are what an interface needs to explain a unit without
/// restating a rule of its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisionField {
    /// Where the unit stands. Sight is measured from here.
    pub origin: Pos,
    /// How far the unit sees, after its commander, the terrain under it and
    /// the weather.
    pub sight: u64,
    /// Tiles the unit reveals, in map order. `origin` is one of them.
    pub seen: Vec<Pos>,
    /// Tiles inside the unit's reach that still conceal a ground unit, in map
    /// order.
    ///
    /// These are the tiles the unit is looking at and cannot see into, and
    /// they are the half of a vision display that a plain radius gets wrong.
    pub blind: Vec<Pos>,
}

/// The tiles `unit` sees of `state`.
///
/// Answered for the unit alone. A tile its team sees for some other reason —
/// it holds the property, the terrain is always visible, or a second unit
/// stands closer — is not reported here, because the question is what this
/// unit contributes and not what the team already knows.
///
/// Reported whether or not the match has fog. Fog decides whether sight
/// matters, not how far a unit sees, and an interface that offers to explain a
/// unit should not change its answer with a lobby setting.
pub fn vision(state: &State, unit: UnitId) -> Result<VisionField, QueryError> {
    let subject = lookup(state, unit)?;
    let sight = semantic::unit_sight(state, subject).ok_or(QueryError::UnitNotOnBoard(unit))?;
    Ok(walk_sight(state, sight))
}

/// The tiles `unit` would see standing at `from`.
///
/// The same answer as [`vision`], asked of a tile the unit has not reached
/// yet. Sight changes with the ground under a unit that climbs to see, so a
/// caller showing what a proposed move uncovers cannot take the unit's current
/// answer and slide it across the board.
///
/// The rest of the state is read as it stands. Nothing else moves with the
/// unit, which is exactly the question: what this move uncovers of the board
/// as it is now.
pub fn vision_from(state: &State, unit: UnitId, from: Pos) -> Result<VisionField, QueryError> {
    let subject = lookup(state, unit)?;
    let sight =
        semantic::unit_sight_at(state, subject, from).ok_or(QueryError::UnitNotOnBoard(unit))?;
    Ok(walk_sight(state, sight))
}

/// Walk everything one resolved sighting reaches.
fn walk_sight(state: &State, sight: semantic::UnitSight) -> VisionField {
    let mut field = VisionField {
        origin: sight.position,
        sight: sight.sight,
        seen: Vec::new(),
        blind: Vec::new(),
    };
    // The reach is a diamond, so the walk is bounded by the sight rather than
    // by the board. A Recon on a 30x30 map looks at 41 tiles, not 900.
    let radius = i16::try_from(sight.sight).unwrap_or(i16::MAX);
    for dy in -radius..=radius {
        let span = radius - dy.abs();
        for dx in -span..=span {
            let Some(position) = sight.position.offset(dx, dy) else {
                continue;
            };
            match semantic::sight_of(state, &sight, position) {
                semantic::VisionLevel::Full => field.seen.push(position),
                semantic::VisionLevel::AirOnly => field.blind.push(position),
                semantic::VisionLevel::None => {}
            }
        }
    }
    // The walk runs row by row, so both lists are already in map order.
    field
}

pub fn reachable(state: &State, unit: UnitId) -> Result<MoveField, QueryError> {
    reachable_into(state, unit, &mut MoveScratch::default())
}

/// [`reachable`], writing into memory the caller keeps.
///
/// The search allocates one board-sized grid, and a caller that asks about
/// unit after unit pays for one of those per unit. Hand the same scratch to
/// each search, and give each spent field back with [`MoveField::recycle`],
/// and the whole sweep allocates once.
///
/// The board tables are not shared this way, because they belong to one
/// player's turn and this entry point answers for any owner. A caller
/// sweeping the units of a single player wants the turn instead.
pub fn reachable_into(
    state: &State,
    unit: UnitId,
    scratch: &mut MoveScratch,
) -> Result<MoveField, QueryError> {
    let subject = lookup(state, unit)?;
    let maps = TurnMaps::for_seat(state, subject.owner).ok_or(QueryError::UnknownOwner {
        unit,
        seat: subject.owner,
    })?;
    reachable_with(state, unit, &maps, scratch)
}

/// [`reachable`] against maps the caller already holds.
///
/// The search asks one question of every tile — what it costs to enter, and
/// what it blocks — which is exactly what the maps hold. Passing them in is
/// what stops a caller enumerating a whole turn from rebuilding the same
/// tables once per unit.
pub(crate) fn reachable_with(
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
                    // A hint and not a length, as `Arrival::depth` says: a
                    // later cheaper route to the predecessor leaves this
                    // stale, and a route longer than a byte saturates. Either
                    // way the vector it sizes still grows correctly, and a
                    // tile off the origin still counts more than one.
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

/// One seat's board tables, held across a search of many of its units.
///
/// [`reachable_into`] answers about one unit and opens the tables that answer
/// it, so a caller that walks unit after unit — an influence map is the
/// example — rebuilds the same entry-cost and blocking grids once per unit,
/// and rebuilds the occupancy index under them too. A sweep opens them once
/// and lends them to every search.
///
/// The tables answer for one position and one seat. The position a sweep
/// cannot outlive, because it borrows the state. The seat it refuses at the
/// door. [`Session::sweep`] is what opens one, because the session owns the
/// epoch that says whether kept tables still describe the board.
///
/// [`Session::sweep`]: crate::session::Session::sweep
#[derive(Debug)]
pub struct Sweep<'a> {
    maps: TurnMaps<'a>,
}

impl<'a> Sweep<'a> {
    /// Lend a turn's maps to a sweep. The opener vouches for the position and
    /// the seat, exactly as [`TurnMaps::with_tables`] asks.
    pub(crate) const fn with_maps(maps: TurnMaps<'a>) -> Self {
        Self { maps }
    }

    /// The seat these tables answer for.
    pub const fn seat(&self) -> PlayerIdx {
        self.maps.seat
    }

    /// [`reachable_into`], reading the tables this sweep already holds.
    ///
    /// A unit of any other seat is refused rather than answered. Entry costs
    /// follow the owner's commander and the weather over it, so reading one
    /// seat's unit out of another seat's tables gives a wrong answer, not a
    /// slow one.
    pub fn reachable_into(
        &self,
        unit: UnitId,
        scratch: &mut MoveScratch,
    ) -> Result<MoveField, QueryError> {
        let owner = lookup(self.maps.state, unit)?.owner;
        if owner != self.maps.seat {
            return Err(QueryError::WrongSeat {
                unit,
                owner,
                seat: self.maps.seat,
            });
        }
        reachable_with(self.maps.state, unit, &self.maps, scratch)
    }
}

/// Movement points from every tile to the nearest of a set of targets.
///
/// [`reachable`] answers "where can this unit go", which is a search for each
/// unit from where it stands. This answers the other direction: "what does it
/// cost to get to one of these tiles", which is one search for a whole set of
/// targets and is then read for any number of units. A caller that scores
/// every tile of the board against every property wants this and not the
/// other one, because the properties do not move and the units do.
///
/// The answer is movement points and not turns. Turns need an allowance, and
/// an allowance is not a property of the board: a commander power changes it
/// and fuel caps it. Divide where it is read, and one table serves every unit
/// of the class.
///
/// **This ignores units in the way.** It is a table about terrain, weather and
/// the seat's commander, so it stays true for a whole turn while units move
/// around inside it. That makes it a lower bound on what a route really costs,
/// which is the same fidelity a straight-line distance already has, and it is
/// why this does not build the blocking map [`reachable`] does.
#[derive(Debug)]
pub struct Travel<'a> {
    maps: TurnMaps<'a>,
    /// The class `costs` was flattened for, so that a caller asking about one
    /// class again does not rebuild it.
    flattened: Option<MovementClass>,
    /// What each tile costs to enter, row-major, absent where it cannot be
    /// entered. The same answer as the turn's entry-cost grid, in the shape
    /// the search reads it in: a search settles every tile and looks at four
    /// neighbours of each, and asking a grid costs a bounds check and a
    /// multiply every time.
    costs: Vec<Option<u16>>,
    /// The largest entry cost on the board, which is the width of the ring
    /// below.
    widest: u16,
    /// Dial's buckets, as a ring. Every edge of this graph costs at most
    /// `widest`, so every tile still waiting sits within `widest` of the cost
    /// being settled and a ring that long holds all of them.
    ring: Vec<Vec<u16>>,
}

impl<'a> Travel<'a> {
    /// Open the tables for one seat. `None` when the roster has no such seat.
    ///
    /// Entry costs follow the seat's commander and weather, so a table opened
    /// for one player must not be read for another.
    pub fn open(state: &'a State, seat: PlayerIdx) -> Option<Self> {
        TurnMaps::for_seat(state, seat).map(Self::with_maps)
    }

    /// The same tables, read through maps the caller already holds.
    ///
    /// This is what lets a caller who searches and then measures distance pay
    /// for one entry-cost grid per movement class instead of two.
    pub(crate) const fn with_maps(maps: TurnMaps<'a>) -> Self {
        Self {
            maps,
            flattened: None,
            costs: Vec::new(),
            widest: 0,
            ring: Vec::new(),
        }
    }

    /// Flatten the turn's entry costs for one class, if they are not already.
    fn flatten(&mut self, class: MovementClass) {
        if self.flattened == Some(class) {
            return;
        }
        let entry = Arc::clone(self.maps.entry_costs(class));
        let dimensions = self.maps.state.board.dimensions();
        self.costs.clear();
        self.costs.reserve(dimensions.len());
        self.widest = 1;
        for position in dimensions.positions() {
            let points = dimensions
                .cell(position)
                .and_then(|cell| entry.at(cell).points());
            if let Some(points) = points {
                self.widest = self.widest.max(points);
            }
            self.costs.push(points);
        }
        self.ring.clear();
        self.ring
            .resize_with(usize::from(self.widest) + 1, Vec::new);
        self.flattened = Some(class);
    }

    /// The entry costs this travel reads for one class, row-major, absent
    /// where a unit of the class cannot stand.
    ///
    /// Every answer [`Travel::points_to`] gives for a class is derived from
    /// this table and the targets it is handed, and from nothing else. A
    /// caller that keeps an answer keeps this beside it: while the table and
    /// the targets are both unchanged, the answer is unchanged too.
    ///
    /// That is what makes it a safe cache key rather than a guess at one. The
    /// terrain, the weather, the seat's commander and the power it is under
    /// are all already folded in here, so a cache keyed on this cannot miss a
    /// rule it did not know to read — including one added later.
    pub fn costs(&mut self, class: MovementClass) -> &[Option<u16>] {
        self.flatten(class);
        &self.costs
    }

    /// The cheapest movement points from each tile to the nearest target.
    ///
    /// `out` is written row-major, one entry per tile, and is `None` where no
    /// route to any target exists for this movement class. A target a unit of
    /// this class cannot enter is not a target and is skipped. `allowance` is
    /// what the unit can spend in one turn. A finite entry cost above it is
    /// impassable, because movement points cannot carry between turns.
    ///
    /// The search is seeded at the targets and expands outward, which is what
    /// makes it one search rather than one for each tile. The cost of a tile
    /// is charged on **leaving** it here, not on entering it: movement charges
    /// on entry, so a route walked backward pays for the tile it came from,
    /// and charging on entry instead would answer the cost of walking *out* of
    /// a property rather than into it. The two differ by the endpoint costs.
    pub fn points_to(
        &mut self,
        class: MovementClass,
        allowance: u16,
        targets: impl IntoIterator<Item = Pos>,
        out: &mut Vec<Option<u16>>,
    ) {
        self.flatten(class);
        let dimensions = self.maps.state.board.dimensions();
        let width = usize::from(dimensions.width());
        let cells = dimensions.len();
        out.clear();
        out.resize(cells, None);
        for bucket in &mut self.ring {
            bucket.clear();
        }
        let ring = self.ring.len();
        let mut waiting = 0usize;

        for target in targets {
            let Some(index) = dimensions.cell_index(target) else {
                continue;
            };
            let index = usize::from(index.get());
            // A tile this class cannot enter cannot be walked to, so it is not
            // a target however much the caller wants it.
            if !self.costs[index].is_some_and(|cost| cost <= allowance) || out[index].is_some() {
                continue;
            }
            out[index] = Some(0);
            self.ring[0].push(index as u16);
            waiting += 1;
        }

        let mut cost = 0u16;
        while waiting > 0 {
            let bucket = usize::from(cost) % ring;
            while let Some(index) = self.ring[bucket].pop() {
                waiting -= 1;
                let index = usize::from(index);
                // A tile is settled once, at the cost it was settled with. A
                // later copy left in the ring by a cheaper route is stale.
                if out[index] != Some(cost) {
                    continue;
                }
                let Some(leaving) = self.costs[index] else {
                    continue;
                };
                // A finite terrain cost can still exceed what this unit can
                // spend in one turn. Such a tile can never occur in its path.
                if leaving > allowance {
                    continue;
                }
                let total = cost.saturating_add(leaving);
                let column = index % width;
                let west = (column > 0).then(|| index - 1);
                let east = (column + 1 < width).then(|| index + 1);
                let north = index.checked_sub(width);
                let south = (index + width < cells).then_some(index + width);
                for next in [west, east, north, south].into_iter().flatten() {
                    // The neighbour is where the route would stand, so it must
                    // be a tile this class can occupy at all.
                    if self.costs[next].is_none() {
                        continue;
                    }
                    if out[next].is_some_and(|best| best <= total) {
                        continue;
                    }
                    out[next] = Some(total);
                    self.ring[usize::from(total) % ring].push(next as u16);
                    waiting += 1;
                }
            }
            let Some(next) = cost.checked_add(1) else {
                break;
            };
            cost = next;
        }
    }

    /// Turns to cross `points` at an allowance, which is what a caller wants.
    ///
    /// **This is a lower bound.** A unit cannot always stop where its
    /// allowance runs out: an occupied tile or a teleporter forces it short,
    /// and a terrain cost can overshoot the boundary. It is optimistic by up
    /// to a turn on awkward ground, and it is still far nearer the truth than
    /// counting tiles.
    ///
    /// Zero points is the tile itself, which is nought turns away.
    pub const fn turns(points: u16, allowance: u16) -> u16 {
        if allowance == 0 {
            return u16::MAX;
        }
        points.div_ceil(allowance)
    }
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
    let width = usize::from(observation.board.width());
    board.set_rare_states(
        observation
            .board
            .tiles()
            .enumerate()
            .filter_map(|(index, observed)| {
                // Read the two fields first. Nearly every tile of a board
                // holds neither, and this walks every tile of every
                // projection the agent reifies.
                let destructible_hp = observed.destructible_hp();
                let teleporter = observed.teleporter();
                if destructible_hp.is_none() && teleporter.is_none() {
                    return None;
                }
                // The coordinate is worked out only for a tile that has
                // something to record, so the walk over the board is a read
                // of two options and not a division for each tile.
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "a board is at most 255 wide and 255 tall, so both fit a u8"
                )]
                let position = Pos::new((index % width) as u8, (index / width) as u8);
                Some((
                    position,
                    crate::semantic::RareTileState {
                        destructible_hp,
                        teleporter: teleporter.cloned(),
                        // A projection carries no trait state, so a reified
                        // board has none either.
                        trait_state: None,
                    },
                ))
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
    crate::benchmark::record_attack_target_call();
    let movement = destination.movement();
    let state = movement.state();
    let unit = movement.unit();
    let subject = lookup(state, unit)?;
    let profile = ruleset::profile(subject.kind);
    if profile.fire_mode == FireMode::None {
        crate::benchmark::record_empty_target_search();
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
            crate::benchmark::record_destination_inspected();
            // The index names the occupant whether or not this team sees it,
            // which is what the roster walk did; `can_attack` refuses the
            // ones the team may not fire at.
            if let Some(candidate) = destination.view().occupant_at(cell)
                && candidate != unit
                && destination.can_attack(AttackTarget::Unit { unit: candidate })?
            {
                units.push(candidate);
                crate::benchmark::record_unit_target_found();
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
                crate::benchmark::record_tile_target_found();
                if units.len() + tiles.len() >= LIMIT {
                    break 'walk;
                }
            }
        }
    }
    // The walk finds units in board order; report them by id, so the list does
    // not depend on where the mover stopped.
    units.sort_unstable();
    crate::benchmark::record_candidate_units_sorted(units.len() as u64);
    if units.is_empty() && tiles.is_empty() {
        crate::benchmark::record_empty_target_search();
    }
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
    // A silo the mover cannot fire refuses every tile of the board for the
    // same reason, and a standard map carries a dozen of them already spent.
    // Asking once is the difference between a board walk and a tile lookup.
    if !destination.can_launch_anywhere()? {
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
