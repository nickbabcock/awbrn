//! Planner v5 with a frozen score from human replay data.

use std::collections::BTreeMap;

use awbrn_ai::agent::Agent;
use awbrn_ai::planner::{PlannerAgent, PlannerConfig};
use awbrn_ai::replay_score::{ReplayReader, ReplayScore};
use awbrn_ai_diagnostic_types::{AgentIdentity, fingerprint_bytes};
use awvm::semantic::PlayerIdx;
use awvm::session::Session;
use serde::{Deserialize, Serialize};

use crate::feature_analysis::{FEATURE_NAMES, FeatureMode};
use crate::tournament::AgentFactory;

/// Frozen coefficients and their training source.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayPlannerModel {
    pub schema_version: u16,
    pub source_report_fingerprint: String,
    pub corpus_fingerprint: String,
    pub mode: FeatureMode,
    pub intercept: f64,
    pub coefficients: BTreeMap<String, f64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra_coefficients: BTreeMap<String, f64>,
}

impl ReplayPlannerModel {
    /// Check the feature schema and the funds conversion.
    pub fn validate(&self) -> Result<(), String> {
        if self.extra_coefficients.iter().any(|(name, value)| {
            !EXTRA_FEATURE_NAMES.contains(&name.as_str())
                || !value.is_finite()
                || !(value
                    / self
                        .coefficients
                        .get("material_delta")
                        .copied()
                        .unwrap_or(0.0))
                .is_finite()
        }) {
            return Err("invalid replay planner extra coefficient".into());
        }
        if self.schema_version != 1
            || self.mode != FeatureMode::FogVisible
            || self.corpus_fingerprint.is_empty()
            || self.source_report_fingerprint.is_empty()
            || !self.intercept.is_finite()
            || self.coefficients.len() != FEATURE_NAMES.len()
            || FEATURE_NAMES
                .iter()
                .any(|name| self.coefficients.get(*name).is_none_or(|v| !v.is_finite()))
        {
            return Err("invalid replay planner model".into());
        }
        if self.coefficients["material_delta"] <= 0.0 {
            return Err("replay planner needs a positive material coefficient".into());
        }
        if self
            .coefficients
            .values()
            .any(|value| !(value / self.coefficients["material_delta"]).is_finite())
        {
            return Err("replay planner funds conversion is not finite".into());
        }
        Ok(())
    }

    #[cfg(test)]
    fn funds_score(
        &self,
        features: crate::feature_analysis::FeatureVector,
        rival_bank: f64,
    ) -> f64 {
        // Half the difference of the two seat logits removes the intercept
        // and turn index. The absolute bank feature becomes half a delta.
        let terms = [
            ("material_delta", features.material_delta),
            ("income_delta", features.income_delta),
            ("own_bank", 0.5 * (features.own_bank - rival_bank)),
            ("unit_count_delta", features.unit_count_delta),
            ("capture_progress_delta", features.capture_progress_delta),
            ("front_position_delta", features.front_position_delta),
            (
                "immediate_threat_safety_delta",
                features.immediate_threat_safety_delta,
            ),
            (
                "deferred_threat_safety_delta",
                features.deferred_threat_safety_delta,
            ),
        ];
        terms
            .iter()
            .map(|(name, value)| self.coefficients[*name] * value)
            .sum::<f64>()
            / self.coefficients["material_delta"]
    }
}

impl ReplayPlannerModel {
    /// Convert the logit coefficients to the funds score of the planner.
    pub fn score(&self) -> ReplayScore {
        let material = self.coefficients["material_delta"];
        let base = |name: &str| self.coefficients[name] / material;
        let extra =
            |name: &str| self.extra_coefficients.get(name).copied().unwrap_or(0.0) / material;
        ReplayScore {
            material: 1.0,
            income: base("income_delta"),
            bank: base("own_bank"),
            unit_count: base("unit_count_delta"),
            capture_progress: base("capture_progress_delta"),
            front_position: base("front_position_delta"),
            immediate_threat: base("immediate_threat_safety_delta"),
            deferred_threat: base("deferred_threat_safety_delta"),
            material_front: extra("material_front"),
            property_capture: extra("property_capture"),
            production: extra("production"),
            contest: extra("contest"),
            power_meter_army: extra("power_meter_army"),
            cop_ready_army: extra("cop_ready_army"),
            scop_ready_army: extra("scop_ready_army"),
            eagle_refresh: extra("eagle_refresh"),
        }
    }

    /// Read one position with a fresh reader, as the planner does at a leaf.
    /// Games without a fitted score use the stock score of planner v5.
    pub fn value_in(&self, session: &Session, seat: PlayerIdx) -> f64 {
        if let Some(value) = awbrn_ai::Evaluator::terminal_value(session.state(), seat) {
            return value;
        }
        if !ReplayScore::applies(session.state()) {
            return awbrn_ai::Evaluator::new(PlannerConfig::V5.eval_weights)
                .value_in(session, seat);
        }
        ReplayReader::new(self.score()).value_in(session, seat)
    }
}

/// Additional features measured in the same way during fitting and play.
pub const EXTRA_FEATURE_NAMES: [&str; 8] = [
    "material_front",
    "property_capture",
    "production",
    "contest",
    "power_meter_army",
    "cop_ready_army",
    "scop_ready_army",
    "eagle_refresh",
];

/// Extract observable position and commander features for a duel.
pub fn replay_extra_features(session: &Session, seat: PlayerIdx) -> BTreeMap<String, f64> {
    let mut stock = awbrn_ai::Evaluator::new(awbrn_ai::EvalWeights::STANDARD);
    EXTRA_FEATURE_NAMES
        .iter()
        .map(|name| (*name).to_owned())
        .zip(extra_values(&mut stock, session, seat))
        .collect()
}

/// Read the extra features in the order of [`EXTRA_FEATURE_NAMES`].
///
/// The stock terms come from `stock`. A stock weight of zero gives zero.
fn extra_values(
    stock: &mut awbrn_ai::Evaluator,
    session: &Session,
    seat: PlayerIdx,
) -> [f64; EXTRA_FEATURE_NAMES.len()] {
    use awvm::commander::{PowerLevel, power_activation_cost};
    use awvm::ruleset::{CommanderKind, UnitKind, profile};
    use awvm::semantic::{Location, PowerState, UnitAction};
    let state = session.state();
    let terms = stock.breakdown_in(session, seat).terms;
    let mut power = [0.0; 4];
    for (owner, player) in state.players.seats() {
        let sign = if owner == seat { 1.0 } else { -1.0 };
        let army = state
            .units
            .iter()
            .filter(|unit| unit.owner == owner)
            .map(|unit| profile(unit.kind).cost as f64 * f64::from(unit.hp) / 100.0)
            .sum::<f64>();
        let Some(co) = player.commanders.iter().find(|co| co.active) else {
            continue;
        };
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
                .map(|unit| profile(unit.kind).cost as f64 * f64::from(unit.hp) / 100.0)
                .sum::<f64>();
            power[3] += sign * refresh;
        }
    }
    [
        terms.front,
        terms.capture,
        terms.production,
        terms.contest,
        power[0],
        power[1],
        power[2],
        power[3],
    ]
}

/// A factory that changes only the position score of planner v5.
#[derive(Clone, Debug)]
pub struct ReplayPlannerFactory {
    identity: AgentIdentity,
    config: PlannerConfig,
}

impl ReplayPlannerFactory {
    pub fn new(
        identifier: &str,
        model: ReplayPlannerModel,
        content_fingerprint: String,
    ) -> Result<Self, String> {
        model.validate()?;
        if identifier.is_empty() || content_fingerprint.is_empty() {
            return Err("replay planner needs an identifier and model fingerprint".into());
        }
        let config = serde_json::to_vec(&(
            identifier,
            PlannerConfig::V5.fingerprint(),
            &model,
            content_fingerprint,
        ))
        .map_err(|error| error.to_string())?;
        Ok(Self {
            identity: AgentIdentity {
                identifier: identifier.into(),
                configuration_fingerprint: fingerprint_bytes(&config),
                executable_fingerprint: "awbrn-ai-replay-planner-v1".into(),
            },
            config: PlannerConfig {
                replay_score: Some(model.score()),
                ..PlannerConfig::V5
            },
        })
    }
}

impl AgentFactory for ReplayPlannerFactory {
    fn identity(&self) -> &AgentIdentity {
        &self.identity
    }

    fn create(&self, seed: u64) -> Box<dyn Agent> {
        Box::new(PlannerAgent::with_config(seed, self.config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature_analysis::FeatureVector;

    fn model() -> ReplayPlannerModel {
        serde_json::from_str(include_str!(
            "../../../assets/ai-diagnostics/human-evaluator/refit-positive-003-model.json"
        ))
        .unwrap()
    }

    #[test]
    fn frozen_model_is_valid_and_duel_score_is_antisymmetric() {
        let model = model();
        model.validate().unwrap();
        let mut state = awbrn_ai::board::arena(false, 3);
        let seats = state
            .players
            .seats()
            .map(|(seat, _)| seat)
            .collect::<Vec<_>>();
        state.player_mut(seats[0]).funds = 20_000;
        state.player_mut(seats[1]).funds = 5_000;
        let session = Session::new(state);
        let seats = session
            .state()
            .players
            .seats()
            .map(|(seat, _)| seat)
            .collect::<Vec<_>>();
        let a = model.value_in(&session, seats[0]);
        let b = model.value_in(&session, seats[1]);
        assert!(a > 0.0);
        assert!((a + b).abs() < 1.0e-9);
    }

    /// The score of the original implementation, with fresh maps for each read.
    fn reference_value(model: &ReplayPlannerModel, session: &Session, seat: PlayerIdx) -> f64 {
        let state = session.state();
        let features = crate::feature_analysis::observable_features(state, seat, 0).unwrap();
        let rival = state
            .players
            .seats()
            .find(|(other, _)| *other != seat)
            .unwrap()
            .1;
        let extra = replay_extra_features(session, seat);
        model.funds_score(features, rival.funds as f64)
            + model
                .extra_coefficients
                .iter()
                .map(|(name, weight)| extra[name] * weight / model.coefficients["material_delta"])
                .sum::<f64>()
    }

    struct MeanLuck;

    impl awvm::random::Entropy for MeanLuck {
        fn luck(
            &mut self,
            _polarity: awvm::random::Luck,
            domain: awvm::commander::Domain,
        ) -> Result<i64, awvm::random::RandomError> {
            Ok(domain.minimum + (domain.maximum - domain.minimum) / 2)
        }

        fn weather(&mut self) -> Result<awvm::ruleset::WeatherKind, awvm::random::RandomError> {
            Ok(awvm::ruleset::WeatherKind::Clear)
        }
    }

    #[test]
    fn reused_maps_give_the_reference_score() {
        let selected = model();
        let mut with_front = selected.clone();
        with_front
            .coefficients
            .insert("front_position_delta".into(), 0.272);
        let models = [selected, with_front];
        let root = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../awbrn-ai/tests/fixtures/replay_regressions/"
        );
        let mut readers = models
            .iter()
            .map(|model| ReplayReader::new(model.score()))
            .collect::<Vec<_>>();
        let mut reads = 0;
        for day in 6..=11 {
            let bytes = std::fs::read(format!("{root}amber-valley-day{day:02}.json")).unwrap();
            let state: awvm::semantic::State = serde_json::from_slice(&bytes).unwrap();
            let mut agent = awbrn_ai::HARD.agent(day);
            let turn = awbrn_ai::harness::run_agent_turn_unmeasured(
                state.clone(),
                agent.as_mut(),
                &mut MeanLuck,
                awbrn_ai::HARD.node_budget(),
            )
            .unwrap();
            let mut session = Session::new(state);
            let seats = session
                .state()
                .players
                .seats()
                .map(|(seat, _)| seat)
                .collect::<Vec<_>>();
            for command in std::iter::once(None).chain(turn.commands.iter().map(Some)) {
                if let Some(command) = command {
                    let order = session.resolve(command).unwrap();
                    session.apply(order, &mut MeanLuck, &mut ()).unwrap();
                }
                if !matches!(
                    session.state().match_state,
                    awvm::semantic::Match::Active { .. }
                ) {
                    break;
                }
                for (model, reader) in models.iter().zip(&mut readers) {
                    for seat in &seats {
                        let expected = reference_value(model, &session, *seat);
                        let actual = reader.value_in(&session, *seat);
                        // The stock score puts its rounding residual in the
                        // front term. Without the unused stock terms, that
                        // residual can change in the last bits.
                        assert!(
                            (expected - actual).abs() <= 1e-9 * expected.abs().max(1.0),
                            "day {day}: {expected} != {actual}"
                        );
                        reads += 1;
                    }
                }
            }
        }
        assert!(reads > 100, "{reads}");
    }

    #[test]
    fn the_planner_score_is_the_selected_fit() {
        let model = serde_json::from_str::<ReplayPlannerModel>(include_str!(
            "../../../assets/ai-diagnostics/human-evaluator/refit-positive-003-model.json"
        ))
        .unwrap();
        assert_eq!(model.score(), ReplayScore::CUP_3_4);
    }

    #[test]
    fn a_fund_of_material_keeps_the_reply_scale() {
        let model = model();
        let features = FeatureVector {
            material_delta: 1.0,
            ..FeatureVector::default()
        };
        assert_eq!(model.funds_score(features, 0.0), 1.0);
        let mut calibration_changed = model.clone();
        calibration_changed.intercept = 1000.0;
        calibration_changed
            .coefficients
            .insert("turn_index".into(), 999.0);
        assert_eq!(
            calibration_changed.funds_score(
                FeatureVector {
                    turn_index: 42.0,
                    ..features
                },
                0.0
            ),
            1.0
        );
        let mut invalid = model.clone();
        invalid.coefficients.insert("material_delta".into(), 0.0);
        assert!(invalid.validate().is_err());
        invalid = model;
        invalid.coefficients.insert("income_delta".into(), f64::NAN);
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn extra_features_are_antisymmetric_and_charge_changes_the_score() {
        let mut state = awbrn_ai::puzzles::suite()
            .into_iter()
            .find(|puzzle| puzzle.name == "focus-fire")
            .unwrap()
            .state;
        let seats = state
            .players
            .seats()
            .map(|(seat, _)| seat)
            .collect::<Vec<_>>();
        state.player_mut(seats[0]).commanders[0].id = awvm::ruleset::CommanderKind::Eagle;
        state.player_mut(seats[0]).commanders[0].power_charge = 81_000;
        let session = Session::new(state.clone());
        let a = replay_extra_features(&session, seats[0]);
        let b = replay_extra_features(&session, seats[1]);
        for name in EXTRA_FEATURE_NAMES {
            assert!((a[name] + b[name]).abs() < 1e-8, "{name}");
        }
        assert!(a["power_meter_army"] > 0.0);
        assert!(a["scop_ready_army"] > 0.0);
        let mut model = model();
        model
            .extra_coefficients
            .insert("power_meter_army".into(), 1e-5);
        model.validate().unwrap();
        let charged = model.value_in(&session, seats[0]);
        state.player_mut(seats[0]).commanders[0].power_charge = 0;
        let zero = Session::new(state);
        assert!(charged > model.value_in(&zero, seats[0]));
        model.extra_coefficients.insert("unknown".into(), 0.0);
        assert!(model.validate().is_err());
    }
}
