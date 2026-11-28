//! A fast estimate of what the enemy reply costs us.
//!
//! The estimate reads a position at the start of the enemy turn. It does not
//! play the reply. It asks two questions of the enemy's legal orders:
//!
//! 1. Which of our units can the enemy army destroy or damage, if each enemy
//!    unit attacks once and the enemy focuses its fire?
//! 2. Which of our properties can an enemy capturer take, or start to take?
//!
//! The first question is the gap in the per-unit threat map. The threat map
//! gives the worst single attacker on one tile. Two tanks that each do 55%
//! together destroy a tank, and only an assignment of attackers to targets
//! sees that.
//!
//! The assignment is greedy. It takes the attack that removes the most value
//! from what is left of its target, then repeats. Damage is the middle of the
//! forecast range. Destination conflicts between enemy attackers are not
//! checked, so the estimate is a little pessimistic in crowded positions.

use crate::agents::greedy::{CaptureTurn, capture_strength};
use awvm::ruleset::{self, TerrainTrait};
use awvm::semantic::{
    CAPTURE_REQUIRED_POINTS, CellIdx, Location, Match, PlayerIdx, PlayerStatus, State, TileOwner,
    UnitId, UnitStore,
};
use awvm::session::{
    AttackCandidate, LegalScope, LegalVisitor, Order, OrderKind, OrderMask, Session, TargetKind,
};
use awvm::transition::Command;

/// The value that the reply estimate puts on our losses, in funds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReplyEstimate {
    /// Unit value the enemy can remove with focused fire.
    pub unit_loss: f64,
    /// Our units the enemy can destroy.
    pub units_destroyed: u32,
    /// Property value the enemy can take or start to take.
    pub property_loss: f64,
    /// Whether an enemy capture can take our headquarters.
    pub headquarters_lost: bool,
    /// Our units that focused fire destroys, most valuable first.
    pub destroyed: Vec<UnitId>,
    /// Our units that focused fire damages or destroys, largest loss first.
    pub exposed: Vec<UnitId>,
    /// Our properties that an enemy capturer can reach, most valuable first.
    pub threatened: Vec<CellIdx>,
    /// The health focused fire takes from each unit of our side, allies too.
    pub health_lost: Vec<(UnitId, f64)>,
    /// The loss of each property of [`ReplyEstimate::threatened`].
    pub property_losses: Vec<(CellIdx, f64)>,
}

impl ReplyEstimate {
    /// The total loss, in funds.
    pub fn total(&self) -> f64 {
        self.unit_loss + self.property_loss
    }
}

/// The property values the estimate uses, in funds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PropertyValues {
    /// A property that pays income.
    pub income: f64,
    /// A property that builds units, added to its income value.
    pub production: f64,
    /// Our headquarters. Its loss ends the match.
    pub headquarters: f64,
    /// The share of a property's value that a partial capture moves.
    pub capture_share: f64,
}

impl PropertyValues {
    /// Values in the scale of [`crate::eval::EvalWeights::STANDARD`].
    pub const STANDARD: Self = Self {
        income: 10_000.0,
        production: 4_000.0,
        headquarters: 1_000_000.0,
        capture_share: 0.6,
    };
}

#[derive(Clone)]
struct Attack {
    attacker: UnitId,
    target: UnitId,
    target_index: usize,
    damage: f64,
}

struct Collector<'a> {
    side: &'a [PlayerIdx],
    units: &'a UnitStore,
    group_start: usize,
    attacks: Vec<Attack>,
    captures: Vec<Order>,
}

impl LegalVisitor for Collector<'_> {
    const ATTACK_CONTEXT: bool = true;
    const ACTIONS: OrderMask =
        OrderMask::from_untargeted(&[OrderKind::Capture]).with_targets(TargetKind::Attack);
    const DELETE: bool = false;

    fn order(&mut self, order: Order) {
        if order.kind() == OrderKind::Capture {
            self.captures.push(order);
        }
    }

    fn attack(&mut self, candidate: AttackCandidate<'_>) {
        let (Some(target), Some(forecast)) = (candidate.target_unit, candidate.forecast) else {
            return;
        };
        if !self.side.contains(&target.owner) {
            return;
        }
        let damage = (f64::from(forecast.attack.low) + f64::from(forecast.attack.high)) / 2.0;
        let attacker = candidate.attacker.id;
        if self
            .attacks
            .last()
            .is_some_and(|attack| attack.attacker != attacker)
        {
            self.group_start = self.attacks.len();
        }
        match self.attacks[self.group_start..]
            .iter_mut()
            .find(|attack| attack.attacker == attacker && attack.target == target.id)
        {
            Some(attack) => attack.damage = attack.damage.max(damage),
            None => self.attacks.push(Attack {
                attacker,
                target: target.id,
                target_index: self
                    .units
                    .index_of(target.id)
                    .expect("an attack target is present"),
                damage,
            }),
        }
    }
}

/// The switches of [`estimate`]. All are off by default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReplyOptions {
    /// When true, an enemy capturer takes the points that the engine gives
    /// it, with the capture bonus of its commander and of each power that the
    /// enemy meter can pay for now. When false, it takes its visible health.
    pub commander_effects: bool,
    /// When true and the enemy meter can pay for a power that lets an enemy
    /// capturer take a property in one capture
    /// ([`super::march::decisive_powers`]), the estimate also counts our
    /// properties that the enemy capturers take with that power, one
    /// property for each capturer ([`super::march::powered_captures`]). The
    /// engine gives the movement and the capture points of the power. The
    /// headquarters counts its whole loss, and each other property its whole
    /// value. Each such property goes into [`ReplyEstimate::threatened`], so
    /// that a block plan can put a unit on it.
    pub march: bool,
    /// When true ([`super::PlannerConfig::contest`]):
    ///
    /// - A decisive property ([`super::contest::decisive_tiles`]) has its
    ///   share of the headquarters value. Our last lab on a map with no
    ///   headquarters counts as our headquarters.
    /// - A capture of a decisive property that starts in the reply and
    ///   completes with the next capture of the same capturer costs at least
    ///   [`super::contest::TWO_TURN_SHARE`] of the headquarters value.
    /// - A unit of ours on a decisive property that focused fire destroys
    ///   does not block it: the estimate counts the enemy captures of the
    ///   property without that unit. A unit that attacks has ended its
    ///   action, so a capture there by an enemy unit counts only when the
    ///   attacks of the other enemy units on the blocker, each unit once,
    ///   add up to its health. The damage is the forecast without an enemy
    ///   power.
    pub contest: bool,
    /// When true, with [`ReplyOptions::march`]
    /// ([`super::PlannerConfig::march_guard`]): the enemy meter can pay for
    /// a decisive power when it lacks at most
    /// [`super::march::METER_MARGIN`] of its cost, because attacks before
    /// the power charge the meter. And a unit of ours on our decisive
    /// property that focused fire destroys does not block it from the
    /// powered captures, by the rule of [`ReplyOptions::contest`].
    pub march_guard: bool,
}

/// Estimate the loss that the active enemy seat can cause to `ours`, with
/// the switches of `options`.
///
/// `session` must hold the position at the start of the enemy turn.
pub fn estimate(
    session: &Session,
    ours: PlayerIdx,
    values: PropertyValues,
    options: ReplyOptions,
) -> ReplyEstimate {
    estimate_for(session, ours, &[ours], values, options)
}

/// [`estimate`] counting fire on any seat of `side` as fire on us.
fn estimate_for(
    session: &Session,
    ours: PlayerIdx,
    side: &[PlayerIdx],
    values: PropertyValues,
    options: ReplyOptions,
) -> ReplyEstimate {
    let ReplyOptions {
        commander_effects,
        march,
        contest,
        march_guard,
    } = options;
    let state = session.state();
    let mut collector = Collector {
        side,
        units: &state.units,
        group_start: 0,
        attacks: Vec::new(),
        captures: Vec::new(),
    };
    let units: Vec<_> = state
        .units
        .iter()
        .filter(|unit| {
            let profile = ruleset::profile(unit.kind);
            profile.can_capture || profile.fire_mode != ruleset::FireMode::None
        })
        .map(|unit| unit.id)
        .collect();
    session.legal().visit_scoped(
        LegalScope {
            units: &units,
            unitless: false,
        },
        &mut collector,
    );

    let mut estimate = ReplyEstimate::default();

    // Focused fire: a greedy assignment of each enemy attacker to one target.
    let mut remaining: Vec<(UnitId, f64, f64)> = state
        .units
        .iter()
        .map(|unit| {
            (
                unit.id,
                f64::from(unit.hp),
                ruleset::profile(unit.kind).cost as f64,
            )
        })
        .collect();
    let mut attacks = collector.attacks;
    let opens_blockers = contest || (march && march_guard);
    let decisive = if opens_blockers {
        super::contest::decisive_tiles(state, ours)
    } else {
        Vec::new()
    };
    // The attacks on our units that block a decisive property, for the
    // captures behind a destroyed blocker ([`ReplyOptions::contest`]).
    let on_blockers: Vec<Attack> = if decisive.is_empty() {
        Vec::new()
    } else {
        attacks
            .iter()
            .filter(|attack| {
                state.units.at(attack.target_index).is_some_and(|unit| {
                    let awvm::semantic::Location::Board { position } = unit.location else {
                        return false;
                    };
                    super::contest::share_at(&decisive, position).is_some_and(|share| share >= 1.0)
                })
            })
            .cloned()
            .collect()
    };
    let mut destroyed_units: Vec<(f64, UnitId)> = Vec::new();
    let mut exposed_units: Vec<(f64, UnitId, f64)> = Vec::new();
    loop {
        let mut best: Option<(usize, f64)> = None;
        for (index, attack) in attacks.iter().enumerate() {
            let (_, hp, cost) = &remaining[attack.target_index];
            if *hp <= 0.0 {
                continue;
            }
            let removed = attack.damage.min(*hp);
            let mut value = removed * cost / 100.0;
            if attack.damage >= *hp {
                // A destroyed unit also stops acting.
                value += cost * 0.1;
            }
            if best.is_none_or(|(_, current)| value > current) {
                best = Some((index, value));
            }
        }
        let Some((index, value)) = best else {
            break;
        };
        let attack = attacks.swap_remove(index);
        let target = &mut remaining[attack.target_index];
        let destroyed = target.1 > 0.0 && attack.damage >= target.1;
        let removed = attack.damage.min(target.1);
        target.1 = (target.1 - attack.damage).max(0.0);
        estimate.unit_loss += value;
        match exposed_units
            .iter_mut()
            .find(|(_, id, _)| *id == attack.target)
        {
            Some((loss, _, health)) => {
                *loss += value;
                *health += removed;
            }
            None => exposed_units.push((value, attack.target, removed)),
        }
        if destroyed {
            estimate.units_destroyed += 1;
            destroyed_units.push((target.2, target.0));
        }
        attacks.retain(|other| other.attacker != attack.attacker);
    }

    // Captures: the best capture for each of our or neutral properties.
    let dimensions = state.board.dimensions();
    let mut taken: Vec<(CellIdx, f64)> = Vec::new();
    // A decisive property of ours whose blocker focused fire destroys is
    // open without it ([`ReplyOptions::contest`]).
    let captures = collector.captures;
    let mut opened: Vec<awvm::semantic::State> = Vec::new();
    if !on_blockers.is_empty() {
        for &(_, id) in &destroyed_units {
            let on_blocker = || on_blockers.iter().filter(move |attack| attack.target == id);
            if on_blocker().next().is_none() {
                continue;
            }
            let (Some(unit), Some(mut open)) =
                (state.units.get(id), super::contest::without_unit(state, id))
            else {
                continue;
            };
            // An attacker that the others cannot destroy the blocker without
            // has ended its action, so it does not capture.
            let total: f64 = on_blocker().map(|attack| attack.damage).sum();
            for attack in on_blocker() {
                if total - attack.damage < f64::from(unit.hp)
                    && let Some(spent) = open.units.get_mut(attack.attacker)
                {
                    spent.action = awvm::semantic::UnitAction::Spent;
                }
            }
            opened.push(open);
        }
    }
    let mut open_captures: Vec<(usize, Order)> = Vec::new();
    if contest {
        for (index, open) in opened.iter().enumerate() {
            let session = Session::new(open.clone());
            let capturers: Vec<UnitId> = open
                .units
                .iter()
                .filter(|unit| unit.owner != ours && ruleset::profile(unit.kind).can_capture)
                .map(|unit| unit.id)
                .collect();
            let mut found = Collector {
                side,
                units: &open.units,
                group_start: 0,
                attacks: Vec::new(),
                captures: Vec::new(),
            };
            session.legal().visit_scoped(
                LegalScope {
                    units: &capturers,
                    unitless: false,
                },
                &mut found,
            );
            let cells: Vec<CellIdx> = state
                .units
                .iter()
                .filter(|unit| !open.units.contains(unit.id))
                .filter_map(|unit| match unit.location {
                    awvm::semantic::Location::Board { position } => dimensions.cell_index(position),
                    awvm::semantic::Location::Cargo { .. } => None,
                })
                .collect();
            open_captures.extend(
                found
                    .captures
                    .into_iter()
                    .filter(|order| cells.contains(&order.destination()))
                    .map(|order| (index, order)),
            );
        }
    }
    let all_captures: Vec<(Option<usize>, Order)> = captures
        .into_iter()
        .map(|order| (None, order))
        .chain(
            open_captures
                .into_iter()
                .map(|(index, order)| (Some(index), order)),
        )
        .collect();
    for (source, order) in all_captures {
        let state = match source {
            Some(index) => &opened[index],
            None => state,
        };
        let Some(position) = dimensions.position_of(order.destination()) else {
            continue;
        };
        let Some(tile) = state.board.get(position) else {
            continue;
        };
        if tile.owner != TileOwner::Owned(ours) {
            continue;
        }
        let Some(capturer) = order
            .unit()
            .and_then(|index| state.units.at(usize::from(index.get())))
        else {
            continue;
        };
        let strength = if commander_effects {
            capture_strength(state, capturer, capturer.hp.div_ceil(10), CaptureTurn::Next)
        } else {
            capturer.hp.div_ceil(10)
        };
        let left = tile.capture_points.unwrap_or(CAPTURE_REQUIRED_POINTS);
        let has = |value| ruleset::terrain_has(tile.terrain, value);
        let share = if contest {
            super::contest::share_at(&decisive, position)
        } else {
            None
        };
        let headquarters = match share {
            Some(share) => share >= 1.0,
            None => has(TerrainTrait::CaptureDefeatsOwner),
        };
        let worth = if headquarters {
            values.headquarters
        } else {
            let produces = has(TerrainTrait::ProducesGround)
                || has(TerrainTrait::ProducesAir)
                || has(TerrainTrait::ProducesSea);
            values.income
                + if produces { values.production } else { 0.0 }
                + share.map_or(0.0, |share| share * values.headquarters)
        };
        let loss = if strength >= left {
            if headquarters {
                estimate.headquarters_lost = true;
            }
            worth
        } else {
            // A partial capture moves a share of the property, and the
            // headquarters share is capped so that it does not read as a loss.
            let progress = f64::from(strength) / f64::from(CAPTURE_REQUIRED_POINTS);
            let base = if headquarters { 30_000.0 } else { worth };
            let partial = values.capture_share * progress * base;
            // With the contest switch, a decisive capture that completes with
            // the next capture of the same capturer costs more.
            if contest && headquarters && strength.saturating_mul(2) >= left {
                partial.max(super::contest::TWO_TURN_SHARE * values.headquarters)
            } else {
                partial
            }
        };
        let cell = order.destination();
        match taken.iter_mut().find(|(other, _)| *other == cell) {
            Some((_, best)) => *best = best.max(loss),
            None => taken.push((cell, loss)),
        }
    }
    if march {
        let margin = if march_guard {
            super::march::METER_MARGIN
        } else {
            0.0
        };
        let mut powered = super::march::powered_captures_with(state, ours, margin);
        if march_guard {
            for open in &opened {
                for found in super::march::powered_captures_with(open, ours, margin) {
                    if !powered.contains(&found) {
                        powered.push(found);
                    }
                }
            }
        }
        for (cell, decisive) in powered {
            let worth = if decisive {
                estimate.headquarters_lost = true;
                values.headquarters
            } else {
                let produces = dimensions.position_of(cell).is_some_and(|position| {
                    let terrain = state.board.tile(position).terrain;
                    ruleset::terrain_has(terrain, TerrainTrait::ProducesGround)
                        || ruleset::terrain_has(terrain, TerrainTrait::ProducesAir)
                        || ruleset::terrain_has(terrain, TerrainTrait::ProducesSea)
                });
                values.income + if produces { values.production } else { 0.0 }
            };
            match taken.iter_mut().find(|(other, _)| *other == cell) {
                Some((_, best)) => *best = best.max(worth),
                None => taken.push((cell, worth)),
            }
        }
    }
    estimate.property_loss = taken.iter().map(|(_, loss)| loss).sum();
    // The plan lists name our own units only: a plan cannot move an ally.
    let own = |id: &UnitId| state.units.get(*id).is_some_and(|unit| unit.owner == ours);
    destroyed_units.sort_by(|left, right| right.0.total_cmp(&left.0));
    estimate.destroyed = destroyed_units
        .into_iter()
        .map(|(_, id)| id)
        .filter(own)
        .collect();
    exposed_units.sort_by(|left, right| right.0.total_cmp(&left.0));
    estimate.exposed = exposed_units
        .iter()
        .map(|(_, id, _)| *id)
        .filter(own)
        .collect();
    estimate.health_lost = exposed_units
        .into_iter()
        .map(|(_, id, health)| (id, health))
        .collect();
    taken.sort_by(|left, right| right.1.total_cmp(&left.1));
    estimate.threatened = taken.iter().map(|(cell, _)| *cell).collect();
    estimate.property_losses = taken;
    estimate
}

/// The live seats that move from the active seat of `state` up to the next
/// turn of `seat`, in turn order.
pub(crate) fn seats_before(state: &State, seat: PlayerIdx) -> Vec<PlayerIdx> {
    let order = &state.turn.order;
    let start = order
        .iter()
        .position(|id| *id == state.turn.active_player)
        .unwrap_or(state.turn.position);
    let mut seats = Vec::new();
    for step in 0..order.len() {
        let Some(index) = state.players.seat(&order[(start + step) % order.len()]) else {
            continue;
        };
        if index == seat {
            break;
        }
        if state.player(index).status == PlayerStatus::Active {
            seats.push(index);
        }
    }
    seats
}

/// The loss over every hostile seat that moves before our next turn, each
/// read after the fire of the ones before it, so a blocker one destroys no
/// longer blocks the next. The plan lists are the first hostile seat's only.
pub(crate) fn round_estimate(
    session: &Session,
    ours: PlayerIdx,
    values: PropertyValues,
    options: ReplyOptions,
) -> ReplyEstimate {
    let state = session.state();
    let before = seats_before(state, ours);
    let hostile = before
        .iter()
        .filter(|seat| crate::threat::hostile(state, ours, **seat))
        .count();
    let side: Vec<PlayerIdx> = state
        .players
        .seats()
        .filter(|(seat, player)| {
            player.status == PlayerStatus::Active && !crate::threat::hostile(state, ours, *seat)
        })
        .map(|(seat, _)| seat)
        .collect();
    let mut round = ReplyEstimate::default();
    let mut health: Vec<(UnitId, f64)> = Vec::new();
    let mut passing = Session::new(state.clone());
    let mut entropy = super::MeanLuck;
    let mut read = 0;
    for _ in 0..before.len() {
        if read == hostile || !matches!(passing.state().match_state, Match::Active { .. }) {
            break;
        }
        let player = passing.state().turn.active_player.clone();
        let Some(seat) = passing.state().players.seat(&player) else {
            break;
        };
        if seat == ours {
            break;
        }
        if crate::threat::hostile(passing.state(), ours, seat) {
            let found = estimate_for(&passing, ours, &side, values, options);
            read += 1;
            if read == 1 {
                round.destroyed = found.destroyed.clone();
                round.exposed = found.exposed.clone();
                round.threatened = found.threatened.clone();
            }
            round.headquarters_lost |= found.headquarters_lost;
            for &(cell, loss) in &found.property_losses {
                match round.property_losses.iter_mut().find(|(at, _)| *at == cell) {
                    Some((_, best)) => *best = best.max(loss),
                    None => round.property_losses.push((cell, loss)),
                }
            }
            for &(id, lost) in &found.health_lost {
                match health.iter_mut().find(|(other, _)| *other == id) {
                    Some((_, total)) => *total += lost,
                    None => health.push((id, lost)),
                }
            }
            if read == hostile {
                break;
            }
            let mut next = passing.state().clone();
            wound(&mut next, &found.health_lost);
            passing = Session::new(next);
        }
        let Ok(end) = passing.resolve(&Command::EndTurn { player }) else {
            break;
        };
        if passing.apply(end, &mut entropy, &mut ()).is_err() {
            break;
        }
    }
    // The health lost over the round, priced on the units as they start it.
    for &(id, lost) in &health {
        let Some(unit) = state.units.get(id) else {
            continue;
        };
        let hp = f64::from(unit.hp);
        let cost = ruleset::profile(unit.kind).cost as f64;
        let lost = lost.min(hp);
        round.unit_loss += lost * cost / 100.0;
        if hp > 0.0 && lost >= hp {
            round.unit_loss += cost * 0.1;
            round.units_destroyed += 1;
        }
    }
    round.health_lost = health;
    round.property_loss = round.property_losses.iter().map(|(_, loss)| loss).sum();
    round
}

/// Take `health` off the units of `state`; a unit with none left goes, with
/// its cargo.
fn wound(state: &mut State, health: &[(UnitId, f64)]) {
    let mut gone: Vec<UnitId> = Vec::new();
    for &(id, lost) in health {
        let Some(unit) = state.units.get_mut(id) else {
            continue;
        };
        let left = f64::from(unit.hp) - lost;
        if left <= 0.0 {
            gone.push(id);
        } else {
            unit.hp = left.ceil() as u8;
        }
    }
    if !gone.is_empty() {
        state.units.retain(|unit| {
            !gone.contains(&unit.id)
                && !matches!(unit.location, Location::Cargo { transport, .. } if gone.contains(&transport))
        });
    }
}
