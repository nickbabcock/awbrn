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

use awvm::ruleset::{self, TerrainTrait};
use awvm::semantic::{CAPTURE_REQUIRED_POINTS, CellIdx, PlayerIdx, TileOwner, UnitId};
use awvm::session::{AttackCandidate, LegalVisitor, Order, OrderKind, Session};

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

struct Attack {
    attacker: UnitId,
    target: UnitId,
    damage: f64,
}

struct Collector {
    ours: PlayerIdx,
    attacks: Vec<Attack>,
    captures: Vec<Order>,
}

impl LegalVisitor for Collector {
    const ATTACK_CONTEXT: bool = true;

    fn order(&mut self, order: Order) {
        if order.kind() == OrderKind::Capture {
            self.captures.push(order);
        }
    }

    fn attack(&mut self, candidate: AttackCandidate<'_>) {
        let (Some(target), Some(forecast)) = (candidate.target_unit, candidate.forecast) else {
            return;
        };
        if target.owner != self.ours {
            return;
        }
        let damage = (f64::from(forecast.attack.low) + f64::from(forecast.attack.high)) / 2.0;
        let attacker = candidate.attacker.id;
        match self
            .attacks
            .iter_mut()
            .find(|attack| attack.attacker == attacker && attack.target == target.id)
        {
            Some(attack) => attack.damage = attack.damage.max(damage),
            None => self.attacks.push(Attack {
                attacker,
                target: target.id,
                damage,
            }),
        }
    }
}

/// Estimate the loss that the active enemy seat can cause to `ours`.
///
/// `session` must hold the position at the start of the enemy turn.
pub fn estimate(session: &Session, ours: PlayerIdx, values: PropertyValues) -> ReplyEstimate {
    let state = session.state();
    let mut collector = Collector {
        ours,
        attacks: Vec::new(),
        captures: Vec::new(),
    };
    session.legal().visit_orders(&mut collector);

    let mut estimate = ReplyEstimate::default();

    // Focused fire: a greedy assignment of each enemy attacker to one target.
    let mut remaining: Vec<(UnitId, f64, f64)> = state
        .units
        .iter()
        .filter(|unit| unit.owner == ours)
        .map(|unit| {
            (
                unit.id,
                f64::from(unit.hp),
                ruleset::profile(unit.kind).cost as f64,
            )
        })
        .collect();
    let mut attacks = collector.attacks;
    let mut destroyed_units: Vec<(f64, UnitId)> = Vec::new();
    let mut exposed_units: Vec<(f64, UnitId)> = Vec::new();
    loop {
        let mut best: Option<(usize, f64)> = None;
        for (index, attack) in attacks.iter().enumerate() {
            let Some((_, hp, cost)) = remaining.iter().find(|(id, ..)| *id == attack.target) else {
                continue;
            };
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
        let target = remaining
            .iter_mut()
            .find(|(id, ..)| *id == attack.target)
            .expect("the target is one of our units");
        let destroyed = target.1 > 0.0 && attack.damage >= target.1;
        target.1 = (target.1 - attack.damage).max(0.0);
        estimate.unit_loss += value;
        match exposed_units
            .iter_mut()
            .find(|(_, id)| *id == attack.target)
        {
            Some((loss, _)) => *loss += value,
            None => exposed_units.push((value, attack.target)),
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
    for order in collector.captures {
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
        let strength = capturer.hp.div_ceil(10);
        let left = tile.capture_points.unwrap_or(CAPTURE_REQUIRED_POINTS);
        let has = |value| ruleset::terrain_has(tile.terrain, value);
        let headquarters = has(TerrainTrait::CaptureDefeatsOwner);
        let worth = if headquarters {
            values.headquarters
        } else {
            let produces = has(TerrainTrait::ProducesGround)
                || has(TerrainTrait::ProducesAir)
                || has(TerrainTrait::ProducesSea);
            values.income + if produces { values.production } else { 0.0 }
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
            values.capture_share * progress * base
        };
        let cell = order.destination();
        match taken.iter_mut().find(|(other, _)| *other == cell) {
            Some((_, best)) => *best = best.max(loss),
            None => taken.push((cell, loss)),
        }
    }
    estimate.property_loss = taken.iter().map(|(_, loss)| loss).sum();
    destroyed_units.sort_by(|left, right| right.0.total_cmp(&left.0));
    estimate.destroyed = destroyed_units.into_iter().map(|(_, id)| id).collect();
    exposed_units.sort_by(|left, right| right.0.total_cmp(&left.0));
    estimate.exposed = exposed_units.into_iter().map(|(_, id)| id).collect();
    taken.sort_by(|left, right| right.1.total_cmp(&left.1));
    estimate.threatened = taken.into_iter().map(|(cell, _)| cell).collect();
    estimate
}
