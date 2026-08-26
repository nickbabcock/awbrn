//! What a position is worth to one seat.
//!
//! Every agent in this crate scores an *action*: it reads the board, ranks the
//! plays that are legal on it, and takes the best one. Nothing scores a
//! *position*. That is the piece a search needs. A search plays a line, stops,
//! and must say what the board it stopped on is worth, because it cannot play
//! every line to its end.
//!
//! This module is that function, and it is deliberately a different thing from
//! [`Weights`](crate::agents::Weights). The agent's weights price a play
//! against the plays beside it, and only their ratios mean anything. These
//! weights price a board in funds, because funds is the one unit the game
//! already counts in: a unit is worth what it costs to replace, a property is
//! worth what it pays, and the money in hand is worth itself. A number in funds
//! can be checked against the board a person is looking at, which a number on
//! an arbitrary scale cannot.
//!
//! The value is the difference between what we hold and what the strongest
//! side at war with us holds. It is therefore zero at the start of a mirror
//! match, positive when we are ahead, and antisymmetric in a duel: what the
//! board is worth to one seat is the negative of what it is worth to the other.
//! A search that maximises it for us minimises it for them, which is what makes
//! it usable as a minimax score later.
//!
//! [`Evaluator::win_probability`] turns the funds into a probability, through
//! one logistic curve with one parameter. That parameter is not guessed: see
//! [`crate::calibration`], which plays games, samples this function at every
//! turn boundary, and fits the curve to what actually happened.
//!
//! Position enters through three terms that are off until measured: immediate
//! enemy damage on a unit's tile, production distance to neutral properties,
//! and the signed production-distance front. Commander charge, fuel and
//! ammunition remain absent. Each is another term to add and measure against
//! the report the calibration prints.

use awvm::commander;
use awvm::ruleset::{self, Terrain, TerrainTrait, UnitKind};
use awvm::semantic::{
    CAPTURE_REQUIRED_POINTS, Location, Match, Outcome, PlayerIdx, PlayerStatus, State, TeamStatus,
};
use awvm::session::Session;

use crate::map::{ContestMap, MAX_DEFICIT};
use crate::threat::ThreatMap;
use crate::threat::hostile;

/// The value of a match that is over.
///
/// Large enough that no position on any board reaches it, and finite so that a
/// search can add to it and compare it without meeting a `NaN`.
pub const DECISIVE: f64 = 1.0e9;

/// What each part of a position is worth, in funds.
///
/// Unlike the agent's weights these are not a ranking. Each one converts
/// something on the board into the money it stands for, so a reading of 1.0
/// means "worth exactly its funds". The positional terms and temperature have
/// separate calibrated defaults for standard and fog games.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct EvalWeights {
    /// A unit, against the funds it costs to replace, scaled by its health.
    ///
    /// One at the default, because that is what a unit is worth. It is a
    /// weight and not a constant so that a sweep can say otherwise: an army
    /// on the board is not quite the same asset as the money that bought it.
    pub army: f64,
    /// A point of funds in hand.
    ///
    /// Below one on purpose. Money that is still in the bank has not been
    /// converted into anything, and a side sitting on its funds because it
    /// holds no factory is not as strong as the number says.
    pub bank: f64,
    /// Days of income a property that pays is worth, on day nothing.
    ///
    /// A property is not worth one day of its rate; it is worth its rate for
    /// the rest of the match. This is the length of "the rest of the match",
    /// read at the start of it. The rate itself comes from the commander
    /// rather than being assumed, so a commander that pays differently is
    /// priced correctly.
    pub income_days: f64,
    /// What is left of [`EvalWeights::income_days`] after one more day.
    ///
    /// The rest of the match gets shorter as the match goes on, and a
    /// property taken on day thirty pays for a few days rather than for ten.
    /// This is that, one multiply for each day: the days a property is worth
    /// on day `n` are `income_days * income_decay^n`.
    ///
    /// **Measured at one, which is switched off.** It is kept rather than
    /// deleted because the sweep that says so is one file and a rerun.
    ///
    /// This term was written to answer a calibration reading. The earlier
    /// reading used the wrong Amber Valley seat order, so its day spread and
    /// sweep results are stale. Rerun the calibration before changing this
    /// value.
    pub income_decay: f64,
    /// Any property at all, held, on top of everything below.
    ///
    /// This is the day-limit win condition, and it is a different shape from
    /// every other term here. When the day limit ends a match the reducer
    /// counts the tiles each side holds and gives the match to whoever holds
    /// the most — see `day_limit_outcome` in `awvm::transition::turn`. It
    /// counts **every owned tile with no filter at all**, so a city, a base
    /// and a headquarters are one each.
    ///
    /// So a property is worth two things that have nothing to do with each
    /// other: the income it pays, which the terms below price and which is
    /// worth less the later it is taken, and one vote in a count that can end
    /// the match. The effect of this term must be measured again after the
    /// Amber Valley seat-order fix.
    ///
    /// Because the value is a difference, this term is exactly the day-limit
    /// margin: `plurality` times the tiles we hold less the tiles they hold.
    ///
    /// **Standard only, and off by default.** The earlier standard and fog
    /// sweeps used the wrong Amber Valley seat order. Their scores and fitted
    /// temperatures are stale. Rerun them before enabling this term.
    pub plurality: f64,
    /// A property that builds units, on top of what it pays.
    ///
    /// A base is a place to convert funds into an army. A city of the same
    /// income is not, and the difference is what this holds.
    pub production: f64,
    /// A headquarters, on top of what it pays and builds.
    ///
    /// Losing it loses the match, so on its own this number is arbitrary. It
    /// is not arbitrary in the term below: capture progress moves a share of
    /// this across, which is what makes an enemy soldier standing on our
    /// headquarters read as the emergency it is.
    pub hq: f64,
    /// The share of a property's worth that moves with a capture in progress.
    ///
    /// Below one because a half-finished capture is not half a property. The
    /// capturer can be killed, and the ruleset then puts the property back to
    /// whole.
    pub capture: f64,
    /// Enemy damage available against a unit on its current tile.
    ///
    /// One means one fund immediately at risk removes one fund of army value.
    pub exposure: f64,
    /// The prospective share of neutral properties indicated by production
    /// distance. One prices that share like a property already held.
    pub contest: f64,
    /// Army value for each full step across the production-distance front.
    ///
    /// One means a unit at the far edge of the bounded front gains its full
    /// replacement value.
    pub front: f64,
    /// Funds of advantage worth one logit of win probability.
    ///
    /// The scale of [`Evaluator::win_probability`], and the one weight here
    /// that is meant to be fitted rather than chosen. [`crate::calibration`]
    /// fits it.
    pub temperature: f64,
}

impl EvalWeights {
    /// Calibrated weights for a standard game.
    ///
    /// Ten days of income because a played game on these boards runs about
    /// twenty days, so a property taken in the middle pays for about ten more.
    /// Four thousand for a factory, which is a little over a tank. Thirty
    /// thousand for a headquarters, which is more than any single property
    /// pays and less than an army.
    ///
    /// The positional coefficients and temperature were selected over 240
    /// games on Amber Valley. Validate them across maps and the ladder before
    /// treating them as universal constants.
    pub const STANDARD: Self = Self {
        army: 1.0,
        bank: 0.8,
        income_days: 10.0,
        income_decay: 1.0,
        plurality: 0.0,
        production: 4_000.0,
        hq: 30_000.0,
        capture: 0.6,
        exposure: 1.0,
        contest: 0.5,
        front: 1.0,
        temperature: 27_486.0,
    };

    /// Calibrated weights for a fog game.
    pub const FOG: Self = Self {
        exposure: 0.0,
        contest: 1.0,
        front: 0.25,
        temperature: 35_008.0,
        ..Self::STANDARD
    };

    /// The historical default name means the standard-game preset.
    pub const DEFAULT: Self = Self::STANDARD;

    /// The compiled preset for the visibility rules of a position.
    pub const fn for_fog(fog: bool) -> Self {
        if fog { Self::FOG } else { Self::STANDARD }
    }
}

impl Default for EvalWeights {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The score and the requested parts of a position score.
///
/// Each part is the change caused by that term. The values are signed from
/// the view of one seat. The remaining terms are in `other`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EvalBreakdown {
    /// The complete score.
    pub score: f64,
    /// The army term.
    pub army: f64,
    /// The income term.
    pub income: f64,
    /// The exposure term.
    pub exposure: f64,
    /// The contest term.
    pub contest: f64,
    /// The front term.
    pub front: f64,
    /// Terms not listed above.
    pub other: f64,
}

/// The `capture_points` of a property nobody is capturing.
///
/// The count runs down from this to nothing, and the ruleset puts it back here
/// when the capturer leaves or dies.
const WHOLE_PROPERTY: u8 = CAPTURE_REQUIRED_POINTS;

/// Reads a position and answers what it is worth.
///
/// It holds scratch and nothing else, so one evaluator can be kept across a
/// whole tournament. The scratch is one entry for each seat and one for each
/// tile, which is what stops a board walk turning into a walk over the units
/// for every tile of it.
#[derive(Debug)]
pub struct Evaluator {
    weights: EvalWeights,
    /// Select a compiled preset from the position before each read.
    mode_defaults: bool,
    /// What each seat holds, in funds, in seat order.
    strengths: Vec<f64>,
    /// The days of income a property is worth on the day being read, which is
    /// [`EvalWeights::income_days`] decayed by the day.
    days: f64,
    /// What one property pays each seat, which the commander can change.
    rates: Vec<f64>,
    /// Who stands on each tile, by cell index.
    occupant: Vec<Option<PlayerIdx>>,
    threat: ThreatMap,
    contest: ContestMap,
}

impl Clone for Evaluator {
    fn clone(&self) -> Self {
        // The maps and filled rows are scratch derived from a position. A
        // clone needs the same evaluator, not stale work from its last read.
        Self {
            mode_defaults: self.mode_defaults,
            ..Self::new(self.weights)
        }
    }
}

impl Evaluator {
    pub const fn new(weights: EvalWeights) -> Self {
        Self {
            weights,
            mode_defaults: false,
            strengths: Vec::new(),
            days: 0.0,
            rates: Vec::new(),
            occupant: Vec::new(),
            threat: ThreatMap::new(),
            contest: ContestMap::new(),
        }
    }

    pub const fn weights(&self) -> &EvalWeights {
        &self.weights
    }

    /// What the position is worth to `seat`, in funds.
    ///
    /// Positive is ahead. A finished match answers [`DECISIVE`] with the sign
    /// of the result, and a draw answers nothing, which is what a draw is.
    ///
    /// A seat the roster does not hold is worth nothing, because there is
    /// nobody to be ahead.
    pub fn value(&mut self, state: &State, seat: PlayerIdx) -> f64 {
        self.select_mode(state);
        if let Some(value) = settled_value(state, seat) {
            return value;
        }
        if !self.position_enabled() {
            self.fill(state);
            return self.value_from_strengths(state, seat);
        }
        let session = Session::new(state.clone());
        self.value_in(&session, seat)
    }

    /// What the position in `session` is worth to `seat`, in funds.
    ///
    /// This entry point reuses the session's movement tables when positional
    /// terms are enabled. Callers that already own a session should prefer it
    /// to [`Evaluator::value`].
    pub fn value_in(&mut self, session: &Session, seat: PlayerIdx) -> f64 {
        let state = session.state();
        self.select_mode(state);
        if let Some(value) = settled_value(state, seat) {
            return value;
        }
        self.fill(state);
        if self.position_enabled() {
            self.fill_position(session);
        }
        self.value_from_strengths(state, seat)
    }

    /// Read the score and its main term contributions.
    pub fn breakdown_in(&mut self, session: &Session, seat: PlayerIdx) -> EvalBreakdown {
        let score = self.value_in(session, seat);
        let weights = self.weights;
        let army = self.term_delta(session, seat, weights, |weights| weights.army = 0.0);
        let income = self.term_delta(session, seat, weights, |weights| {
            weights.income_days = 0.0;
        });
        let exposure = self.term_delta(session, seat, weights, |weights| weights.exposure = 0.0);
        let contest = self.term_delta(session, seat, weights, |weights| weights.contest = 0.0);
        let front = self.term_delta(session, seat, weights, |weights| weights.front = 0.0);
        let other = score - army - income - exposure - contest - front;
        EvalBreakdown {
            score,
            army,
            income,
            exposure,
            contest,
            front,
            other,
        }
    }

    /// Read the contribution of one term by removing it from the weights.
    fn term_delta(
        &self,
        session: &Session,
        seat: PlayerIdx,
        weights: EvalWeights,
        remove: impl FnOnce(&mut EvalWeights),
    ) -> f64 {
        let with_term = Evaluator::new(weights).value_in(session, seat);
        let mut without_term = weights;
        remove(&mut without_term);
        with_term - Evaluator::new(without_term).value_in(session, seat)
    }

    /// Turn filled seat strengths into the value seen by one seat.
    fn value_from_strengths(&self, state: &State, seat: PlayerIdx) -> f64 {
        let player = &state.players[seat.get()];
        let team = player.team.clone();

        let ours: f64 = state
            .players
            .seats_on_team(&team)
            .map(|seat| self.strengths[seat.get()])
            .sum();

        // The strongest side at war with us, and not the sum of them. Being
        // ahead of two enemies who are each ahead of the other is not a
        // position anybody has to win twice.
        let mut rival = f64::NEG_INFINITY;
        for other in &state.teams {
            if other.id == team || other.status != TeamStatus::Active {
                continue;
            }
            let total: f64 = state
                .players
                .seats_on_team(&other.id)
                .map(|seat| self.strengths[seat.get()])
                .sum();
            rival = rival.max(total);
        }

        // No active enemy team, and the reducer has not called it yet. There
        // is nothing left to beat.
        if !rival.is_finite() {
            return DECISIVE;
        }
        ours - rival
    }

    /// What one seat holds, in funds, without reading the seats against it.
    ///
    /// This is the half of [`Evaluator::value`] a report prints when it wants
    /// to say where an advantage came from. It is not a score: a strength on
    /// its own says nothing about who is winning.
    pub fn strength(&mut self, state: &State, seat: PlayerIdx) -> f64 {
        self.select_mode(state);
        self.fill(state);
        if self.position_enabled() {
            let session = Session::new(state.clone());
            self.fill_position(&session);
        }
        self.strengths.get(seat.get()).copied().unwrap_or(0.0)
    }

    /// The chance `value` wins, through the logistic curve `temperature` sets.
    ///
    /// The curve is symmetric, so a value of nothing is an even game, and it
    /// saturates at [`DECISIVE`] rather than overflowing.
    pub fn win_probability(&self, value: f64) -> f64 {
        win_probability(value, self.weights.temperature)
    }

    /// Fill [`Evaluator::strengths`] for every seat on the roster.
    fn fill(&mut self, state: &State) {
        // A day beyond any match ever played decays to nothing anyway, so the
        // clamp only stops a silly day from reaching `powf`.
        let day = (state.turn.day.min(1_000)) as f64;
        self.days = self.weights.income_days * self.weights.income_decay.powf(day);

        let seats = state.players.len();
        self.strengths.clear();
        self.strengths.resize(seats, 0.0);
        self.rates.clear();
        self.rates.reserve(seats);
        for (seat, player) in state.players.seats() {
            self.rates
                .push(commander::effective_income_per_property(state, seat) as f64);
            self.strengths[seat.get()] += self.weights.bank * player.funds as f64;
        }

        // Cargo counts. A unit inside a transport is a unit that was paid for
        // and that will be put down somewhere.
        for unit in state.units.iter() {
            if let Some(strength) = self.strengths.get_mut(unit.owner.get()) {
                *strength += self.weights.army * cost(unit.kind) * f64::from(unit.hp) / 100.0;
            }
        }

        self.fill_occupants(state);
        self.fill_properties(state);

        // A seat that is out of the match holds nothing, whatever is still on
        // the board under its name. The elimination sweep is a separate
        // command, so a state between the two would otherwise credit an army
        // that is about to be removed.
        for (seat, player) in state.players.seats() {
            if player.status != PlayerStatus::Active {
                self.strengths[seat.get()] = 0.0;
            }
        }
    }

    /// Whether this weighting needs either positional map.
    fn position_enabled(&self) -> bool {
        self.weights.exposure != 0.0 || self.weights.contest != 0.0 || self.weights.front != 0.0
    }

    /// Adopt the preset that matches this position when no override was given.
    fn select_mode(&mut self, state: &State) {
        if self.mode_defaults {
            self.weights = EvalWeights::for_fog(state.settings.fog);
        }
    }

    /// Apply terms that depend on where assets stand or are likely to land.
    fn fill_position(&mut self, session: &Session) {
        let state = session.state();
        let dimensions = state.board.dimensions();
        for (seat, player) in state.players.seats() {
            if player.status != PlayerStatus::Active {
                continue;
            }

            if self.weights.exposure != 0.0 {
                self.threat.build(session, seat);
                for unit in state.units.iter().filter(|unit| unit.owner == seat) {
                    let Location::Board { position } = unit.location else {
                        continue;
                    };
                    let Some(cell) = dimensions.cell_index(position) else {
                        continue;
                    };
                    let at_risk = self
                        .threat
                        .immediate(cell, unit.kind)
                        .min(cost(unit.kind) * f64::from(unit.hp) / 100.0);
                    self.strengths[seat.get()] -= self.weights.exposure * at_risk;
                }
            }

            if self.weights.contest == 0.0 && self.weights.front == 0.0 {
                continue;
            }
            self.contest.build(state, seat);
            if !self.contest.is_built() {
                continue;
            }

            if self.weights.contest != 0.0 {
                for (position, tile) in state.board.iter() {
                    if tile.owner.is_ownable() && tile.owner.player().is_none() {
                        let Some(cell) = dimensions.cell_index(position) else {
                            continue;
                        };
                        let deficit = f64::from(self.contest.deficit(usize::from(cell.get())));
                        let share = 1.0 - deficit / f64::from(MAX_DEFICIT);
                        self.strengths[seat.get()] +=
                            self.weights.contest * share * self.property_value(tile.terrain, seat);
                    }
                }
            }

            if self.weights.front != 0.0 {
                for unit in state.units.iter().filter(|unit| unit.owner == seat) {
                    let Location::Board { position } = unit.location else {
                        continue;
                    };
                    let Some(cell) = dimensions.cell_index(position) else {
                        continue;
                    };
                    let progress = f64::from(self.contest.front(usize::from(cell.get())))
                        / f64::from(MAX_DEFICIT);
                    let army = cost(unit.kind) * f64::from(unit.hp) / 100.0;
                    self.strengths[seat.get()] += self.weights.front * progress * army;
                }
            }
        }
    }

    /// Who stands on each tile, so that the board walk below is one pass.
    fn fill_occupants(&mut self, state: &State) {
        let dimensions = state.board.dimensions();
        self.occupant.clear();
        self.occupant.resize(dimensions.len(), None);
        for unit in state.units.iter() {
            let Location::Board { position } = unit.location else {
                continue;
            };
            if let Some(cell) = dimensions.cell_index(position) {
                self.occupant[usize::from(cell.get())] = Some(unit.owner);
            }
        }
    }

    /// Credit every property to whoever holds it, and move the share of one
    /// that is being captured across.
    fn fill_properties(&mut self, state: &State) {
        let dimensions = state.board.dimensions();
        for (position, tile) in state.board.iter() {
            if !tile.owner.is_ownable() {
                continue;
            }
            let holder = tile.owner.player();
            if let Some(holder) = holder {
                self.strengths[holder.get()] += self.property_value(tile.terrain, holder);
            }

            // Below the whole means somebody is standing here turning the
            // crank. The ruleset resets the count when that unit leaves or
            // dies, so the occupant is the capturer and no search is needed
            // to find it.
            let points = tile.capture_points.unwrap_or(WHOLE_PROPERTY);
            if points >= WHOLE_PROPERTY {
                continue;
            }
            let Some(cell) = dimensions.cell_index(position) else {
                continue;
            };
            let Some(capturer) = self.occupant[usize::from(cell.get())] else {
                continue;
            };
            if holder.is_some_and(|holder| !hostile(state, holder, capturer)) {
                continue;
            }

            let progress = f64::from(WHOLE_PROPERTY - points) / f64::from(WHOLE_PROPERTY);
            let share = self.weights.capture * progress;
            if let Some(holder) = holder {
                self.strengths[holder.get()] -= share * self.property_value(tile.terrain, holder);
            }
            self.strengths[capturer.get()] += share * self.property_value(tile.terrain, capturer);
        }
    }

    /// What one property is worth to `seat`, in funds.
    ///
    /// The traits add rather than select, which is the difference between this
    /// and the agent's `property_weight`: an airport pays income and builds
    /// air units, and it is worth both. A headquarters pays, builds and ends
    /// the match, and it is worth all three.
    fn property_value(&self, terrain: Terrain, seat: PlayerIdx) -> f64 {
        let has = |value| ruleset::terrain_has(terrain, value);
        // Every held tile is one vote whatever it is, which is how the day
        // limit counts them.
        let mut total = self.weights.plurality;
        if has(TerrainTrait::Income) {
            total += self.days * self.rates.get(seat.get()).copied().unwrap_or(0.0);
        }
        if has(TerrainTrait::ProducesGround)
            || has(TerrainTrait::ProducesAir)
            || has(TerrainTrait::ProducesSea)
        {
            total += self.weights.production;
        }
        if has(TerrainTrait::CaptureDefeatsOwner) {
            total += self.weights.hq;
        }
        total
    }
}

impl Default for Evaluator {
    fn default() -> Self {
        Self {
            mode_defaults: true,
            ..Self::new(EvalWeights::STANDARD)
        }
    }
}

/// The chance a lead of `value` funds wins, on a curve of `temperature` funds.
///
/// Free of an evaluator so that the calibration can fit the temperature
/// without building one for each candidate.
pub fn win_probability(value: f64, temperature: f64) -> f64 {
    if temperature <= 0.0 {
        return if value > 0.0 {
            1.0
        } else if value < 0.0 {
            0.0
        } else {
            0.5
        };
    }
    1.0 / (1.0 + (-value / temperature).exp())
}

/// What one unit costs to replace, in funds.
fn cost(kind: UnitKind) -> f64 {
    ruleset::profile(kind).cost as f64
}

/// A value that can be answered without reading any assets on the board.
fn settled_value(state: &State, seat: PlayerIdx) -> Option<f64> {
    let Some(player) = state.players.get(seat.get()) else {
        return Some(0.0);
    };
    let Match::Finished { outcome } = &state.match_state else {
        return None;
    };
    Some(match outcome {
        Outcome::Victory { winners, .. } => {
            if winners.contains(&player.team) {
                DECISIVE
            } else {
                -DECISIVE
            }
        }
        Outcome::Draw { .. } | Outcome::Cancelled { .. } => 0.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{amber_valley, arena};
    use awvm::semantic::{Pos, TileOwner, Unit, UnitId};

    /// The two seats of a two-player board, in roster order.
    fn seats(state: &State) -> (PlayerIdx, PlayerIdx) {
        let mut seats = state.players.seats().map(|(seat, _)| seat);
        let first = seats.next().expect("the board seats two");
        let second = seats.next().expect("the board seats two");
        (first, second)
    }

    /// Where `seat` keeps its headquarters.
    fn headquarters(state: &State, seat: PlayerIdx) -> Pos {
        state
            .board
            .iter()
            .find(|(_, tile)| {
                tile.owner.is_owned_by(seat)
                    && ruleset::terrain_has(tile.terrain, TerrainTrait::CaptureDefeatsOwner)
            })
            .map(|(position, _)| position)
            .expect("a seat holds its own headquarters at the start")
    }

    /// A soldier of `seat`, standing on `position`.
    fn soldier(state: &mut State, seat: PlayerIdx, position: Pos) -> UnitId {
        let id = UnitId::from(9_000 + state.units.len() as u32);
        state.units.push(Unit {
            id,
            kind: UnitKind::Infantry,
            owner: seat,
            hp: 100,
            fuel: 99,
            ammo: 0,
            action: awvm::semantic::UnitAction::Ready,
            concealment: awvm::semantic::Concealment::Exposed,
            location: Location::Board { position },
        });
        id
    }

    /// The original material terms, isolated from calibrated position terms.
    fn material_weights() -> EvalWeights {
        EvalWeights {
            exposure: 0.0,
            contest: 0.0,
            front: 0.0,
            ..EvalWeights::STANDARD
        }
    }

    /// What one seat sees is the negative of what the other sees.
    ///
    /// This is the property a minimax search is built on: maximising the value
    /// for us is minimising it for them. A term added to [`Evaluator`] that
    /// breaks it breaks the search that reads it, and nothing else would say
    /// so.
    #[test]
    fn a_duel_is_worth_the_same_to_both_sides_and_opposite() {
        let mut evaluator = Evaluator::default();
        let state = arena(false, 7);
        let (first, second) = seats(&state);
        let ours = evaluator.value(&state, first);
        let theirs = evaluator.value(&state, second);
        assert!(
            (ours + theirs).abs() < 1e-9,
            "{ours} and {theirs} do not cancel"
        );
    }

    /// The arena board starts one infantry apart, and the evaluation says by
    /// how much.
    ///
    /// Amber Valley predeploys one Teal Galaxy infantry, and
    /// [`SEATS`](crate::board::SEATS) seats Teal Galaxy second. The extra unit
    /// pays for the first-player advantage.
    ///
    /// Amber Valley is a fair map, and none of this is a fault in it. It is a
    /// reading about the agents: one that plays a capture race and little else
    /// compounds a single extra capturer into income, into more capturers, and
    /// no term any weighting on the ladder holds pays that back.
    ///
    #[test]
    fn each_arena_board_starts_one_infantry_apart() {
        let mut evaluator = Evaluator::new(EvalWeights {
            bank: 0.0,
            ..material_weights()
        });
        let infantry = cost(UnitKind::Infantry);

        let state = arena(false, 7);
        let (first, _) = seats(&state);
        let value = evaluator.value(&state, first);
        assert!(
            (value + infantry).abs() < 1e-9,
            "Amber Valley starts the second seat one infantry up, and the value read {value}"
        );
    }

    /// A match that is over is worth the result and nothing else.
    #[test]
    fn a_finished_match_is_decisive() {
        let mut state = amber_valley(false, 7);
        let (first, second) = seats(&state);
        let winners = vec![state.player(first).team.clone()];
        state.match_state = Match::Finished {
            outcome: Outcome::Victory {
                winners,
                reason: awvm::ruleset::VictoryReason::HqCapture,
            },
        };

        let mut evaluator = Evaluator::new(material_weights());
        assert_eq!(evaluator.value(&state, first), DECISIVE);
        assert_eq!(evaluator.value(&state, second), -DECISIVE);
    }

    /// A property is worth what it pays, for as many days as the weights say.
    #[test]
    fn a_property_is_worth_the_days_of_income_it_pays() {
        let mut state = amber_valley(false, 7);
        let (first, _) = seats(&state);
        let mut evaluator = Evaluator::new(material_weights());
        let before = evaluator.value(&state, first);

        let neutral = state
            .board
            .iter()
            .find(|(_, tile)| {
                tile.owner == TileOwner::Neutral
                    && ruleset::terrain_has(tile.terrain, TerrainTrait::Income)
                    && !ruleset::terrain_has(tile.terrain, TerrainTrait::ProducesGround)
                    && !ruleset::terrain_has(tile.terrain, TerrainTrait::ProducesAir)
                    && !ruleset::terrain_has(tile.terrain, TerrainTrait::ProducesSea)
            })
            .map(|(position, _)| position)
            .expect("amber valley holds a neutral city");
        state.board.tile_mut(neutral).owner = TileOwner::Owned(first);

        let after = evaluator.value(&state, first);
        let rate = commander::effective_income_per_property(&state, first) as f64;
        let expected = EvalWeights::DEFAULT.plurality + EvalWeights::DEFAULT.income_days * rate;
        assert!(
            (after - before - expected).abs() < 1e-9,
            "one city moved the value by {}, and it pays {expected}",
            after - before
        );
    }

    /// A capture in progress moves a share of the property before it lands.
    ///
    /// The headquarters is the case that matters. An enemy soldier standing on
    /// it is one turn from ending the match, and an evaluation that reads the
    /// board as unchanged until the capture completes cannot see that at all.
    #[test]
    fn an_enemy_on_our_headquarters_costs_us_before_it_lands() {
        let mut state = amber_valley(false, 7);
        let (first, second) = seats(&state);
        let mut evaluator = Evaluator::new(material_weights());
        let before = evaluator.value(&state, first);

        let hq = headquarters(&state, first);
        soldier(&mut state, second, hq);
        state.board.tile_mut(hq).capture_points = Some(10);

        let after = evaluator.value(&state, first);
        assert!(
            after < before,
            "a half-captured headquarters read as {after}, up from {before}"
        );

        // Half the points are gone, so half of the share moves, and it moves
        // twice: off us and onto them.
        let value = EvalWeights::DEFAULT.plurality
            + EvalWeights::DEFAULT.hq
            + EvalWeights::DEFAULT.income_days
                * commander::effective_income_per_property(&state, first) as f64;
        let moved = EvalWeights::DEFAULT.capture * 0.5 * value;
        let soldier_value = cost(UnitKind::Infantry);
        assert!(
            (before - after - 2.0 * moved - soldier_value).abs() < 1e-6,
            "the capture moved {}, and half of a headquarters is {moved} each way",
            before - after
        );
    }

    /// Progress with nobody standing on it is not progress.
    ///
    /// The ruleset resets the count when the capturer leaves or dies, so a
    /// tile below the whole with no occupant is a state that cannot happen.
    /// The evaluation must not credit anybody for it if one ever does.
    #[test]
    fn capture_progress_with_no_capturer_moves_nothing() {
        let mut state = amber_valley(false, 7);
        let (first, _) = seats(&state);
        let mut evaluator = Evaluator::new(material_weights());
        let before = evaluator.value(&state, first);

        let hq = headquarters(&state, first);
        state.board.tile_mut(hq).capture_points = Some(1);

        assert_eq!(evaluator.value(&state, first), before);
    }

    /// A seat that is out of the match holds nothing.
    #[test]
    fn an_eliminated_seat_holds_nothing() {
        let mut state = amber_valley(false, 7);
        let (first, second) = seats(&state);
        let mut evaluator = Evaluator::new(material_weights());
        let held = evaluator.strength(&state, second);
        assert!(held > 0.0, "a seat at the start holds something");

        state.players.player_mut(second).status = PlayerStatus::Eliminated;
        assert_eq!(evaluator.strength(&state, second), 0.0);
        assert_eq!(
            evaluator.value(&state, first),
            evaluator.strength(&state, first),
            "with nobody left to beat, the value is what we hold"
        );
    }

    /// An army is worth what it costs to replace, at the health it has.
    #[test]
    fn a_damaged_unit_is_worth_its_health() {
        let mut state = amber_valley(false, 7);
        let (first, _) = seats(&state);
        let mut evaluator = Evaluator::new(material_weights());
        let before = evaluator.value(&state, first);

        let hq = headquarters(&state, first);
        let position = hq
            .offset(0, 1)
            .expect("the board holds a tile below the hq");
        let id = soldier(&mut state, first, position);
        let whole = evaluator.value(&state, first);
        assert!((whole - before - cost(UnitKind::Infantry)).abs() < 1e-9);

        state.units.get_mut(id).expect("the soldier was pushed").hp = 50;
        let half = evaluator.value(&state, first);
        assert!((half - before - cost(UnitKind::Infantry) / 2.0).abs() < 1e-9);
    }

    /// A default evaluator follows the visibility mode of each position.
    #[test]
    fn default_weights_follow_fog() {
        let mut state = amber_valley(false, 7);
        let (first, _) = seats(&state);
        let mut evaluator = Evaluator::default();

        evaluator.value(&state, first);
        assert_eq!(*evaluator.weights(), EvalWeights::STANDARD);

        state.settings.fog = true;
        evaluator.value(&state, first);
        assert_eq!(*evaluator.weights(), EvalWeights::FOG);
    }

    /// Immediate fire reduces the value of the unit standing under it.
    #[test]
    fn exposure_discounts_an_army_under_fire() {
        let mut state = amber_valley(false, 7);
        let (first, second) = seats(&state);
        let first_hq = headquarters(&state, first);
        let target = first_hq.offset(0, 1).expect("ground below the first hq");
        let attacker = target.offset(1, 0).expect("ground beside the target");
        soldier(&mut state, first, target);
        soldier(&mut state, second, attacker);

        let mut safe = Evaluator::new(material_weights());
        let mut exposed = Evaluator::new(EvalWeights {
            exposure: 1.0,
            ..material_weights()
        });
        let safe_value = safe.value(&state, first);
        let exposed_value = exposed.value(&state, first);
        assert!(
            exposed_value < safe_value,
            "enemy fire left the value at {exposed_value}, from {safe_value}"
        );
    }

    /// The contest term assigns more neutral value to the nearer side.
    #[test]
    fn contest_prices_neutral_properties_before_they_are_taken() {
        let state = amber_valley(false, 7);
        let (first, _) = seats(&state);
        let mut without = Evaluator::new(material_weights());
        let mut with = Evaluator::new(EvalWeights {
            contest: 1.0,
            ..material_weights()
        });
        assert!(with.strength(&state, first) > without.strength(&state, first));
    }

    /// Moving the same unit across the production front raises its value.
    #[test]
    fn front_values_an_army_farther_into_hostile_ground() {
        let mut state = amber_valley(false, 7);
        let (first, _) = seats(&state);
        let mut map = ContestMap::new();
        map.build(&state, first);
        let dimensions = state.board.dimensions();
        let (rear, _) = state
            .board
            .positions()
            .filter_map(|position| {
                let cell = dimensions.cell_index(position)?;
                Some((position, map.front(usize::from(cell.get()))))
            })
            .min_by_key(|(_, front)| *front)
            .expect("the board has rear ground");
        let (forward, _) = state
            .board
            .positions()
            .filter_map(|position| {
                let cell = dimensions.cell_index(position)?;
                Some((position, map.front(usize::from(cell.get()))))
            })
            .max_by_key(|(_, front)| *front)
            .expect("the board has forward ground");

        let id = soldier(&mut state, first, rear);
        let mut evaluator = Evaluator::new(EvalWeights {
            front: 1.0,
            ..material_weights()
        });
        let behind = evaluator.value(&state, first);
        state
            .units
            .get_mut(id)
            .expect("the soldier was pushed")
            .location = Location::Board { position: forward };
        let ahead = evaluator.value(&state, first);
        assert!(ahead > behind, "forward was {ahead}, behind was {behind}");
    }
}
