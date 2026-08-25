//! What a game looked like, beside who won it.
//!
//! A score says which agent is better. It does not say what changed, so it
//! cannot show how the agents reached that result.
//!
//! So the harness counts the shape of a game as well as its result. Each seat
//! gets the units it built, the units it lost, and three metrics sampled at
//! the end of each of its turns: how many units it holds, what they are worth
//! in funds, and what its properties pay each day.
//!
//! **A loss is not an elimination.** When a player is defeated the ruleset
//! removes the army that is left, and counting that as losses would report the
//! loser's whole force as casualties. [`Shape::observe`] skips
//! [`KnownReason::Elimination`] for that reason.
//!
//! **The last sample is the last turn, not the last state.** The same sweep
//! empties the loser's board, so a metric read from the terminal state reports
//! nothing for the side that lost. Each seat keeps its final sample instead.

use awvm::commander;
use awvm::event::Event;
use awvm::ruleset::{self, KnownReason, TerrainTrait, UnitKind};
use awvm::semantic::{PlayerIdx, Reason, State};

/// What one seat did in one game.
#[derive(Clone, Debug, Default)]
pub struct SeatShape {
    /// Units the seat produced.
    pub built: u32,
    /// Units the seat lost, which is every removal but the elimination sweep.
    pub lost: u32,
    /// Turns the seat completed, and the number of samples behind each sum.
    pub turns: u32,
    /// Units held, summed over the samples.
    pub units_total: f64,
    /// Army value in funds, summed over the samples.
    pub value_total: f64,
    /// Income each day in funds, summed over the samples.
    pub income_total: f64,
    /// Units the seat held at the end of its last turn.
    pub last_units: u32,
    /// What those units were worth in funds.
    pub last_value: f64,
    /// What the seat's properties paid each day at that point.
    pub last_income: u64,
}

impl SeatShape {
    /// Units held, over the seat's turns.
    pub fn mean_units(&self) -> f64 {
        self.mean(self.units_total)
    }

    /// Army value in funds, over the seat's turns.
    pub fn mean_value(&self) -> f64 {
        self.mean(self.value_total)
    }

    /// Income each day in funds, over the seat's turns.
    pub fn mean_income(&self) -> f64 {
        self.mean(self.income_total)
    }

    fn mean(&self, sum: f64) -> f64 {
        if self.turns == 0 {
            return 0.0;
        }
        sum / f64::from(self.turns)
    }
}

/// What one game looked like, seat by seat.
#[derive(Clone, Debug, Default)]
pub struct Shape {
    /// One entry for each seat, in seat order.
    pub seats: Vec<SeatShape>,
}

impl Shape {
    /// A shape with room for every seat on the roster.
    pub fn new(seats: usize) -> Self {
        Self {
            seats: vec![SeatShape::default(); seats],
        }
    }

    /// The seat's numbers, or `None` for a seat off the roster.
    pub fn seat(&self, seat: PlayerIdx) -> Option<&SeatShape> {
        self.seats.get(seat.get())
    }

    /// Count what one accepted command did.
    ///
    /// `before` is the state the command ran against, which is the only state
    /// that still holds the units the command removed.
    pub fn observe(&mut self, before: &State, events: &[Event]) {
        for event in events {
            match event {
                Event::UnitCreated { owner, .. } => {
                    if let Some(seat) = before.players.seat(owner)
                        && let Some(shape) = self.seats.get_mut(seat.get())
                    {
                        shape.built += 1;
                    }
                }
                Event::UnitRemoved { unit, reason } => {
                    if matches!(reason, Reason::Known(KnownReason::Elimination)) {
                        continue;
                    }
                    if let Some(owner) = before.units.get(*unit).map(|unit| unit.owner)
                        && let Some(shape) = self.seats.get_mut(owner.get())
                    {
                        shape.lost += 1;
                    }
                }
                _ => {}
            }
        }
    }

    /// Sample the seat that has just completed a turn.
    ///
    /// The income is what the seat's properties pay, which it collects at the
    /// start of its next turn rather than now.
    pub fn sample_turn(&mut self, state: &State, seat: PlayerIdx) {
        let Some(shape) = self.seats.get_mut(seat.get()) else {
            return;
        };
        let mut units = 0;
        let mut value = 0.0;
        for unit in state.units.iter().filter(|unit| unit.owner == seat) {
            units += 1;
            value += cost(unit.kind) * f64::from(unit.hp) / 100.0;
        }
        let income = income(state, seat);

        shape.turns += 1;
        shape.units_total += f64::from(units);
        shape.value_total += value;
        shape.income_total += income as f64;
        shape.last_units = units;
        shape.last_value = value;
        shape.last_income = income;
    }
}

/// What one unit costs to replace, in funds.
fn cost(kind: UnitKind) -> f64 {
    ruleset::profile(kind).cost as f64
}

/// What the seat's properties pay each day.
///
/// This is the turn-start payment the reducer makes, counted the same way, so
/// a commander that changes the rate is read and not assumed.
fn income(state: &State, seat: PlayerIdx) -> u64 {
    let properties = state
        .board
        .tiles()
        .filter(|tile| {
            tile.owner.is_owned_by(seat) && ruleset::terrain_has(tile.terrain, TerrainTrait::Income)
        })
        .count();
    commander::effective_income_per_property(state, seat)
        .saturating_mul(properties.try_into().unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Agent;
    use crate::agents::{GreedyAgent, Weights};
    use crate::board::arena;
    use crate::harness::{Limits, play_measured};
    use crate::rng::Rng;
    use awvm::session::Session;

    /// Keep this fixture's combat shape independent of the live tuning
    /// baseline. The assertions below test accounting, not which doctrine
    /// wins Amber Valley.
    const SHAPE_WEIGHTS: Weights = Weights {
        hq_approach: 2_000.0,
        capture_completion: 0.8,
        capture_two_turn: 0.5,
        proximity_decay: 0.75,
        capturer_shortfall: 150.0,
        ..Weights::DEFAULT
    };

    /// Play one arena game between two greedy agents.
    fn game(fog: bool, seed: u64) -> crate::harness::Record {
        let mut session = Session::new(arena(fog, seed));
        let mut entropy = Rng::from_seed(seed);
        let mut first = GreedyAgent::with_weights(seed ^ 0x2, SHAPE_WEIGHTS);
        let mut second = GreedyAgent::with_weights(seed ^ 0x3, SHAPE_WEIGHTS);
        let mut agents: [&mut dyn Agent; 2] = [&mut first, &mut second];
        play_measured(
            arena(fog, seed),
            &mut session,
            &mut agents,
            &mut entropy,
            Limits::DEFAULT,
        )
    }

    /// Every turn the harness counts is a turn one seat is sampled on.
    ///
    /// A sample the report averages over must be a turn that was played, and a
    /// turn that was played must give a sample. Anything else silently weights
    /// one seat above the other.
    #[test]
    fn a_seat_is_sampled_once_for_each_turn_it_plays() {
        let record = game(false, 7);
        let samples: u32 = record.shape.seats.iter().map(|seat| seat.turns).sum();
        assert_eq!(samples, record.turns);
    }

    /// The first sample reads the properties the seat starts the game with.
    #[test]
    fn income_is_what_the_properties_pay() {
        let state = arena(false, 7);
        let seat = state.players.seats().next().expect("the arena seats two").0;
        let properties = state
            .board
            .tiles()
            .filter(|tile| {
                tile.owner.is_owned_by(seat)
                    && ruleset::terrain_has(tile.terrain, TerrainTrait::Income)
            })
            .count() as u64;

        let mut shape = Shape::new(state.players.len());
        shape.sample_turn(&state, seat);

        let sampled = shape.seat(seat).expect("the seat was sampled");
        assert_eq!(sampled.last_income, properties * 1000);
        assert!(properties > 0, "the arena board pays its seats something");
    }

    /// The sweep that clears a defeated army is not a casualty count.
    ///
    /// The loser can still hold an army when the ruleset removes it. Counting
    /// that as losses would report the whole force as dead after the match.
    #[test]
    fn losing_the_match_is_not_losing_the_army() {
        let record = game(false, 7);
        assert!(
            !record.abandoned(),
            "the game ended on its own, so a side was defeated"
        );
        for seat in &record.shape.seats {
            assert!(
                seat.lost < seat.built,
                "a seat lost {} of the {} units it built, which is the sweep and not combat",
                seat.lost,
                seat.built
            );
            assert!(seat.built > 0, "a greedy agent builds");
        }
    }

    /// A combat removal is counted as a loss.
    #[test]
    fn a_unit_that_dies_is_counted() {
        use awvm::event::Event;
        use awvm::semantic::KnownReason;

        let state = arena(false, 7);
        let unit = state.units.iter().next().expect("the arena starts a unit");
        let mut shape = Shape::new(state.players.len());
        shape.observe(
            &state,
            &[Event::UnitRemoved {
                unit: unit.id,
                reason: KnownReason::Combat.into(),
            }],
        );

        let lost: u32 = shape.seats.iter().map(|seat| seat.lost).sum();
        assert_eq!(lost, 1, "a combat removal counts as one loss");
    }
}
