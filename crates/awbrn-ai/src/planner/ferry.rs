//! Ferry: orders that the Hard policy does not give when a game is decided by
//! properties that its capturers cannot reach.
//!
//! The Hard policy walks a capturer to properties on its own landmass. It
//! does not carry a capturer across water. On some maps, small islands hold
//! neutral properties that no side takes in 35 days, and the property count
//! at the day limit then ends in a draw. The ferry in this module sends a
//! spare capturer to such a property in a transport.
//!
//! All rules read the observed position only, so the result does not change
//! when the host builds the agent again. The rules do not apply in fog.

use std::collections::BinaryHeap;

use awvm::ruleset::{self, MovementClass, TerrainTrait, UnitKind, WeatherKind};
use awvm::semantic::{CellIdx, Location, PlayerIdx, Pos, State, TileOwner, Unit, UnitId};
use awvm::session::{LegalScope, LegalVisitor, Order, OrderKind, Session};

use crate::agent::Play;
use crate::threat;

/// The most transports that the ferry uses at one time.
const FERRY_TRANSPORTS: usize = 2;

/// A distance that is not reachable.
const FAR: u32 = u32::MAX;

/// The foot landmasses of a board: a component number for each cell, or
/// `None` for a cell that a foot unit cannot enter.
struct Landmasses {
    width: usize,
    component: Vec<Option<u32>>,
}

impl Landmasses {
    fn of(state: &State) -> Self {
        let dimensions = state.board.dimensions();
        let width = usize::from(dimensions.width());
        let height = usize::from(dimensions.height());
        let mut component = vec![None; width * height];
        let passable = |x: usize, y: usize| {
            let position = Pos::new(x as u8, y as u8);
            state.board.get(position).is_some_and(|tile| {
                ruleset::movement_cost(tile.terrain, WeatherKind::Clear, MovementClass::Foot)
                    .is_some()
            })
        };
        let mut next = 0;
        let mut stack = Vec::new();
        for start in 0..width * height {
            if component[start].is_some() || !passable(start % width, start / width) {
                continue;
            }
            component[start] = Some(next);
            stack.push(start);
            while let Some(cell) = stack.pop() {
                let (x, y) = (cell % width, cell / width);
                for (nx, ny) in neighbours(x, y, width, height) {
                    let index = ny * width + nx;
                    if component[index].is_none() && passable(nx, ny) {
                        component[index] = Some(next);
                        stack.push(index);
                    }
                }
            }
            next += 1;
        }
        Self { width, component }
    }

    fn at(&self, position: Pos) -> Option<u32> {
        let index = usize::from(position.y) * self.width + usize::from(position.x);
        self.component.get(index).copied().flatten()
    }
}

fn neighbours(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> impl Iterator<Item = (usize, usize)> {
    [
        (x.wrapping_sub(1), y),
        (x + 1, y),
        (x, y.wrapping_sub(1)),
        (x, y + 1),
    ]
    .into_iter()
    .filter(move |(nx, ny)| *nx < width && *ny < height)
}

fn board_position(unit: &Unit) -> Option<Pos> {
    match unit.location {
        Location::Board { position } => Some(position),
        Location::Cargo { .. } => None,
    }
}

/// Whether a unit kind can carry a capturer across water.
fn ferries(kind: UnitKind) -> bool {
    let profile = ruleset::profile(kind);
    let Some(transport) = profile.transport else {
        return false;
    };
    matches!(
        profile.movement_class,
        MovementClass::Air | MovementClass::Sea | MovementClass::Lander
    ) && transport.cargo.contains(UnitKind::Infantry)
}

/// The facts of one position that the ferry reads.
struct Ferry<'a> {
    state: &'a State,
    seat: PlayerIdx,
    land: Landmasses,
    /// The landmasses that hold a property to take.
    targets: Vec<u32>,
    /// The cells of the properties to take.
    target_cells: Vec<Pos>,
}

impl<'a> Ferry<'a> {
    fn new(state: &'a State, seat: PlayerIdx) -> Self {
        let land = Landmasses::of(state);
        let hostile = |owner: PlayerIdx| threat::hostile(state, seat, owner);
        // Landmasses with an own capturer, an enemy unit, or an enemy
        // production site are not targets.
        let mut closed = Vec::new();
        for unit in state.units.iter() {
            let Some(position) = board_position(unit) else {
                continue;
            };
            let Some(component) = land.at(position) else {
                continue;
            };
            let own_capturer = unit.owner == seat && ruleset::profile(unit.kind).can_capture;
            if own_capturer || hostile(unit.owner) {
                closed.push(component);
            }
        }
        for (position, tile) in state.board.iter() {
            if let TileOwner::Owned(holder) = tile.owner
                && hostile(holder)
                && (ruleset::terrain_has(tile.terrain, TerrainTrait::ProducesGround)
                    || ruleset::terrain_has(tile.terrain, TerrainTrait::CaptureDefeatsOwner))
                && let Some(component) = land.at(position)
            {
                closed.push(component);
            }
        }
        // Capturers in transports already go to a landmass.
        let mut targets = Vec::new();
        let mut target_cells = Vec::new();
        for (position, tile) in state.board.iter() {
            if !ruleset::terrain_has(tile.terrain, TerrainTrait::Capturable) {
                continue;
            }
            let takeable = match tile.owner {
                TileOwner::NotOwnable => false,
                TileOwner::Neutral => true,
                TileOwner::Owned(holder) => hostile(holder),
            };
            let Some(component) = land.at(position) else {
                continue;
            };
            if takeable && !closed.contains(&component) {
                if !targets.contains(&component) {
                    targets.push(component);
                }
                target_cells.push(position);
            }
        }
        Self {
            state,
            seat,
            land,
            targets,
            target_cells,
        }
    }

    fn is_target(&self, position: Pos) -> bool {
        self.land
            .at(position)
            .is_some_and(|component| self.targets.contains(&component))
    }

    /// Whether the landmass of `position` has no property left to take.
    fn done(&self, position: Pos) -> bool {
        let Some(component) = self.land.at(position) else {
            return false;
        };
        !self.state.board.iter().any(|(at, tile)| {
            self.land.at(at) == Some(component)
                && ruleset::terrain_has(tile.terrain, TerrainTrait::Capturable)
                && match tile.owner {
                    TileOwner::NotOwnable => false,
                    TileOwner::Neutral => true,
                    TileOwner::Owned(holder) => holder != self.seat,
                }
        })
    }

    /// Own capturers that stand on a landmass with no property left to
    /// take, in roster order.
    fn spare_capturers(&self) -> Vec<(UnitId, Pos)> {
        self.state
            .units
            .iter()
            .filter(|unit| unit.owner == self.seat && ruleset::profile(unit.kind).can_capture)
            .filter_map(|unit| Some((unit.id, board_position(unit)?)))
            .filter(|(_, position)| self.done(*position))
            .collect()
    }

    /// Movement cost distances for `class` from the `sources`, ignoring
    /// units.
    fn distances(&self, class: MovementClass, sources: &[Pos]) -> Vec<u32> {
        let dimensions = self.state.board.dimensions();
        let width = usize::from(dimensions.width());
        let height = usize::from(dimensions.height());
        let mut distance = vec![FAR; width * height];
        let mut queue = BinaryHeap::new();
        for source in sources {
            let index = usize::from(source.y) * width + usize::from(source.x);
            distance[index] = 0;
            queue.push(std::cmp::Reverse((0u32, index)));
        }
        while let Some(std::cmp::Reverse((cost, index))) = queue.pop() {
            if cost > distance[index] {
                continue;
            }
            for (nx, ny) in neighbours(index % width, index / width, width, height) {
                let Some(tile) = self.state.board.get(Pos::new(nx as u8, ny as u8)) else {
                    continue;
                };
                let Some(step) = ruleset::movement_cost(tile.terrain, WeatherKind::Clear, class)
                else {
                    continue;
                };
                let next = cost.saturating_add(step as u32);
                let target = ny * width + nx;
                if next < distance[target] {
                    distance[target] = next;
                    queue.push(std::cmp::Reverse((next, target)));
                }
            }
        }
        distance
    }

    fn index(&self, position: Pos) -> usize {
        usize::from(position.y) * usize::from(self.state.board.dimensions().width())
            + usize::from(position.x)
    }

    /// Cells where a transport of `class` can stand and put a capturer down
    /// on a target landmass.
    fn drop_cells(&self, class: MovementClass) -> Vec<Pos> {
        let mut cells = Vec::new();
        for (position, tile) in self.state.board.iter() {
            if ruleset::movement_cost(tile.terrain, WeatherKind::Clear, class).is_none() {
                continue;
            }
            if position.orthogonal().any(|next| self.is_target(next)) {
                cells.push(position);
            }
        }
        cells
    }
}

/// The legal orders of one unit.
fn unit_orders(session: &Session, unit: UnitId) -> Vec<Order> {
    struct Collect<'a>(&'a mut Vec<Order>);
    impl LegalVisitor for Collect<'_> {
        const DELETE: bool = false;

        fn order(&mut self, order: Order) {
            self.0.push(order);
        }
    }
    let mut orders = Vec::new();
    session.legal().visit_scoped(
        LegalScope {
            units: &[unit],
            unitless: false,
        },
        &mut Collect(&mut orders),
    );
    orders
}

/// The legal production orders of the position.
fn production_orders(session: &Session) -> Vec<Order> {
    struct Collect<'a>(&'a mut Vec<Order>);
    impl LegalVisitor for Collect<'_> {
        const DELETE: bool = false;

        fn order(&mut self, order: Order) {
            if matches!(order.kind(), OrderKind::Produce(_)) {
                self.0.push(order);
            }
        }
    }
    let mut orders = Vec::new();
    session.legal().visit_scoped(
        LegalScope {
            units: &[],
            unitless: true,
        },
        &mut Collect(&mut orders),
    );
    orders
}

/// The ferry plays for the position in `session`, in the order to play
/// them.
///
/// The plays are for the active seat. A play can become illegal after an
/// earlier play of the turn. The planner then skips it.
pub(crate) fn ferry_plays(session: &Session) -> Vec<Play> {
    let state = session.state();
    let Some(seat) = state.players.seat(&state.turn.active_player) else {
        return Vec::new();
    };
    if state.settings.fog {
        return Vec::new();
    }
    let ferry = Ferry::new(state, seat);
    if ferry.targets.is_empty() {
        return Vec::new();
    }
    let dimensions = state.board.dimensions();
    let cell = |position: Pos| dimensions.cell_index(position);
    let position_of = |cell: CellIdx| dimensions.position_of(cell);
    let mut plays = Vec::new();
    let mut used = 0;

    // Transports that carry a capturer go to a target and put it down.
    let mut empty = Vec::new();
    for unit in state.units.iter() {
        if unit.owner != seat || !ferries(unit.kind) || board_position(unit).is_none() {
            continue;
        }
        let cargo: Vec<&Unit> = state
            .units
            .iter()
            .filter(|cargo| {
                matches!(cargo.location, Location::Cargo { transport, .. } if transport == unit.id)
            })
            .collect();
        let Some(capturer) = cargo
            .iter()
            .find(|cargo| ruleset::profile(cargo.kind).can_capture)
        else {
            if cargo.is_empty() {
                empty.push(unit);
            }
            continue;
        };
        used += 1;
        let orders = unit_orders(session, unit.id);
        // Put the capturer down now, on a target property first.
        let unload = orders
            .iter()
            .filter(|order| matches!(order.kind(), OrderKind::Unload(_)))
            .filter_map(|order| {
                let play = Play::from_order(session, *order)?;
                let at = position_of(order.destination())?;
                (play.cargo() == Some(capturer.id) && ferry.is_target(at)).then(|| {
                    (
                        !ferry.target_cells.contains(&at),
                        order.destination().get(),
                        play,
                    )
                })
            })
            .min_by_key(|(far, cell, _)| (*far, *cell));
        if let Some((_, _, play)) = unload {
            plays.push(play);
            continue;
        }
        let class = ruleset::profile(unit.kind).movement_class;
        let foot = ruleset::profile(capturer.kind).movement_class;
        let waits: Vec<(Pos, Order)> = orders
            .iter()
            .filter(|order| order.kind() == OrderKind::Wait)
            .filter_map(|order| Some((position_of(order.destination())?, *order)))
            .collect();
        // Move next to a target and put the capturer down there.
        let occupied = |position: Pos| {
            state
                .units
                .iter()
                .any(|other| board_position(other) == Some(position) && other.id != unit.id)
        };
        let drop = waits
            .iter()
            .filter_map(|(at, order)| {
                let landing = at
                    .orthogonal()
                    .filter(|next| ferry.is_target(*next) && !occupied(*next))
                    .filter(|next| {
                        state.board.get(*next).is_some_and(|tile| {
                            ruleset::movement_cost(tile.terrain, WeatherKind::Clear, foot).is_some()
                        })
                    })
                    .min_by_key(|next| {
                        (
                            !ferry.target_cells.contains(next),
                            cell(*next).map_or(u16::MAX, CellIdx::get),
                        )
                    })?;
                Some((
                    !ferry.target_cells.contains(&landing),
                    order.destination().get(),
                    *order,
                    landing,
                ))
            })
            .min_by_key(|(far, cell, _, _)| (*far, *cell));
        if let Some((_, _, order, landing)) = drop {
            let slot = match capturer.location {
                Location::Cargo { slot, .. } => u8::try_from(slot).ok(),
                Location::Board { .. } => None,
            };
            if let (Some(play), Some(slot), Some(landing)) =
                (Play::from_order(session, order), slot, cell(landing))
            {
                plays.push(play);
                plays.push(Play::unload(unit.id, capturer.id, landing, slot));
            }
            continue;
        }
        // Move closer to a cell where it can put the capturer down.
        let distance = ferry.distances(class, &ferry.drop_cells(class));
        let here = board_position(unit).map_or(FAR, |at| distance[ferry.index(at)]);
        let step = waits
            .iter()
            .map(|(at, order)| {
                (
                    distance[ferry.index(*at)],
                    order.destination().get(),
                    *order,
                )
            })
            .filter(|(d, _, _)| *d < here)
            .min_by_key(|(d, cell, _)| (*d, *cell));
        if let Some((_, _, order)) = step
            && let Some(play) = Play::from_order(session, order)
        {
            plays.push(play);
        }
    }

    // Empty transports pick up a spare capturer.
    let spare = ferry.spare_capturers();
    if spare.is_empty() {
        return plays;
    }
    let mut taken: Vec<UnitId> = Vec::new();
    for unit in empty.iter().take(FERRY_TRANSPORTS.saturating_sub(used)) {
        let Some(at) = board_position(unit) else {
            continue;
        };
        let Some(transport_cell) = cell(at) else {
            continue;
        };
        // A spare capturer that can board now.
        let load = spare
            .iter()
            .filter(|(capturer, _)| !taken.contains(capturer))
            .find_map(|(capturer, _)| {
                unit_orders(session, *capturer)
                    .into_iter()
                    .find(|order| {
                        order.kind() == OrderKind::Load && order.destination() == transport_cell
                    })
                    .and_then(|order| Some((*capturer, Play::from_order(session, order)?)))
            });
        if let Some((capturer, play)) = load {
            taken.push(capturer);
            plays.push(play);
            continue;
        }
        // Otherwise move to a cell that a spare capturer can walk onto.
        let sources: Vec<Pos> = spare
            .iter()
            .filter(|(capturer, _)| !taken.contains(capturer))
            .map(|(_, position)| *position)
            .collect();
        let walk = ferry.distances(MovementClass::Foot, &sources);
        let here = walk[ferry.index(at)];
        let step = unit_orders(session, unit.id)
            .into_iter()
            .filter(|order| order.kind() == OrderKind::Wait)
            .filter_map(|order| {
                let to = position_of(order.destination())?;
                Some((walk[ferry.index(to)], order.destination().get(), order))
            })
            .filter(|(d, _, _)| *d < here)
            .min_by_key(|(d, cell, _)| (*d, *cell));
        if let Some((_, _, order)) = step
            && let Some(play) = Play::from_order(session, order)
        {
            plays.push(play);
        }
    }

    // Without a transport, build the cheapest one that the funds allow.
    let has_transport = state
        .units
        .iter()
        .any(|unit| unit.owner == seat && ferries(unit.kind));
    if !has_transport {
        let build = production_orders(session)
            .into_iter()
            .filter_map(|order| match order.kind() {
                OrderKind::Produce(kind) if ferries(kind) => Some((
                    ruleset::profile(kind).cost,
                    order.destination().get(),
                    order,
                )),
                _ => None,
            })
            .min_by_key(|(cost, cell, _)| (*cost, *cell));
        if let Some((_, _, order)) = build
            && let Some(play) = Play::from_order(session, order)
        {
            plays.push(play);
        }
    }
    plays
}
