//! The contest of decisive captures ([`super::PlannerConfig::contest`]).
//!
//! A decisive property is one whose capture ends the match for its owner:
//! a headquarters, or the last lab on a map with no headquarters. Without
//! this module the planner undervalues the threat to one:
//!
//! - The reply estimate prices an enemy capture that starts on our
//!   headquarters at a capped share of a property, about the value of a
//!   tank, and it prices a lab like any property that pays nothing, so a
//!   block of the property or a kill of the capturer loses to a turn that
//!   wins a few thousand funds elsewhere.
//! - The reply check scores the position at the start of our next turn with
//!   the position score, which sees a capture in progress as a small share,
//!   even when the capture completes on the next enemy turn and no unit of
//!   ours can stop it.
//! - A block with a unit that enemy fire can destroy does not hold, but the
//!   estimate sees the blocked tile as safe.
//!
//! No part names a commander or a map: the decisive properties come from the
//! terrain traits, and the capture points and the powers come from the
//! engine.
//!
//! 1. [`decisive_tiles`]: the properties whose capture ends the match for
//!    their owner, with a share of the headquarters value.
//! 2. [`danger`]: at the start of our turn, the loss of a decisive property
//!    that our attacks cannot stop in time.
//! 3. [`run_bonus`]: at the start of our turn, the value of our capture of
//!    an enemy decisive property that completes now.
//! 4. [`plays`]: the orders that contest a decisive capture: attacks on the
//!    capturer, blocks by units that are hard to destroy, attacks from the
//!    decisive tile, and moves nearer to it.

use crate::agent::Play;
use crate::agents::greedy::{CaptureTurn, capture_strength, has_headquarters};
use awvm::ruleset::{self, TerrainTrait};
use awvm::semantic::{
    CAPTURE_REQUIRED_POINTS, CellIdx, Location, PlayerIdx, Pos, State, TileOwner, Unit, UnitId,
    UnitStore,
};
use awvm::session::{
    AttackCandidate, LegalScope, LegalVisitor, Order, OrderKind, OrderMask, Session, TargetKind,
};

use super::reply::PropertyValues;

/// The share of the headquarters value that a capture of a decisive property
/// costs when the same capturer completes it with its next capture.
///
/// The reply estimate prices such a capture that the enemy can start in its
/// reply at least at this share ([`super::reply::estimate`]). At the start
/// of our turn, [`danger`] counts it for a capture in progress that no unit
/// of ours can attack. The capture is still two enemy captures away, so the
/// share is small.
pub(crate) const TWO_TURN_SHARE: f64 = 0.05;

/// The share of the headquarters value of a lab of a player who holds more
/// than one lab, on a map with no headquarters. Its loss leaves the player
/// one capture from defeat.
pub(crate) const LAB_STEP_SHARE: f64 = 0.05;

/// The share of the headquarters value of our capture of an enemy decisive
/// property that completes in our turn, after the simulated enemy reply has
/// not stopped it ([`run_bonus`]).
pub(crate) const RUN_SHARE: f64 = 0.05;

/// The largest number of kill plans of one capturer.
pub(crate) const KILL_PLANS: usize = 2;

/// The enemy capturers on or next to a decisive property of `seat` with the
/// share 1.
pub(crate) fn capturers_near(state: &State, seat: PlayerIdx) -> Vec<UnitId> {
    let decisive: Vec<Pos> = decisive_tiles(state, seat)
        .into_iter()
        .filter(|(_, share)| *share >= 1.0)
        .map(|(at, _)| at)
        .collect();
    if decisive.is_empty() {
        return Vec::new();
    }
    state
        .units
        .iter()
        .filter(|unit| {
            unit.owner != seat
                && crate::threat::hostile(state, seat, unit.owner)
                && ruleset::profile(unit.kind).can_capture
        })
        .filter(|unit| {
            board_position(unit).is_some_and(|at| decisive.iter().any(|key| key.distance(at) <= 1))
        })
        .map(|unit| unit.id)
        .collect()
}

/// The largest number of attack plans on one capturer.
const ATTACK_PLANS: usize = 3;

/// The largest number of block plans for one decisive property.
const BLOCK_PLANS: usize = 2;

/// The largest number of plans that attack from a decisive tile.
const FROM_TILE_PLANS: usize = 2;

/// The largest number of units that get a plan to move near a decisive
/// property.
const APPROACH_UNITS: usize = 3;

/// Units farther than this from the property get no approach plan.
const APPROACH_RANGE: u64 = 10;

/// The properties of `seat` whose capture ends the match or brings it near,
/// with the share of the headquarters value that each one is worth.
///
/// A headquarters has the share 1. On a map with no headquarters, a lab has
/// the share 1 when it is the last lab of `seat`, and [`LAB_STEP_SHARE`]
/// when `seat` holds more than one.
pub(crate) fn decisive_tiles(state: &State, seat: PlayerIdx) -> Vec<(Pos, f64)> {
    let owned = |tile: &&awvm::semantic::Tile| tile.owner == TileOwner::Owned(seat);
    if has_headquarters(state) {
        return state
            .board
            .iter()
            .filter(|(_, tile)| {
                owned(tile) && ruleset::terrain_has(tile.terrain, TerrainTrait::CaptureDefeatsOwner)
            })
            .map(|(position, _)| (position, 1.0))
            .collect();
    }
    let labs: Vec<Pos> = state
        .board
        .iter()
        .filter(|(_, tile)| {
            owned(tile) && ruleset::terrain_has(tile.terrain, TerrainTrait::LabVictory)
        })
        .map(|(position, _)| position)
        .collect();
    let share = if labs.len() == 1 { 1.0 } else { LAB_STEP_SHARE };
    labs.into_iter().map(|lab| (lab, share)).collect()
}

/// The share of `position` in `decisive`, if it is there.
pub(crate) fn share_at(decisive: &[(Pos, f64)], position: Pos) -> Option<f64> {
    decisive
        .iter()
        .find(|(at, _)| *at == position)
        .map(|(_, share)| *share)
}

fn board_position(unit: &Unit) -> Option<Pos> {
    match unit.location {
        Location::Board { position } => Some(position),
        Location::Cargo { .. } => None,
    }
}

/// A copy of `state` without the unit `removed`.
pub(crate) fn without_unit(state: &State, removed: UnitId) -> Option<State> {
    let mut copy = state.clone();
    copy.units = UnitStore::new(
        state
            .units
            .iter()
            .filter(|unit| unit.id != removed)
            .cloned()
            .collect(),
    )
    .ok()?;
    Some(copy)
}

/// The loss of a decisive property of `seat` that the position score does
/// not see, in funds, at the start of the turn of `seat`.
///
/// For each enemy capturer on a decisive property of `seat`:
///
/// - If the capturer completes the capture with its next capture and the
///   attacks of `seat` cannot take enough health to stop it, the loss is the
///   share of the headquarters value. The damage of the attacks is the sum
///   of the middle forecast of each unit that can attack the capturer.
/// - If it completes the capture in two captures and no unit of `seat` can
///   attack it, the loss is [`TWO_TURN_SHARE`] of that value.
///
/// When the turn of another seat is active, the loss is zero.
pub(crate) fn danger(session: &Session, seat: PlayerIdx, values: PropertyValues) -> f64 {
    let state = session.state();
    if state.players.seat(&state.turn.active_player) != Some(seat) {
        return 0.0;
    }
    let decisive = decisive_tiles(state, seat);
    if decisive.is_empty() {
        return 0.0;
    }
    let mut loss = 0.0;
    for unit in state.units.iter() {
        if unit.owner == seat
            || !crate::threat::hostile(state, seat, unit.owner)
            || !ruleset::profile(unit.kind).can_capture
        {
            continue;
        }
        let Some(position) = board_position(unit) else {
            continue;
        };
        let Some(share) = share_at(&decisive, position) else {
            continue;
        };
        let left = state
            .board
            .tile(position)
            .capture_points
            .unwrap_or(CAPTURE_REQUIRED_POINTS);
        // The capture points that the capturer takes in its next turn at one
        // visible health, with the capture bonus of its commander and of each
        // power that its meter can pay for.
        let strength = |visual: u8| capture_strength(state, unit, visual, CaptureTurn::Next);
        let now = strength(unit.hp.div_ceil(10));
        if now >= left {
            let needed = stop_damage(unit.hp, left, strength);
            if damage_on(session, seat, unit.id) < f64::from(needed) {
                loss += share * values.headquarters;
            }
        } else if now.saturating_mul(2) >= left && damage_on(session, seat, unit.id) <= 0.0 {
            loss += share * TWO_TURN_SHARE * values.headquarters;
        }
    }
    loss
}

/// The value of our captures of enemy decisive properties that complete in
/// this turn of `seat`, in funds: [`RUN_SHARE`] of the share of the
/// headquarters value for each. When the turn of another seat is active,
/// the value is zero.
pub(crate) fn run_bonus(state: &State, seat: PlayerIdx, values: PropertyValues) -> f64 {
    if state.players.seat(&state.turn.active_player) != Some(seat) {
        return 0.0;
    }
    let mut bonus = 0.0;
    for (enemy, _) in state.players.seats() {
        if enemy == seat || !crate::threat::hostile(state, seat, enemy) {
            continue;
        }
        let decisive = decisive_tiles(state, enemy);
        for unit in state.units.iter().filter(|unit| unit.owner == seat) {
            let Some(position) = board_position(unit) else {
                continue;
            };
            let Some(share) = share_at(&decisive, position) else {
                continue;
            };
            if !ruleset::profile(unit.kind).can_capture {
                continue;
            }
            let left = state
                .board
                .tile(position)
                .capture_points
                .unwrap_or(CAPTURE_REQUIRED_POINTS);
            if capture_strength(state, unit, unit.hp.div_ceil(10), CaptureTurn::Now) >= left {
                bonus += share * RUN_SHARE * values.headquarters;
            }
        }
    }
    bonus
}

/// The least damage that leaves a capturer at `hp` health too weak to take
/// `left` points in one capture. `strength` gives the points of a capture at
/// one visible health.
fn stop_damage(hp: u8, left: u8, strength: impl Fn(u8) -> u8) -> u8 {
    let mut visual = hp.div_ceil(10);
    while visual > 0 && strength(visual) >= left {
        visual -= 1;
    }
    // A unit at `visual` visible health has at most `visual * 10` health.
    hp.saturating_sub(visual.saturating_mul(10))
}

/// The attacks of the active seat on enemy units at `targets`: for each
/// attacker and target, the best middle forecast and the order.
struct Attacks {
    targets: Vec<Pos>,
    found: Vec<(UnitId, Pos, f64, Order)>,
}

impl LegalVisitor for Attacks {
    const ATTACK_CONTEXT: bool = true;
    const ACTIONS: OrderMask = OrderMask::from_untargeted(&[]).with_targets(TargetKind::Attack);
    const DELETE: bool = false;

    fn order(&mut self, _order: Order) {}

    fn attack(&mut self, candidate: AttackCandidate<'_>) {
        let (Some(target), Some(forecast)) = (candidate.target_unit, candidate.forecast) else {
            return;
        };
        let Some(at) = board_position(target) else {
            return;
        };
        if !self.targets.contains(&at) {
            return;
        }
        let damage = (f64::from(forecast.attack.low) + f64::from(forecast.attack.high)) / 2.0;
        let attacker = candidate.attacker.id;
        match self
            .found
            .iter_mut()
            .find(|(unit, position, _, _)| *unit == attacker && *position == at)
        {
            Some(found) if damage > found.2 => {
                found.2 = damage;
                found.3 = candidate.order;
            }
            Some(_) => {}
            None => self.found.push((attacker, at, damage, candidate.order)),
        }
    }
}

/// The attacks of `seat`, the active seat, on the units at `targets`.
fn attacks_on(
    session: &Session,
    seat: PlayerIdx,
    targets: Vec<Pos>,
) -> Vec<(UnitId, Pos, f64, Order)> {
    let state = session.state();
    let units: Vec<UnitId> = state
        .units
        .iter()
        .filter(|unit| {
            unit.owner == seat && ruleset::profile(unit.kind).fire_mode != ruleset::FireMode::None
        })
        .map(|unit| unit.id)
        .collect();
    if units.is_empty() || targets.is_empty() {
        return Vec::new();
    }
    let mut visitor = Attacks {
        targets,
        found: Vec::new(),
    };
    session.legal().visit_scoped(
        LegalScope {
            units: &units,
            unitless: false,
        },
        &mut visitor,
    );
    visitor.found
}

/// The damage that the units of `seat`, the active seat, can do to `target`
/// in this turn: the sum over the units of the largest middle forecast.
fn damage_on(session: &Session, seat: PlayerIdx, target: UnitId) -> f64 {
    let Some(at) = session.state().units.get(target).and_then(board_position) else {
        return 0.0;
    };
    attacks_on(session, seat, vec![at])
        .iter()
        .map(|(_, _, damage, _)| damage)
        .sum()
}

/// The plays that contest the decisive properties of `seat`.
///
/// `session` must hold a position in the turn of `seat`. A decisive property
/// is contested when it is in `threatened` (an enemy capturer reaches it in
/// the reply estimate), when an enemy capturer stands on it or next to it,
/// or when an enemy capturer can reach it in two of its moves. For each
/// contested property:
///
/// - Attacks on each enemy capturer on it or next to it: for each capturer,
///   up to [`ATTACK_PLANS`] attacks by different units, the most damage
///   first. Damage slows a capture, and a kill stops it.
/// - Blocks by the [`BLOCK_PLANS`] units that can stand on it and that are
///   the hardest to destroy: the most army value first. The block plan of
///   the planner uses the cheapest unit, which enemy fire can often destroy.
/// - Up to [`FROM_TILE_PLANS`] attacks from the tile, which also block it.
/// - When no unit can stand on it: for each of the [`APPROACH_UNITS`] units
///   nearest to it, the wait that ends nearest to it.
///
/// Each play is one plan. The plays are in this order, without repeats.
pub(crate) fn plays(session: &Session, seat: PlayerIdx, threatened: &[CellIdx]) -> Vec<Play> {
    let state = session.state();
    let dimensions = state.board.dimensions();
    let decisive = decisive_tiles(state, seat);
    if decisive.is_empty() {
        return Vec::new();
    }
    let capturers: Vec<(Pos, u64)> = state
        .units
        .iter()
        .filter(|unit| {
            unit.owner != seat
                && crate::threat::hostile(state, seat, unit.owner)
                && ruleset::profile(unit.kind).can_capture
        })
        .filter_map(|unit| {
            let movement = ruleset::profile(unit.kind).movement.max(1);
            Some((board_position(unit)?, movement))
        })
        .collect();
    let contested: Vec<Pos> = decisive
        .iter()
        .filter(|(at, share)| {
            *share >= 1.0
                && (dimensions
                    .cell_index(*at)
                    .is_some_and(|cell| threatened.contains(&cell))
                    || capturers
                        .iter()
                        .any(|(from, movement)| from.distance(*at) <= 2 * movement + 1))
        })
        .map(|(at, _)| *at)
        .collect();
    if contested.is_empty() {
        return Vec::new();
    }
    let mut plays: Vec<Play> = Vec::new();
    let push = |plays: &mut Vec<Play>, play: Play| {
        if !plays.contains(&play) {
            plays.push(play);
        }
    };

    // Attacks on capturers on or next to a contested property.
    let near: Vec<Pos> = capturers
        .iter()
        .map(|(at, _)| *at)
        .filter(|at| contested.iter().any(|key| key.distance(*at) <= 1))
        .collect();
    let mut attacks = attacks_on(session, seat, near.clone());
    attacks.sort_by(|left, right| right.2.total_cmp(&left.2));
    for target in &near {
        let mut taken = 0;
        for (_, at, _, order) in &attacks {
            if taken >= ATTACK_PLANS {
                break;
            }
            if at == target
                && let Some(play) = Play::from_order(session, *order)
            {
                push(&mut plays, play);
                taken += 1;
            }
        }
    }

    // Blocks, attacks from the tile and approaches.
    struct Collector(Vec<Order>);
    impl LegalVisitor for Collector {
        const ACTIONS: OrderMask =
            OrderMask::from_untargeted(&[OrderKind::Wait]).with_targets(TargetKind::Attack);
        const DELETE: bool = false;

        fn order(&mut self, order: Order) {
            self.0.push(order);
        }
    }
    let own: Vec<&Unit> = state
        .units
        .iter()
        .filter(|unit| unit.owner == seat)
        .collect();
    let units: Vec<UnitId> = own.iter().map(|unit| unit.id).collect();
    let mut collector = Collector(Vec::new());
    session.legal().visit_scoped(
        LegalScope {
            units: &units,
            unitless: false,
        },
        &mut collector,
    );
    let orders: Vec<Play> = collector
        .0
        .into_iter()
        .filter_map(|order| Play::from_order(session, order))
        .collect();
    let army = |unit: Option<UnitId>| {
        unit.and_then(|unit| state.units.get(unit))
            .map_or(0, |unit| {
                ruleset::profile(unit.kind).cost * u64::from(unit.hp)
            })
    };
    for key in &contested {
        let Some(cell) = dimensions.cell_index(*key) else {
            continue;
        };
        let mut blocks: Vec<&Play> = orders
            .iter()
            .filter(|play| play.destination() == cell && play.kind() == OrderKind::Wait)
            .collect();
        blocks.sort_by_key(|play| std::cmp::Reverse(army(play.unit())));
        let blockable = !blocks.is_empty();
        for play in blocks.into_iter().take(BLOCK_PLANS) {
            push(&mut plays, *play);
        }
        let mut from_tile: Vec<&Play> = orders
            .iter()
            .filter(|play| {
                play.destination() == cell && matches!(play.kind(), OrderKind::Attack(_))
            })
            .collect();
        from_tile.sort_by_key(|play| std::cmp::Reverse(army(play.unit())));
        for play in from_tile.into_iter().take(FROM_TILE_PLANS) {
            push(&mut plays, *play);
        }
        if blockable {
            continue;
        }
        let distance = |position: Pos| position.distance(*key);
        let mut nearest: Vec<(u64, UnitId)> = own
            .iter()
            .filter_map(|unit| {
                let far = distance(board_position(unit)?);
                (far > 0 && far <= APPROACH_RANGE).then_some((far, unit.id))
            })
            .collect();
        nearest.sort_unstable();
        for (far, unit) in nearest.into_iter().take(APPROACH_UNITS) {
            let best = orders
                .iter()
                .filter(|play| play.unit() == Some(unit) && play.kind() == OrderKind::Wait)
                .filter_map(|play| {
                    let position = dimensions.position_of(play.destination())?;
                    Some((distance(position), play))
                })
                .min_by_key(|(closer, _)| *closer);
            if let Some((closer, play)) = best
                && closer < far
            {
                push(&mut plays, *play);
            }
        }
    }
    plays
}
