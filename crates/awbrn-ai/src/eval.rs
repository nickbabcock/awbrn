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
//!
//! [`EvalBreakdown`] names every score term. It does not use an unnamed
//! remainder: bank, production, headquarters, plurality, unit count, and
//! capture progress each have their own contribution. The breakdown uses one
//! raw extraction pass. This is important for audits because a term must not
//! be measured by evaluating the same position again with one weight removed.

use std::ops::Deref;

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

/// Maximum absolute score error allowed by the mirror fixtures.
pub const MIRROR_TOLERANCE: f64 = 1.0e-9;

/// Largest share of a score that may separate the named terms from the score.
///
/// The two accumulation orders in [`reconcile_terms_score`] can disagree in
/// the low bits. Anything above this is a missing term, not rounding.
pub const RESIDUAL_TOLERANCE: f64 = 1.0e-9;

/// The kind of transformation used by one evaluator weight.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvalWeightKind {
    /// The raw delta is multiplied by the weight.
    Linear,
    /// The raw delta is changed by another weight or a bounded operation.
    Transformed,
}

/// One entry in the score-term table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvalWeightTerm {
    /// The field in [`EvalWeights`].
    pub weight: &'static str,
    /// The named contribution in [`EvalTerms`].
    pub contribution: &'static str,
    /// How the weight enters the contribution.
    pub kind: EvalWeightKind,
}

/// The score terms and the transformations that produce them.
///
/// The two income weights are one transformed contribution. `income_decay`
/// changes the day factor before `income_days` scales the raw property rate.
/// Capture and contest use per-property raw records because property income
/// can depend on the seat that would hold the property.
pub const EVAL_WEIGHT_TERMS: [EvalWeightTerm; 13] = [
    EvalWeightTerm {
        weight: "army",
        contribution: "army",
        kind: EvalWeightKind::Linear,
    },
    EvalWeightTerm {
        weight: "unit_count",
        contribution: "unit_count",
        kind: EvalWeightKind::Linear,
    },
    EvalWeightTerm {
        weight: "bank",
        contribution: "bank",
        kind: EvalWeightKind::Linear,
    },
    EvalWeightTerm {
        weight: "income_days",
        contribution: "income",
        kind: EvalWeightKind::Transformed,
    },
    EvalWeightTerm {
        weight: "income_decay",
        contribution: "income",
        kind: EvalWeightKind::Transformed,
    },
    EvalWeightTerm {
        weight: "plurality",
        contribution: "plurality",
        kind: EvalWeightKind::Linear,
    },
    EvalWeightTerm {
        weight: "production",
        contribution: "production",
        kind: EvalWeightKind::Linear,
    },
    EvalWeightTerm {
        weight: "hq",
        contribution: "hq",
        kind: EvalWeightKind::Linear,
    },
    EvalWeightTerm {
        weight: "capture",
        contribution: "capture",
        kind: EvalWeightKind::Transformed,
    },
    EvalWeightTerm {
        weight: "exposure",
        contribution: "exposure",
        kind: EvalWeightKind::Transformed,
    },
    EvalWeightTerm {
        weight: "contest",
        contribution: "contest",
        kind: EvalWeightKind::Transformed,
    },
    EvalWeightTerm {
        weight: "front",
        contribution: "front",
        kind: EvalWeightKind::Transformed,
    },
    EvalWeightTerm {
        weight: "temperature",
        contribution: "win-probability",
        kind: EvalWeightKind::Transformed,
    },
];

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
    /// One fielded unit, independent of its price and health.
    ///
    /// Units are actions, screens, blockers, and capture threats. Replacement
    /// cost alone does not price those uses: one medium tank and several cheap
    /// units can have similar material value but very different board control.
    /// The shipped presets leave this term at zero until it is calibrated.
    pub unit_count: f64,
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
        unit_count: 0.0,
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

/// The numeric score terms for one position comparison.
///
/// Every field is signed from the view of one seat. The exact sum of the
/// named terms is stored in [`EvalBreakdown::score`]. This part is safe to
/// add over decisions.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct EvalTerms {
    /// The army term.
    pub army: f64,
    /// The fielded-unit count term.
    pub unit_count: f64,
    /// The bank term.
    pub bank: f64,
    /// The income term.
    pub income: f64,
    /// The day-limit plurality term.
    pub plurality: f64,
    /// The production-property term.
    pub production: f64,
    /// The headquarters term.
    pub hq: f64,
    /// The capture-progress term.
    pub capture: f64,
    /// The exposure term.
    pub exposure: f64,
    /// The contest term.
    pub contest: f64,
    /// The front term.
    pub front: f64,
}

/// Unweighted values extracted for one active side.
///
/// Distances stay in steps and utilization-style ratios stay outside this
/// record. The evaluator converts only values with a direct funds meaning.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct EvalRawFeatures {
    /// Replacement value of all units, scaled by health.
    pub army_value: f64,
    /// Number of units on the board.
    pub fielded_unit_count: f64,
    /// Funds in the bank.
    pub bank: f64,
    /// Property income for one day.
    pub property_income_per_day: f64,
    /// Number of owned tiles.
    pub owned_tile_count: f64,
    /// Number of owned production properties.
    pub production_property_count: f64,
    /// Number of owned headquarters.
    pub headquarters_count: f64,
    /// Capture progress share credited to the capturer.
    pub capture_progress_share: f64,
    /// Funds at risk from immediate fire.
    pub at_risk_funds: f64,
    /// Sum of neutral contest shares.
    pub neutral_contest_share: f64,
    /// Army value multiplied by front progress.
    pub front_steps: f64,
}

impl EvalTerms {
    /// Return the exact sum of all named contributions.
    pub fn named_sum(self) -> f64 {
        self.army
            + self.unit_count
            + self.bank
            + self.income
            + self.plurality
            + self.production
            + self.hq
            + self.capture
            + self.exposure
            + self.contest
            + self.front
    }

    /// Return one named contribution for diagnostics and tests.
    pub fn value(self, name: &str) -> Option<f64> {
        match name {
            "army" => Some(self.army),
            "unit_count" => Some(self.unit_count),
            "bank" => Some(self.bank),
            "income" => Some(self.income),
            "plurality" => Some(self.plurality),
            "production" => Some(self.production),
            "hq" => Some(self.hq),
            "capture" => Some(self.capture),
            "exposure" => Some(self.exposure),
            "contest" => Some(self.contest),
            "front" => Some(self.front),
            _ => None,
        }
    }

    /// Add another numeric breakdown.
    /// Return this term set minus `other`, term by term.
    pub fn difference(self, other: Self) -> Self {
        Self {
            army: self.army - other.army,
            unit_count: self.unit_count - other.unit_count,
            bank: self.bank - other.bank,
            income: self.income - other.income,
            plurality: self.plurality - other.plurality,
            production: self.production - other.production,
            hq: self.hq - other.hq,
            capture: self.capture - other.capture,
            exposure: self.exposure - other.exposure,
            contest: self.contest - other.contest,
            front: self.front - other.front,
        }
    }

    pub fn add_assign(&mut self, other: Self) {
        self.army += other.army;
        self.unit_count += other.unit_count;
        self.bank += other.bank;
        self.income += other.income;
        self.plurality += other.plurality;
        self.production += other.production;
        self.hq += other.hq;
        self.capture += other.capture;
        self.exposure += other.exposure;
        self.contest += other.contest;
        self.front += other.front;
    }
}

/// The evaluator preset that supplied the active weights.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvalPreset {
    /// The caller supplied the weights directly.
    #[default]
    Explicit,
    /// Standard clear-game defaults.
    Standard,
    /// Fog-game defaults.
    Fog,
}

/// Why an evaluator returned a terminal score.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvalTerminalReason {
    /// The friendly team won.
    Victory,
    /// The friendly team lost.
    Defeat,
    /// The match ended in a draw.
    Draw,
    /// The match was cancelled.
    Cancelled,
    /// The requested seat does not exist.
    InvalidSeat,
    /// There is no active hostile team left in an active state.
    NoActiveHostileTeam,
}

/// Army-relative diagnostics for positional terms.
///
/// The denominator is the larger active-side raw army value, with a floor of
/// one fund. These values are diagnostics only. They do not change the score.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct EvalArmyRelativeShares {
    /// The army-value denominator used for the shares.
    pub army_value: f64,
    /// The exposure contribution divided by the denominator.
    pub exposure: f64,
    /// The contest contribution divided by the denominator.
    pub contest: f64,
    /// The front contribution divided by the denominator.
    pub front: f64,
}

/// Context for one evaluator reading.
///
/// Context is not summed across decisions. It identifies the weights and
/// mode that produced the numeric terms and records terminal and gate state.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct EvalBreakdownContext {
    /// Active evaluator weights.
    pub weights: EvalWeights,
    /// Preset that supplied the weights.
    pub preset: EvalPreset,
    /// Whether positional extraction ran.
    pub position_enabled: bool,
    /// Terminal outcome, when the position had one.
    pub terminal_reason: Option<EvalTerminalReason>,
    /// Position evaluations used to produce this row.
    pub position_evaluations: u64,
    /// Funds the named terms had to absorb to reach the score.
    ///
    /// Rounding noise between the two accumulation orders. A value that is
    /// large next to the score means a term is missing from `build_terms`.
    pub term_residual: f64,
    /// Friendly raw values used by this row.
    pub friendly_raw: EvalRawFeatures,
    /// Hostile raw values used by this row.
    pub hostile_raw: EvalRawFeatures,
    /// Army-relative positional diagnostics.
    pub positional_shares: EvalArmyRelativeShares,
}

/// A summed difference between two leaf scores.
///
/// A sum of differences has no weights, no preset, no terminal reason, and no
/// raw features: those describe one position, and this describes the movement
/// between many pairs of them. Only the score and the named terms add.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvalScoreDelta {
    /// Summed score difference.
    pub score: f64,
    /// Summed named contribution differences.
    #[serde(flatten)]
    pub terms: EvalTerms,
}

impl EvalScoreDelta {
    /// Add the selected-minus-seed difference of one decision.
    pub fn add_difference(&mut self, seed: EvalBreakdown, selected: EvalBreakdown) {
        self.score += selected.score - seed.score;
        self.terms.add_assign(selected.terms.difference(seed.terms));
    }

    /// Add another sum of differences.
    pub fn add_assign(&mut self, other: Self) {
        self.score += other.score;
        self.terms.add_assign(other.terms);
    }
}

/// The score and the named parts of a position score.
///
/// `terms` is the summable numeric part. `context` is deliberately separate:
/// weights, preset, gate state, terminal reason, and diagnostics must not be
/// added when `SearchStats` combines decisions.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvalBreakdown {
    /// The complete score. It is the named-term sum for nonterminal states.
    pub score: f64,
    /// Summable numeric contributions. They are flattened in JSON so audit
    /// output shows each named term directly.
    #[serde(flatten)]
    pub terms: EvalTerms,
    /// Non-summable evaluation context.
    pub context: EvalBreakdownContext,
}

impl Default for EvalBreakdown {
    fn default() -> Self {
        Self {
            score: 0.0,
            terms: EvalTerms::default(),
            context: EvalBreakdownContext::default(),
        }
    }
}

impl EvalBreakdown {
    /// Return the summable numeric part.
    pub const fn terms(self) -> EvalTerms {
        self.terms
    }
}

impl Deref for EvalBreakdown {
    type Target = EvalTerms;

    fn deref(&self) -> &Self::Target {
        &self.terms
    }
}

/// The `capture_points` of a property nobody is capturing.
///
/// The count runs down from this to nothing, and the ruleset puts it back here
/// when the capturer leaves or dies.
const WHOLE_PROPERTY: u8 = CAPTURE_REQUIRED_POINTS;

/// Unweighted values extracted for one seat.
///
/// The scalar values are independent of evaluator weights. Capture and
/// contest records stay separate because their property value depends on the
/// seat that receives it and on the terrain traits of that property.
#[derive(Clone, Copy, Debug, Default)]
struct RawSeatFeatures {
    army: f64,
    unit_count: f64,
    bank: f64,
    income_per_day: f64,
    owned_tiles: f64,
    production_properties: f64,
    headquarters: f64,
    capture_progress_share: f64,
    at_risk_funds: f64,
    neutral_contest_share: f64,
    front_steps: f64,
}

impl From<RawSeatFeatures> for EvalRawFeatures {
    fn from(raw: RawSeatFeatures) -> Self {
        Self {
            army_value: raw.army,
            fielded_unit_count: raw.unit_count,
            bank: raw.bank,
            property_income_per_day: raw.income_per_day,
            owned_tile_count: raw.owned_tiles,
            production_property_count: raw.production_properties,
            headquarters_count: raw.headquarters,
            capture_progress_share: raw.capture_progress_share,
            at_risk_funds: raw.at_risk_funds,
            neutral_contest_share: raw.neutral_contest_share,
            front_steps: raw.front_steps,
        }
    }
}

impl RawSeatFeatures {
    fn add_assign(&mut self, other: Self) {
        self.army += other.army;
        self.unit_count += other.unit_count;
        self.bank += other.bank;
        self.income_per_day += other.income_per_day;
        self.owned_tiles += other.owned_tiles;
        self.production_properties += other.production_properties;
        self.headquarters += other.headquarters;
        self.capture_progress_share += other.capture_progress_share;
        self.at_risk_funds += other.at_risk_funds;
        self.neutral_contest_share += other.neutral_contest_share;
        self.front_steps += other.front_steps;
    }
}

/// One active capture that changes ownership value between two seats.
#[derive(Clone, Copy, Debug)]
struct CaptureTransfer {
    terrain: Terrain,
    progress: f64,
    holder: Option<PlayerIdx>,
    capturer: PlayerIdx,
}

/// One neutral property credit from the contest map.
#[derive(Clone, Copy, Debug)]
struct ContestTarget {
    terrain: Terrain,
    share: f64,
    seat: PlayerIdx,
}

/// Result of one state read before it is wrapped in a serialized breakdown.
#[derive(Clone, Copy, Debug)]
struct EvaluationRead {
    score: f64,
    terms: EvalTerms,
    term_residual: f64,
    terminal_reason: Option<EvalTerminalReason>,
    position_evaluations: u64,
    friendly_raw: EvalRawFeatures,
    hostile_raw: EvalRawFeatures,
    positional_shares: EvalArmyRelativeShares,
}

impl EvaluationRead {
    fn invalid() -> Self {
        Self {
            score: 0.0,
            terms: EvalTerms::default(),
            term_residual: 0.0,
            terminal_reason: Some(EvalTerminalReason::InvalidSeat),
            position_evaluations: 0,
            friendly_raw: EvalRawFeatures::default(),
            hostile_raw: EvalRawFeatures::default(),
            positional_shares: EvalArmyRelativeShares::default(),
        }
    }
}

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
    /// The preset that supplied the current weights.
    preset: EvalPreset,
    /// Unweighted values for each seat, in seat order.
    raw: Vec<RawSeatFeatures>,
    /// Named weighted contributions for each seat, in seat order.
    seat_terms: Vec<EvalTerms>,
    /// What each seat holds, in funds, in seat order.
    strengths: Vec<f64>,
    /// The days of income a property is worth on the day being read, which is
    /// [`EvalWeights::income_days`] decayed by the day.
    days: f64,
    /// What one property pays each seat, which the commander can change.
    rates: Vec<f64>,
    /// Who stands on each tile, by cell index.
    occupant: Vec<Option<PlayerIdx>>,
    /// Active captures from the current raw extraction.
    capture_transfers: Vec<CaptureTransfer>,
    /// Neutral contest credits from the current raw extraction.
    contest_targets: Vec<ContestTarget>,
    threat: ThreatMap,
    contest: ContestMap,
    /// Number of nonterminal position evaluations performed by this reader.
    position_evaluations: u64,
}

impl Clone for Evaluator {
    fn clone(&self) -> Self {
        // The maps and filled rows are scratch derived from a position. A
        // clone needs the same evaluator, not stale work from its last read.
        Self {
            mode_defaults: self.mode_defaults,
            preset: self.preset,
            ..Self::new(self.weights)
        }
    }
}

impl Evaluator {
    pub const fn new(weights: EvalWeights) -> Self {
        Self {
            weights,
            mode_defaults: false,
            preset: EvalPreset::Explicit,
            raw: Vec::new(),
            seat_terms: Vec::new(),
            strengths: Vec::new(),
            days: 0.0,
            rates: Vec::new(),
            occupant: Vec::new(),
            capture_transfers: Vec::new(),
            contest_targets: Vec::new(),
            threat: ThreatMap::new(),
            contest: ContestMap::new(),
            position_evaluations: 0,
        }
    }

    pub const fn weights(&self) -> &EvalWeights {
        &self.weights
    }

    /// Return the number of position evaluations performed by this reader.
    pub const fn position_evaluations(&self) -> u64 {
        self.position_evaluations
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
        let session = self.position_enabled().then(|| Session::new(state.clone()));
        self.read(state, session.as_ref(), seat).score
    }

    /// What the position in `session` is worth to `seat`, in funds.
    ///
    /// This entry point reuses the session's movement tables when positional
    /// terms are enabled. Callers that already own a session should prefer it
    /// to [`Evaluator::value`].
    pub fn value_in(&mut self, session: &Session, seat: PlayerIdx) -> f64 {
        let state = session.state();
        self.select_mode(state);
        self.read(state, Some(session), seat).score
    }

    /// Read the score and every named term contribution.
    ///
    /// Equality policy: for nonterminal positions the returned score uses the
    /// exact bit pattern of the sum of the named contributions. Terminal
    /// positions have no term decomposition and record their terminal reason
    /// in the context instead.
    pub fn breakdown_in(&mut self, session: &Session, seat: PlayerIdx) -> EvalBreakdown {
        self.select_mode(session.state());
        let read = self.read(session.state(), Some(session), seat);
        EvalBreakdown {
            score: read.score,
            terms: read.terms,
            context: EvalBreakdownContext {
                weights: self.weights,
                preset: self.preset,
                position_enabled: self.position_enabled(),
                terminal_reason: read.terminal_reason,
                position_evaluations: read.position_evaluations,
                term_residual: read.term_residual,
                friendly_raw: read.friendly_raw,
                hostile_raw: read.hostile_raw,
                positional_shares: read.positional_shares,
            },
        }
    }

    /// Read one state and extract all raw values once.
    fn read(
        &mut self,
        state: &State,
        session: Option<&Session>,
        seat: PlayerIdx,
    ) -> EvaluationRead {
        if let Some(value) = settled_value(state, seat) {
            return EvaluationRead {
                score: value.0,
                terms: EvalTerms::default(),
                term_residual: 0.0,
                terminal_reason: Some(value.1),
                position_evaluations: 0,
                friendly_raw: EvalRawFeatures::default(),
                hostile_raw: EvalRawFeatures::default(),
                positional_shares: EvalArmyRelativeShares::default(),
            };
        }

        self.extract_raw(state);
        if self.position_enabled() {
            let Some(session) = session else {
                return EvaluationRead::invalid();
            };
            self.fill_position(session);
        }
        self.build_terms(state);
        self.position_evaluations += 1;

        let Some(terms) = self.delta_terms(state, seat) else {
            return EvaluationRead {
                score: DECISIVE,
                terms: EvalTerms::default(),
                term_residual: 0.0,
                terminal_reason: Some(EvalTerminalReason::NoActiveHostileTeam),
                position_evaluations: 1,
                friendly_raw: EvalRawFeatures::default(),
                hostile_raw: EvalRawFeatures::default(),
                positional_shares: EvalArmyRelativeShares::default(),
            };
        };
        let Some(score) = self.value_from_strengths(state, seat) else {
            return EvaluationRead {
                score: DECISIVE,
                terms: EvalTerms::default(),
                term_residual: 0.0,
                terminal_reason: Some(EvalTerminalReason::NoActiveHostileTeam),
                position_evaluations: 1,
                friendly_raw: EvalRawFeatures::default(),
                hostile_raw: EvalRawFeatures::default(),
                positional_shares: EvalArmyRelativeShares::default(),
            };
        };
        let mut terms = terms;
        let term_residual = reconcile_terms_score(&mut terms, score, self.weights);
        let (friendly_raw, hostile_raw) = self.raw_sides(state, seat).unwrap_or_default();
        let positional_shares = self.positional_shares(state, seat, terms);
        EvaluationRead {
            score,
            terms,
            term_residual,
            terminal_reason: None,
            position_evaluations: 1,
            friendly_raw,
            hostile_raw,
            positional_shares,
        }
    }

    /// What one seat holds, in funds, without reading the seats against it.
    ///
    /// This is the half of [`Evaluator::value`] a report prints when it wants
    /// to say where an advantage came from. It is not a score: a strength on
    /// its own says nothing about who is winning.
    pub fn strength(&mut self, state: &State, seat: PlayerIdx) -> f64 {
        self.select_mode(state);
        if state.players.get(seat.get()).is_none() {
            return 0.0;
        }
        self.extract_raw(state);
        let session = self.position_enabled().then(|| Session::new(state.clone()));
        if let Some(session) = session.as_ref() {
            self.fill_position(session);
        }
        self.build_terms(state);
        self.strengths.get(seat.get()).copied().unwrap_or(0.0)
    }

    /// The chance `value` wins, through the logistic curve `temperature` sets.
    ///
    /// The curve is symmetric, so a value of nothing is an even game, and it
    /// saturates at [`DECISIVE`] rather than overflowing.
    pub fn win_probability(&self, value: f64) -> f64 {
        win_probability(value, self.weights.temperature)
    }

    /// Extract unweighted values for every seat on the roster.
    fn extract_raw(&mut self, state: &State) {
        // A day beyond any match ever played decays to nothing anyway, so the
        // clamp only stops a silly day from reaching `powf`.
        let day = (state.turn.day.min(1_000)) as f64;
        self.days = self.weights.income_days * self.weights.income_decay.powf(day);

        let seats = state.players.len();
        self.raw.clear();
        self.raw.resize(seats, RawSeatFeatures::default());
        self.strengths.clear();
        self.strengths.resize(seats, 0.0);
        self.rates.clear();
        self.rates.reserve(seats);
        for (seat, player) in state.players.seats() {
            self.rates
                .push(commander::effective_income_per_property(state, seat) as f64);
            self.raw[seat.get()].bank = player.funds as f64;
            self.strengths[seat.get()] += self.weights.bank * player.funds as f64;
        }

        // A cargo unit keeps its material value, but it has no fielded action
        // until it leaves the transport.
        for unit in state.units.iter() {
            if let Some(raw) = self.raw.get_mut(unit.owner.get()) {
                if matches!(unit.location, Location::Board { .. }) {
                    raw.unit_count += 1.0;
                    self.strengths[unit.owner.get()] += self.weights.unit_count;
                }
                raw.army += cost(unit.kind) * f64::from(unit.hp) / 100.0;
                self.strengths[unit.owner.get()] +=
                    self.weights.army * cost(unit.kind) * f64::from(unit.hp) / 100.0;
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
                self.raw[seat.get()] = RawSeatFeatures::default();
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
            self.preset = if state.settings.fog {
                EvalPreset::Fog
            } else {
                EvalPreset::Standard
            };
            self.weights = EvalWeights::for_fog(state.settings.fog);
        }
    }

    /// Apply terms that depend on where assets stand or are likely to land.
    fn fill_position(&mut self, session: &Session) {
        let state = session.state();
        let dimensions = state.board.dimensions();
        self.contest_targets.clear();
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
                    self.raw[seat.get()].at_risk_funds += at_risk;
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
                        self.raw[seat.get()].neutral_contest_share += share;
                        self.contest_targets.push(ContestTarget {
                            terrain: tile.terrain,
                            share,
                            seat,
                        });
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
                    self.raw[seat.get()].front_steps += progress * army;
                    self.strengths[seat.get()] += self.weights.front * progress * army;
                }
            }
        }
    }

    /// Convert the extracted raw values into named weighted contributions.
    fn build_terms(&mut self, state: &State) {
        self.seat_terms.clear();
        self.seat_terms
            .resize(state.players.len(), EvalTerms::default());
        for (seat, player) in state.players.seats() {
            if player.status != PlayerStatus::Active {
                continue;
            }
            let raw = self.raw[seat.get()];
            let mut terms = EvalTerms {
                army: self.weights.army * raw.army,
                unit_count: self.weights.unit_count * raw.unit_count,
                bank: self.weights.bank * raw.bank,
                income: self.days * raw.income_per_day,
                plurality: self.weights.plurality * raw.owned_tiles,
                production: self.weights.production * raw.production_properties,
                hq: self.weights.hq * raw.headquarters,
                exposure: -self.weights.exposure * raw.at_risk_funds,
                front: self.weights.front * raw.front_steps,
                ..EvalTerms::default()
            };
            for transfer in &self.capture_transfers {
                let value = self.weights.capture
                    * transfer.progress
                    * self.property_value(transfer.terrain, transfer.capturer);
                if transfer.capturer == seat {
                    terms.capture += value;
                }
                if transfer.holder == Some(seat) {
                    terms.capture -= self.weights.capture
                        * transfer.progress
                        * self.property_value(transfer.terrain, seat);
                }
            }
            if self.weights.contest != 0.0 {
                for target in self
                    .contest_targets
                    .iter()
                    .filter(|target| target.seat == seat)
                {
                    terms.contest += self.weights.contest
                        * target.share
                        * self.property_value(target.terrain, seat);
                }
            }
            self.seat_terms[seat.get()] = terms;
        }
    }

    /// Turn filled legacy strengths into the value seen by one seat.
    ///
    /// This keeps the pre-refactor operation order for the production score.
    /// Named terms are extracted beside it, so a breakdown can be audited
    /// without changing the baseline score's bits.
    fn value_from_strengths(&self, state: &State, seat: PlayerIdx) -> Option<f64> {
        let player = state.players.get(seat.get())?;
        let team = &player.team;
        let ours: f64 = state
            .players
            .seats_on_team(team)
            .map(|seat| self.strengths[seat.get()])
            .sum();
        let rival = self.strongest_rival_team(state, team)?;
        let hostile: f64 = state
            .players
            .seats_on_team(&rival)
            .map(|seat| self.strengths[seat.get()])
            .sum();
        Some(ours - hostile)
    }

    /// Return the friendly-minus-rival named contributions.
    fn delta_terms(&self, state: &State, seat: PlayerIdx) -> Option<EvalTerms> {
        let player = state.players.get(seat.get())?;
        let rival = self.strongest_rival_team(state, &player.team)?;
        let ours = self.team_terms(state, &player.team);
        let hostile = self.team_terms(state, &rival);
        let terms = EvalTerms {
            army: ours.army - hostile.army,
            unit_count: ours.unit_count - hostile.unit_count,
            bank: ours.bank - hostile.bank,
            income: ours.income - hostile.income,
            plurality: ours.plurality - hostile.plurality,
            production: ours.production - hostile.production,
            hq: ours.hq - hostile.hq,
            capture: ours.capture - hostile.capture,
            exposure: ours.exposure - hostile.exposure,
            contest: ours.contest - hostile.contest,
            front: ours.front - hostile.front,
        };
        Some(terms)
    }

    /// Return one team's named contributions.
    fn team_terms(&self, state: &State, team: &awvm::semantic::TeamId) -> EvalTerms {
        let mut terms = EvalTerms::default();
        for seat in state.players.seats_on_team(team) {
            terms.add_assign(self.seat_terms[seat.get()]);
        }
        terms
    }

    /// Return the strongest active hostile team, using the old tie policy.
    fn strongest_rival_team(
        &self,
        state: &State,
        team: &awvm::semantic::TeamId,
    ) -> Option<awvm::semantic::TeamId> {
        let mut strongest = None;
        let mut strength = f64::NEG_INFINITY;
        for other in &state.teams {
            if other.id == *team || other.status != TeamStatus::Active {
                continue;
            }
            let total: f64 = state
                .players
                .seats_on_team(&other.id)
                .map(|seat| self.strengths[seat.get()])
                .sum();
            if total > strength {
                strength = total;
                strongest = Some(other.id.clone());
            }
        }
        strongest
    }

    /// Return army-relative shares for the friendly and strongest hostile
    /// teams.
    fn positional_shares(
        &self,
        state: &State,
        seat: PlayerIdx,
        terms: EvalTerms,
    ) -> EvalArmyRelativeShares {
        let Some(player) = state.players.get(seat.get()) else {
            return EvalArmyRelativeShares::default();
        };
        let Some(rival) = self.strongest_rival_team(state, &player.team) else {
            return EvalArmyRelativeShares::default();
        };
        let ours_army = self.team_raw_army(state, &player.team).abs();
        let hostile_army = self.team_raw_army(state, &rival).abs();
        let army_value = ours_army.max(hostile_army).max(1.0);
        EvalArmyRelativeShares {
            army_value,
            exposure: terms.exposure / army_value,
            contest: terms.contest / army_value,
            front: terms.front / army_value,
        }
    }

    fn team_raw_army(&self, state: &State, team: &awvm::semantic::TeamId) -> f64 {
        state
            .players
            .seats_on_team(team)
            .map(|seat| self.raw[seat.get()].army)
            .sum()
    }

    /// Return the raw friendly and strongest-hostile team values.
    fn raw_sides(
        &self,
        state: &State,
        seat: PlayerIdx,
    ) -> Option<(EvalRawFeatures, EvalRawFeatures)> {
        let player = state.players.get(seat.get())?;
        let rival = self.strongest_rival_team(state, &player.team)?;
        Some((
            self.team_raw_features(state, &player.team),
            self.team_raw_features(state, &rival),
        ))
    }

    fn team_raw_features(&self, state: &State, team: &awvm::semantic::TeamId) -> EvalRawFeatures {
        let mut raw = RawSeatFeatures::default();
        for seat in state.players.seats_on_team(team) {
            raw.add_assign(self.raw[seat.get()]);
        }
        raw.into()
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
        self.capture_transfers.clear();
        for (position, tile) in state.board.iter() {
            if !tile.owner.is_ownable() {
                continue;
            }
            let holder = tile.owner.player();
            if let Some(holder) = holder {
                let raw = &mut self.raw[holder.get()];
                raw.owned_tiles += 1.0;
                if ruleset::terrain_has(tile.terrain, TerrainTrait::Income) {
                    raw.income_per_day += self.rates[holder.get()];
                }
                if ruleset::terrain_has(tile.terrain, TerrainTrait::ProducesGround)
                    || ruleset::terrain_has(tile.terrain, TerrainTrait::ProducesAir)
                    || ruleset::terrain_has(tile.terrain, TerrainTrait::ProducesSea)
                {
                    raw.production_properties += 1.0;
                }
                if ruleset::terrain_has(tile.terrain, TerrainTrait::CaptureDefeatsOwner) {
                    raw.headquarters += 1.0;
                }
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
            if let Some(raw) = self.raw.get_mut(capturer.get()) {
                raw.capture_progress_share += progress;
            }
            let share = self.weights.capture * progress;
            if let Some(holder) = holder {
                self.strengths[holder.get()] -= share * self.property_value(tile.terrain, holder);
            }
            self.strengths[capturer.get()] += share * self.property_value(tile.terrain, capturer);
            self.capture_transfers.push(CaptureTransfer {
                terrain: tile.terrain,
                progress,
                holder,
                capturer,
            });
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
            preset: EvalPreset::Standard,
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

/// Preserve the legacy score's exact floating-point result in the breakdown.
///
/// The score comes from the traversal-order accumulation in `strengths`, and
/// the named terms come from the per-term accumulation in `build_terms`. The
/// two paths read the same raw values with the same weights, so they differ
/// only in the order IEEE-754 addition sees. Two orders can differ in the low
/// bits, and this places that rounding residual in the last enabled named
/// term.
///
/// The duplicate path is deliberate. Deriving the score from the term sum
/// would change the summation order, which moves the low bits of every score,
/// which changes the baseline configuration fingerprints. Those fingerprints
/// must not move, so the score keeps the old operation order and the terms are
/// extracted beside it.
///
/// The residual is therefore rounding noise and nothing else. A residual
/// larger than that means a weighted contribution reached `strengths` without
/// reaching `build_terms`, and those funds would land in an unrelated named
/// term with no other signal. That is the unnamed remainder this module exists
/// to remove, so [`RESIDUAL_TOLERANCE`] fails the debug build instead.
fn reconcile_terms_score(terms: &mut EvalTerms, score: f64, weights: EvalWeights) -> f64 {
    let residual = score - terms.named_sum();
    if residual == 0.0 {
        return residual;
    }
    debug_assert!(
        residual.abs() <= RESIDUAL_TOLERANCE * score.abs().max(1.0),
        "the named terms lost {residual} funds of a {score} score: a weighted \
         contribution reaches `strengths` but not `build_terms`"
    );
    if weights.front != 0.0 {
        terms.front += residual;
    } else if weights.contest != 0.0 {
        terms.contest += residual;
    } else if weights.exposure != 0.0 {
        terms.exposure += residual;
    } else if weights.capture != 0.0 {
        terms.capture += residual;
    } else if weights.hq != 0.0 {
        terms.hq += residual;
    } else if weights.production != 0.0 {
        terms.production += residual;
    } else if weights.plurality != 0.0 {
        terms.plurality += residual;
    } else if weights.income_days != 0.0 {
        terms.income += residual;
    } else if weights.bank != 0.0 {
        terms.bank += residual;
    } else if weights.unit_count != 0.0 {
        terms.unit_count += residual;
    } else {
        terms.army += residual;
    }
    debug_assert_eq!(terms.named_sum(), score);
    residual
}

/// A value that can be answered without reading any assets on the board.
fn settled_value(state: &State, seat: PlayerIdx) -> Option<(f64, EvalTerminalReason)> {
    let Some(player) = state.players.get(seat.get()) else {
        return Some((0.0, EvalTerminalReason::InvalidSeat));
    };
    let Match::Finished { outcome } = &state.match_state else {
        return None;
    };
    Some(match outcome {
        Outcome::Victory { winners, .. } => {
            if winners.contains(&player.team) {
                (DECISIVE, EvalTerminalReason::Victory)
            } else {
                (-DECISIVE, EvalTerminalReason::Defeat)
            }
        }
        Outcome::Draw { .. } => (0.0, EvalTerminalReason::Draw),
        Outcome::Cancelled { .. } => (0.0, EvalTerminalReason::Cancelled),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{amber_valley, arena};
    use awbrn_map::AwbrnMap;
    use awbrn_types::{
        Faction, GraphicalTerrain, PlayerFaction, Property, Unit as MapUnit, VisualHp,
    };
    use awvm::semantic::{Dimensions, Observation, Pos, TileOwner, Unit, UnitId, observe};

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
    fn field_unit(state: &mut State, seat: PlayerIdx, position: Pos, kind: UnitKind) -> UnitId {
        let id = UnitId::from(9_000 + state.units.len() as u32);
        state.units.push(Unit {
            id,
            kind,
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

    fn soldier(state: &mut State, seat: PlayerIdx, position: Pos) -> UnitId {
        field_unit(state, seat, position, UnitKind::Infantry)
    }

    /// Build a small map with equal terrain on opposite sides.
    fn mirrored_clear_state() -> State {
        let dimensions = Dimensions::new(7, 3);
        let mut map = AwbrnMap::new(dimensions, GraphicalTerrain::Plain);
        let pink = PlayerFaction::PinkCosmos;
        let teal = PlayerFaction::TealGalaxy;
        let neutral = Faction::Neutral;
        let player = |faction| Faction::Player(faction);
        for (position, property) in [
            (Pos::new(0, 1), Property::City(player(pink))),
            (Pos::new(1, 1), Property::HQ(pink)),
            (Pos::new(2, 1), Property::City(neutral)),
            (Pos::new(4, 1), Property::City(neutral)),
            (Pos::new(5, 1), Property::HQ(teal)),
            (Pos::new(6, 1), Property::City(player(teal))),
        ] {
            map.set_terrain(position, GraphicalTerrain::Property(property));
        }
        for (position, faction) in [(Pos::new(2, 0), pink), (Pos::new(4, 0), teal)] {
            map.deploy(
                position,
                awbrn_map::Deployment {
                    unit: MapUnit::Infantry,
                    hp: VisualHp::new(10),
                    faction,
                },
            )
            .expect("the mirrored deployment tile is empty");
        }
        let mut state = crate::board::try_state_from_map(map, &[pink, teal], false, 1)
            .expect("the mirrored clear fixture is valid");
        let funds = state.player(seats(&state).0).funds;
        state.player_mut(seats(&state).1).funds = funds;
        state
    }

    /// Build the two fog observations of the same symmetric state.
    fn mirrored_fog_observations() -> (Observation, Observation) {
        let mut state = mirrored_clear_state();
        let (first, second) = seats(&state);
        state.player_mut(first).funds = 0;
        state.player_mut(second).funds = 0;
        let first = observe(
            &awvm::semantic::AwbwVisibility,
            &state,
            state.player(first).id(),
        )
        .expect("the first mirror observation is valid");
        let second = observe(
            &awvm::semantic::AwbwVisibility,
            &state,
            state.player(second).id(),
        )
        .expect("the second mirror observation is valid");
        (first, second)
    }

    /// The original material terms, isolated from calibrated position terms.
    fn material_weights() -> EvalWeights {
        EvalWeights {
            unit_count: 0.0,
            exposure: 0.0,
            contest: 0.0,
            front: 0.0,
            ..EvalWeights::STANDARD
        }
    }

    /// Reproduce the pre-Phase-1 score accumulation for the fixture corpus.
    ///
    /// This test-only reader records the equality policy. It must remain
    /// separate from the production reader so the comparison can detect a
    /// change in operation order.
    fn legacy_value(state: &State, seat: PlayerIdx, weights: EvalWeights) -> f64 {
        let day = (state.turn.day.min(1_000)) as f64;
        let days = weights.income_days * weights.income_decay.powf(day);
        let mut strengths = vec![0.0; state.players.len()];
        let mut rates = Vec::new();
        for (seat, player) in state.players.seats() {
            rates.push(commander::effective_income_per_property(state, seat) as f64);
            strengths[seat.get()] += weights.bank * player.funds as f64;
        }
        for unit in state.units.iter() {
            if let Some(strength) = strengths.get_mut(unit.owner.get()) {
                if matches!(unit.location, Location::Board { .. }) {
                    *strength += weights.unit_count;
                }
                *strength += weights.army * cost(unit.kind) * f64::from(unit.hp) / 100.0;
            }
        }

        let dimensions = state.board.dimensions();
        let mut occupant = vec![None; dimensions.len()];
        for unit in state.units.iter() {
            let Location::Board { position } = unit.location else {
                continue;
            };
            if let Some(cell) = dimensions.cell_index(position) {
                occupant[usize::from(cell.get())] = Some(unit.owner);
            }
        }
        let property_value = |terrain: Terrain, holder: PlayerIdx| {
            let has = |value| ruleset::terrain_has(terrain, value);
            let mut total = weights.plurality;
            if has(TerrainTrait::Income) {
                total += days * rates.get(holder.get()).copied().unwrap_or(0.0);
            }
            if has(TerrainTrait::ProducesGround)
                || has(TerrainTrait::ProducesAir)
                || has(TerrainTrait::ProducesSea)
            {
                total += weights.production;
            }
            if has(TerrainTrait::CaptureDefeatsOwner) {
                total += weights.hq;
            }
            total
        };
        for (position, tile) in state.board.iter() {
            if !tile.owner.is_ownable() {
                continue;
            }
            let holder = tile.owner.player();
            if let Some(holder) = holder {
                strengths[holder.get()] += property_value(tile.terrain, holder);
            }
            let points = tile.capture_points.unwrap_or(WHOLE_PROPERTY);
            if points >= WHOLE_PROPERTY {
                continue;
            }
            let Some(cell) = dimensions.cell_index(position) else {
                continue;
            };
            let Some(capturer) = occupant[usize::from(cell.get())] else {
                continue;
            };
            if holder.is_some_and(|holder| !hostile(state, holder, capturer)) {
                continue;
            }
            let progress = f64::from(WHOLE_PROPERTY - points) / f64::from(WHOLE_PROPERTY);
            let share = weights.capture * progress;
            if let Some(holder) = holder {
                strengths[holder.get()] -= share * property_value(tile.terrain, holder);
            }
            strengths[capturer.get()] += share * property_value(tile.terrain, capturer);
        }

        let mut threat = ThreatMap::new();
        let mut contest = ContestMap::new();
        for (current, player) in state.players.seats() {
            if player.status != PlayerStatus::Active {
                continue;
            }
            if weights.exposure != 0.0 {
                threat.build(&Session::new(state.clone()), current);
                for unit in state.units.iter().filter(|unit| unit.owner == current) {
                    let Location::Board { position } = unit.location else {
                        continue;
                    };
                    let Some(cell) = dimensions.cell_index(position) else {
                        continue;
                    };
                    let at_risk = threat
                        .immediate(cell, unit.kind)
                        .min(cost(unit.kind) * f64::from(unit.hp) / 100.0);
                    strengths[current.get()] -= weights.exposure * at_risk;
                }
            }
            if weights.contest == 0.0 && weights.front == 0.0 {
                continue;
            }
            contest.build(state, current);
            if !contest.is_built() {
                continue;
            }
            if weights.contest != 0.0 {
                for (position, tile) in state.board.iter() {
                    if tile.owner.is_ownable() && tile.owner.player().is_none() {
                        let Some(cell) = dimensions.cell_index(position) else {
                            continue;
                        };
                        let deficit = f64::from(contest.deficit(usize::from(cell.get())));
                        let share = 1.0 - deficit / f64::from(MAX_DEFICIT);
                        strengths[current.get()] +=
                            weights.contest * share * property_value(tile.terrain, current);
                    }
                }
            }
            if weights.front != 0.0 {
                for unit in state.units.iter().filter(|unit| unit.owner == current) {
                    let Location::Board { position } = unit.location else {
                        continue;
                    };
                    let Some(cell) = dimensions.cell_index(position) else {
                        continue;
                    };
                    let progress =
                        f64::from(contest.front(usize::from(cell.get()))) / f64::from(MAX_DEFICIT);
                    let army = cost(unit.kind) * f64::from(unit.hp) / 100.0;
                    strengths[current.get()] += weights.front * progress * army;
                }
            }
        }
        for (current, player) in state.players.seats() {
            if player.status != PlayerStatus::Active {
                strengths[current.get()] = 0.0;
            }
        }
        let team = state.player(seat).team.clone();
        let ours: f64 = state
            .players
            .seats_on_team(&team)
            .map(|current| strengths[current.get()])
            .sum();
        let mut rival = f64::NEG_INFINITY;
        for other in &state.teams {
            if other.id == team || other.status != TeamStatus::Active {
                continue;
            }
            let total: f64 = state
                .players
                .seats_on_team(&other.id)
                .map(|current| strengths[current.get()])
                .sum();
            rival = rival.max(total);
        }
        if rival.is_finite() {
            ours - rival
        } else {
            DECISIVE
        }
    }

    #[test]
    fn fixture_scores_keep_exact_bit_equality() {
        let fixtures = [
            (arena(false, 7), EvalWeights::STANDARD),
            (arena(true, 7), EvalWeights::FOG),
            (amber_valley(false, 11), EvalWeights::STANDARD),
        ];
        for (state, weights) in fixtures {
            let session = Session::new(state.clone());
            for (seat, _) in state.players.seats() {
                let expected = legacy_value(&state, seat, weights);
                let mut evaluator = Evaluator::new(weights);
                let actual = evaluator.value_in(&session, seat);
                let breakdown = evaluator.breakdown_in(&session, seat);
                assert_eq!(
                    actual, expected,
                    "fixture score changed for seat {seat:?}: {breakdown:?}"
                );
            }
        }
    }

    /// The named terms account for every fund, not almost every fund.
    ///
    /// The score keeps the legacy operation order so the baseline
    /// fingerprints hold, and the terms are accumulated separately, so the two
    /// can differ in the low bits. Nothing larger is rounding: it is a
    /// weighted contribution that reaches `strengths` and never reaches
    /// `build_terms`, and those funds would sit inside an unrelated named term
    /// telling the audit a lie. This is the check that used to be impossible
    /// while an unnamed remainder absorbed them.
    #[test]
    fn named_terms_part_from_the_score_only_by_rounding() {
        let fixtures = [
            (arena(false, 7), EvalWeights::STANDARD),
            (arena(true, 7), EvalWeights::FOG),
            (amber_valley(false, 11), EvalWeights::STANDARD),
        ];
        for (state, weights) in fixtures {
            let session = Session::new(state.clone());
            for (seat, _) in state.players.seats() {
                let breakdown = Evaluator::new(weights).breakdown_in(&session, seat);
                let residual = breakdown.context.term_residual;
                assert!(
                    residual.abs() <= RESIDUAL_TOLERANCE * breakdown.score.abs().max(1.0),
                    "seat {seat:?} lost {residual} funds of a {} score",
                    breakdown.score
                );
                assert_eq!(breakdown.terms.named_sum(), breakdown.score);
            }
        }
    }

    /// The named terms sit beside the score, not under it.
    ///
    /// The terms are flattened so that audit output shows each term at the
    /// top level of a row. That shape is what the diagnostics artifacts hold,
    /// so a change to it changes every artifact the pipeline reads back.
    /// `deny_unknown_fields` still refuses a key that belongs to neither
    /// part, which is what stops a renamed term from being read as an absent
    /// one.
    #[test]
    fn a_score_delta_reads_back_the_terms_it_flattened() {
        let delta = EvalScoreDelta {
            score: 12.0,
            terms: EvalTerms {
                army: 3.0,
                front: -1.0,
                ..EvalTerms::default()
            },
        };
        let mut value = serde_json::to_value(delta).expect("delta serializes");
        assert_eq!(value["score"], 12.0);
        assert_eq!(value["army"], 3.0);
        assert_eq!(value["front"], -1.0);
        assert_eq!(
            serde_json::from_value::<EvalScoreDelta>(value.clone()).expect("delta reads back"),
            delta
        );

        value["renamed_term"] = serde_json::json!(1.0);
        serde_json::from_value::<EvalScoreDelta>(value)
            .expect_err("a term this reader does not know must not be dropped in silence");
    }

    /// A breakdown keeps its terms flat and its context nested.
    #[test]
    fn a_breakdown_reads_back_its_flat_terms_and_nested_context() {
        let state = arena(false, 7);
        let session = Session::new(state.clone());
        let (seat, _) = seats(&state);
        let breakdown = Evaluator::new(EvalWeights::STANDARD).breakdown_in(&session, seat);

        let mut value = serde_json::to_value(breakdown).expect("breakdown serializes");
        assert_eq!(value["army"], breakdown.terms.army);
        assert!(value["context"]["weights"].is_object());
        assert_eq!(
            serde_json::from_value::<EvalBreakdown>(value.clone()).expect("breakdown reads back"),
            breakdown
        );

        value["renamed_term"] = serde_json::json!(1.0);
        serde_json::from_value::<EvalBreakdown>(value)
            .expect_err("a term this reader does not know must not be dropped in silence");
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
            (ours + theirs).abs() <= MIRROR_TOLERANCE,
            "{ours} and {theirs} do not cancel"
        );
    }

    #[test]
    fn a_constructed_clear_mirror_scores_zero() {
        let state = mirrored_clear_state();
        let (first, second) = seats(&state);
        let session = Session::new(state);
        let mut evaluator = Evaluator::new(EvalWeights::STANDARD);
        let first_breakdown = evaluator.breakdown_in(&session, first);
        let second_breakdown = evaluator.breakdown_in(&session, second);
        let first_score = first_breakdown.score;
        let second_score = second_breakdown.score;

        assert!(
            first_score.abs() <= MIRROR_TOLERANCE,
            "first mirrored score is {first_score}"
        );
        assert!(
            second_score.abs() <= MIRROR_TOLERANCE,
            "second mirrored score is {second_score}"
        );
    }

    #[test]
    fn mirrored_fog_observations_score_opposite_within_tolerance() {
        let (first, second) = mirrored_fog_observations();
        let first_session = Session::from_observation(&first).expect("the first session opens");
        let second_session = Session::from_observation(&second).expect("the second session opens");
        let first_seat = first_session
            .state()
            .players
            .seat(&first.recipient)
            .expect("the first recipient has a seat");
        let second_seat = second_session
            .state()
            .players
            .seat(&second.recipient)
            .expect("the second recipient has a seat");
        let mut evaluator = Evaluator::new(EvalWeights::FOG);
        let first_breakdown = evaluator.breakdown_in(&first_session, first_seat);
        let second_breakdown = evaluator.breakdown_in(&second_session, second_seat);
        let first_score = first_breakdown.score;
        let second_score = second_breakdown.score;

        assert!(
            (first_score + second_score).abs() <= MIRROR_TOLERANCE,
            "mirrored fog scores are {first_score} and {second_score}"
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

    #[test]
    fn unit_count_prices_one_more_fielded_action() {
        let mut state = arena(false, 7);
        let (first, _) = seats(&state);
        let weights = EvalWeights {
            army: 0.0,
            bank: 0.0,
            unit_count: 1_500.0,
            income_days: 0.0,
            plurality: 0.0,
            production: 0.0,
            hq: 0.0,
            capture: 0.0,
            ..material_weights()
        };
        let mut evaluator = Evaluator::new(weights);
        let before = evaluator.strength(&state, first);
        let headquarters = headquarters(&state, first);
        soldier(&mut state, first, headquarters);
        let after = evaluator.strength(&state, first);

        assert_eq!(after - before, weights.unit_count);
    }

    #[test]
    fn unit_count_remains_separate_from_the_army_breakdown() {
        let mut state = arena(false, 7);
        let (first, _) = seats(&state);
        let weights = EvalWeights {
            army: 2.0,
            bank: 0.0,
            unit_count: 1_500.0,
            income_days: 0.0,
            plurality: 0.0,
            production: 0.0,
            hq: 0.0,
            capture: 0.0,
            ..material_weights()
        };
        let headquarters = headquarters(&state, first);
        soldier(&mut state, first, headquarters);
        soldier(&mut state, first, headquarters);
        let session = Session::new(state);
        let breakdown = Evaluator::new(weights).breakdown_in(&session, first);

        assert_eq!(breakdown.army, weights.army * cost(UnitKind::Infantry));
        assert_eq!(breakdown.unit_count, weights.unit_count);
        assert_eq!(breakdown.score, breakdown.named_sum());
    }

    #[test]
    fn breakdown_names_every_term_and_uses_one_position_evaluation() {
        let state = arena(false, 7);
        let (first, _) = seats(&state);
        let session = Session::new(state);
        let mut evaluator = Evaluator::new(EvalWeights::STANDARD);
        let breakdown = evaluator.breakdown_in(&session, first);

        assert_eq!(evaluator.position_evaluations(), 1);
        assert_eq!(breakdown.context.position_evaluations, 1);
        assert_eq!(breakdown.score, breakdown.named_sum());
        assert_eq!(breakdown.context.weights, EvalWeights::STANDARD);
        assert_eq!(breakdown.context.preset, EvalPreset::Explicit);
        assert!(breakdown.context.position_enabled);
        let serialized = serde_json::to_value(breakdown).expect("breakdown serializes");
        assert!(serialized.get("army").is_some());
        assert!(serialized.get("bank").is_some());
        assert!(serialized.get("other").is_none());
    }

    #[test]
    fn the_weight_table_names_every_eval_weight() {
        let weights = EVAL_WEIGHT_TERMS
            .iter()
            .map(|term| term.weight)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(weights.len(), EVAL_WEIGHT_TERMS.len());
        assert_eq!(weights.len(), 13);
        assert!(EVAL_WEIGHT_TERMS.iter().all(|term| {
            term.contribution == "win-probability"
                || EvalTerms::default().value(term.contribution).is_some()
        }));
    }

    #[test]
    fn linear_weight_removal_changes_only_its_named_contribution() {
        let state = arena(false, 7);
        let (first, _) = seats(&state);
        let session = Session::new(state);
        let base = EvalWeights {
            exposure: 0.0,
            contest: 0.0,
            front: 0.0,
            ..EvalWeights::STANDARD
        };
        let full = Evaluator::new(base).breakdown_in(&session, first);

        for term in EVAL_WEIGHT_TERMS
            .iter()
            .filter(|term| term.kind == EvalWeightKind::Linear)
        {
            let mut without = base;
            match term.weight {
                "army" => without.army = 0.0,
                "unit_count" => without.unit_count = 0.0,
                "bank" => without.bank = 0.0,
                "plurality" => without.plurality = 0.0,
                "production" => without.production = 0.0,
                "hq" => without.hq = 0.0,
                weight => panic!("unexpected linear weight {weight}"),
            }
            let changed = Evaluator::new(without).breakdown_in(&session, first);
            for contribution in [
                "army",
                "unit_count",
                "bank",
                "income",
                "plurality",
                "production",
                "hq",
                "capture",
                "exposure",
                "contest",
                "front",
            ] {
                if contribution == term.contribution {
                    continue;
                }
                assert_eq!(
                    full.terms().value(contribution),
                    changed.terms().value(contribution),
                    "removing {} changed {contribution}",
                    term.weight
                );
            }
        }
    }

    #[test]
    fn terminal_breakdown_has_no_terms_and_records_the_reason() {
        let mut state = amber_valley(false, 7);
        let (first, _) = seats(&state);
        let winners = vec![state.player(first).team.clone()];
        state.match_state = Match::Finished {
            outcome: Outcome::Victory {
                winners,
                reason: awvm::ruleset::VictoryReason::HqCapture,
            },
        };
        let session = Session::new(state);
        let mut evaluator = Evaluator::new(EvalWeights::STANDARD);
        let breakdown = evaluator.breakdown_in(&session, first);

        assert_eq!(breakdown.score, DECISIVE);
        assert_eq!(breakdown.terms(), EvalTerms::default());
        assert_eq!(
            breakdown.context.terminal_reason,
            Some(EvalTerminalReason::Victory)
        );
        assert_eq!(evaluator.position_evaluations(), 0);
        assert_eq!(breakdown.context.position_evaluations, 0);
    }

    #[test]
    fn cargo_keeps_material_value_but_not_fielded_unit_count() {
        let mut state = arena(false, 7);
        let (first, _) = seats(&state);
        let position = headquarters(&state, first);
        let id = soldier(&mut state, first, position);
        let weights = EvalWeights {
            army: 0.0,
            bank: 0.0,
            unit_count: 1_500.0,
            income_days: 0.0,
            plurality: 0.0,
            production: 0.0,
            hq: 0.0,
            capture: 0.0,
            ..material_weights()
        };
        let mut evaluator = Evaluator::new(weights);
        let fielded = evaluator.strength(&state, first);
        state
            .units
            .get_mut(id)
            .expect("the soldier was pushed")
            .location = Location::Cargo {
            transport: UnitId::new(9_999),
            slot: 0,
        };
        let cargo = evaluator.strength(&state, first);

        assert_eq!(fielded - cargo, weights.unit_count);
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

    #[test]
    fn a_medium_tank_in_counterattack_range_loses_leaf_value() {
        let mut state = amber_valley(false, 7);
        let (first, second) = seats(&state);
        let first_hq = headquarters(&state, first);
        let exposed_tile = first_hq.offset(0, 1).expect("ground below the first hq");
        let counterattacker = exposed_tile
            .offset(1, 0)
            .expect("ground beside the exposed tile");
        let medium_tank = field_unit(&mut state, first, exposed_tile, UnitKind::MdTank);
        let safe_tile = headquarters(&state, second);
        let enemy_tank = field_unit(&mut state, second, safe_tile, UnitKind::MdTank);

        let mut evaluator = Evaluator::new(EvalWeights {
            contest: 0.0,
            front: 0.0,
            ..EvalWeights::STANDARD
        });
        let safe = evaluator.value(&state, first);
        state
            .units
            .get_mut(enemy_tank)
            .expect("the counterattacker exists")
            .location = Location::Board {
            position: counterattacker,
        };
        let exposed = evaluator.value(&state, first);

        assert!(
            exposed < safe,
            "counterattack exposure must reduce leaf value"
        );
        assert!(state.units.get(medium_tank).is_some());
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
