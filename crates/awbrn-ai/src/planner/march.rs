//! March: a capture that wins the game, made with a commander power.
//!
//! Some commander powers let a capturer take a whole property in one capture.
//! Sami's super power (Victory March) does this, and it also gives her foot
//! units two more movement. A capturer that stands near the enemy
//! headquarters when the meter can pay for such a power can then win the
//! game in one turn. Strong human players prepare this over many turns.
//!
//! This module has four parts. All of them read the effects of a power from
//! the engine. No part names a commander.
//!
//! 1. [`win_now`] looks for a capture that wins at once, with no power and
//!    with each legal power. The check is the same for all commanders. The
//!    planner uses it only for a seat with a power from [`decisive_powers`],
//!    so that the games of other commanders do not change.
//! 2. [`prepare`] makes a plan over many turns when a power of the seat
//!    gives a decisive capture. It holds the other powers, steers one or
//!    two capturers to tiles near the enemy headquarters, and holds the
//!    decisive power until a capturer is in reach.
//! 3. [`powered_captures`] finds the enemy captures of our headquarters, and
//!    of our other properties, that an affordable enemy power makes possible.
//!    The reply estimate reads it ([`super::reply::ReplyOptions::march`]).
//! 4. [`swing_turn`] is the swing rule. It uses a decisive power when the
//!    properties that it takes in this turn, less those taken without it,
//!    are a large part of the properties of the map, and it holds the other
//!    power when the meter can pay for the decisive power soon.

use awvm::commander::{self, PowerLevel};
use awvm::ruleset::{self, TerrainTrait};
use awvm::semantic::{
    CAPTURE_REQUIRED_POINTS, CellIdx, Location, PlayerIdx, Pos, PowerState, State, TileOwner, Unit,
    UnitId,
};
use awvm::session::{LegalScope, LegalVisitor, Order, OrderKind, OrderMask, Session};

use crate::agent::Play;
use crate::eval::Evaluator;
use crate::power::PowerHold;
use crate::threat::{self, ThreatMap};

use super::MeanLuck;

/// The most own turns until the meter can pay for the decisive power. A plan
/// that needs more turns is not made.
const HORIZON: u32 = 6;

/// The most turns that a capturer can be late. The meter can pay for the
/// power this number of turns before the capturer gets to a staging tile.
const SLACK: u32 = 2;

/// The most own turns that the plan holds a decisive power that the
/// meter can already pay for.
const MAX_HOLD: u32 = 3;

/// The most capturers that the plan steers.
const STEER_UNITS: usize = 2;

/// The lowest ratio of own army value to enemy army value. Below it, the
/// plan is not made, and the normal power style applies.
const MIN_ARMY_RATIO: f64 = 0.6;

/// The own army value of `seat` over the enemy army value, each scaled by
/// health. An enemy army below one thousand funds counts as one thousand.
fn army_ratio(state: &State, seat: PlayerIdx) -> f64 {
    let mut army = 0.0;
    let mut enemy_army = 0.0;
    for unit in state.units.iter() {
        let value = ruleset::profile(unit.kind).cost as f64 * f64::from(unit.hp) / 100.0;
        if unit.owner == seat {
            army += value;
        } else if threat::hostile(state, seat, unit.owner) {
            enemy_army += value;
        }
    }
    army / f64::max(enemy_army, 1_000.0)
}

/// The cells whose capture by `seat` can win the game.
///
/// These are the enemy headquarters. On a map with no headquarters, they are
/// the enemy labs.
fn decisive_targets(state: &State, seat: PlayerIdx) -> Vec<Pos> {
    let headquarters = state
        .board
        .tiles()
        .any(|tile| ruleset::terrain_has(tile.terrain, TerrainTrait::CaptureDefeatsOwner));
    let decisive = if headquarters {
        TerrainTrait::CaptureDefeatsOwner
    } else {
        TerrainTrait::LabVictory
    };
    state
        .board
        .iter()
        .filter(|(_, tile)| {
            ruleset::terrain_has(tile.terrain, decisive)
                && matches!(tile.owner, TileOwner::Owned(owner)
                    if owner != seat && threat::hostile(state, seat, owner))
        })
        .map(|(position, _)| position)
        .collect()
}

/// A copy of `state` where the power `level` of `seat` is active.
///
/// The copy changes only the power state, so the engine reads the movement,
/// the movement costs and the capture points of that power from it. The
/// instant effects of the power, such as a heal, are not applied.
fn powered(state: &State, seat: PlayerIdx, level: PowerLevel) -> Option<State> {
    let player = state.players.get(seat.get())?;
    let slot = u8::try_from(player.commanders.iter().position(|co| co.active)?).ok()?;
    let mut copy = state.clone();
    copy.player_mut(seat).power_state = match level {
        PowerLevel::Cop => PowerState::Cop {
            commander_slot: slot,
        },
        PowerLevel::Scop => PowerState::Scop {
            commander_slot: slot,
        },
    };
    Some(copy)
}

/// The meter cost of the power `level` of `seat`, if the commander has it.
fn power_cost(state: &State, seat: PlayerIdx, level: PowerLevel) -> Option<(u64, u64)> {
    let co = state
        .players
        .get(seat.get())?
        .commanders
        .iter()
        .find(|co| co.active)?;
    let cost = commander::power_activation_cost(co.id, level, co.power_uses)
        .ok()
        .flatten()?;
    Some((cost, co.power_charge))
}

/// Whether `unit` takes a whole property in one capture at full health with
/// the power `level` of its owner.
fn captures_at_once(state: &State, unit: &Unit, level: PowerLevel) -> bool {
    ruleset::profile(unit.kind).can_capture
        && commander::capture_points_with_power(state, unit, 10, Some(level))
            >= u64::from(CAPTURE_REQUIRED_POINTS)
}

/// The power levels of `seat` with which an own capturer takes a whole
/// property in one capture. The values come from the engine.
pub(crate) fn decisive_powers(state: &State, seat: PlayerIdx) -> Vec<PowerLevel> {
    [PowerLevel::Cop, PowerLevel::Scop]
        .into_iter()
        .filter(|level| power_cost(state, seat, *level).is_some())
        .filter(|level| {
            state
                .units
                .iter()
                .any(|unit| unit.owner == seat && captures_at_once(state, unit, *level))
        })
        .collect()
}

fn board_position(unit: &Unit) -> Option<Pos> {
    match unit.location {
        Location::Board { position } => Some(position),
        Location::Cargo { .. } => None,
    }
}

/// The legal orders of `units` that `keep` accepts.
fn orders_of(session: &Session, units: &[UnitId], keep: impl Fn(&Order) -> bool) -> Vec<Order> {
    struct Collect<'a, F: Fn(&Order) -> bool>(&'a mut Vec<Order>, F);
    impl<F: Fn(&Order) -> bool> LegalVisitor for Collect<'_, F> {
        const ACTIONS: OrderMask =
            OrderMask::from_untargeted(&[OrderKind::Wait, OrderKind::Capture]);
        const DELETE: bool = false;

        fn order(&mut self, order: Order) {
            if (self.1)(&order) {
                self.0.push(order);
            }
        }
    }
    let mut orders = Vec::new();
    session.legal().visit_scoped(
        LegalScope {
            units,
            unitless: false,
        },
        &mut Collect(&mut orders, keep),
    );
    orders
}

/// The legal power orders of the position.
fn power_orders(session: &Session) -> Vec<Order> {
    struct Powers<'a>(&'a mut Vec<Order>);
    impl LegalVisitor for Powers<'_> {
        fn order(&mut self, order: Order) {
            if matches!(order.kind(), OrderKind::Power(_)) {
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
        &mut Powers(&mut orders),
    );
    orders
}

/// The own capturers on the board.
fn capturers(state: &State, seat: PlayerIdx) -> Vec<UnitId> {
    state
        .units
        .iter()
        .filter(|unit| {
            unit.owner == seat
                && ruleset::profile(unit.kind).can_capture
                && board_position(unit).is_some()
        })
        .map(|unit| unit.id)
        .collect()
}

/// A capture of one of `cells` that wins the game for `seat` in `session`.
///
/// The engine plays each capture, and the result decides. The session is
/// back at its start position when this returns.
fn winning_capture(session: &mut Session, seat: PlayerIdx, cells: &[CellIdx]) -> Option<Play> {
    let units = capturers(session.state(), seat);
    if units.is_empty() {
        return None;
    }
    let orders = orders_of(session, &units, |order| {
        order.kind() == OrderKind::Capture && cells.contains(&order.destination())
    });
    for order in orders {
        let Some(play) = Play::from_order(session, order) else {
            continue;
        };
        let Ok(mark) = session.apply(order, &mut MeanLuck, &mut ()) else {
            continue;
        };
        let won = Evaluator::terminal_value(session.state(), seat).is_some_and(|value| value > 0.0);
        session.rewind(mark);
        if won {
            return Some(play);
        }
    }
    None
}

/// The plays of a capture that wins the game in this turn, if one exists.
///
/// The check tries no power first, and then each legal power. With a power,
/// the first play uses the power and the second play is the capture. The
/// engine applies the power, moves the capturer and completes the capture,
/// so the check finds a normal last capture of a headquarters and the
/// decisive capture of any commander power. A unit that a transport puts
/// down cannot act in the same turn, so the check moves only the capturers
/// on the board. The session is back at its start position when this
/// returns.
pub(crate) fn win_now(session: &mut Session, seat: PlayerIdx) -> Option<Vec<Play>> {
    let state = session.state();
    let dimensions = state.board.dimensions();
    let cells: Vec<CellIdx> = decisive_targets(state, seat)
        .into_iter()
        .filter_map(|position| dimensions.cell_index(position))
        .collect();
    if cells.is_empty() {
        return None;
    }
    if let Some(play) = winning_capture(session, seat, &cells) {
        return Some(vec![play]);
    }
    for order in power_orders(session) {
        let Some(power) = Play::from_order(session, order) else {
            continue;
        };
        let Ok(mark) = session.apply(order, &mut MeanLuck, &mut ()) else {
            continue;
        };
        let capture = winning_capture(session, seat, &cells);
        session.rewind(mark);
        if let Some(capture) = capture {
            return Some(vec![power, capture]);
        }
    }
    None
}

/// The own turns until the meter of `seat` can pay for the power `level`.
///
/// `rate` is the meter charge in one own turn. `None` when the meter does
/// not get there.
fn turns_to_power(
    state: &State,
    seat: PlayerIdx,
    level: PowerLevel,
    rate: Option<f64>,
) -> Option<u32> {
    let (cost, charge) = power_cost(state, seat, level)?;
    if charge >= cost {
        return Some(0);
    }
    let rate = rate.filter(|rate| *rate > 0.0)?;
    let turns = ((cost - charge) as f64 / rate).ceil();
    (turns <= f64::from(HORIZON)).then_some(turns as u32)
}

/// The plan of one turn.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct March {
    /// The powers that the plan holds.
    pub hold: PowerHold,
    /// The moves of the steered capturers, in order. A move can become
    /// illegal after an earlier play of the turn. The planner then skips it.
    pub steer: Vec<Play>,
    /// Whether the plan holds a decisive power that the meter can pay
    /// for.
    pub holding: bool,
    /// The own turns until the nearest capturer gets to a staging tile.
    pub runner_turns: u32,
}

/// One capturer that can get to a staging tile.
struct Runner {
    unit: UnitId,
    /// The own turns to walk to the nearest staging tile.
    turns: u32,
    /// The movement points to the nearest staging tile.
    points: u16,
    /// The movement points from each tile to the nearest staging tile.
    walk: Vec<Option<u16>>,
}

/// The plan of `seat` in `session` for a decisive capture with a power.
///
/// `rate` is the meter charge in one own turn, and `held` is the number of
/// earlier own turns in a row in which the plan held a decisive power
/// that the meter could pay for.
///
/// The plan exists only when all of these are true:
///
/// - A power of the seat lets an own capturer take a whole property in one
///   capture ([`decisive_powers`]). For all other commanders, this returns
///   `None`, and their play does not change.
/// - No power is active now, and no enemy capturer stands on a property of
///   ours whose capture defeats us.
/// - Own army value is at least [`MIN_ARMY_RATIO`] of the enemy army value.
/// - An enemy headquarters (or a lab on a map with no headquarters) has no
///   unit on it.
/// - The meter can pay for the power within [`HORIZON`] own turns.
/// - An own capturer can walk to a staging tile no more than [`SLACK`] turns
///   after the meter can pay. A staging tile is a tile from which the
///   capturer can get to the target with the power active. The engine gives
///   the movement and the movement costs of the power.
/// - The plan has held an affordable decisive power for fewer than
///   [`MAX_HOLD`] turns.
///
/// While the plan exists, the other powers are held. A decisive super power
/// is held when the meter can pay for it, because the caller looks for a win
/// with [`win_now`] first. A decisive normal power is not held. No commander
/// has a normal power that takes a property in one capture.
///
/// Up to [`STEER_UNITS`] capturers that must move now to get to a staging
/// tile in time walk toward one. A tile where the enemy
/// can take half of the unit or more is used only when no safer tile is
/// equally near.
pub(crate) fn prepare(
    session: &Session,
    seat: PlayerIdx,
    rate: Option<f64>,
    held: u32,
) -> Option<March> {
    let state = session.state();
    let player = state.players.get(seat.get())?;
    if !matches!(player.power_state, PowerState::None) {
        return None;
    }
    let level = *decisive_powers(state, seat).first()?;
    if crate::power::decisive_capture(state, seat) || army_ratio(state, seat) < MIN_ARMY_RATIO {
        return None;
    }
    let turns = turns_to_power(state, seat, level, rate)?;
    if turns == 0 && held >= MAX_HOLD {
        return None;
    }
    let targets: Vec<Pos> = decisive_targets(state, seat)
        .into_iter()
        .filter(|target| {
            !state
                .units
                .iter()
                .any(|unit| board_position(unit) == Some(*target))
        })
        .collect();
    if targets.is_empty() {
        return None;
    }
    let powered_state = powered(state, seat, level)?;
    let dimensions = state.board.dimensions();
    let index = |position: Pos| {
        dimensions
            .cell_index(position)
            .map(|cell| usize::from(cell.get()))
    };

    let mut runners = Vec::new();
    let mut reach_points = Vec::new();
    for id in capturers(state, seat) {
        let Some(unit) = state.units.get(id) else {
            continue;
        };
        let Some(at) = board_position(unit) else {
            continue;
        };
        if !captures_at_once(state, unit, level) {
            continue;
        }
        // A capturer in the middle of a capture keeps capturing.
        let tile = state.board.tile(at);
        if tile.owner != TileOwner::Owned(seat)
            && tile
                .capture_points
                .is_some_and(|points| points < CAPTURE_REQUIRED_POINTS)
        {
            continue;
        }
        let profile = ruleset::profile(unit.kind);
        let reach =
            commander::effective_move(&powered_state, unit, profile.movement, profile.domain);
        let walk_move = commander::effective_move(state, unit, profile.movement, profile.domain);
        let (Ok(reach), Ok(walk_move)) = (u16::try_from(reach), u16::try_from(walk_move)) else {
            continue;
        };
        if walk_move == 0 {
            continue;
        }
        // The movement points from each tile to a target with the power.
        reach_points.clear();
        let mut travel = awvm::query::Travel::open(&powered_state, seat)?;
        travel.points_to(
            profile.movement_class,
            reach,
            targets.iter().copied(),
            &mut reach_points,
        );
        let staging: Vec<Pos> = dimensions
            .positions()
            .filter(|position| {
                index(*position)
                    .and_then(|cell| reach_points.get(cell).copied().flatten())
                    .is_some_and(|points| points <= reach && points > 0)
            })
            .collect();
        if staging.is_empty() {
            continue;
        }
        let mut walk = Vec::new();
        let mut travel = awvm::query::Travel::open(state, seat)?;
        travel.points_to(profile.movement_class, walk_move, staging, &mut walk);
        let Some(points) = index(at).and_then(|cell| walk.get(cell).copied().flatten()) else {
            continue;
        };
        runners.push(Runner {
            unit: id,
            turns: u32::from(points.div_ceil(walk_move)),
            points,
            walk,
        });
    }
    runners.sort_by_key(|runner| (runner.turns, runner.points, runner.unit.get()));
    let best = runners.first()?;
    if best.turns > turns + SLACK {
        return None;
    }
    let runner_turns = best.turns;

    let hold = PowerHold {
        seat,
        cop: level != PowerLevel::Cop,
        scop: level != PowerLevel::Scop || turns == 0,
    };

    // Steer the capturers that must move now to be in reach in time.
    let mut threat = ThreatMap::new();
    threat.build(session, seat);
    let mut steer = Vec::new();
    let mut used: Vec<CellIdx> = Vec::new();
    for runner in runners
        .iter()
        .filter(|runner| runner.turns + 1 >= turns)
        .take(STEER_UNITS)
    {
        let Some(unit) = state.units.get(runner.unit) else {
            continue;
        };
        let cost = ruleset::profile(unit.kind).cost as f64;
        let at = board_position(unit).and_then(|at| dimensions.cell_index(at));
        let waits = orders_of(session, &[runner.unit], |order| {
            order.kind() == OrderKind::Wait
        });
        let choice = waits
            .into_iter()
            .filter(|order| !used.contains(&order.destination()))
            .filter_map(|order| {
                let cell = order.destination();
                let left = runner
                    .walk
                    .get(usize::from(cell.get()))
                    .copied()
                    .flatten()?;
                let danger = threat.immediate(cell, unit.kind) >= cost * 0.5;
                // In reach already: stay on a staging tile, and stay put
                // when that is safe.
                let key = if runner.points == 0 {
                    if left > 0 {
                        return None;
                    }
                    (danger, u16::from(Some(cell) != at), cell.get())
                } else {
                    (danger, left, cell.get())
                };
                Some((key, order))
            })
            .min_by_key(|(key, _)| *key);
        if let Some((_, order)) = choice
            && let Some(play) = Play::from_order(session, order)
        {
            used.push(order.destination());
            steer.push(play);
        }
    }
    Some(March {
        hold,
        steer,
        holding: turns == 0,
        runner_turns,
    })
}

/// The least swing gain that fires a decisive power, as a share of the
/// properties of the map. See [`swing_turn`].
const SWING_SHARE: f64 = 0.06;

/// The swing gain that fires a decisive power also when a headquarters
/// capture is near, as a share of the properties of the map.
const LARGE_SWING_SHARE: f64 = 0.12;

/// The own turns within which a capturer of the march plan must get to a
/// staging tile for the plan to wait for the headquarters capture.
const NEAR_STAGING: u32 = 2;

/// What the swing rule does in one turn.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SwingTurn {
    /// The swing rule does nothing.
    None,
    /// Hold the other powers: the meter pays for the decisive power within
    /// [`HORIZON`] turns, and the swing is large enough now.
    Hold(PowerHold),
    /// Use the decisive power with the first play now, and then make the
    /// captures of the other plays: one capturer for each property, the
    /// most valuable first ([`completed_captures`]).
    Fire(Play, Vec<Play>),
}

/// The swing gain of the decisive power `level` of `seat`, as a share of the
/// properties of the map.
///
/// The gain is the [`Swing::value`] of the captures that complete in this
/// turn with the power, less the value of those that complete without it.
pub(crate) fn swing_share(state: &State, seat: PlayerIdx, level: PowerLevel) -> f64 {
    let total = properties(state);
    if total == 0 {
        return 0.0;
    }
    let gain = swing(state, seat, Some(level)).value() - swing(state, seat, None).value();
    gain / f64::from(total)
}

/// The swing rule for `seat` in `session`.
///
/// A decisive power also takes many properties at once. In the human games,
/// a Sami super power that did not win took a median of 8.7% of the map's
/// properties. The swing gain at the start of the turn was at least
/// [`SWING_SHARE`] on 13 of 24 super power turns, and on none of 14 turns
/// when the meter could pay but the player did not use it.
///
/// - When the meter can pay for the decisive power now, the power is legal,
///   and the gain is at least [`SWING_SHARE`], the rule fires the power. When
///   the march plan `plan` has a capturer within [`NEAR_STAGING`] turns of a
///   staging tile, the rule fires only for a gain of at least
///   [`LARGE_SWING_SHARE`], and otherwise waits for the headquarters.
/// - When the meter pays within [`HORIZON`] turns and the gain is at least
///   [`SWING_SHARE`], the rule holds the other powers.
/// - Otherwise the rule does nothing, and the normal power style applies.
pub(crate) fn swing_turn(
    session: &Session,
    seat: PlayerIdx,
    rate: Option<f64>,
    plan: Option<&March>,
) -> SwingTurn {
    let state = session.state();
    let Some(player) = state.players.get(seat.get()) else {
        return SwingTurn::None;
    };
    if !matches!(player.power_state, PowerState::None) {
        return SwingTurn::None;
    }
    let Some(level) = decisive_powers(state, seat).first().copied() else {
        return SwingTurn::None;
    };
    let Some(turns) = turns_to_power(state, seat, level, rate) else {
        return SwingTurn::None;
    };
    let share = swing_share(state, seat, level);
    if share < SWING_SHARE {
        return SwingTurn::None;
    }
    if turns > 0 {
        return SwingTurn::Hold(PowerHold {
            seat,
            cop: level != PowerLevel::Cop,
            scop: level != PowerLevel::Scop,
        });
    }
    let near = plan.is_some_and(|plan| plan.runner_turns <= NEAR_STAGING);
    if near && share < LARGE_SWING_SHARE {
        return SwingTurn::None;
    }
    let Some(order) = power_orders(session)
        .into_iter()
        .find(|order| order.kind() == OrderKind::Power(level))
    else {
        return SwingTurn::None;
    };
    let Some(power) = Play::from_order(session, order) else {
        return SwingTurn::None;
    };
    // The captures come from the position after the power, as the engine
    // plays it.
    let mut after = Session::new(state.clone());
    if after.apply(order, &mut MeanLuck, &mut ()).is_err() {
        return SwingTurn::None;
    }
    let captures = completed_captures(&after, seat)
        .into_iter()
        .map(|(unit, cell, _)| Play::new(unit, cell, OrderKind::Capture))
        .collect::<Vec<_>>();
    SwingTurn::Fire(power, captures)
}

/// The value of an enemy property in a swing. Its capture moves income both
/// ways.
const ENEMY_PROPERTY: f64 = 1.0;

/// The value of a neutral property in a swing.
const NEUTRAL_PROPERTY: f64 = 0.5;

/// The properties that the capturers of one seat can take in one turn.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Swing {
    /// Enemy properties taken.
    pub enemy: u32,
    /// Neutral properties taken.
    pub neutral: u32,
}

impl Swing {
    /// The value of the swing, in enemy properties. A neutral property is
    /// worth half of an enemy property.
    pub(crate) fn value(&self) -> f64 {
        f64::from(self.enemy) * ENEMY_PROPERTY + f64::from(self.neutral) * NEUTRAL_PROPERTY
    }
}

/// The number of properties on the board.
pub(crate) fn properties(state: &State) -> u32 {
    let count = state
        .board
        .tiles()
        .filter(|tile| ruleset::terrain_has(tile.terrain, TerrainTrait::Capturable))
        .count();
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// The captures that the active seat `seat` completes in `session` in this
/// turn, one capturer for each property and one property for each
/// capturer, as `(unit, cell, value)`.
///
/// The engine gives the legal capture orders and the capture points. A
/// capture completes when the points of the capturer are at least the
/// points left on the property. A unit on a property blocks it. The
/// assignment is greedy: the most valuable capture first, and among equal
/// captures, the capturer with the fewest options first.
fn completed_captures(session: &Session, seat: PlayerIdx) -> Vec<(UnitId, CellIdx, f64)> {
    let state = session.state();
    let units = capturers(state, seat);
    if units.is_empty() {
        return Vec::new();
    }
    let dimensions = state.board.dimensions();
    let mut options: Vec<(UnitId, CellIdx, f64)> = Vec::new();
    for order in orders_of(session, &units, |order| order.kind() == OrderKind::Capture) {
        let Some(unit) = order
            .unit()
            .and_then(|index| state.units.at(usize::from(index.get())))
        else {
            continue;
        };
        let cell = order.destination();
        let Some(position) = dimensions.position_of(cell) else {
            continue;
        };
        let tile = state.board.tile(position);
        let value = match tile.owner {
            TileOwner::Neutral => NEUTRAL_PROPERTY,
            TileOwner::Owned(owner) if owner != seat && threat::hostile(state, seat, owner) => {
                ENEMY_PROPERTY
            }
            _ => continue,
        };
        let left = tile.capture_points.unwrap_or(CAPTURE_REQUIRED_POINTS);
        let points =
            commander::effective_capture_points(state, unit, u64::from(unit.hp.div_ceil(10)));
        if points >= u64::from(left) {
            options.push((unit.id, cell, value));
        }
    }
    let choices = |unit: UnitId| {
        options
            .iter()
            .filter(|(other, _, _)| *other == unit)
            .count()
    };
    let mut ranked: Vec<(f64, usize, u32, u16, UnitId, CellIdx)> = options
        .iter()
        .map(|(unit, cell, value)| (*value, choices(*unit), unit.get(), cell.get(), *unit, *cell))
        .collect();
    ranked.sort_by(|left, right| {
        right
            .0
            .total_cmp(&left.0)
            .then(left.1.cmp(&right.1))
            .then(left.2.cmp(&right.2))
            .then(left.3.cmp(&right.3))
    });
    let mut taken: Vec<(UnitId, CellIdx, f64)> = Vec::new();
    for (value, _, _, _, unit, cell) in ranked {
        if taken
            .iter()
            .any(|(other, at, _)| *other == unit || *at == cell)
        {
            continue;
        }
        taken.push((unit, cell, value));
    }
    taken
}

fn swing_of(captures: &[(UnitId, CellIdx, f64)]) -> Swing {
    let mut swing = Swing::default();
    for (_, _, value) in captures {
        if *value >= ENEMY_PROPERTY {
            swing.enemy += 1;
        } else {
            swing.neutral += 1;
        }
    }
    swing
}

/// The properties that the capturers of `seat` can take in this turn, with
/// the power `level` active, or with the current power state when `level`
/// is `None`.
///
/// `seat` must be the active seat. The power is set on a copy of the state,
/// so the meter does not need to pay for it.
pub(crate) fn swing(state: &State, seat: PlayerIdx, level: Option<PowerLevel>) -> Swing {
    if state.players.seat(&state.turn.active_player) != Some(seat) {
        return Swing::default();
    }
    let copy = match level {
        Some(level) => match powered(state, seat, level) {
            Some(copy) => copy,
            None => return Swing::default(),
        },
        None => state.clone(),
    };
    swing_of(&completed_captures(&Session::new(copy), seat))
}

/// Our headquarters cells that the active enemy can capture in this turn
/// with a power that its meter can pay for and that lets a capturer take a
/// whole property in one capture.
///
/// On a map with no headquarters, our last lab counts. The engine gives the
/// movement, the movement costs and the capture points of the power. The
/// result can also contain a capture that the enemy can make with no power.
/// The reply estimate keeps the larger loss for each cell.
#[cfg(test)]
pub(crate) fn powered_headquarters_captures(state: &State, ours: PlayerIdx) -> Vec<CellIdx> {
    powered_captures(state, ours)
        .into_iter()
        .filter_map(|(cell, headquarters)| headquarters.then_some(cell))
        .collect()
}

/// Our properties that the active enemy can take in this turn with a power
/// that its meter can pay for and that lets a capturer take a whole property
/// in one capture, as `(cell, decisive)`.
///
/// `decisive` is true for a headquarters, or for our last lab on a map with
/// no headquarters. The result also has the other properties of ours that
/// the enemy capturers take with the power, one property for each capturer
/// ([`swing`]).
pub(crate) fn powered_captures(state: &State, ours: PlayerIdx) -> Vec<(CellIdx, bool)> {
    let Some(enemy) = state.players.seat(&state.turn.active_player) else {
        return Vec::new();
    };
    if enemy == ours
        || !threat::hostile(state, ours, enemy)
        || !matches!(
            state.players.get(enemy.get()).map(|p| &p.power_state),
            Some(PowerState::None)
        )
    {
        return Vec::new();
    }
    let levels: Vec<PowerLevel> = decisive_powers(state, enemy)
        .into_iter()
        .filter(|level| {
            power_cost(state, enemy, *level).is_some_and(|(cost, charge)| charge >= cost)
        })
        .collect();
    if levels.is_empty() {
        return Vec::new();
    }
    let headquarters = state
        .board
        .tiles()
        .any(|tile| ruleset::terrain_has(tile.terrain, TerrainTrait::CaptureDefeatsOwner));
    let ours_decisive: Vec<Pos> = if headquarters {
        state
            .board
            .iter()
            .filter(|(_, tile)| {
                tile.owner == TileOwner::Owned(ours)
                    && ruleset::terrain_has(tile.terrain, TerrainTrait::CaptureDefeatsOwner)
            })
            .map(|(position, _)| position)
            .collect()
    } else {
        let labs: Vec<Pos> = state
            .board
            .iter()
            .filter(|(_, tile)| {
                tile.owner == TileOwner::Owned(ours)
                    && ruleset::terrain_has(tile.terrain, TerrainTrait::LabVictory)
            })
            .map(|(position, _)| position)
            .collect();
        if labs.len() == 1 { labs } else { Vec::new() }
    };
    let dimensions = state.board.dimensions();
    let cells: Vec<CellIdx> = ours_decisive
        .iter()
        .filter_map(|position| dimensions.cell_index(*position))
        .collect();
    let mut found: Vec<(CellIdx, bool)> = Vec::new();
    for level in levels {
        let Some(powered_state) = powered(state, enemy, level) else {
            continue;
        };
        // Each step costs at least one point, so a capturer farther than its
        // movement in steps cannot get there.
        let near: Vec<UnitId> = powered_state
            .units
            .iter()
            .filter(|unit| unit.owner == enemy && captures_at_once(&powered_state, unit, level))
            .filter(|unit| {
                let Some(at) = board_position(unit) else {
                    return false;
                };
                let profile = ruleset::profile(unit.kind);
                let reach = commander::effective_move(
                    &powered_state,
                    unit,
                    profile.movement,
                    profile.domain,
                );
                ours_decisive
                    .iter()
                    .any(|target| at.distance(*target) <= reach)
            })
            .map(|unit| unit.id)
            .collect();
        let session = Session::new(powered_state);
        let state = session.state();
        for order in orders_of(&session, &near, |order| {
            order.kind() == OrderKind::Capture && cells.contains(&order.destination())
        }) {
            let cell = order.destination();
            let Some(capturer) = order
                .unit()
                .and_then(|index| state.units.at(usize::from(index.get())))
            else {
                continue;
            };
            let Some(position) = dimensions.position_of(cell) else {
                continue;
            };
            let left = state
                .board
                .tile(position)
                .capture_points
                .unwrap_or(CAPTURE_REQUIRED_POINTS);
            let points = commander::effective_capture_points(
                state,
                capturer,
                u64::from(capturer.hp.div_ceil(10)),
            );
            if points >= u64::from(left) && !found.contains(&(cell, true)) {
                found.push((cell, true));
            }
        }
        for (_, cell, _) in completed_captures(&session, enemy) {
            let owned = dimensions
                .position_of(cell)
                .is_some_and(|position| state.board.tile(position).owner == TileOwner::Owned(ours));
            if owned && !found.iter().any(|(other, _)| *other == cell) {
                found.push((cell, false));
            }
        }
    }
    found
}
