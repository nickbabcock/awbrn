//! What is legal, rather than whether one thing is.
//!
//! [`crate::transition::execute`] answers a question the caller already knows
//! how to ask. A user interface has the opposite problem: before it can offer a
//! command it must know which commands exist here, and the only way to find out
//! from the reducer alone is to guess and be told no. So interfaces compute
//! their own move ranges — and a range computed beside the rules is a range
//! that disagrees with them, silently, wherever weather, a commander effect, or
//! a hidden blocker was left out.
//!
//! Everything here is derived from the reducer, not restated alongside it:
//!
//! * Action, production, delete, and unload queries use the reducer's
//!   preparation checks. These queries do not clone or change the state, and
//!   they cannot drift because they do not contain a second copy of the action
//!   rules.
//! * [`reachable`] is the one exception. A probe per tile would answer whether
//!   a path is legal but not produce one, and a caller needs the path to build
//!   the command, so the search is written out here. `tests/query.rs` holds it
//!   to `execute`'s verdict for every unit and every tile in the fixture
//!   corpus, which is what keeps the exception honest.
//!
//! None of this is authoritative. A server still executes the command it
//! receives; this exists so a client can offer commands the server will take.

use std::borrow::Borrow;
use std::cell::OnceCell;
use std::collections::HashSet;
use std::sync::Arc;

use crate::combat::Forecast;
use crate::commander::{self, Holdings};
use crate::event::AttackTarget;
use crate::ruleset::{self, FireMode, MovementClass, TerrainTrait};
use crate::semantic::{
    AwbwView, Grid, Location, Observation, ObservedMatch, ObservedPlayer, PlayerId, PlayerIdx,
    PlayerStatus, Pos, State, Unit, UnitId, UnitKindId, WeatherKind,
};
use crate::transition::{
    ActiveTurn, ExecuteError, PreparedActiveUnit, PreparedDestination, board_position,
    forecast_tile_attack, forecast_unit_attack, prepare_active_unit, prepare_movement,
    prepare_production_site, prepare_unload_transport,
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
pub struct TurnMaps<'a> {
    state: &'a State,
    /// The seat these maps answer for. Entry costs follow this player's
    /// commander, so a unit of any other player must not be searched with
    /// them.
    seat: PlayerIdx,
    /// The weather this player's units move through.
    weather: WeatherKind,
    view: AwbwView<'a>,
    blocking: OnceCell<Arc<Grid<Blocking>>>,
    /// Entry costs, one map per movement class, each built when a unit of that
    /// class first asks.
    entries: [OnceCell<Arc<Grid<EntryCost>>>; MovementClass::COUNT],
    /// What every player holds on the board. Commander combat rules read the
    /// tower and property counts of both sides of every strike, and scoring
    /// one attack candidate used to count them from the board twice.
    holdings: OnceCell<Holdings<'a>>,
}

impl<'a> TurnMaps<'a> {
    /// Open the tables a player's units share, or `None` when the player is
    /// not on the roster.
    pub(crate) fn new(state: &'a State, mover: &'a PlayerId) -> Option<Self> {
        Self::for_seat(state, state.player_index(mover)?)
    }

    /// The same tables, opened for a seat a unit already names.
    pub(crate) fn for_seat(state: &'a State, seat: PlayerIdx) -> Option<Self> {
        let player = state.players.get(seat.get())?;
        Some(Self {
            state,
            seat,
            weather: commander::player_weather(state, seat),
            view: AwbwView::new(state, &player.team),
            blocking: OnceCell::new(),
            entries: [(); MovementClass::COUNT].map(|()| OnceCell::new()),
            holdings: OnceCell::new(),
        })
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
        self.blocking.get_or_init(|| {
            // The table asks about every tile, and every destination queried
            // through a field asks again.
            self.view.index_occupancy();
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
        self.entries[class.index()].get_or_init(|| {
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
pub struct PreparedMoveField<'a, M = TurnMaps<'a>> {
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
    pub fn new(active: PreparedActiveUnit<'a>) -> Result<Self, QueryError> {
        let state = active.state();
        let subject = lookup(state, active.unit())?;
        let maps = TurnMaps::for_seat(state, subject.owner).ok_or(QueryError::UnknownOwner {
            unit: active.unit(),
            seat: subject.owner,
        })?;
        Self::with_maps(active, maps)
    }
}

impl<'a, M> PreparedMoveField<'a, M>
where
    M: Borrow<TurnMaps<'a>>,
{
    /// Compute a movement field against maps the caller already holds.
    fn with_maps(active: PreparedActiveUnit<'a>, maps: M) -> Result<Self, QueryError> {
        let field = reachable_with(active.state(), active.unit(), maps.borrow())?;
        Ok(Self {
            active,
            field,
            maps,
        })
    }

    /// The movement geometry bound to this proof.
    pub const fn field(&self) -> &MoveField {
        &self.field
    }

    /// The unit this field was computed for.
    pub const fn unit(&self) -> UnitId {
        self.field.unit()
    }

    /// Where the unit started.
    pub const fn origin(&self) -> Pos {
        self.field.origin()
    }

    /// Movement points available to the unit.
    pub const fn budget(&self) -> u64 {
        self.field.budget()
    }

    /// What it costs to arrive at `position`.
    pub fn step(&self, position: Pos) -> Option<Step> {
        self.field.step(position)
    }

    /// Whether a movement command may end at `position`.
    pub fn can_stop_at(&self, position: Pos) -> bool {
        self.field.can_stop_at(position)
    }

    /// Every tile where the unit can stop, with its movement cost.
    pub fn destinations(&self) -> impl Iterator<Item = (Pos, u64)> + '_ {
        self.field.destinations()
    }

    /// Every tile the unit can reach, with its movement cost.
    pub fn reach(&self) -> impl Iterator<Item = (Pos, u64)> + '_ {
        self.field.reach()
    }

    /// Return the field's route to `position`.
    pub fn path_to(&self, position: Pos) -> Option<Vec<Pos>> {
        self.field.path_to(position)
    }

    /// Validate and price a caller-chosen route through this field.
    pub fn route_cost(&self, path: &[Pos]) -> Option<u64> {
        self.field.route_cost(path)
    }

    /// Bind one reachable destination to its prepared movement.
    ///
    /// Transit-only teleporter tiles do not produce destinations. Occupied
    /// destinations remain available for join, load, and attack queries.
    pub fn prepare_destination<'field>(
        &'field self,
        position: Pos,
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
        let mut path = Vec::with_capacity(depth);
        let mut entry_costs = Vec::with_capacity(depth);
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

    /// Enumerate actions and name their targets by board position.
    pub fn observed_actions_at(&self, position: Pos) -> Result<ObservedActionSet, QueryError> {
        self.actions_at(position)
            .map(|actions| by_position(self.active.state(), actions))
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
    ) -> Result<Option<PreparedMoveField<'a, &'turn TurnMaps<'a>>>, QueryError> {
        let Ok(active) = self.unit(unit)? else {
            return Ok(None);
        };
        PreparedMoveField::with_maps(active, self.maps()).map(Some)
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
    reachable_with(state, unit, &maps)
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

    // The only board-sized thing a search allocates: everything else it needs
    // is shared with the rest of the turn.
    let mut arrivals = Grid::filled(state.board.dimensions(), None);
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
    let mut buckets = vec![Vec::new(); bucket_count];
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

/// Everywhere the recipient-safe observation says `unit` can go.
///
/// This is [`reachable`] for a client that holds an [`Observation`] rather
/// than a [`State`], and the movement counterpart of [`observed_actions_at`]:
/// the observation is reified into a provisional state and the same search
/// runs against it, so no movement rule is restated on the client.
///
/// This deliberately cannot account for facts hidden by fog. A hidden blocker
/// can make an offered destination unreachable, and the field is advisory in
/// the same way [`observed_actions_at`] is; executing the command against
/// authoritative state remains the final validation step.
///
/// Only friendly units are nameable here. Enemy identities are deliberately
/// absent from an observation, so this is not a threat-range query.
pub fn observed_reachable(
    observation: &Observation,
    unit: UnitId,
) -> Result<MoveField, QueryError> {
    ObservedQuery::new(observation)?.reachable(unit)
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

/// What a recipient may do at a destination, named the way a recipient can name
/// it.
///
/// This is [`ActionSet`] with every target given as a position instead of a
/// unit id. A projection carries no id for an enemy — `ObservedUnitRef::Enemy`
/// holds only a position — so an id derived from one is an invention of the
/// reification and means nothing to the server that would receive it. Reporting
/// positions keeps that invention inside this module.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObservedActionSet {
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
    /// Where this unit may fire from that destination.
    pub attack: Vec<Pos>,
    /// Where the friendly units it may repair are standing.
    pub repair: Vec<Pos>,
    /// Tiles a missile silo underfoot may be fired at.
    pub launch: Vec<Pos>,
}

/// Recipient-safe queries over one reified observation.
///
/// Construct this once and reuse it for a movement field and its action
/// destinations. This prevents each action query from rebuilding the same
/// provisional state.
#[derive(Debug)]
pub struct ObservedQuery {
    state: State,
    may_command: bool,
}

impl ObservedQuery {
    /// Reify one observation for subsequent queries.
    pub fn new(observation: &Observation) -> Result<Self, QueryError> {
        Ok(Self {
            state: reify(observation)?,
            may_command: recipient_may_command(observation),
        })
    }

    /// Compute the recipient-safe movement field for `unit`.
    pub fn reachable(&self, unit: UnitId) -> Result<MoveField, QueryError> {
        reachable(&self.state, unit)
    }

    /// Open the recipient's turn over this observation.
    ///
    /// `Ok(None)` means the recipient may not issue commands against this
    /// observation at all. Enumerating several units wants this rather than
    /// [`Self::prepared_reachable`], which resolves the recipient's sightings
    /// and unit positions again for every unit.
    pub fn turn(&self) -> Result<Option<ActiveTurn<'_>>, QueryError> {
        if !self.may_command {
            return Ok(None);
        }
        Ok(ActiveTurn::open(&self.state, &self.state.turn.active_player)?.ok())
    }

    /// Bind a movement field to a commandable unit in this observation.
    pub fn prepared_reachable(
        &self,
        unit: UnitId,
    ) -> Result<Option<PreparedMoveField<'_>>, QueryError> {
        if !self.may_command {
            return Ok(None);
        }
        prepared_move_field(&self.state, unit)
    }

    /// Query one destination and compute its path when necessary.
    pub fn actions_at(
        &self,
        unit: UnitId,
        destination: Pos,
    ) -> Result<ObservedActionSet, QueryError> {
        if !self.may_command {
            return Ok(ObservedActionSet::default());
        }
        self.prepared_reachable(unit)?.map_or_else(
            || Ok(ObservedActionSet::default()),
            |field| field.observed_actions_at(destination),
        )
    }

    /// Query a path obtained from a movement field without rebuilding it.
    pub fn actions_for_path(
        &self,
        unit: UnitId,
        path: Vec<Pos>,
    ) -> Result<ObservedActionSet, QueryError> {
        if !self.may_command {
            return Ok(ObservedActionSet::default());
        }
        Ok(by_position(
            &self.state,
            actions_for_path(&self.state, unit, path)?,
        ))
    }
}

/// The legal attacks from one candidate movement destination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservedAttacksFrom {
    pub from: Pos,
    pub targets: Vec<Pos>,
}

/// One standalone AWBW unload order available to the recipient.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObservedUnload {
    pub cargo: UnitId,
    pub cargo_kind: UnitKindId,
    pub destination: Pos,
}

impl ObservedActionSet {
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
fn recipient_may_command(observation: &Observation) -> bool {
    ruleset::supports(&observation.ruleset)
        && observation.turn.active_player == observation.recipient
        && observation.turn.phase == crate::semantic::Phase::UnitAction
        && matches!(observation.match_state, ObservedMatch::Active { .. })
}

/// Which commands the recipient-safe observation says `unit` has at
/// `destination`.
///
/// This is [`actions_at`] for a client that holds an [`Observation`] rather
/// than a [`State`]. It does not restate a single rule: the observation is
/// reified into a provisional state and the reducer answers, exactly as it does
/// for the authoritative caller. What the recipient cannot see is filled with
/// the most conservative reading, so the reducer is never told a fact the
/// projection withheld.
///
/// This deliberately cannot account for facts hidden by fog. A hidden blocker
/// can make an offered command illegal, and a hidden target can make a legal
/// one missing. The returned set is advisory; executing the selected command
/// against authoritative state is still the final validation step.
pub fn observed_actions_at(
    observation: &Observation,
    unit: UnitId,
    destination: Pos,
) -> Result<ObservedActionSet, QueryError> {
    if !recipient_may_command(observation) {
        return Ok(ObservedActionSet::default());
    }

    ObservedQuery::new(observation)?.actions_at(unit, destination)
}

/// What `unit` may attack from each candidate movement destination.
///
/// This is the batch form of reading [`ObservedActionSet::attack`] from
/// [`observed_actions_at`]. Reification is the expensive part of an observed
/// query, so a board that highlights all targets must pay that cost once per
/// selection, not once per destination.
pub fn observed_attacks_from(
    observation: &Observation,
    unit: UnitId,
    destinations: &[Pos],
) -> Result<Vec<ObservedAttacksFrom>, QueryError> {
    if !recipient_may_command(observation) {
        return Ok(Vec::new());
    }

    let state = reify(observation)?;
    if destinations.is_empty() {
        return Ok(Vec::new());
    }
    let Some(field) = prepared_move_field(&state, unit)? else {
        return Ok(destinations
            .iter()
            .copied()
            .map(|from| ObservedAttacksFrom {
                from,
                targets: Vec::new(),
            })
            .collect());
    };
    destinations
        .iter()
        .copied()
        .map(|from| {
            let attack = match field.prepare_destination(from) {
                Some(destination) => attack_targets_for_destination(&destination)?,
                None => Vec::new(),
            };
            Ok(ObservedAttacksFrom {
                from,
                targets: by_position(
                    &state,
                    ActionSet {
                        attack,
                        ..ActionSet::default()
                    },
                )
                .attack,
            })
        })
        .collect()
}

/// What each of these attacks would cost both sides, before any dice.
///
/// A player choosing between two attacks is choosing between two brackets, and
/// until they can see both they are guessing at the one number the game
/// actually turns on. This answers for a whole menu at once because that is how
/// the question is asked: reifying the observation is the expensive half, and a
/// caller asking per target would pay it once per row.
///
/// `targets` are positions rather than ids for the reason
/// [`ObservedActionSet`] gives — a projection carries no id for an enemy — so a
/// target is read as whatever stands there: a unit if one does, otherwise a
/// destructible tile. Results are positional, one entry per requested target.
/// An entry is `None` when the attack is not one this unit could make from
/// `from`, which is the same answer the interface should give when it has
/// nothing trustworthy to show: no number at all rather than a wrong one.
///
/// Like every observed-side query this is computed against the recipient's own
/// projection, so fog can make it wrong in both directions. It is advisory,
/// exactly as the order list it annotates already is.
pub fn observed_forecasts(
    observation: &Observation,
    unit: UnitId,
    from: Pos,
    targets: &[Pos],
) -> Result<Vec<Option<Forecast>>, QueryError> {
    if !recipient_may_command(observation) {
        return Ok(vec![None; targets.len()]);
    }

    let hidden_hp: HashSet<_> = observation
        .units
        .iter()
        .filter(|unit| unit.hp.exact().is_none())
        .filter_map(|unit| match unit.location {
            Location::Board { position } => Some(position),
            Location::Cargo { .. } => None,
        })
        .collect();
    let state = reify(observation)?;
    let index = state
        .units
        .index_of(unit)
        .ok_or(QueryError::UnitNotFound(unit))?;
    let player = state.player_id(state.units[index].owner).clone();
    let holdings = Holdings::tally(&state);

    Ok(targets
        .iter()
        .map(|target| {
            (!hidden_hp.contains(target))
                .then(|| forecast_at(&state, &holdings, &player, index, unit, from, *target))
                .flatten()
        })
        .collect())
}

/// One target's forecast, dispatched on what is standing there.
fn forecast_at(
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

/// Which free unload commands the recipient may issue from `transport` now.
///
/// Unlike a move follow-up, unloading neither moves nor spends the transport,
/// so this remains available when the transport has already acted. Results are
/// ordered by cargo id and then map position.
pub fn observed_unloads(
    observation: &Observation,
    transport: UnitId,
) -> Result<Vec<ObservedUnload>, QueryError> {
    if !recipient_may_command(observation) {
        return Ok(Vec::new());
    }

    let state = reify(observation)?;
    let subject = lookup(&state, transport)?;
    let Location::Board { position } = subject.location else {
        return Err(QueryError::UnitNotOnBoard(transport));
    };
    let player = state.player_id(subject.owner).clone();
    let Ok(Ok(prepared_transport)) = prepare_unload_transport(&state, &player, transport) else {
        return Ok(Vec::new());
    };
    let mut orders = Vec::new();
    for candidate in state.units.iter().filter(|candidate| {
        matches!(
            candidate.location,
            Location::Cargo {
                transport: carried_by,
                ..
            } if carried_by == transport
        )
    }) {
        let Ok(Ok(prepared_cargo)) = prepared_transport.clone().prepare_cargo(candidate.id) else {
            continue;
        };
        for destination in position.orthogonal() {
            if matches!(
                prepared_cargo.clone().prepare_destination(destination),
                Ok(Ok(_))
            ) {
                orders.push(ObservedUnload {
                    cargo: candidate.id,
                    cargo_kind: candidate.kind,
                    destination,
                });
            }
        }
    }
    orders.sort_by_key(|order| (order.cargo, order.destination));
    Ok(orders)
}

/// Whether the recipient may remove `unit` from the board now.
///
/// The reducer remains the authority for readiness, ownership, phase, and
/// board-position checks.
pub fn observed_can_delete(observation: &Observation, unit: UnitId) -> Result<bool, QueryError> {
    if !recipient_may_command(observation) {
        return Ok(false);
    }

    let state = reify(observation)?;
    let player = state.player_id(lookup(&state, unit)?.owner).clone();
    let Ok(Ok(active)) = prepare_active_unit(&state, &player, unit) else {
        return Ok(false);
    };
    Ok(matches!(active.prepare_delete(), Ok(Ok(_))))
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

    Ok(State {
        ruleset: observation.ruleset.clone(),
        settings: observation.settings.clone(),
        board,
        teams: observation.teams.clone(),
        players,
        turn: observation.turn.clone(),
        weather: observation.weather.clone(),
        units,
        next_unit_id: None,
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
    players: &[crate::semantic::Player],
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
                    let seat = players
                        .iter()
                        .position(|player| player.id == *name)
                        .and_then(|seat| u8::try_from(seat).ok())
                        .map(crate::semantic::PlayerIdx::from_seat)
                        .ok_or(QueryError::Unprojectable(
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
        } => crate::semantic::Player {
            id: id.clone(),
            team: team.clone(),
            funds: *funds,
            status: *status,
            commanders: commanders.clone(),
            power_state: power_state.clone(),
        },
        ObservedPlayer::Public {
            id,
            team,
            status,
            commanders,
            power_state,
            ..
        } => crate::semantic::Player {
            id: id.clone(),
            team: team.clone(),
            funds: 0,
            status: *status,
            commanders: commanders
                .iter()
                .map(|commander| crate::semantic::Commander {
                    id: commander.id,
                    active: commander.active,
                    power_charge: commander.power_charge,
                    power_uses: commander.power_uses,
                })
                .collect(),
            power_state: power_state.clone(),
        },
    }
}

fn reified_units(
    observation: &Observation,
    players: &[crate::semantic::Player],
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
                .iter()
                .position(|player| player.id == observed.owner)
                .and_then(|seat| u8::try_from(seat).ok())
                .map(PlayerIdx::from_seat)
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
    PreparedMoveField::new(active).map(Some)
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
        attack: attack_targets_for_destination(&destination)?,
        repair: repair_targets(&destination)?,
        launch: launch_targets(&destination)?,
    })
}

fn attack_targets_for_destination<'a, M>(
    destination: &PreparedDestination<'a, M>,
) -> Result<Vec<AttackTarget>, QueryError>
where
    M: Borrow<TurnMaps<'a>>,
{
    let movement = destination.movement();
    let state = movement.state();
    let unit = movement.unit();
    let subject = lookup(state, unit)?;
    let profile = ruleset::profile(subject.kind);
    if profile.fire_mode == FireMode::None {
        return Ok(Vec::new());
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
    let mut units: Vec<UnitId> = Vec::new();
    let mut tiles: Vec<Pos> = Vec::new();
    let dimensions = state.board.dimensions();
    for y in minimum_y..=maximum_y {
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
            }
            if ruleset::terrain(state.board.at(cell).terrain)
                .destructible
                .is_some()
                && destination.can_attack(AttackTarget::Tile { position })?
            {
                tiles.push(position);
            }
        }
    }
    // The walk finds units in board order; report them by id, so the list does
    // not depend on where the mover stopped.
    units.sort_unstable();
    let mut targets: Vec<AttackTarget> = units
        .into_iter()
        .map(|unit| AttackTarget::Unit { unit })
        .collect();
    targets.extend(
        tiles
            .into_iter()
            .map(|position| AttackTarget::Tile { position }),
    );
    Ok(targets)
}

/// Friendly units `unit` may repair after moving along `path`.
fn repair_targets<'a, M>(
    destination: &PreparedDestination<'a, M>,
) -> Result<Vec<UnitId>, QueryError>
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
        return Ok(Vec::new());
    };
    if repair.relation != ruleset::Relation::Adjacent {
        return Ok(Vec::new());
    }
    let from = movement.plan().destination();
    let mut targets = Vec::new();
    for candidate in from
        .orthogonal()
        .filter_map(|position| destination.view().occupant(position))
        .filter(|candidate| *candidate != unit)
    {
        if destination.can_repair(candidate)? {
            targets.push(candidate);
        }
    }
    Ok(targets)
}

/// Every tile a silo under the end of `path` may be fired at.
fn launch_targets<'a, M>(destination: &PreparedDestination<'a, M>) -> Result<Vec<Pos>, QueryError>
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
        return Ok(Vec::new());
    }
    let mut targets = Vec::new();
    for target in state.board.positions() {
        if destination.can_launch(target)? {
            targets.push(target);
        }
    }
    Ok(targets)
}

/// Which unit kinds `player` may produce at `position` right now.
///
/// A production site is a tile rather than a unit, so it has no reachable set
/// and no [`ActionSet`]; this is the same question asked of one.
pub fn production_options(state: &State, player: &PlayerId, position: Pos) -> Vec<UnitKindId> {
    let Ok(Ok(site)) = prepare_production_site(state, player, position) else {
        return Vec::new();
    };
    UnitKindId::ALL
        .iter()
        .copied()
        .filter(|kind| matches!(site.clone().prepare_kind(*kind), Ok(Ok(_))))
        .collect()
}

/// A unit this facility produces for the recipient, and its effective cost.
///
/// `affordable` is false when the recipient's funds do not reach `cost`. The
/// option is still reported, because a menu that hides what a base could build
/// hides the information a player is deciding with; an interface shows the
/// price it cannot pay rather than a shorter list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObservedProductionOption {
    pub kind: UnitKindId,
    pub cost: u64,
    pub affordable: bool,
}

/// Which units the recipient-safe observation says this facility produces.
///
/// The list is ordered by base cost, which is the order the units are presented
/// in, and it is stable under commander cost effects because those change the
/// effective price rather than the ordering.
///
/// This deliberately cannot account for facts hidden by fog. The returned list
/// is advisory; executing the selected command against authoritative state is
/// still the final validation step.
pub fn observed_production_options(
    observation: &Observation,
    position: Pos,
) -> Vec<ObservedProductionOption> {
    if !recipient_may_command(observation) {
        return Vec::new();
    }

    let Some(ObservedPlayer::Private {
        funds,
        status,
        commanders,
        power_state,
        ..
    }) = observation.players.iter().find(|player| match player {
        ObservedPlayer::Private { id, .. } | ObservedPlayer::Public { id, .. } => {
            id == &observation.recipient
        }
    })
    else {
        return Vec::new();
    };
    if *status != PlayerStatus::Active {
        return Vec::new();
    }

    let Some(tile) = observation.board.get(position) else {
        return Vec::new();
    };
    if !tile.owner.is_owned_by(&observation.recipient)
        || observation.units.iter().any(
            |unit| matches!(unit.location, Location::Board { position: occupied } if occupied == position),
        )
    {
        return Vec::new();
    }

    let owned_units = observation
        .units
        .iter()
        .filter(|unit| unit.owner == observation.recipient)
        .count() as u64;
    if observation
        .settings
        .unit_limit
        .is_some_and(|limit| owned_units >= limit)
    {
        return Vec::new();
    }
    let owns_lab = observation.board.tiles().any(|tile| {
        tile.terrain == crate::semantic::TerrainId::Lab
            && tile.owner.is_owned_by(&observation.recipient)
    });
    let mut options: Vec<(u64, ObservedProductionOption)> = UnitKindId::ALL
        .iter()
        .copied()
        .filter_map(|kind| {
            let profile = ruleset::profile(kind);
            let is_site = commander::observed_production_site(
                commanders,
                power_state,
                tile.terrain,
                profile.domain,
            );
            if !is_site
                || observation.settings.unit_bans.contains(&kind)
                || (observation.settings.lab_units.contains(&kind) && !owns_lab)
            {
                return None;
            }
            let cost =
                commander::observed_effective_build_cost(commanders, power_state, profile.cost)?;
            Some((
                profile.cost,
                ObservedProductionOption {
                    kind,
                    cost,
                    affordable: cost <= *funds,
                },
            ))
        })
        .collect();

    // Base cost, then the identifier, so two units priced the same always come
    // back in the same order. `UnitKindId::ALL` is alphabetical, which is not an
    // order any player reads a build menu in.
    options.sort_by(|(left, a), (right, b)| left.cmp(right).then(a.kind.cmp(&b.kind)));
    options.into_iter().map(|(_, option)| option).collect()
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
