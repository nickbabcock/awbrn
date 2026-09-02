//! Per-turn feature extraction and offline logistic regression.
//!
//! The event log is the source of truth. This module derives labelled rows
//! after completed turns. It writes two views of each row:
//!
//! - `authoritative` uses the full post-hoc state.
//! - `fog-visible` uses the state reified from the acting player's view.
//!
//! The second view is the only view that can support a live policy. Neither
//! view is wired into the production evaluator by this module.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use awbrn_ai::{ContestMap, ThreatMap};
use awbrn_ai_diagnostic_types::{PairKey, SeatOrderVariant};
use awvm::commander;
use awvm::ruleset::{self, TerrainTrait};
use awvm::semantic::{AwbwVisibility, Location, Match, Outcome, PlayerIdx, State, observe};
use awvm::session::Session;
use awvm::transition::Command;
use serde::{Deserialize, Serialize};

use crate::events::{EventKind, EventLogError, EventRow, latest_attempt_rows, read_event_log};

/// The feature-analysis output schema.
pub const FEATURE_ANALYSIS_SCHEMA_VERSION: u16 = 4;

fn default_converged() -> bool {
    true
}

/// Features used by both model views, in coefficient order.
pub const FEATURE_NAMES: [&str; 9] = [
    "turn_index",
    "material_delta",
    "income_delta",
    "own_bank",
    "unit_count_delta",
    "capture_progress_delta",
    "front_position_delta",
    "immediate_threat_safety_delta",
    "deferred_threat_safety_delta",
];

const FEATURE_COUNT: usize = FEATURE_NAMES.len();
const MINIMUM_MATCHES: usize = 100;
const REQUESTED_FOLDS: usize = 5;
const VALIDATION_REPEATS: usize = 3;
const L2_PENALTY: f64 = 0.1;
const HIGH_COLLINEARITY: f64 = 0.8;
const MAX_ITERATIONS: usize = 5_000;
const GRADIENT_TOLERANCE: f64 = 1e-7;
const STEP_SIZE: f64 = 0.2;

/// The state source used to calculate a feature row.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FeatureMode {
    /// Full state. This mode is valid for post-hoc analysis only.
    Authoritative,
    /// State reified from the acting player's fog-limited observation.
    FogVisible,
}

impl FeatureMode {
    /// Return the modes in stable output order.
    pub const ALL: [Self; 2] = [Self::Authoritative, Self::FogVisible];
}

/// A relative feature vector for the player that just ended the turn.
///
/// Positive deltas mean that player leads, except safety deltas, where a
/// positive value means that player has less threat exposure.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FeatureVector {
    pub turn_index: f64,
    pub material_delta: f64,
    pub income_delta: f64,
    pub own_bank: f64,
    pub unit_count_delta: f64,
    pub capture_progress_delta: f64,
    pub front_position_delta: f64,
    pub immediate_threat_safety_delta: f64,
    pub deferred_threat_safety_delta: f64,
}

impl FeatureVector {
    fn values(self) -> [f64; FEATURE_COUNT] {
        [
            self.turn_index,
            self.material_delta,
            self.income_delta,
            self.own_bank,
            self.unit_count_delta,
            self.capture_progress_delta,
            self.front_position_delta,
            self.immediate_threat_safety_delta,
            self.deferred_threat_safety_delta,
        ]
    }

    fn value(self, name: &str) -> Option<f64> {
        FEATURE_NAMES
            .iter()
            .position(|candidate| *candidate == name)
            .map(|index| self.values()[index])
    }
}

/// One post-turn observation from one completed match.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FeatureRow {
    pub schema_version: u16,
    pub mode: FeatureMode,
    pub match_id: String,
    pub pair: PairKey,
    pub group_id: String,
    pub match_seed: u64,
    pub seat_order: SeatOrderVariant,
    pub day: u64,
    pub turn_index: u32,
    pub terminal_turn_index: u32,
    pub perspective_seat: u8,
    pub just_acted_seat: u8,
    pub active_seat: u8,
    pub winner: bool,
    pub features: FeatureVector,
}

/// Feature rows and extraction coverage.
#[derive(Clone, Debug, PartialEq)]
pub struct FeatureExtraction {
    pub rows: Vec<FeatureRow>,
    pub event_rows: usize,
    pub matches: usize,
    pub matches_with_rows: usize,
    pub skipped_draws: usize,
    pub skipped_incomplete: usize,
    /// Fingerprint of the source event log, or a caller-supplied corpus id.
    pub corpus_fingerprint: String,
}

/// Summary returned after writing the derived feature files.
#[derive(Clone, Debug, PartialEq)]
pub struct FeatureAnalysisSummary {
    pub output: PathBuf,
    pub extraction: FeatureExtraction,
    pub report: FeatureAnalysisReport,
}

/// The complete report for all state views.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FeatureAnalysisReport {
    pub schema_version: u16,
    pub event_rows: usize,
    pub matches: usize,
    pub matches_with_rows: usize,
    pub skipped_draws: usize,
    pub skipped_incomplete: usize,
    pub rows: usize,
    pub minimum_matches: usize,
    pub sufficient_corpus: bool,
    #[serde(default)]
    pub corpus_fingerprint: String,
    pub modes: Vec<ModeAnalysisReport>,
}

/// Analysis for one authoritative or fog-visible state view.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModeAnalysisReport {
    pub mode: FeatureMode,
    pub rows: usize,
    pub matches: usize,
    pub validation_groups: usize,
    pub model: ModelReport,
    pub ablations: Vec<AblationReport>,
    pub turn_ranges: Vec<TurnRangeReport>,
    pub map_turn_ranges: Vec<MapTurnRangeReport>,
    pub collinearity: Vec<CollinearityReport>,
}

/// Model settings, learned weights, and validation metrics.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelReport {
    pub feature_names: Vec<String>,
    pub intercept: f64,
    pub weights: Vec<FeatureWeight>,
    pub reduced_intercept: f64,
    pub reduced_weights: Vec<FeatureWeight>,
    pub l2_penalty: f64,
    pub iterations: usize,
    pub converged: bool,
    #[serde(default = "default_converged")]
    pub reduced_converged: bool,
    pub fit_metrics: DatasetMetrics,
    /// Held-out metrics for the exact reduced feature set consumed by the
    /// offline evaluator.
    pub cross_validation: CrossValidationReport,
    /// Full-model metrics retained for comparison with the reduced model.
    #[serde(default)]
    pub full_cross_validation: Option<CrossValidationReport>,
    /// The label-independent rule used to select the reduced feature set.
    #[serde(default)]
    pub selection_rule: String,
}

/// One learned coefficient in original feature units.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FeatureWeight {
    pub name: String,
    pub coefficient: f64,
    pub odds_ratio: f64,
    pub selected: bool,
}

/// Binary classification metrics for one data slice.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DatasetMetrics {
    pub rows: usize,
    pub groups: usize,
    pub log_loss: f64,
    pub brier_score: f64,
    pub accuracy: f64,
    pub baseline_probability: f64,
    pub baseline_log_loss: f64,
    pub baseline_brier_score: f64,
    pub baseline_accuracy: f64,
}

/// A mean and 95% confidence interval over validation evaluations.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MetricSummary {
    pub samples: usize,
    pub mean: f64,
    pub stddev: f64,
    pub ci95_low: f64,
    pub ci95_high: f64,
}

/// Repeated grouped cross-validation results.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CrossValidationReport {
    pub repeats: usize,
    pub requested_folds: usize,
    pub folds: usize,
    pub evaluations: usize,
    pub rows: usize,
    pub groups: usize,
    pub log_loss: MetricSummary,
    pub brier_score: MetricSummary,
    pub accuracy: MetricSummary,
    pub baseline_log_loss: MetricSummary,
    pub baseline_brier_score: MetricSummary,
    pub baseline_accuracy: MetricSummary,
    /// Held-out metrics aggregated once per independent paired group.
    #[serde(default)]
    pub pair_metrics: Vec<PairMetric>,
    /// Held-out log loss by map. Each map has its own pair-level interval.
    #[serde(default)]
    pub map_log_loss: BTreeMap<u32, MetricSummary>,
    /// Equal-map aggregate. This prevents large maps from dominating the CI.
    #[serde(default)]
    pub equal_map_log_loss: MetricSummary,
}

/// Held-out predictions reduced to one independent paired group.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PairMetric {
    pub group_id: String,
    pub map_id: u32,
    pub rows: usize,
    pub repeats: usize,
    pub mean_probability: f64,
    pub mean_winner: f64,
    pub log_loss: f64,
    pub brier_score: f64,
    pub accuracy: f64,
    pub baseline_log_loss: f64,
    pub baseline_brier_score: f64,
    pub baseline_accuracy: f64,
}

/// The held-out effect of removing one feature.
///
/// `selected` records membership in the label-independent reduced set. It is
/// not a claim that the ablation passed a label-based selection threshold.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AblationReport {
    pub name: String,
    pub delta_log_loss: MetricSummary,
    pub delta_brier_score: MetricSummary,
    pub selected: bool,
}

/// A coarse early, middle, or late turn range.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TurnRange {
    Early,
    Middle,
    Late,
}

/// Validation metrics for one turn range.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TurnRangeReport {
    pub range: TurnRange,
    pub rows: usize,
    pub groups: usize,
    pub log_loss: MetricSummary,
    pub brier_score: MetricSummary,
    pub accuracy: MetricSummary,
}

/// Validation metrics for one map and turn range.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapTurnRangeReport {
    pub map_id: u32,
    pub range: TurnRange,
    pub rows: usize,
    pub groups: usize,
    pub log_loss: MetricSummary,
    pub brier_score: MetricSummary,
    pub accuracy: MetricSummary,
}

/// Pairwise Pearson correlation between two model features.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CollinearityReport {
    pub left: String,
    pub right: String,
    pub correlation: f64,
    pub absolute_correlation: f64,
    pub high: bool,
}

/// An offline evaluator made from selected observable model weights.
///
/// This type is intentionally owned by diagnostics. It is not the production
/// position evaluator and it does not write coefficients into `eval.rs`.
#[derive(Clone, Debug, PartialEq)]
pub struct ReducedEvaluator {
    mode: FeatureMode,
    intercept: f64,
    weights: Vec<(String, f64)>,
}

impl ReducedEvaluator {
    /// Build an evaluator from one fitted mode report.
    pub fn from_report(report: &ModeAnalysisReport) -> Self {
        let weights = report
            .model
            .reduced_weights
            .iter()
            .map(|weight| (weight.name.clone(), weight.coefficient))
            .collect();
        Self {
            mode: report.mode,
            intercept: report.model.reduced_intercept,
            weights,
        }
    }

    /// Return the state source used by this evaluator.
    pub const fn mode(&self) -> FeatureMode {
        self.mode
    }

    /// Return the selected feature names.
    pub fn feature_names(&self) -> impl Iterator<Item = &str> {
        self.weights.iter().map(|(name, _)| name.as_str())
    }

    /// Return the predicted probability that the perspective player wins.
    pub fn probability(&self, features: FeatureVector) -> f64 {
        let score = self
            .weights
            .iter()
            .filter_map(|(name, coefficient)| features.value(name).map(|value| coefficient * value))
            .fold(self.intercept, |score, term| score + term);
        sigmoid(score)
    }
}

/// Errors from feature extraction or model fitting.
#[derive(Debug, thiserror::Error)]
pub enum FeatureAnalysisError {
    #[error(transparent)]
    EventLog(#[from] EventLogError),
    #[error("feature analysis I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("feature analysis JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("feature analysis data error: {0}")]
    Data(String),
}

/// Read an event log, derive feature rows, and write the analysis output.
pub fn analyze_event_log(
    events: impl AsRef<Path>,
    output: impl AsRef<Path>,
) -> Result<FeatureAnalysisSummary, FeatureAnalysisError> {
    let events = events.as_ref();
    let corpus_fingerprint = awbrn_ai_diagnostic_types::fingerprint_bytes(&fs::read(events)?);
    let rows = read_event_log(events)?;
    let mut extraction = extract_feature_rows(&rows)?;
    extraction.corpus_fingerprint = corpus_fingerprint;
    let report = fit_feature_analysis(&extraction)?;
    let output = output.as_ref().to_owned();
    fs::create_dir_all(&output)?;
    write_feature_rows(&output.join("features.jsonl"), &extraction.rows)?;
    fs::write(
        output.join("feature-analysis.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(FeatureAnalysisSummary {
        output,
        extraction,
        report,
    })
}

/// Derive two labelled rows from each completed turn in the newest attempts.
pub fn extract_feature_rows(rows: &[EventRow]) -> Result<FeatureExtraction, FeatureAnalysisError> {
    let latest = latest_attempt_rows(rows);
    let mut grouped = BTreeMap::<String, Vec<EventRow>>::new();
    for row in latest {
        grouped.entry(row.match_id.clone()).or_default().push(row);
    }

    let mut extraction = FeatureExtraction {
        rows: Vec::new(),
        event_rows: rows.len(),
        matches: grouped.len(),
        matches_with_rows: 0,
        skipped_draws: 0,
        skipped_incomplete: 0,
        corpus_fingerprint: "event-rows".into(),
    };

    for (match_id, match_rows) in grouped {
        let Some(terminal) = match_rows
            .iter()
            .rev()
            .find(|row| row.event_kind == EventKind::Terminal)
        else {
            extraction.skipped_incomplete += 1;
            continue;
        };
        let Some(winners) = victory_teams(&terminal.state) else {
            extraction.skipped_draws += 1;
            continue;
        };
        let terminal_turn_index = terminal.turn_index.saturating_add(1);
        let mut match_row_count = 0;

        for row in match_rows
            .iter()
            .filter(|row| row.event_kind == EventKind::TurnEnd)
        {
            let Some(Command::EndTurn { player }) = row.command.as_ref() else {
                continue;
            };
            let Some(perspective) = row.state.players.seat(player) else {
                continue;
            };
            let Some(authoritative_rival) = rival_seat(&row.state, perspective) else {
                continue;
            };
            let visible_state = visible_state(&row.state, perspective)?;
            let Some(visible_perspective) = visible_state.players.seat(player) else {
                continue;
            };
            let Some(visible_rival) = rival_seat(&visible_state, visible_perspective) else {
                continue;
            };
            let team = &row.state.player(perspective).team;
            let turn_index = row.turn_index.saturating_add(1);
            let states = [
                (
                    FeatureMode::Authoritative,
                    &row.state,
                    perspective,
                    authoritative_rival,
                ),
                (
                    FeatureMode::FogVisible,
                    &visible_state,
                    visible_perspective,
                    visible_rival,
                ),
            ];
            for (mode, state, perspective, rival) in states {
                let own = raw_features(state, perspective);
                let other = raw_features(state, rival);
                extraction.rows.push(FeatureRow {
                    schema_version: FEATURE_ANALYSIS_SCHEMA_VERSION,
                    mode,
                    match_id: match_id.clone(),
                    pair: row.pair.clone(),
                    group_id: group_id(&row.pair),
                    match_seed: row.match_seed,
                    seat_order: row.seat_order,
                    day: row.day,
                    turn_index,
                    terminal_turn_index,
                    perspective_seat: u8::try_from(perspective.get()).unwrap_or(u8::MAX),
                    just_acted_seat: u8::try_from(perspective.get()).unwrap_or(u8::MAX),
                    active_seat: state
                        .players
                        .seat(&state.turn.active_player)
                        .and_then(|seat| u8::try_from(seat.get()).ok())
                        .unwrap_or(u8::MAX),
                    winner: winners.contains(team),
                    features: own.delta(other, turn_index),
                });
                match_row_count += 1;
            }
        }
        if match_row_count > 0 {
            extraction.matches_with_rows += 1;
        }
    }
    extraction.rows.sort_by(|left, right| {
        left.mode
            .cmp(&right.mode)
            .then(left.group_id.cmp(&right.group_id))
            .then(left.match_id.cmp(&right.match_id))
            .then(left.turn_index.cmp(&right.turn_index))
    });
    Ok(extraction)
}

/// Return features from a state that is already restricted to one player's
/// knowledge. This is useful for offline policy experiments.
pub fn observable_features(
    state: &State,
    perspective: PlayerIdx,
    turn_index: u32,
) -> Option<FeatureVector> {
    let rival = rival_seat(state, perspective)?;
    Some(raw_features(state, perspective).delta(raw_features(state, rival), turn_index))
}

fn visible_state(state: &State, perspective: PlayerIdx) -> Result<State, FeatureAnalysisError> {
    let view = observe(&AwbwVisibility, state, state.player_id(perspective))
        .map_err(|error| FeatureAnalysisError::Data(format!("fog projection failed: {error}")))?;
    Session::from_observation(&view)
        .map(|session| session.state().clone())
        .map_err(|error| FeatureAnalysisError::Data(format!("fog reification failed: {error}")))
}

/// Fit all state views with repeated grouped validation and ablations.
pub fn fit_feature_analysis(
    extraction: &FeatureExtraction,
) -> Result<FeatureAnalysisReport, FeatureAnalysisError> {
    if extraction.rows.is_empty() {
        return Err(FeatureAnalysisError::Data(
            "no completed-turn feature rows are available".into(),
        ));
    }
    let mut modes = Vec::new();
    for mode in FeatureMode::ALL {
        let rows = extraction
            .rows
            .iter()
            .filter(|row| row.mode == mode)
            .cloned()
            .collect::<Vec<_>>();
        if !rows.is_empty() {
            modes.push(fit_mode_analysis(mode, &rows)?);
        }
    }
    if modes.is_empty() {
        return Err(FeatureAnalysisError::Data(
            "no feature mode has usable rows".into(),
        ));
    }
    Ok(FeatureAnalysisReport {
        schema_version: FEATURE_ANALYSIS_SCHEMA_VERSION,
        event_rows: extraction.event_rows,
        matches: extraction.matches,
        matches_with_rows: extraction.matches_with_rows,
        skipped_draws: extraction.skipped_draws,
        skipped_incomplete: extraction.skipped_incomplete,
        rows: extraction.rows.len(),
        minimum_matches: MINIMUM_MATCHES,
        sufficient_corpus: extraction.matches_with_rows >= MINIMUM_MATCHES,
        corpus_fingerprint: extraction.corpus_fingerprint.clone(),
        modes,
    })
}

fn fit_mode_analysis(
    mode: FeatureMode,
    rows: &[FeatureRow],
) -> Result<ModeAnalysisReport, FeatureAnalysisError> {
    let groups = rows
        .iter()
        .map(|row| row.group_id.as_str())
        .collect::<BTreeSet<_>>();
    if groups.len() < 2 {
        return Err(FeatureAnalysisError::Data(format!(
            "feature mode {mode:?} has fewer than two validation groups"
        )));
    }
    let group_counts = group_counts(rows);
    let all_indices = (0..rows.len()).collect::<Vec<_>>();
    let all_features = (0..FEATURE_COUNT).collect::<Vec<_>>();
    let final_model = fit_model(rows, &all_indices, &all_features, &group_counts);
    let fit_metrics = metrics(rows, &all_indices, &final_model, &group_counts);
    let (full_cross_validation, mut ablations, _) =
        repeated_validation(rows, &group_counts, &all_features)?;
    let collinearity = collinearity(rows, &group_counts);

    let reduced_features = fixed_selected_features(&collinearity);
    let selected = reduced_features
        .iter()
        .map(|feature| FEATURE_NAMES[*feature])
        .collect::<BTreeSet<_>>();
    for ablation in &mut ablations {
        ablation.selected = selected.contains(ablation.name.as_str());
    }
    let weights = all_features
        .iter()
        .enumerate()
        .map(|(weight_index, feature)| FeatureWeight {
            name: FEATURE_NAMES[*feature].into(),
            coefficient: final_model.raw_coefficients[weight_index],
            odds_ratio: final_model.raw_coefficients[weight_index]
                .clamp(-50.0, 50.0)
                .exp(),
            selected: selected.contains(FEATURE_NAMES[*feature]),
        })
        .collect();
    let reduced_model = fit_model(rows, &all_indices, &reduced_features, &group_counts);
    let (cross_validation, segment_samples) =
        cross_validate(rows, &group_counts, &reduced_features)?;
    let reduced_weights = reduced_features
        .iter()
        .enumerate()
        .map(|(weight_index, feature)| FeatureWeight {
            name: FEATURE_NAMES[*feature].into(),
            coefficient: reduced_model.raw_coefficients[weight_index],
            odds_ratio: reduced_model.raw_coefficients[weight_index]
                .clamp(-50.0, 50.0)
                .exp(),
            selected: true,
        })
        .collect();
    ablations.sort_by(|left, right| {
        right
            .delta_log_loss
            .mean
            .total_cmp(&left.delta_log_loss.mean)
            .then(left.name.cmp(&right.name))
    });

    let (turn_ranges, map_turn_ranges) = segment_reports(segment_samples);
    Ok(ModeAnalysisReport {
        mode,
        rows: rows.len(),
        matches: rows
            .iter()
            .map(|row| row.match_id.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        validation_groups: groups.len(),
        model: ModelReport {
            feature_names: FEATURE_NAMES.iter().map(|name| (*name).into()).collect(),
            intercept: final_model.raw_intercept,
            weights,
            reduced_intercept: reduced_model.raw_intercept,
            reduced_weights,
            l2_penalty: L2_PENALTY,
            iterations: final_model.iterations,
            converged: final_model.converged,
            reduced_converged: reduced_model.converged,
            fit_metrics,
            cross_validation,
            full_cross_validation: Some(full_cross_validation),
            selection_rule: format!(
                "retain the first feature in each pair with absolute correlation >= {HIGH_COLLINEARITY:.1}; do not use outcome labels"
            ),
        },
        ablations,
        turn_ranges,
        map_turn_ranges,
        collinearity,
    })
}

fn write_feature_rows(path: &Path, rows: &[FeatureRow]) -> Result<(), FeatureAnalysisError> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    for row in rows {
        serde_json::to_writer(&mut writer, row)?;
        std::io::Write::write_all(&mut writer, b"\n")?;
    }
    Ok(())
}

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

impl SeatFeatures {
    fn delta(self, other: Self, turn_index: u32) -> FeatureVector {
        FeatureVector {
            turn_index: f64::from(turn_index),
            material_delta: self.material - other.material,
            income_delta: self.income - other.income,
            own_bank: self.bank,
            unit_count_delta: self.unit_count - other.unit_count,
            capture_progress_delta: self.capture_progress - other.capture_progress,
            front_position_delta: self.front_position - other.front_position,
            immediate_threat_safety_delta: other.immediate_threat - self.immediate_threat,
            deferred_threat_safety_delta: other.deferred_threat - self.deferred_threat,
        }
    }
}

fn raw_features(state: &State, seat: PlayerIdx) -> SeatFeatures {
    let dimensions = state.board.dimensions();
    let mut contest = ContestMap::new();
    contest.build(state, seat);
    let mut threat = ThreatMap::new();
    let session = Session::new(state.clone());
    threat.build(&session, seat);

    let mut features = SeatFeatures {
        bank: state.player(seat).funds as f64,
        ..SeatFeatures::default()
    };
    let mut front_total = 0.0;
    let mut front_units = 0_u64;
    for unit in state.units.iter().filter(|unit| unit.owner == seat) {
        features.material +=
            ruleset::profile(unit.kind).cost as f64 * f64::from(unit.hp) / f64::from(100_u8);
        features.unit_count += 1.0;
        let Location::Board { position } = unit.location else {
            continue;
        };
        let Some(cell) = dimensions.cell_index(position) else {
            continue;
        };
        front_total += f64::from(contest.front(usize::from(cell.get())));
        front_units += 1;
        features.immediate_threat += threat.immediate(cell, unit.kind);
        features.deferred_threat += threat.deferred(cell, unit.kind);
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
    let income_rate = commander::effective_income_per_property(state, seat) as f64;
    features.income = income_properties * income_rate;

    for (position, tile) in state.board.iter() {
        let Some(points) = tile.capture_points else {
            continue;
        };
        if points >= awvm::semantic::CAPTURE_REQUIRED_POINTS
            || !ruleset::terrain_has(tile.terrain, TerrainTrait::Capturable)
        {
            continue;
        }
        let occupied_by_us = state.units.iter().any(|unit| {
            unit.owner == seat
                && matches!(unit.location, Location::Board { position: unit_position } if unit_position == position)
        });
        if occupied_by_us {
            features.capture_progress +=
                f64::from(awvm::semantic::CAPTURE_REQUIRED_POINTS - points)
                    / f64::from(awvm::semantic::CAPTURE_REQUIRED_POINTS);
        }
    }
    features
}

fn rival_seat(state: &State, seat: PlayerIdx) -> Option<PlayerIdx> {
    state
        .players
        .seats()
        .map(|(candidate, _)| candidate)
        .filter(|candidate| *candidate != seat)
        .filter(|candidate| awbrn_ai::threat::hostile(state, seat, *candidate))
        .max_by_key(|candidate| {
            state
                .units
                .iter()
                .filter(|unit| unit.owner == *candidate)
                .map(|unit| ruleset::profile(unit.kind).cost)
                .sum::<u64>()
        })
}

fn victory_teams(state: &State) -> Option<&[awvm::semantic::TeamId]> {
    let Match::Finished { outcome } = &state.match_state else {
        return None;
    };
    match outcome {
        Outcome::Victory { winners, .. } => Some(winners),
        Outcome::Draw { .. } | Outcome::Cancelled { .. } => None,
    }
}

fn group_id(pair: &PairKey) -> String {
    format!(
        "map-{}-seed-{}-pair-{}",
        pair.map_id, pair.run_seed, pair.pair_index
    )
}

#[derive(Clone, Debug)]
struct FittedModel {
    features: Vec<usize>,
    raw_intercept: f64,
    raw_coefficients: Vec<f64>,
    baseline_probability: f64,
    iterations: usize,
    converged: bool,
}

fn group_counts(rows: &[FeatureRow]) -> BTreeMap<&str, usize> {
    let mut counts = BTreeMap::new();
    for row in rows {
        *counts.entry(row.group_id.as_str()).or_insert(0) += 1;
    }
    counts
}

fn repeated_validation(
    rows: &[FeatureRow],
    group_counts: &BTreeMap<&str, usize>,
    all_features: &[usize],
) -> Result<(CrossValidationReport, Vec<AblationReport>, SegmentSamples), FeatureAnalysisError> {
    let groups = rows
        .iter()
        .map(|row| row.group_id.as_str())
        .collect::<BTreeSet<_>>();
    if groups.len() < 2 {
        return Err(FeatureAnalysisError::Data(
            "at least two validation groups are required".into(),
        ));
    }
    let folds = REQUESTED_FOLDS.min(groups.len()).max(2);
    let mut full_log_loss = Vec::new();
    let mut full_brier = Vec::new();
    let mut full_accuracy = Vec::new();
    let mut baseline_log_loss = Vec::new();
    let mut baseline_brier = Vec::new();
    let mut baseline_accuracy = Vec::new();
    let mut rows_evaluated = 0;
    let mut ablation_log_loss = vec![Vec::new(); FEATURE_COUNT];
    let mut ablation_brier = vec![Vec::new(); FEATURE_COUNT];
    let mut segments = SegmentSamples::default();

    for repeat in 0..VALIDATION_REPEATS {
        for fold in 0..folds {
            let (train, test) = grouped_fold(rows, repeat, fold, folds);
            if train.is_empty() || test.is_empty() {
                continue;
            }
            let full = fit_model(rows, &train, all_features, group_counts);
            let full_metrics = metrics(rows, &test, &full, group_counts);
            rows_evaluated += test.len();
            full_log_loss.push(full_metrics.log_loss);
            full_brier.push(full_metrics.brier_score);
            full_accuracy.push(full_metrics.accuracy);
            baseline_log_loss.push(full_metrics.baseline_log_loss);
            baseline_brier.push(full_metrics.baseline_brier_score);
            baseline_accuracy.push(full_metrics.baseline_accuracy);
            record_segments(&mut segments, rows, &test, &full, group_counts);

            for feature in 0..FEATURE_COUNT {
                let without = all_features
                    .iter()
                    .copied()
                    .filter(|candidate| *candidate != feature)
                    .collect::<Vec<_>>();
                let ablated = fit_model(rows, &train, &without, group_counts);
                let ablated_metrics = metrics(rows, &test, &ablated, group_counts);
                ablation_log_loss[feature].push(ablated_metrics.log_loss - full_metrics.log_loss);
                ablation_brier[feature]
                    .push(ablated_metrics.brier_score - full_metrics.brier_score);
            }
        }
    }
    if full_log_loss.is_empty() {
        return Err(FeatureAnalysisError::Data(
            "grouped validation produced no non-empty folds".into(),
        ));
    }
    let mut ablations = Vec::with_capacity(FEATURE_COUNT);
    for feature in 0..FEATURE_COUNT {
        let delta_log_loss = metric_summary(&ablation_log_loss[feature]);
        ablations.push(AblationReport {
            name: FEATURE_NAMES[feature].into(),
            delta_log_loss: delta_log_loss.clone(),
            delta_brier_score: metric_summary(&ablation_brier[feature]),
            selected: false,
        });
    }
    let mut validation = CrossValidationReport {
        repeats: VALIDATION_REPEATS,
        requested_folds: REQUESTED_FOLDS,
        folds,
        evaluations: full_log_loss.len(),
        rows: rows_evaluated,
        groups: groups.len(),
        log_loss: metric_summary(&full_log_loss),
        brier_score: metric_summary(&full_brier),
        accuracy: metric_summary(&full_accuracy),
        baseline_log_loss: metric_summary(&baseline_log_loss),
        baseline_brier_score: metric_summary(&baseline_brier),
        baseline_accuracy: metric_summary(&baseline_accuracy),
        pair_metrics: Vec::new(),
        map_log_loss: BTreeMap::new(),
        equal_map_log_loss: MetricSummary::default(),
    };
    let pair_validation = pair_validation(rows, group_counts, all_features)?;
    validation.pair_metrics = pair_validation.pairs;
    validation.map_log_loss = pair_validation.map_log_loss;
    validation.equal_map_log_loss = pair_validation.equal_map_log_loss;
    set_pair_level_summaries(&mut validation);
    Ok((validation, ablations, segments))
}

/// Validate one fixed feature set with grouped repeated cross-validation.
///
/// Feature selection is performed by [`fixed_selected_features`], which only
/// reads feature collinearity. This separate pass reports held-out metrics for
/// the exact reduced model used by [`ReducedEvaluator`].
fn cross_validate(
    rows: &[FeatureRow],
    group_counts: &BTreeMap<&str, usize>,
    features: &[usize],
) -> Result<(CrossValidationReport, SegmentSamples), FeatureAnalysisError> {
    let groups = rows
        .iter()
        .map(|row| row.group_id.as_str())
        .collect::<BTreeSet<_>>();
    if groups.len() < 2 {
        return Err(FeatureAnalysisError::Data(
            "at least two validation groups are required".into(),
        ));
    }
    let folds = REQUESTED_FOLDS.min(groups.len()).max(2);
    let mut log_loss = Vec::new();
    let mut brier = Vec::new();
    let mut accuracy = Vec::new();
    let mut baseline_log_loss = Vec::new();
    let mut baseline_brier = Vec::new();
    let mut baseline_accuracy = Vec::new();
    let mut rows_evaluated = 0;
    let mut segments = SegmentSamples::default();

    for repeat in 0..VALIDATION_REPEATS {
        for fold in 0..folds {
            let (train, test) = grouped_fold(rows, repeat, fold, folds);
            if train.is_empty() || test.is_empty() {
                continue;
            }
            let model = fit_model(rows, &train, features, group_counts);
            let held_out = metrics(rows, &test, &model, group_counts);
            rows_evaluated += test.len();
            log_loss.push(held_out.log_loss);
            brier.push(held_out.brier_score);
            accuracy.push(held_out.accuracy);
            baseline_log_loss.push(held_out.baseline_log_loss);
            baseline_brier.push(held_out.baseline_brier_score);
            baseline_accuracy.push(held_out.baseline_accuracy);
            record_segments(&mut segments, rows, &test, &model, group_counts);
        }
    }
    if log_loss.is_empty() {
        return Err(FeatureAnalysisError::Data(
            "grouped validation produced no non-empty folds".into(),
        ));
    }
    let mut validation = CrossValidationReport {
        repeats: VALIDATION_REPEATS,
        requested_folds: REQUESTED_FOLDS,
        folds,
        evaluations: log_loss.len(),
        rows: rows_evaluated,
        groups: groups.len(),
        log_loss: metric_summary(&log_loss),
        brier_score: metric_summary(&brier),
        accuracy: metric_summary(&accuracy),
        baseline_log_loss: metric_summary(&baseline_log_loss),
        baseline_brier_score: metric_summary(&baseline_brier),
        baseline_accuracy: metric_summary(&baseline_accuracy),
        pair_metrics: Vec::new(),
        map_log_loss: BTreeMap::new(),
        equal_map_log_loss: MetricSummary::default(),
    };
    let pair_validation = pair_validation(rows, group_counts, features)?;
    validation.pair_metrics = pair_validation.pairs;
    validation.map_log_loss = pair_validation.map_log_loss;
    validation.equal_map_log_loss = pair_validation.equal_map_log_loss;
    set_pair_level_summaries(&mut validation);
    Ok((validation, segments))
}

/// Select a stable, label-independent feature set.
fn fixed_selected_features(collinearity: &[CollinearityReport]) -> Vec<usize> {
    let mut selected = (0..FEATURE_COUNT).collect::<BTreeSet<_>>();
    for pair in collinearity.iter().filter(|pair| pair.high) {
        let Some(left) = FEATURE_NAMES.iter().position(|name| *name == pair.left) else {
            continue;
        };
        let Some(right) = FEATURE_NAMES.iter().position(|name| *name == pair.right) else {
            continue;
        };
        if selected.contains(&left) && selected.contains(&right) {
            selected.remove(&right);
        }
    }
    selected.into_iter().collect()
}

fn grouped_fold(
    rows: &[FeatureRow],
    repeat: usize,
    fold: usize,
    folds: usize,
) -> (Vec<usize>, Vec<usize>) {
    let mut groups_by_map = BTreeMap::<u32, BTreeSet<&str>>::new();
    for row in rows {
        groups_by_map
            .entry(row.pair.map_id)
            .or_default()
            .insert(row.group_id.as_str());
    }
    let mut test_groups = BTreeSet::new();
    for groups in groups_by_map.values() {
        let mut groups = groups.iter().copied().collect::<Vec<_>>();
        groups.sort_by(|left, right| {
            splitmix64(hash_group(left) ^ repeat_seed(repeat))
                .cmp(&splitmix64(hash_group(right) ^ repeat_seed(repeat)))
                .then(left.cmp(right))
        });
        test_groups.extend(
            groups
                .into_iter()
                .enumerate()
                .filter(|(index, _)| index % folds == fold)
                .map(|(_, group)| group),
        );
    }
    let mut train = Vec::new();
    let mut test = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if test_groups.contains(row.group_id.as_str()) {
            test.push(index);
        } else {
            train.push(index);
        }
    }
    (train, test)
}

struct PairValidation {
    pairs: Vec<PairMetric>,
    map_log_loss: BTreeMap<u32, MetricSummary>,
    equal_map_log_loss: MetricSummary,
}

fn set_pair_level_summaries(validation: &mut CrossValidationReport) {
    let pairs = &validation.pair_metrics;
    validation.log_loss = bootstrap_metric_summary(
        &pairs.iter().map(|pair| pair.log_loss).collect::<Vec<_>>(),
        0x006c_6f67_6c6f_7373,
    );
    validation.brier_score = bootstrap_metric_summary(
        &pairs
            .iter()
            .map(|pair| pair.brier_score)
            .collect::<Vec<_>>(),
        0x0062_7269_6572,
    );
    validation.accuracy = bootstrap_metric_summary(
        &pairs.iter().map(|pair| pair.accuracy).collect::<Vec<_>>(),
        0x6163_6375_7261_6379,
    );
    validation.baseline_log_loss = bootstrap_metric_summary(
        &pairs
            .iter()
            .map(|pair| pair.baseline_log_loss)
            .collect::<Vec<_>>(),
        0x6261_7365_6c6f_7373,
    );
    validation.baseline_brier_score = bootstrap_metric_summary(
        &pairs
            .iter()
            .map(|pair| pair.baseline_brier_score)
            .collect::<Vec<_>>(),
        0x6261_7365_6272_6965,
    );
    validation.baseline_accuracy = bootstrap_metric_summary(
        &pairs
            .iter()
            .map(|pair| pair.baseline_accuracy)
            .collect::<Vec<_>>(),
        0x0062_6173_6561_6363,
    );
}

#[derive(Default)]
struct PairAccumulator {
    map_id: u32,
    repeats: usize,
    rows: usize,
    probability: f64,
    winner: f64,
    log_loss: f64,
    brier_score: f64,
    accuracy: f64,
    baseline_log_loss: f64,
    baseline_brier_score: f64,
    baseline_accuracy: f64,
}

fn pair_validation(
    rows: &[FeatureRow],
    group_counts: &BTreeMap<&str, usize>,
    features: &[usize],
) -> Result<PairValidation, FeatureAnalysisError> {
    let groups = rows
        .iter()
        .map(|row| row.group_id.as_str())
        .collect::<BTreeSet<_>>();
    if groups.len() < 2 {
        return Err(FeatureAnalysisError::Data(
            "at least two validation groups are required".into(),
        ));
    }
    let folds = REQUESTED_FOLDS.min(groups.len()).max(2);
    let mut accumulators = BTreeMap::<String, PairAccumulator>::new();
    for repeat in 0..VALIDATION_REPEATS {
        for fold in 0..folds {
            let (train, test) = grouped_fold(rows, repeat, fold, folds);
            if train.is_empty() || test.is_empty() {
                continue;
            }
            let model = fit_model(rows, &train, features, group_counts);
            let mut held_out = BTreeMap::<&str, Vec<usize>>::new();
            for index in test {
                held_out
                    .entry(rows[index].group_id.as_str())
                    .or_default()
                    .push(index);
            }
            for (group, indices) in held_out {
                let baseline = model.baseline_probability.clamp(1e-12, 1.0 - 1e-12);
                let mut accumulator = PairAccumulator {
                    map_id: rows[indices[0]].pair.map_id,
                    repeats: 1,
                    ..PairAccumulator::default()
                };
                for index in indices {
                    let row = &rows[index];
                    let label = f64::from(row.winner);
                    let prediction = model.predict(row);
                    accumulator.rows += 1;
                    accumulator.probability += prediction;
                    accumulator.winner += label;
                    accumulator.log_loss += binary_loss(prediction, label);
                    accumulator.brier_score += (prediction - label).powi(2);
                    accumulator.accuracy += f64::from((prediction >= 0.5) == row.winner);
                    accumulator.baseline_log_loss += binary_loss(baseline, label);
                    accumulator.baseline_brier_score += (baseline - label).powi(2);
                    accumulator.baseline_accuracy += f64::from((baseline >= 0.5) == row.winner);
                }
                let entry = accumulators.entry(group.to_owned()).or_default();
                entry.map_id = accumulator.map_id;
                entry.repeats += accumulator.repeats;
                entry.rows += accumulator.rows;
                entry.probability += accumulator.probability / accumulator.rows as f64;
                entry.winner += accumulator.winner / accumulator.rows as f64;
                entry.log_loss += accumulator.log_loss / accumulator.rows as f64;
                entry.brier_score += accumulator.brier_score / accumulator.rows as f64;
                entry.accuracy += accumulator.accuracy / accumulator.rows as f64;
                entry.baseline_log_loss += accumulator.baseline_log_loss / accumulator.rows as f64;
                entry.baseline_brier_score +=
                    accumulator.baseline_brier_score / accumulator.rows as f64;
                entry.baseline_accuracy += accumulator.baseline_accuracy / accumulator.rows as f64;
            }
        }
    }
    let pairs = accumulators
        .into_iter()
        .map(|(group_id, accumulator)| {
            let repeats = accumulator.repeats.max(1) as f64;
            PairMetric {
                group_id,
                map_id: accumulator.map_id,
                rows: accumulator.rows,
                repeats: accumulator.repeats,
                mean_probability: accumulator.probability / repeats,
                mean_winner: accumulator.winner / repeats,
                log_loss: accumulator.log_loss / repeats,
                brier_score: accumulator.brier_score / repeats,
                accuracy: accumulator.accuracy / repeats,
                baseline_log_loss: accumulator.baseline_log_loss / repeats,
                baseline_brier_score: accumulator.baseline_brier_score / repeats,
                baseline_accuracy: accumulator.baseline_accuracy / repeats,
            }
        })
        .collect::<Vec<_>>();
    let mut map_values = BTreeMap::<u32, Vec<f64>>::new();
    for pair in &pairs {
        map_values
            .entry(pair.map_id)
            .or_default()
            .push(pair.log_loss);
    }
    let map_log_loss = map_values
        .iter()
        .map(|(map_id, values)| (*map_id, bootstrap_metric_summary(values, *map_id as u64)))
        .collect::<BTreeMap<_, _>>();
    let map_means = map_values
        .values()
        .map(|values| values.iter().sum::<f64>() / values.len() as f64)
        .collect::<Vec<_>>();
    Ok(PairValidation {
        pairs,
        map_log_loss,
        equal_map_log_loss: bootstrap_metric_summary(&map_means, 0x0000_006d_6170),
    })
}

fn repeat_seed(repeat: usize) -> u64 {
    splitmix64(0x9e37_79b9_7f4a_7c15 ^ repeat as u64)
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn metrics(
    rows: &[FeatureRow],
    indices: &[usize],
    model: &FittedModel,
    group_counts: &BTreeMap<&str, usize>,
) -> DatasetMetrics {
    let baseline_probability = model.baseline_probability;
    let mut log_loss = 0.0;
    let mut brier_score = 0.0;
    let mut accuracy = 0.0;
    let mut baseline_log_loss = 0.0;
    let mut baseline_brier_score = 0.0;
    let mut baseline_accuracy = 0.0;
    let mut total_weight = 0.0;
    for index in indices {
        let row = &rows[*index];
        let weight = row_weight(row, group_counts);
        let label = f64::from(row.winner);
        let prediction = model.predict(row);
        let baseline = baseline_probability.clamp(1e-12, 1.0 - 1e-12);
        log_loss += weight * binary_loss(prediction, label);
        brier_score += weight * (prediction - label).powi(2);
        accuracy += weight * f64::from((prediction >= 0.5) == row.winner);
        baseline_log_loss += weight * binary_loss(baseline, label);
        baseline_brier_score += weight * (baseline - label).powi(2);
        baseline_accuracy += weight * f64::from((baseline >= 0.5) == row.winner);
        total_weight += weight;
    }
    let normalizer = total_weight.max(1e-9);
    DatasetMetrics {
        rows: indices.len(),
        groups: indices
            .iter()
            .map(|index| rows[*index].group_id.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        log_loss: log_loss / normalizer,
        brier_score: brier_score / normalizer,
        accuracy: accuracy / normalizer,
        baseline_probability,
        baseline_log_loss: baseline_log_loss / normalizer,
        baseline_brier_score: baseline_brier_score / normalizer,
        baseline_accuracy: baseline_accuracy / normalizer,
    }
}

#[derive(Clone, Debug, Default)]
struct SegmentSamples {
    turn: BTreeMap<TurnRange, SegmentSample>,
    map_turn: BTreeMap<(u32, TurnRange), SegmentSample>,
}

#[derive(Clone, Debug, Default)]
struct SegmentSample {
    rows: usize,
    groups: BTreeSet<String>,
    group_metrics: BTreeMap<String, Vec<SegmentMetric>>,
}

#[derive(Clone, Copy, Debug)]
struct SegmentMetric {
    log_loss: f64,
    brier_score: f64,
    accuracy: f64,
}

fn record_segments(
    samples: &mut SegmentSamples,
    rows: &[FeatureRow],
    indices: &[usize],
    model: &FittedModel,
    group_counts: &BTreeMap<&str, usize>,
) {
    let mut by_turn = BTreeMap::<TurnRange, Vec<usize>>::new();
    let mut by_map_turn = BTreeMap::<(u32, TurnRange), Vec<usize>>::new();
    for index in indices {
        let row = &rows[*index];
        let range = turn_range(row);
        by_turn.entry(range).or_default().push(*index);
        by_map_turn
            .entry((row.pair.map_id, range))
            .or_default()
            .push(*index);
    }
    for (range, segment) in by_turn {
        record_segment_sample(
            samples.turn.entry(range).or_default(),
            rows,
            &segment,
            model,
            group_counts,
        );
    }
    for (key, segment) in by_map_turn {
        record_segment_sample(
            samples.map_turn.entry(key).or_default(),
            rows,
            &segment,
            model,
            group_counts,
        );
    }
}

fn record_segment_sample(
    sample: &mut SegmentSample,
    rows: &[FeatureRow],
    indices: &[usize],
    model: &FittedModel,
    group_counts: &BTreeMap<&str, usize>,
) {
    if indices.is_empty() {
        return;
    }
    sample.rows += indices.len();
    let mut by_group = BTreeMap::<String, Vec<usize>>::new();
    for index in indices {
        by_group
            .entry(rows[*index].group_id.clone())
            .or_default()
            .push(*index);
    }
    for (group, group_indices) in by_group {
        let metrics = metrics(rows, &group_indices, model, group_counts);
        sample.groups.insert(group.clone());
        sample
            .group_metrics
            .entry(group)
            .or_default()
            .push(SegmentMetric {
                log_loss: metrics.log_loss,
                brier_score: metrics.brier_score,
                accuracy: metrics.accuracy,
            });
    }
}

fn segment_summary(
    sample: &SegmentSample,
    select: impl Fn(SegmentMetric) -> f64,
    seed: u64,
) -> MetricSummary {
    let values = sample
        .group_metrics
        .values()
        .map(|metrics| {
            metrics.iter().map(|metric| select(*metric)).sum::<f64>() / metrics.len() as f64
        })
        .collect::<Vec<_>>();
    bootstrap_metric_summary(&values, seed)
}

fn segment_reports(samples: SegmentSamples) -> (Vec<TurnRangeReport>, Vec<MapTurnRangeReport>) {
    let turn_ranges = samples
        .turn
        .into_iter()
        .map(|(range, sample)| TurnRangeReport {
            range,
            rows: sample.rows,
            groups: sample.groups.len(),
            log_loss: segment_summary(&sample, |metric| metric.log_loss, 0x0000_0065_6172_6c79),
            brier_score: segment_summary(
                &sample,
                |metric| metric.brier_score,
                0x0000_0065_6172_6c79,
            ),
            accuracy: segment_summary(&sample, |metric| metric.accuracy, 0x0000_0065_6172_6c79),
        })
        .collect();
    let map_turn_ranges = samples
        .map_turn
        .into_iter()
        .map(|((map_id, range), sample)| MapTurnRangeReport {
            map_id,
            range,
            rows: sample.rows,
            groups: sample.groups.len(),
            log_loss: segment_summary(&sample, |metric| metric.log_loss, u64::from(map_id)),
            brier_score: segment_summary(&sample, |metric| metric.brier_score, u64::from(map_id)),
            accuracy: segment_summary(&sample, |metric| metric.accuracy, u64::from(map_id)),
        })
        .collect();
    (turn_ranges, map_turn_ranges)
}

fn turn_range(row: &FeatureRow) -> TurnRange {
    let terminal = row.terminal_turn_index.max(row.turn_index).max(1);
    if row.turn_index.saturating_mul(3) <= terminal {
        TurnRange::Early
    } else if row.turn_index.saturating_mul(3) <= terminal.saturating_mul(2) {
        TurnRange::Middle
    } else {
        TurnRange::Late
    }
}

fn collinearity(
    rows: &[FeatureRow],
    group_counts: &BTreeMap<&str, usize>,
) -> Vec<CollinearityReport> {
    let mut reports = Vec::new();
    for (left, left_name) in FEATURE_NAMES.iter().enumerate() {
        for (right, right_name) in FEATURE_NAMES.iter().enumerate().skip(left + 1) {
            let correlation = weighted_correlation(rows, left, right, group_counts);
            reports.push(CollinearityReport {
                left: (*left_name).into(),
                right: (*right_name).into(),
                correlation,
                absolute_correlation: correlation.abs(),
                high: correlation.abs() >= HIGH_COLLINEARITY,
            });
        }
    }
    reports.sort_by(|left, right| {
        right
            .absolute_correlation
            .total_cmp(&left.absolute_correlation)
            .then(left.left.cmp(&right.left))
            .then(left.right.cmp(&right.right))
    });
    reports
}

fn weighted_correlation(
    rows: &[FeatureRow],
    left: usize,
    right: usize,
    group_counts: &BTreeMap<&str, usize>,
) -> f64 {
    let total_weight = rows
        .iter()
        .map(|row| row_weight(row, group_counts))
        .sum::<f64>();
    if total_weight <= 0.0 {
        return 0.0;
    }
    let mean_left = rows
        .iter()
        .map(|row| row_weight(row, group_counts) * row.features.values()[left])
        .sum::<f64>()
        / total_weight;
    let mean_right = rows
        .iter()
        .map(|row| row_weight(row, group_counts) * row.features.values()[right])
        .sum::<f64>()
        / total_weight;
    let mut covariance = 0.0;
    let mut variance_left = 0.0;
    let mut variance_right = 0.0;
    for row in rows {
        let weight = row_weight(row, group_counts);
        let left_delta = row.features.values()[left] - mean_left;
        let right_delta = row.features.values()[right] - mean_right;
        covariance += weight * left_delta * right_delta;
        variance_left += weight * left_delta * left_delta;
        variance_right += weight * right_delta * right_delta;
    }
    let denominator = (variance_left * variance_right).sqrt();
    if denominator <= 1e-12 {
        0.0
    } else {
        (covariance / denominator).clamp(-1.0, 1.0)
    }
}

fn fit_model(
    rows: &[FeatureRow],
    train: &[usize],
    features: &[usize],
    group_counts: &BTreeMap<&str, usize>,
) -> FittedModel {
    let mut means = vec![0.0; features.len()];
    let mut scales = vec![1.0; features.len()];
    let total_weight = train
        .iter()
        .map(|index| row_weight(&rows[*index], group_counts))
        .sum::<f64>();
    if total_weight > 0.0 {
        for (feature_index, feature) in features.iter().enumerate() {
            means[feature_index] = train
                .iter()
                .map(|index| {
                    row_weight(&rows[*index], group_counts)
                        * rows[*index].features.values()[*feature]
                })
                .sum::<f64>()
                / total_weight;
            let variance = train
                .iter()
                .map(|index| {
                    let difference =
                        rows[*index].features.values()[*feature] - means[feature_index];
                    row_weight(&rows[*index], group_counts) * difference * difference
                })
                .sum::<f64>()
                / total_weight;
            scales[feature_index] = variance.sqrt().max(1e-9);
        }
    }

    let prevalence = train
        .iter()
        .map(|index| row_weight(&rows[*index], group_counts) * f64::from(rows[*index].winner))
        .sum::<f64>()
        / total_weight.max(1e-9);
    let mut intercept = logit(prevalence);
    let mut coefficients = vec![0.0; features.len()];
    let mut converged = false;
    let mut iterations = 0;
    for iteration in 1..=MAX_ITERATIONS {
        let mut intercept_gradient = 0.0;
        let mut gradients = vec![0.0; features.len()];
        for index in train {
            let row = &rows[*index];
            let weight = row_weight(row, group_counts);
            let values = row.features.values();
            let mut score = intercept;
            for (position, feature) in features.iter().enumerate() {
                score += coefficients[position] * (values[*feature] - means[position])
                    / scales[position];
            }
            let residual = sigmoid(score) - f64::from(row.winner);
            intercept_gradient += weight * residual;
            for (position, feature) in features.iter().enumerate() {
                gradients[position] +=
                    weight * residual * (values[*feature] - means[position]) / scales[position];
            }
        }
        intercept_gradient /= total_weight.max(1e-9);
        for gradient in &mut gradients {
            *gradient /= total_weight.max(1e-9);
        }
        for (gradient, coefficient) in gradients.iter_mut().zip(&coefficients) {
            *gradient += L2_PENALTY * coefficient;
        }
        let maximum_gradient = gradients
            .iter()
            .copied()
            .chain(std::iter::once(intercept_gradient))
            .map(f64::abs)
            .fold(0.0, f64::max);
        intercept -= STEP_SIZE * intercept_gradient;
        for (coefficient, gradient) in coefficients.iter_mut().zip(gradients) {
            *coefficient -= STEP_SIZE * gradient;
        }
        iterations = iteration;
        if maximum_gradient < GRADIENT_TOLERANCE {
            converged = true;
            break;
        }
    }

    let raw_coefficients = coefficients
        .iter()
        .zip(&scales)
        .map(|(coefficient, scale)| coefficient / scale)
        .collect::<Vec<_>>();
    let raw_intercept = intercept
        - raw_coefficients
            .iter()
            .zip(&means)
            .map(|(coefficient, mean)| coefficient * mean)
            .sum::<f64>();
    FittedModel {
        features: features.to_vec(),
        raw_intercept,
        raw_coefficients,
        baseline_probability: prevalence,
        iterations,
        converged,
    }
}

impl FittedModel {
    fn predict(&self, row: &FeatureRow) -> f64 {
        let values = row.features.values();
        let score = self
            .features
            .iter()
            .zip(&self.raw_coefficients)
            .fold(self.raw_intercept, |score, (feature, coefficient)| {
                score + coefficient * values[*feature]
            });
        sigmoid(score)
    }
}

fn row_weight(row: &FeatureRow, group_counts: &BTreeMap<&str, usize>) -> f64 {
    1.0 / group_counts
        .get(row.group_id.as_str())
        .copied()
        .unwrap_or(1) as f64
}

fn metric_summary(values: &[f64]) -> MetricSummary {
    if values.is_empty() {
        return MetricSummary::default();
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance = values
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / values.len() as f64;
    let stddev = variance.sqrt();
    let margin = if values.len() > 1 {
        1.96 * stddev / (values.len() as f64).sqrt()
    } else {
        0.0
    };
    MetricSummary {
        samples: values.len(),
        mean,
        stddev,
        ci95_low: mean - margin,
        ci95_high: mean + margin,
    }
}

fn bootstrap_metric_summary(values: &[f64], seed: u64) -> MetricSummary {
    let mut summary = metric_summary(values);
    if values.len() < 2 {
        return summary;
    }
    let mut bootstrap = Vec::with_capacity(2_000);
    for sample in 0..2_000_u64 {
        let mut total = 0.0;
        for draw in 0..values.len() {
            let index = (splitmix64(seed ^ sample ^ (draw as u64).rotate_left(17))
                % values.len() as u64) as usize;
            total += values[index];
        }
        bootstrap.push(total / values.len() as f64);
    }
    bootstrap.sort_by(f64::total_cmp);
    summary.ci95_low = bootstrap[50];
    summary.ci95_high = bootstrap[1_949];
    summary
}

fn hash_group(group: &str) -> u64 {
    group.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

fn sigmoid(value: f64) -> f64 {
    if value >= 0.0 {
        1.0 / (1.0 + (-value).exp())
    } else {
        let exponential = value.exp();
        exponential / (1.0 + exponential)
    }
}

fn logit(probability: f64) -> f64 {
    let probability = probability.clamp(1e-6, 1.0 - 1e-6);
    (probability / (1.0 - probability)).ln()
}

fn binary_loss(prediction: f64, label: f64) -> f64 {
    let prediction = prediction.clamp(1e-12, 1.0 - 1e-12);
    -(label * prediction.ln() + (1.0 - label) * (1.0 - prediction).ln())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_extraction() -> FeatureExtraction {
        let mut rows = Vec::new();
        for group in 0..30_u64 {
            let winner = group % 2 == 0;
            for turn in 1..=3_u32 {
                let material = if winner { 4.0 } else { -4.0 };
                for mode in FeatureMode::ALL {
                    rows.push(FeatureRow {
                        schema_version: FEATURE_ANALYSIS_SCHEMA_VERSION,
                        mode,
                        match_id: format!("match-{group}"),
                        pair: PairKey::new(1, 42, group),
                        group_id: format!("group-{group}"),
                        match_seed: group,
                        seat_order: SeatOrderVariant::AgentFirst,
                        day: u64::from(turn),
                        turn_index: turn,
                        terminal_turn_index: 3,
                        perspective_seat: 0,
                        just_acted_seat: 0,
                        active_seat: 1,
                        winner,
                        features: FeatureVector {
                            turn_index: f64::from(turn),
                            material_delta: material,
                            ..FeatureVector::default()
                        },
                    });
                }
            }
        }
        FeatureExtraction {
            rows,
            event_rows: 90,
            matches: 30,
            matches_with_rows: 30,
            skipped_draws: 0,
            skipped_incomplete: 0,
            corpus_fingerprint: "synthetic".into(),
        }
    }

    #[test]
    fn grouped_model_learns_material_signal_in_both_views() {
        let extraction = synthetic_extraction();
        let report = fit_feature_analysis(&extraction).expect("synthetic model fits");
        assert_eq!(report.modes.len(), 2);
        for mode in &report.modes {
            let material = mode
                .model
                .weights
                .iter()
                .find(|weight| weight.name == "material_delta")
                .expect("material weight");
            assert!(material.coefficient > 0.0);
            assert!(material.selected);
            assert!(mode.model.cross_validation.log_loss.mean < std::f64::consts::LN_2);
            assert_eq!(mode.model.cross_validation.groups, 30);
        }
    }

    #[test]
    fn reduced_evaluator_uses_only_selected_weights() {
        let extraction = synthetic_extraction();
        let report = fit_feature_analysis(&extraction).expect("synthetic model fits");
        let mode = report
            .modes
            .iter()
            .find(|mode| mode.mode == FeatureMode::FogVisible)
            .expect("visible mode");
        let evaluator = ReducedEvaluator::from_report(mode);
        assert!(
            evaluator
                .feature_names()
                .any(|name| name == "material_delta")
        );
        assert!(
            evaluator.probability(FeatureVector {
                material_delta: 4.0,
                ..FeatureVector::default()
            }) > 0.5
        );
    }

    #[test]
    fn repeated_group_partitions_are_deterministic_and_independent() {
        let rows = synthetic_extraction()
            .rows
            .into_iter()
            .filter(|row| row.mode == FeatureMode::FogVisible)
            .collect::<Vec<_>>();
        let assignment = |repeat| {
            let mut result = BTreeMap::new();
            for fold in 0..5 {
                let (_, test) = grouped_fold(&rows, repeat, fold, 5);
                for index in test {
                    result.insert(rows[index].group_id.clone(), fold);
                }
            }
            result
        };
        let first = assignment(0);
        assert_eq!(first, assignment(0));
        assert_ne!(first, assignment(1));
        assert_eq!(first.len(), 30);
    }

    #[test]
    fn reduced_validation_is_reported_for_the_consumed_model() {
        let report = fit_feature_analysis(&synthetic_extraction()).expect("synthetic model fits");
        for mode in &report.modes {
            let full = mode
                .model
                .full_cross_validation
                .as_ref()
                .expect("full validation is retained for comparison");
            assert_eq!(mode.model.cross_validation.evaluations, full.evaluations);
            assert_eq!(mode.model.cross_validation.groups, mode.validation_groups);
            assert!(!mode.model.selection_rule.is_empty());
            assert_eq!(
                mode.model
                    .reduced_weights
                    .iter()
                    .map(|weight| weight.name.as_str())
                    .collect::<Vec<_>>(),
                mode.model
                    .weights
                    .iter()
                    .filter(|weight| weight.selected)
                    .map(|weight| weight.name.as_str())
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn validation_uncertainty_is_pair_level_and_map_aware() {
        let report = fit_feature_analysis(&synthetic_extraction()).expect("synthetic model fits");
        let mode = &report.modes[0];
        let validation = &mode.model.cross_validation;
        assert_eq!(validation.pair_metrics.len(), 30);
        assert_eq!(validation.log_loss.samples, 30);
        assert_eq!(validation.map_log_loss.len(), 1);
        assert_eq!(validation.equal_map_log_loss.samples, 1);
        assert!(validation.pair_metrics.iter().all(|pair| pair.repeats > 0));
    }

    #[test]
    fn model_report_reads_pre_reduced_validation_schema() {
        let report = fit_feature_analysis(&synthetic_extraction()).expect("synthetic model fits");
        let mode = &report.modes[0];
        let mut value = serde_json::to_value(&mode.model).expect("model serializes");
        let object = value.as_object_mut().expect("model is an object");
        object.remove("full_cross_validation");
        object.remove("selection_rule");
        let old: ModelReport = serde_json::from_value(value).expect("old model reads");
        assert!(old.full_cross_validation.is_none());
        assert!(old.selection_rule.is_empty());
    }
}
