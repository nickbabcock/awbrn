//! A position score fitted to human AWBW replays.
//!
//! The fit predicts the winner of tournament duels from each completed turn,
//! as the player who ended the turn sees the board. The planner uses half the
//! difference of the two seat logits, which removes the intercept and the
//! turn calibration of the fit. The result is divided by the material
//! coefficient, so one fund of material has the value one, and the reply
//! estimate keeps its scale.
//!
//! The score applies only to duels without fog, which is the content of the
//! fitted corpus. Other games use the stock score of the planner.
//!
//! The coefficients come from the model file of the fit. A test compares
//! them.

use awvm::commander::{self, PowerLevel, power_activation_cost};
use awvm::ruleset::{self, CommanderKind, TerrainTrait, UnitKind};
use awvm::semantic::{CAPTURE_REQUIRED_POINTS, Location, PlayerIdx, PowerState, State, UnitAction};
use awvm::session::Session;

use crate::eval::{EvalWeights, Evaluator};
use crate::map::ContestMap;
use crate::threat::ThreatMap;

/// The coefficients of the score, in funds for one unit of each feature.
///
/// Each delta feature is the value of the seat less the value of its rival.
/// A coefficient of zero turns off the work for its feature.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayScore {
    /// Army value, scaled by health. This is one by definition.
    pub material: f64,
    /// Property income for one day.
    pub income: f64,
    /// Funds in the bank. The score uses half of the bank difference.
    pub bank: f64,
    /// Number of units, with cargo.
    pub unit_count: f64,
    /// Capture progress of our units on properties, as shares of a capture.
    pub capture_progress: f64,
    /// Mean front position of our board units, in steps.
    pub front_position: f64,
    /// Immediate threat on the rival's units less the threat on ours.
    pub immediate_threat: f64,
    /// Deferred threat on the rival's units less the threat on ours.
    pub deferred_threat: f64,
    /// The front term of the stock score.
    pub material_front: f64,
    /// The capture term of the stock score.
    pub property_capture: f64,
    /// The production term of the stock score.
    pub production: f64,
    /// The contest term of the stock score.
    pub contest: f64,
    /// Army value multiplied by the share of the full power meter.
    pub power_meter_army: f64,
    /// Army value when the normal power is available.
    pub cop_ready_army: f64,
    /// Army value when the super power is available.
    pub scop_ready_army: f64,
    /// Value of Eagle's units that the super power can refresh.
    pub eagle_refresh: f64,
}

/// The material coefficient of the Cup 3 and 4 fit, in logits per fund.
const CUP_3_4_MATERIAL: f64 = 6.206_188_904_349_089e-5;

impl ReplayScore {
    /// The fit on Cups 3 and 4, Division 1: nonnegative weights with an L2
    /// penalty of 0.003. Source:
    /// `assets/ai-diagnostics/human-evaluator/refit-positive-003-model.json`.
    pub const CUP_3_4: Self = Self {
        material: 1.0,
        income: 3.306_273_208_283_262_6e-4 / CUP_3_4_MATERIAL,
        bank: 9.690_198_422_248_923e-5 / CUP_3_4_MATERIAL,
        unit_count: 0.099_613_397_579_772_51 / CUP_3_4_MATERIAL,
        capture_progress: 0.086_030_140_260_857_66 / CUP_3_4_MATERIAL,
        front_position: 0.0,
        immediate_threat: 8.671_284_560_132_035e-6 / CUP_3_4_MATERIAL,
        deferred_threat: 7.389_935_168_284_499e-6 / CUP_3_4_MATERIAL,
        material_front: 9.719_046_650_727_131e-6 / CUP_3_4_MATERIAL,
        property_capture: 4.374_585_270_084_091e-5 / CUP_3_4_MATERIAL,
        production: 6.300_178_189_646_71e-5 / CUP_3_4_MATERIAL,
        contest: 0.0,
        power_meter_army: 3.919_376_025_746_918e-6 / CUP_3_4_MATERIAL,
        cop_ready_army: 0.0,
        scop_ready_army: 5.929_110_029_162_188e-6 / CUP_3_4_MATERIAL,
        eagle_refresh: 0.0,
    };

    /// Whether the score applies to this game.
    pub fn applies(state: &State) -> bool {
        state.players.len() == 2 && !state.settings.fog
    }

    /// Return the stock weights for the stock terms. A term that the score
    /// does not read has the weight zero, so that the stock score does not
    /// build its maps.
    fn stock_weights(&self) -> EvalWeights {
        let standard = EvalWeights::STANDARD;
        let weight = |coefficient: f64, value: f64| if coefficient == 0.0 { 0.0 } else { value };
        EvalWeights {
            exposure: 0.0,
            front: weight(self.material_front, standard.front),
            capture: weight(self.property_capture, standard.capture),
            production: weight(self.production, standard.production),
            contest: weight(self.contest, standard.contest),
            ..standard
        }
    }

    fn reads_stock(&self) -> bool {
        self.material_front != 0.0
            || self.property_capture != 0.0
            || self.production != 0.0
            || self.contest != 0.0
    }

    fn reads_threat(&self) -> bool {
        self.immediate_threat != 0.0 || self.deferred_threat != 0.0
    }
}

/// The features of one seat.
#[derive(Clone, Copy, Debug, Default)]
struct SeatFeatures {
    material: f64,
    income: f64,
    bank: f64,
    unit_count: f64,
    capture_progress: f64,
    front_position: f64,
    immediate_threat: f64,
    deferred_threat: f64,
}

/// Reads [`ReplayScore`] values and keeps its maps between reads.
///
/// A threat map reuses its work for units that did not change, so a search
/// that reads related positions does less work than a fresh read.
#[derive(Debug)]
pub struct ReplayReader {
    score: ReplayScore,
    threat: Vec<ThreatMap>,
    contest: Vec<ContestMap>,
    stock: Evaluator,
}

impl ReplayReader {
    pub fn new(score: ReplayScore) -> Self {
        Self {
            score,
            threat: Vec::new(),
            contest: Vec::new(),
            stock: Evaluator::new(score.stock_weights()),
        }
    }

    /// What an active duel without fog is worth to `seat`, in funds.
    ///
    /// The caller checks [`ReplayScore::applies`] and gives terminal positions
    /// their stock scores.
    pub fn value_in(&mut self, session: &Session, seat: PlayerIdx) -> f64 {
        let state = session.state();
        let Some(rival) = state
            .players
            .seats()
            .map(|(other, _)| other)
            .find(|other| *other != seat && crate::threat::hostile(state, seat, *other))
        else {
            return 0.0;
        };
        let score = self.score;
        let own = self.seat(session, seat);
        let other = self.seat(session, rival);
        let mut value = score.material * (own.material - other.material)
            + score.income * (own.income - other.income)
            + score.bank * 0.5 * (own.bank - other.bank)
            + score.unit_count * (own.unit_count - other.unit_count)
            + score.capture_progress * (own.capture_progress - other.capture_progress)
            + score.front_position * (own.front_position - other.front_position)
            + score.immediate_threat * (other.immediate_threat - own.immediate_threat)
            + score.deferred_threat * (other.deferred_threat - own.deferred_threat);
        if score.reads_stock() {
            let terms = self.stock.breakdown_in(session, seat).terms;
            value += score.material_front * terms.front
                + score.property_capture * terms.capture
                + score.production * terms.production
                + score.contest * terms.contest;
        }
        let power = power_features(state, seat);
        value += score.power_meter_army * power[0]
            + score.cop_ready_army * power[1]
            + score.scop_ready_army * power[2]
            + score.eagle_refresh * power[3];
        value
    }

    fn seat(&mut self, session: &Session, seat: PlayerIdx) -> SeatFeatures {
        let state = session.state();
        let index = seat.get();
        if self.threat.len() <= index {
            self.threat.resize_with(index + 1, ThreatMap::new);
            self.contest.resize_with(index + 1, ContestMap::new);
        }
        let contest = (self.score.front_position != 0.0).then(|| {
            let map = &mut self.contest[index];
            map.build(state, seat);
            &*map
        });
        let threat = self.score.reads_threat().then(|| {
            let map = &mut self.threat[index];
            map.build(session, seat);
            &*map
        });
        seat_features(state, seat, contest, threat)
    }
}

/// Read the features of one seat. A map that is absent gives zero for its
/// features.
fn seat_features(
    state: &State,
    seat: PlayerIdx,
    contest: Option<&ContestMap>,
    threat: Option<&ThreatMap>,
) -> SeatFeatures {
    let dimensions = state.board.dimensions();
    let mut features = SeatFeatures {
        bank: state.player(seat).funds as f64,
        ..SeatFeatures::default()
    };
    let mut front_total = 0.0;
    let mut front_units = 0_u64;
    for unit in state.units.iter().filter(|unit| unit.owner == seat) {
        features.material += army_value(unit.kind, unit.hp);
        features.unit_count += 1.0;
        let Location::Board { position } = unit.location else {
            continue;
        };
        let Some(cell) = dimensions.cell_index(position) else {
            continue;
        };
        if let Some(contest) = contest {
            front_total += f64::from(contest.front(usize::from(cell.get())));
        }
        front_units += 1;
        if let Some(threat) = threat {
            features.immediate_threat += threat.immediate(cell, unit.kind);
            features.deferred_threat += threat.deferred(cell, unit.kind);
        }
    }
    if front_units > 0 {
        features.front_position = front_total / front_units as f64;
    }

    let income_properties = state
        .board
        .tiles()
        .filter(|tile| {
            tile.owner.is_owned_by(seat) && ruleset::terrain_has(tile.terrain, TerrainTrait::Income)
        })
        .count() as f64;
    features.income =
        income_properties * commander::effective_income_per_property(state, seat) as f64;

    for unit in state.units.iter().filter(|unit| unit.owner == seat) {
        let Location::Board { position } = unit.location else {
            continue;
        };
        let Some(tile) = state.board.get(position) else {
            continue;
        };
        let Some(points) = tile.capture_points else {
            continue;
        };
        if points >= CAPTURE_REQUIRED_POINTS
            || !ruleset::terrain_has(tile.terrain, TerrainTrait::Capturable)
        {
            continue;
        }
        features.capture_progress +=
            f64::from(CAPTURE_REQUIRED_POINTS - points) / f64::from(CAPTURE_REQUIRED_POINTS);
    }
    features
}

fn army_value(kind: UnitKind, hp: u8) -> f64 {
    ruleset::profile(kind).cost as f64 * f64::from(hp) / 100.0
}

/// The power features from the view of `seat`: the power meter share, the
/// normal power, the super power, and Eagle's refresh. Each is signed, so
/// the rival's value counts against the seat.
fn power_features(state: &State, seat: PlayerIdx) -> [f64; 4] {
    let mut power = [0.0; 4];
    for (owner, player) in state.players.seats() {
        let sign = if owner == seat { 1.0 } else { -1.0 };
        let Some(co) = player.commanders.iter().find(|co| co.active) else {
            continue;
        };
        let army = state
            .units
            .iter()
            .filter(|unit| unit.owner == owner)
            .map(|unit| army_value(unit.kind, unit.hp))
            .sum::<f64>();
        let cop = power_activation_cost(co.id, PowerLevel::Cop, co.power_uses)
            .ok()
            .flatten();
        let scop = power_activation_cost(co.id, PowerLevel::Scop, co.power_uses)
            .ok()
            .flatten();
        let full = scop.or(cop).unwrap_or(u64::MAX).max(1);
        power[0] += sign * army * (co.power_charge as f64 / full as f64).min(1.0);
        power[1] += sign * army * f64::from(cop.is_some_and(|cost| co.power_charge >= cost));
        power[2] += sign * army * f64::from(scop.is_some_and(|cost| co.power_charge >= cost));
        // Only Lightning Strike, the super power, refreshes unit actions.
        if co.id == CommanderKind::Eagle {
            let available = scop.is_some_and(|cost| co.power_charge >= cost);
            let active = matches!(player.power_state, PowerState::Scop { .. });
            let refresh = state
                .units
                .iter()
                .filter(|unit| {
                    unit.owner == owner
                        && matches!(unit.location, Location::Board { .. })
                        && !matches!(unit.kind, UnitKind::Infantry | UnitKind::Mech)
                        && ((available && unit.action == UnitAction::Spent)
                            || (active && unit.action == UnitAction::Ready))
                })
                .map(|unit| army_value(unit.kind, unit.hp))
                .sum::<f64>();
            power[3] += sign * refresh;
        }
    }
    power
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each coefficient of the model file, divided by the material
    /// coefficient, is the coefficient of [`ReplayScore::CUP_3_4`].
    #[test]
    fn the_cup_3_4_score_is_the_model_file() {
        let model: serde_json::Value = serde_json::from_str(include_str!(
            "../../../assets/ai-diagnostics/human-evaluator/refit-positive-003-model.json"
        ))
        .unwrap();
        let coefficient = |group: &str, name: &str| model[group][name].as_f64().unwrap();
        let material = coefficient("coefficients", "material_delta");
        assert_eq!(material, CUP_3_4_MATERIAL);
        let score = ReplayScore::CUP_3_4;
        for (group, name, value) in [
            ("coefficients", "income_delta", score.income),
            ("coefficients", "own_bank", score.bank),
            ("coefficients", "unit_count_delta", score.unit_count),
            (
                "coefficients",
                "capture_progress_delta",
                score.capture_progress,
            ),
            ("coefficients", "front_position_delta", score.front_position),
            (
                "coefficients",
                "immediate_threat_safety_delta",
                score.immediate_threat,
            ),
            (
                "coefficients",
                "deferred_threat_safety_delta",
                score.deferred_threat,
            ),
            ("extra_coefficients", "material_front", score.material_front),
            (
                "extra_coefficients",
                "property_capture",
                score.property_capture,
            ),
            ("extra_coefficients", "production", score.production),
            ("extra_coefficients", "contest", score.contest),
            (
                "extra_coefficients",
                "power_meter_army",
                score.power_meter_army,
            ),
            ("extra_coefficients", "cop_ready_army", score.cop_ready_army),
            (
                "extra_coefficients",
                "scop_ready_army",
                score.scop_ready_army,
            ),
            ("extra_coefficients", "eagle_refresh", score.eagle_refresh),
        ] {
            assert_eq!(coefficient(group, name) / material, value, "{name}");
        }
        assert_eq!(score.material, 1.0);
    }
}
