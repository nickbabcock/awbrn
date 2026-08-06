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
//! * [`actions_at`] and [`attack_targets`] *run* the reducer against a probe
//!   entropy source and report what it accepted. They cannot drift, because
//!   there is no second copy of the rule to drift from — a command this module
//!   offers is one `execute` has already accepted against this state.
//! * [`reachable`] is the one exception. A probe per tile would answer whether
//!   a path is legal but not produce one, and a caller needs the path to build
//!   the command, so the search is written out here. `tests/query.rs` holds it
//!   to `execute`'s verdict for every unit and every tile in the fixture
//!   corpus, which is what keeps the exception honest.
//!
//! None of this is authoritative. A server still executes the command it
//! receives; this exists so a client can offer commands the server will take.

use std::collections::{BinaryHeap, HashSet};

use crate::commander::{self, Domain};
use crate::event::AttackTarget;
use crate::random::{Entropy, Luck, RandomError};
use crate::ruleset::{self, FireMode, TerrainTrait};
use crate::semantic::{
    AwbwVisibility, Location, Observation, ObservedMatch, ObservedPlayer, PlayerId, PlayerStatus,
    Pos, State, TeamId, Unit, UnitId, UnitKindId, Viewpoint, Visibility, WeatherKind,
};
use crate::transition::{Command, ExecuteOutcome, execute_with};
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
    #[error("unit {unit} is owned by unknown player {owner}")]
    UnknownOwner { unit: UnitId, owner: PlayerId },
    #[error("this observation does not describe a whole board: {0}")]
    Unprojectable(&'static str),
}

/// An entropy source for probing, which answers every draw with the lowest
/// value the reducer said was legal.
///
/// A probe must never fail for want of a token: an attack asked about with an
/// empty tape reports [`crate::transition::ExecuteError::UnsupportedCommand`],
/// which would read as "illegal" and hide a legal attack. Taking the domain's
/// minimum is always in range by construction, and the resulting state is
/// discarded — only the accept-or-reject verdict is kept, and no rule in the
/// specification makes legality depend on the roll.
struct Probe;

impl Entropy for Probe {
    fn luck(&mut self, _: Luck, domain: Domain) -> Result<i64, RandomError> {
        Ok(domain.minimum)
    }

    fn weather(&mut self) -> Result<WeatherKind, RandomError> {
        Ok(WeatherKind::Clear)
    }
}

/// Whether the reducer accepts `command` against `state`.
///
/// The execution is thrown away. That it is computed at all is what makes this
/// exact: the answer comes from the same code that will run when the command is
/// really issued.
fn is_accepted(state: &State, command: Command) -> bool {
    matches!(
        execute_with(state, command, &mut Probe),
        Ok(ExecuteOutcome::Accepted(_))
    )
}

/// Why the reducer would refuse to act with this unit at all, if it would.
///
/// A greyed-out unit in an interface is a question — *why* — and this answers
/// it with the violation the reducer would produce, without needing a
/// destination or an action to ask about. `Ok(Ok(()))` means some command with
/// this unit is worth offering.
pub fn can_act(state: &State, unit: UnitId) -> Result<Result<(), Violation>, QueryError> {
    let subject = lookup(state, unit)?;
    let Location::Board { position: origin } = subject.location else {
        return Err(QueryError::UnitNotOnBoard(unit));
    };
    // Waiting in place is the cheapest command that runs the whole shared
    // prologue — ruleset, match, phase, actor, ownership, board, readiness —
    // and nothing beyond it that a unit standing still can fail, except a
    // teleporter, which is a fact about the tile rather than the unit.
    match execute_with(
        state,
        Command::MoveWait {
            player: subject.owner.clone(),
            unit,
            path: vec![origin],
        },
        &mut Probe,
    ) {
        Ok(ExecuteOutcome::Accepted(_)) => Ok(Ok(())),
        Ok(ExecuteOutcome::Rejected(violation)) => Ok(Err(violation)),
        Err(_) => Err(QueryError::UnknownOwner {
            unit,
            owner: subject.owner.clone(),
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
    width: u8,
    height: u8,
    /// Row-major, one entry per tile, `None` where the unit cannot reach.
    steps: Vec<Option<Step>>,
    entry_costs: Vec<Option<u64>>,
    route_blocked: Vec<bool>,
    budget: u64,
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

    fn index(&self, position: Pos) -> Option<usize> {
        (position.x < self.width && position.y < self.height)
            .then(|| usize::from(position.y) * usize::from(self.width) + usize::from(position.x))
    }

    /// What it costs to arrive at `position`, if the unit can arrive at all.
    pub fn step(&self, position: Pos) -> Option<Step> {
        self.index(position).and_then(|index| self.steps[index])
    }

    /// Whether a `move-*` command may end here.
    pub fn can_stop_at(&self, position: Pos) -> bool {
        self.step(position).is_some_and(|step| step.can_stop)
    }

    /// Every tile the unit can end its move on, with the cost of getting there.
    ///
    /// This is the set an interface highlights.
    pub fn destinations(&self) -> impl Iterator<Item = (Pos, u64)> + '_ {
        self.reach()
            .filter(|(position, _)| self.can_stop_at(*position))
    }

    /// Every tile the unit can arrive at, including those it cannot stop on.
    ///
    /// Join, load and a moving attack all name a destination the mover does not
    /// come to rest on alone, so they are asked about against this rather than
    /// against [`MoveField::destinations`].
    pub fn reach(&self) -> impl Iterator<Item = (Pos, u64)> + '_ {
        let width = usize::from(self.width);
        self.steps
            .iter()
            .enumerate()
            .filter_map(move |(index, step)| {
                let step = (*step)?;
                let (x, y) = (index % width, index / width);
                let position = Pos::new(
                    u8::try_from(x).expect("a column index fits the board width"),
                    u8::try_from(y).expect("a row index fits the board height"),
                );
                Some((position, step.cost))
            })
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
        while let Some(previous) = self.step(cursor).and_then(|step| step.previous) {
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
            let index = self.index(edge[1])?;
            let is_last = edge_index + 2 == path.len();
            if self.route_blocked[index] && !is_last {
                return None;
            }
            total = total.checked_add(self.entry_costs[index]?)?;
            if total > self.budget {
                return None;
            }
        }
        Some(total)
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
    let Location::Board { position: origin } = subject.location else {
        return Err(QueryError::UnitNotOnBoard(unit));
    };
    let team = state
        .find_player(&subject.owner)
        .map(|player| player.team.clone())
        .ok_or_else(|| QueryError::UnknownOwner {
            unit,
            owner: subject.owner.clone(),
        })?;

    let profile = ruleset::profile(subject.kind);
    let allowance = commander::effective_move(state, subject, profile.movement, profile.domain);
    let budget = allowance.min(subject.fuel);
    let weather = commander::effective_weather(state, subject);
    let (width, height) = (state.board.width(), state.board.height());
    let occupancy = Occupancy::new(state, subject, &team);

    let mut entry_costs = Vec::with_capacity(usize::from(width) * usize::from(height));
    let mut route_blocked = Vec::with_capacity(entry_costs.capacity());
    for y in 0..height {
        for x in 0..width {
            let position = Pos::new(x, y);
            entry_costs.push(entry_cost(state, subject, position, weather));
            route_blocked.push(occupancy.blocks_route(position));
        }
    }

    let mut steps: Vec<Option<Step>> = vec![None; usize::from(width) * usize::from(height)];
    let index_of =
        |position: Pos| usize::from(position.y) * usize::from(width) + usize::from(position.x);

    steps[index_of(origin)] = Some(Step {
        cost: 0,
        // A predeployed unit standing on a teleporter may leave but may not
        // wait in place: the tile is traversable and cannot hold a unit at rest.
        can_stop: !is_teleporter(state, origin),
        previous: None,
    });

    // Ordinary Dijkstra. Entry costs are non-negative — zero only on
    // teleporters — so the first time a tile leaves the frontier its cost is
    // final.
    let mut frontier = BinaryHeap::new();
    frontier.push(Frontier {
        cost: 0,
        position: origin,
    });
    while let Some(Frontier { cost, position }) = frontier.pop() {
        if steps[index_of(position)].is_some_and(|step| step.cost < cost) {
            continue;
        }
        // A disclosed enemy blocks the route through its tile. Allied units
        // may be crossed but remain invalid destinations.
        if position != origin && occupancy.blocks_route(position) {
            continue;
        }
        for next in position.orthogonal() {
            if next.x >= width || next.y >= height {
                continue;
            }
            let Some(entry) = entry_cost(state, subject, next, weather) else {
                continue;
            };
            let Some(total) = cost.checked_add(entry).filter(|total| *total <= budget) else {
                continue;
            };
            if steps[index_of(next)].is_some_and(|step| step.cost <= total) {
                continue;
            }
            steps[index_of(next)] = Some(Step {
                cost: total,
                can_stop: !is_teleporter(state, next) && !occupancy.blocks_stop(next),
                previous: Some(position),
            });
            frontier.push(Frontier {
                cost: total,
                position: next,
            });
        }
    }

    Ok(MoveField {
        unit,
        origin,
        width,
        height,
        steps,
        entry_costs,
        route_blocked,
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
    let state = reify(observation)?;
    reachable(&state, unit)
}

/// What `unit` may do if it moves to `destination`.
///
/// Every field is the reducer's own verdict on the corresponding command, so an
/// interface can enable exactly the buttons that will work. `destination` is
/// normally one of [`MoveField::reach`]; an unreachable one yields an empty
/// set, because the shared movement prefix rejects it first.
///
/// This runs one reduction per candidate command — a dozen or so state clones.
/// That is a per-selection cost in an interface, not a per-frame one, and
/// caching belongs to the caller, which knows when its state changed.
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

    let state = reify(observation)?;
    Ok(by_position(&state, actions_at(&state, unit, destination)?))
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
    let player = subject.owner.clone();
    let mut orders = state
        .units
        .iter()
        .filter_map(|candidate| {
            matches!(
                candidate.location,
                Location::Cargo {
                    transport: carried_by,
                    ..
                } if carried_by == transport
            )
            .then_some((candidate.id, candidate.kind))
        })
        .flat_map(|(cargo, cargo_kind)| {
            let player = player.clone();
            let state = &state;
            position.orthogonal().filter_map(move |destination| {
                is_accepted(
                    state,
                    Command::Unload {
                        player: player.clone(),
                        transport,
                        cargo,
                        destination,
                    },
                )
                .then_some(ObservedUnload {
                    cargo,
                    cargo_kind,
                    destination,
                })
            })
        })
        .collect::<Vec<_>>();
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
    let player = lookup(&state, unit)?.owner.clone();
    Ok(is_accepted(&state, Command::DeleteUnit { player, unit }))
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
    let board = crate::semantic::Board::new(
        observation.board.width(),
        observation.board.height(),
        board_tiles(observation),
    )
    .map_err(|_| QueryError::Unprojectable("its board is not a whole rectangle"))?;

    let units = crate::semantic::UnitStore::new(reified_units(observation))
        .map_err(|_| QueryError::Unprojectable("it names one unit twice"))?;

    Ok(State {
        ruleset: observation.ruleset.clone(),
        settings: observation.settings.clone(),
        board,
        teams: observation.teams.clone(),
        players: observation.players.iter().map(reified_player).collect(),
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

fn board_tiles(observation: &Observation) -> Vec<crate::semantic::Tile> {
    let mut tiles = Vec::with_capacity(
        usize::from(observation.board.width()) * usize::from(observation.board.height()),
    );
    for y in 0..observation.board.height() {
        for x in 0..observation.board.width() {
            let observed = observation.board.tile(Pos::new(x, y));
            let mut tile = crate::semantic::Tile::new(observed.terrain);
            tile.owner = observed.owner.clone();
            tile.capture_points = observed.capture_points;
            tile.silo = observed.silo;
            tile.set_destructible_hp(observed.destructible_hp());
            tile.set_teleporter(observed.teleporter().cloned());
            tiles.push(tile);
        }
    }
    tiles
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

fn reified_units(observation: &Observation) -> Vec<Unit> {
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
        .map(|(observed, id)| Unit {
            id: *id,
            kind: observed.kind,
            owner: observed.owner.clone(),
            hp: observed.hp,
            fuel: observed.fuel,
            ammo: observed.ammo,
            action: observed.action,
            concealment: observed.concealment,
            location: observed.location.clone(),
        })
        .collect()
}

/// Enumerate every command `unit` could issue ending at `destination`.
pub fn actions_at(state: &State, unit: UnitId, destination: Pos) -> Result<ActionSet, QueryError> {
    let subject = lookup(state, unit)?;
    let player = subject.owner.clone();
    if !matches!(subject.location, Location::Board { .. }) {
        return Err(QueryError::UnitNotOnBoard(unit));
    }

    let Some(path) = reachable(state, unit)?.path_to(destination) else {
        return Ok(ActionSet::default());
    };
    let simple = |make: fn(PlayerId, UnitId, Vec<Pos>) -> Command| {
        is_accepted(state, make(player.clone(), unit, path.clone()))
    };
    let occupant = occupant(state, destination);

    Ok(ActionSet {
        wait: simple(|player, unit, path| Command::MoveWait { player, unit, path }),
        capture: simple(|player, unit, path| Command::MoveCapture { player, unit, path }),
        supply: simple(|player, unit, path| Command::MoveSupply { player, unit, path }),
        hide: simple(|player, unit, path| Command::MoveHide { player, unit, path }),
        reveal: simple(|player, unit, path| Command::MoveReveal { player, unit, path }),
        explode: simple(|player, unit, path| Command::MoveExplode { player, unit, path }),
        join: occupant.is_some_and(|target| {
            is_accepted(
                state,
                Command::MoveJoin {
                    player: player.clone(),
                    unit,
                    path: path.clone(),
                    target,
                },
            )
        }),
        load: occupant.is_some_and(|transport| {
            is_accepted(
                state,
                Command::MoveLoad {
                    player: player.clone(),
                    unit,
                    path: path.clone(),
                    transport,
                },
            )
        }),
        attack: attack_targets_along(state, unit, &path)?,
        repair: repair_targets(state, &player, unit, &path),
        launch: launch_targets(state, &player, unit, &path),
    })
}

/// Everything `unit` may attack from `from`, having moved there if it must.
///
/// Candidates are narrowed by range first — the geometry is cheap and the
/// reduction is not — and each survivor is then put to the reducer, so fire
/// mode, ammunition, visibility, target legality and every commander effect on
/// all of them are answered by the code that answers at execution time.
pub fn attack_targets(
    state: &State,
    unit: UnitId,
    from: Pos,
) -> Result<Vec<AttackTarget>, QueryError> {
    let subject = lookup(state, unit)?;
    let Location::Board { position: origin } = subject.location else {
        return Err(QueryError::UnitNotOnBoard(unit));
    };
    let path = if from == origin {
        vec![origin]
    } else {
        match reachable(state, unit)?.path_to(from) {
            Some(path) => path,
            None => return Ok(Vec::new()),
        }
    };
    attack_targets_along(state, unit, &path)
}

fn attack_targets_along(
    state: &State,
    unit: UnitId,
    path: &[Pos],
) -> Result<Vec<AttackTarget>, QueryError> {
    let subject = lookup(state, unit)?;
    let player = subject.owner.clone();
    let profile = ruleset::profile(subject.kind);
    if profile.fire_mode == FireMode::None {
        return Ok(Vec::new());
    }
    let from = *path.last().expect("a path holds at least its origin");

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
    let probe = |target: AttackTarget| {
        is_accepted(
            state,
            Command::MoveAttack {
                player: player.clone(),
                unit,
                path: path.to_vec(),
                target,
            },
        )
    };

    let mut targets = Vec::new();
    for candidate in state.units.iter() {
        let Location::Board { position } = candidate.location else {
            continue;
        };
        if candidate.id == unit || !in_range(position) {
            continue;
        }
        let target = AttackTarget::Unit { unit: candidate.id };
        if probe(target) {
            targets.push(target);
        }
    }
    for (position, tile) in state.board.rows().flatten() {
        if !in_range(position) || ruleset::terrain(tile.terrain).destructible.is_none() {
            continue;
        }
        let target = AttackTarget::Tile { position };
        if probe(target) {
            targets.push(target);
        }
    }
    Ok(targets)
}

/// Friendly units `unit` may repair after moving along `path`.
fn repair_targets(state: &State, player: &PlayerId, unit: UnitId, path: &[Pos]) -> Vec<UnitId> {
    let from = *path.last().expect("a path holds at least its origin");
    state
        .units
        .iter()
        .filter(|candidate| candidate.id != unit)
        .filter(|candidate| {
            matches!(candidate.location, Location::Board { position }
                if from.distance(position) == 1)
        })
        .map(|candidate| candidate.id)
        .filter(|target| {
            is_accepted(
                state,
                Command::MoveRepair {
                    player: player.clone(),
                    unit,
                    path: path.to_vec(),
                    target: *target,
                },
            )
        })
        .collect()
}

/// Every tile a silo under the end of `path` may be fired at.
fn launch_targets(state: &State, player: &PlayerId, unit: UnitId, path: &[Pos]) -> Vec<Pos> {
    let destination = *path.last().expect("a path holds at least its origin");
    // Launching is rare and the scan is over the whole board, so refuse early
    // unless the tile underfoot actually carries a silo.
    if state
        .board
        .get(destination)
        .is_none_or(|tile| tile.silo.is_none())
    {
        return Vec::new();
    }
    state
        .board
        .positions()
        .filter(|target| {
            is_accepted(
                state,
                Command::MoveLaunch {
                    player: player.clone(),
                    unit,
                    path: path.to_vec(),
                    target: *target,
                },
            )
        })
        .collect()
}

/// Which unit kinds `player` may produce at `position` right now.
///
/// A production site is a tile rather than a unit, so it has no reachable set
/// and no [`ActionSet`]; this is the same question asked of one.
pub fn production_options(state: &State, player: &PlayerId, position: Pos) -> Vec<UnitKindId> {
    UnitKindId::ALL
        .iter()
        .copied()
        .filter(|kind| {
            is_accepted(
                state,
                Command::ProduceUnit {
                    player: player.clone(),
                    position,
                    kind: *kind,
                },
            )
        })
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

/// Which tiles have disclosed occupants that block stopping or traversal.
struct Occupancy {
    stop: Vec<bool>,
    route: Vec<bool>,
    width: u8,
}

impl Occupancy {
    fn new(state: &State, mover: &Unit, team: &TeamId) -> Self {
        let width = state.board.width();
        let size = usize::from(width) * usize::from(state.board.height());
        let mut stop = vec![false; size];
        let mut route = vec![false; size];
        let view = AwbwVisibility.view(state, team);
        for unit in state.units.iter() {
            if unit.id == mover.id {
                continue;
            }
            let Location::Board { position } = unit.location else {
                continue;
            };
            if state.board.contains(position) && view.unit(unit) {
                let index = usize::from(position.y) * usize::from(width) + usize::from(position.x);
                stop[index] = true;
                route[index] = state
                    .find_player(&unit.owner)
                    .is_some_and(|owner| owner.team != *team);
            }
        }
        Self { stop, route, width }
    }

    fn index(&self, position: Pos) -> usize {
        usize::from(position.y) * usize::from(self.width) + usize::from(position.x)
    }

    fn blocks_stop(&self, position: Pos) -> bool {
        self.stop[self.index(position)]
    }

    fn blocks_route(&self, position: Pos) -> bool {
        self.route[self.index(position)]
    }
}

/// What entering `position` costs this unit, or `None` when it cannot.
fn entry_cost(state: &State, unit: &Unit, position: Pos, weather: WeatherKind) -> Option<u64> {
    let terrain = state.board.tile(position).terrain;
    let base = ruleset::movement_cost(terrain, weather, ruleset::profile(unit.kind).movement_class);
    // A teleporter's zero is terrain behaviour, not a finite cost for the
    // commander cost-set operators to replace (`spec/semantics/movement.md`).
    if ruleset::terrain_has(terrain, TerrainTrait::Teleporter) {
        base
    } else {
        commander::effective_movement_cost(state, unit, base)
    }
}

fn is_teleporter(state: &State, position: Pos) -> bool {
    ruleset::terrain_has(state.board.tile(position).terrain, TerrainTrait::Teleporter)
}

fn occupant(state: &State, position: Pos) -> Option<UnitId> {
    state
        .units
        .iter()
        .find(
            |unit| matches!(unit.location, Location::Board { position: held } if held == position),
        )
        .map(|unit| unit.id)
}

fn lookup(state: &State, unit: UnitId) -> Result<&Unit, QueryError> {
    state.units.get(unit).ok_or(QueryError::UnitNotFound(unit))
}

/// A frontier entry ordered so [`BinaryHeap`] pops the cheapest first.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Frontier {
    cost: u64,
    position: Pos,
}

impl Ord for Frontier {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .cost
            .cmp(&self.cost)
            .then_with(|| self.position.cmp(&other.position))
    }
}

impl PartialOrd for Frontier {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
