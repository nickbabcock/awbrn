//! Producer-usability diagnostics and run artifacts.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use awbrn_ai::{
    ProducerUsability, ProducerUsabilityCounts, ProducerUsabilityCountsReport,
    ProducerUsabilityExtractor, ProducerUsabilityRecord, ProducerUsabilityReport,
    classify_producers, classify_producers_in_observation,
};
use awbrn_ai_diagnostic_types::{EVENT_LOG_SCHEMA_VERSION, PairKey, SeatOrderVariant};
use awbrn_ai_diagnostic_types::{RunManifest, fingerprint_bytes};
use awbrn_map::AwbwMapData;
use awbw_replay::ReplayParser;
use awbw_replay::turn_models::Action;
use awvm::ruleset::{self, TerrainTrait, UnitKind};
use awvm::semantic::{
    AwbwVisibility, Concealment, Location, Observation, PlayerId, PlayerIdx, Pos, State, Unit,
    UnitAction, UnitId, observe,
};
use awvm_awbw::RecordedAdapter;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::events::{
    EventKind, EventRow, command_stream_changes, command_stream_fingerprint,
    event_stream_fingerprint, latest_attempt_rows, read_event_log,
};
use crate::feature_analysis::{
    extract_feature_rows, extract_feature_rows_without_producer, feature_extraction_from_rows,
    read_feature_rows, write_feature_rows,
};

/// Maximum allowed median extraction-time increase.
pub const MAX_PRODUCER_MEDIAN_RELATIVE_CHANGE: f64 = 0.05;
/// Maximum allowed p95 extraction-time increase.
pub const MAX_PRODUCER_P95_RELATIVE_CHANGE: f64 = 0.05;
/// The only search decision change allowed by the producer-usability experiment.
pub const REQUIRED_SEARCH_DECISION_CHANGES: u64 = 0;
/// The producer diagnostic artifact schema.
pub const PRODUCER_DIAGNOSTIC_SCHEMA_VERSION: u16 = 2;
/// The stable producer-usability experiment identifier.
pub const PRODUCER_EXPERIMENT_ID: &str = "producer-usability-v1";
/// The fixed producer-feature baseline label.
pub const PRODUCER_BASELINE_IDENTIFIER: &str = "production-property-count-v1";
/// The fixed producer-feature candidate label.
pub const PRODUCER_CANDIDATE_IDENTIFIER: &str = "producer-usability-v1";
const PRODUCER_BENCHMARK_WARMUP_ITERATIONS: u32 = 10;
const PRODUCER_BENCHMARK_SAMPLE_COUNT: u32 = 100;
const LATE_GAME_REPLAY: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../bench/fixtures/ai/late-game-standard-1507289/replay.zip"
));
const LATE_GAME_MAP: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../bench/fixtures/ai/late-game-standard-1507289/map-143619.json"
));

fn default_experiment_id() -> String {
    PRODUCER_EXPERIMENT_ID.into()
}

/// Extraction cost counters for one producer diagnostic.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerUsabilityCost {
    pub movement_or_legality_queries: u64,
    pub threat_map_builds: u64,
    pub scratch_allocations: u64,
    pub full_state_clones: u64,
}

/// Time and cost comparison for one fixture.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerUsabilityPerformance {
    pub schema_version: u16,
    pub fixture: String,
    pub sample_count: u32,
    pub isolated_baseline_median_nanos: u64,
    pub isolated_baseline_p95_nanos: u64,
    pub isolated_candidate_median_nanos: u64,
    pub isolated_candidate_p95_nanos: u64,
    pub isolated_median_relative_change: f64,
    pub isolated_p95_relative_change: f64,
    pub complete_baseline_median_nanos: u64,
    pub complete_baseline_p95_nanos: u64,
    pub complete_candidate_median_nanos: u64,
    pub complete_candidate_p95_nanos: u64,
    pub complete_median_relative_change: f64,
    pub complete_p95_relative_change: f64,
    pub baseline_producer_count: u32,
    pub baseline_occupied_producer_count: u32,
    pub baseline_cost: ProducerUsabilityCost,
    pub candidate_cost: ProducerUsabilityCost,
}

/// The performance artifact written by a producer-usability run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerUsabilityPerformanceArtifact {
    pub schema_version: u16,
    pub fixtures: Vec<ProducerUsabilityPerformance>,
}

/// One expected and observed scenario class.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerUsabilityScenarioResult {
    pub fixture: String,
    pub mode: String,
    pub producer_records: Vec<ProducerUsabilityRecord>,
    pub expected_classes: BTreeMap<String, ProducerUsability>,
    pub actual_classes: BTreeMap<String, ProducerUsability>,
    pub passed: bool,
}

/// The scenario artifact written by a producer-usability run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerUsabilityScenarioArtifact {
    pub schema_version: u16,
    pub scenarios: Vec<ProducerUsabilityScenarioResult>,
}

/// Command and event fingerprints from the enabled and disabled controls.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerUsabilityBehaviorArtifact {
    pub schema_version: u16,
    pub enabled_command_fingerprint: String,
    pub disabled_command_fingerprint: String,
    pub enabled_event_fingerprint: String,
    pub disabled_event_fingerprint: String,
    pub search_decision_changes: u64,
    pub passed: bool,
}

/// Correctness and performance limits declared for the decision.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerUsabilityThresholds {
    pub maximum_median_relative_change: f64,
    pub maximum_p95_relative_change: f64,
    pub required_search_decision_changes: u64,
}

impl Default for ProducerUsabilityThresholds {
    fn default() -> Self {
        Self {
            maximum_median_relative_change: MAX_PRODUCER_MEDIAN_RELATIVE_CHANGE,
            maximum_p95_relative_change: MAX_PRODUCER_P95_RELATIVE_CHANGE,
            required_search_decision_changes: REQUIRED_SEARCH_DECISION_CHANGES,
        }
    }
}

/// Fixture and threshold configuration for producer-usability diagnostics.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerUsabilityPlan {
    #[serde(default = "default_experiment_id")]
    pub experiment_id: String,
    pub scenario_fixtures: Vec<String>,
    pub performance_fixtures: Vec<String>,
    pub sample_count: u32,
    pub thresholds: ProducerUsabilityThresholds,
}

impl Default for ProducerUsabilityPlan {
    fn default() -> Self {
        Self {
            experiment_id: PRODUCER_EXPERIMENT_ID.into(),
            scenario_fixtures: vec![
                "empty-producer".into(),
                "unblock-and-produce".into(),
                "matched-friendly-blocked".into(),
                "hostile-blocked".into(),
                "fog-hidden-occupation".into(),
            ],
            performance_fixtures: vec!["arena".into(), "amber-valley".into(), "late-game".into()],
            sample_count: PRODUCER_BENCHMARK_SAMPLE_COUNT,
            thresholds: ProducerUsabilityThresholds::default(),
        }
    }
}

impl ProducerUsabilityPlan {
    /// Check the fixture and threshold configuration.
    pub fn validate(&self) -> Result<(), String> {
        const REQUIRED_SCENARIOS: [&str; 5] = [
            "empty-producer",
            "unblock-and-produce",
            "matched-friendly-blocked",
            "hostile-blocked",
            "fog-hidden-occupation",
        ];
        const REQUIRED_PERFORMANCE_FIXTURES: [&str; 3] = ["arena", "amber-valley", "late-game"];
        if self.experiment_id != PRODUCER_EXPERIMENT_ID {
            return Err(format!(
                "producer usability plan must use experiment id {PRODUCER_EXPERIMENT_ID}"
            ));
        }
        if self.scenario_fixtures.is_empty() || self.performance_fixtures.is_empty() {
            return Err("producer usability plan needs scenario and performance fixtures".into());
        }
        if self.sample_count < PRODUCER_BENCHMARK_SAMPLE_COUNT {
            return Err(format!(
                "producer usability plan needs at least {PRODUCER_BENCHMARK_SAMPLE_COUNT} samples"
            ));
        }
        if self.scenario_fixtures.len() < REQUIRED_SCENARIOS.len()
            || REQUIRED_SCENARIOS.iter().any(|required| {
                !self
                    .scenario_fixtures
                    .iter()
                    .any(|fixture| fixture == required)
            })
        {
            return Err(
                "producer usability plan must include all required scenario fixtures".into(),
            );
        }
        if REQUIRED_PERFORMANCE_FIXTURES.iter().any(|required| {
            !self
                .performance_fixtures
                .iter()
                .any(|fixture| fixture == required)
        }) {
            return Err(
                "producer usability plan must include all required performance fixtures".into(),
            );
        }
        if self.thresholds.required_search_decision_changes != REQUIRED_SEARCH_DECISION_CHANGES {
            return Err(
                "producer-usability experiment requires zero search decision changes".into(),
            );
        }
        if !self.thresholds.maximum_median_relative_change.is_finite()
            || self.thresholds.maximum_median_relative_change < 0.0
            || !self.thresholds.maximum_p95_relative_change.is_finite()
            || self.thresholds.maximum_p95_relative_change < 0.0
        {
            return Err("producer usability thresholds must be finite and non-negative".into());
        }
        Ok(())
    }
}

/// Feature-row coverage included in the decision record.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerFeatureRowCoverage {
    pub rows: usize,
    pub matches: usize,
    pub matches_with_rows: usize,
    pub authoritative_rows: usize,
    pub fog_visible_rows: usize,
}

/// A compact result for one required scenario.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerUsabilityScenarioSummary {
    pub fixture: String,
    pub expected: ProducerUsability,
    pub actual: ProducerUsability,
    pub passed: bool,
}

/// The authoritative producer-usability decision record.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerUsabilityDecision {
    pub schema_version: u16,
    pub experiment_id: String,
    pub decision_record_fingerprint: String,
    pub source_revision: String,
    pub dirty_worktree: bool,
    pub executable_fingerprint: String,
    pub experiment_plan_fingerprint: String,
    pub input_corpus_fingerprint: String,
    pub baseline_identifier: String,
    pub candidate_identifier: String,
    pub fixture_coverage: Vec<String>,
    pub performance_fixture_coverage: Vec<String>,
    pub map_coverage: Vec<u32>,
    pub pair_coverage: Vec<String>,
    pub seed_coverage: Vec<u64>,
    pub feature_row_coverage: ProducerFeatureRowCoverage,
    pub thresholds: ProducerUsabilityThresholds,
    pub threshold_results: BTreeMap<String, bool>,
    pub unblock_and_produce: ProducerUsabilityScenarioSummary,
    pub matched_blocked_fixture: ProducerUsabilityScenarioSummary,
    pub authoritative_counts: Vec<ProducerUsabilityCounts>,
    pub fog_visible_counts: Vec<ProducerUsabilityCounts>,
    pub fog_unknown_counts: Vec<u32>,
    pub search_decision_changes: u64,
    pub behavior: ProducerUsabilityBehaviorArtifact,
    pub observed_error_concern: String,
    pub recommendations: ProducerUsabilityRecommendations,
    pub decision: String,
    pub reasons: Vec<String>,
    pub artifact_fingerprints: BTreeMap<String, String>,
    pub artifact_locations: BTreeMap<String, String>,
}

/// Separate use recommendations for the diagnostics-only experiment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerUsabilityRecommendations {
    pub diagnostic: String,
    pub production_opportunity: String,
    pub greedy: String,
    pub search: String,
}

/// Outputs written by the producer-usability diagnostic stage.
#[derive(Clone, Debug, PartialEq)]
pub struct ProducerUsabilitySummary {
    pub scenarios: ProducerUsabilityScenarioArtifact,
    pub performance: ProducerUsabilityPerformanceArtifact,
    pub decision: ProducerUsabilityDecision,
    pub output: PathBuf,
}

/// Errors from the producer-usability diagnostic stage.
#[derive(Debug, thiserror::Error)]
pub enum ProducerUsabilityDiagnosticError {
    #[error("producer diagnostic I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("producer diagnostic JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("producer diagnostic event log failed: {0}")]
    Events(#[from] crate::events::EventLogError),
    #[error("producer diagnostic feature extraction failed: {0}")]
    Features(#[from] crate::feature_analysis::FeatureAnalysisError),
    #[error("producer diagnostic observation failed: {0}")]
    Observation(String),
    #[error("producer diagnostic manifest error: {0}")]
    Manifest(#[from] crate::manifest::ManifestError),
    #[error("producer diagnostic configuration error: {0}")]
    Configuration(String),
}

/// Run producer-usability diagnostics and write its artifacts and decision views.
pub fn run_producer_usability_diagnostics(
    manifest: &RunManifest,
    events: impl AsRef<Path>,
    output: impl AsRef<Path>,
) -> Result<ProducerUsabilitySummary, ProducerUsabilityDiagnosticError> {
    if manifest.producer_usability_plan.is_some() {
        return run_producer_usability_diagnostics_from_manifest(manifest, events, output);
    }
    run_producer_usability_diagnostics_with_plan(
        manifest,
        events,
        output,
        &ProducerUsabilityPlan::default(),
    )
}

/// Reanalyse a run with the producer plan stored in its manifest.
pub fn run_producer_usability_diagnostics_from_manifest(
    manifest: &RunManifest,
    events: impl AsRef<Path>,
    output: impl AsRef<Path>,
) -> Result<ProducerUsabilitySummary, ProducerUsabilityDiagnosticError> {
    let value = manifest.producer_usability_plan.as_ref().ok_or_else(|| {
        ProducerUsabilityDiagnosticError::Configuration(
            "run manifest does not contain producer usability settings".into(),
        )
    })?;
    let plan = serde_json::from_value(value.clone())?;
    run_producer_usability_diagnostics_with_plan(manifest, events, output, &plan)
}

/// Run producer-usability diagnostics with the fixture and threshold configuration from a plan.
pub fn run_producer_usability_diagnostics_with_plan(
    manifest: &RunManifest,
    events: impl AsRef<Path>,
    output: impl AsRef<Path>,
    plan: &ProducerUsabilityPlan,
) -> Result<ProducerUsabilitySummary, ProducerUsabilityDiagnosticError> {
    plan.validate()
        .map_err(ProducerUsabilityDiagnosticError::Configuration)?;
    if let Some(value) = &manifest.producer_usability_plan {
        let materialized: ProducerUsabilityPlan = serde_json::from_value(value.clone())?;
        if &materialized != plan {
            return Err(ProducerUsabilityDiagnosticError::Configuration(
                "producer plan does not match the materialized manifest settings".into(),
            ));
        }
    }
    let events = events.as_ref();
    let output = output.as_ref().to_owned();
    fs::create_dir_all(&output)?;
    let event_rows = read_event_log(events)?;
    let corpus_fingerprint = fingerprint_bytes(&fs::read(events)?);
    let feature_output = output.join("feature-analysis");
    fs::create_dir_all(&feature_output)?;
    let feature_path = feature_output.join("features.jsonl");
    let feature_rows = if feature_path.exists() {
        feature_extraction_from_rows(
            &event_rows,
            read_feature_rows(&feature_path)?,
            corpus_fingerprint.clone(),
        )
    } else {
        let mut extraction = extract_feature_rows(&event_rows)?;
        extraction.corpus_fingerprint = corpus_fingerprint.clone();
        write_feature_rows(&feature_path, &extraction.rows)?;
        extraction
    };
    let behavior = read_behavior_artifact(output.join("producer-usability-behavior.json"))?;
    if !behavior.passed || behavior.search_decision_changes != REQUIRED_SEARCH_DECISION_CHANGES {
        return Err(ProducerUsabilityDiagnosticError::Configuration(
            "producer usability behavior control did not pass".into(),
        ));
    }
    let behavior_bytes = fs::read(output.join("producer-usability-behavior.json"))?;
    let disabled_corpus_fingerprint = fingerprint_bytes(&fs::read(
        output.join("producer-usability-disabled/events.jsonl"),
    )?);
    let scenarios = scenario_artifact(plan)?;
    let performance = performance_artifact(plan)?;
    let scenario_bytes = write_json(output.join("producer-usability-scenarios.json"), &scenarios)?;
    let performance_bytes = write_json(
        output.join("producer-usability-performance.json"),
        &performance,
    )?;
    let decision = build_decision(
        manifest,
        &feature_rows,
        &scenarios,
        &performance,
        DecisionProvenance {
            experiment_id: &plan.experiment_id,
            corpus_fingerprint: &corpus_fingerprint,
            scenario_fingerprint: &fingerprint_bytes(&scenario_bytes),
            performance_fingerprint: &fingerprint_bytes(&performance_bytes),
            feature_rows_fingerprint: &fingerprint_bytes(&fs::read(&feature_path)?),
            behavior_fingerprint: &fingerprint_bytes(&behavior_bytes),
            disabled_corpus_fingerprint: &disabled_corpus_fingerprint,
            behavior: &behavior,
            thresholds: &plan.thresholds,
        },
    );
    write_json(output.join("producer-usability-decision.json"), &decision)?;
    fs::write(
        output.join("producer-usability-decision.md"),
        render_decision_markdown(&decision),
    )?;
    verify_producer_usability_artifacts(manifest, &output)?;
    Ok(ProducerUsabilitySummary {
        scenarios,
        performance,
        decision,
        output,
    })
}

fn write_json<T: Serialize>(
    path: PathBuf,
    value: &T,
) -> Result<Vec<u8>, ProducerUsabilityDiagnosticError> {
    let bytes = serde_json::to_vec_pretty(value)?;
    fs::write(path, &bytes)?;
    Ok(bytes)
}

fn read_json<T: DeserializeOwned>(
    path: impl AsRef<Path>,
) -> Result<T, ProducerUsabilityDiagnosticError> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

/// Compare gameplay with the producer stage enabled and disabled.
pub(crate) fn build_behavior_artifact(
    enabled: &[EventRow],
    disabled: &[EventRow],
) -> ProducerUsabilityBehaviorArtifact {
    let enabled_command_fingerprint = command_stream_fingerprint(enabled);
    let disabled_command_fingerprint = command_stream_fingerprint(disabled);
    let enabled_event_fingerprint = event_stream_fingerprint(enabled);
    let disabled_event_fingerprint = event_stream_fingerprint(disabled);
    let search_decision_changes = command_stream_changes(enabled, disabled);
    ProducerUsabilityBehaviorArtifact {
        schema_version: PRODUCER_DIAGNOSTIC_SCHEMA_VERSION,
        enabled_command_fingerprint: enabled_command_fingerprint.clone(),
        disabled_command_fingerprint: disabled_command_fingerprint.clone(),
        enabled_event_fingerprint: enabled_event_fingerprint.clone(),
        disabled_event_fingerprint: disabled_event_fingerprint.clone(),
        search_decision_changes,
        passed: search_decision_changes == REQUIRED_SEARCH_DECISION_CHANGES
            && enabled_command_fingerprint == disabled_command_fingerprint
            && enabled_event_fingerprint == disabled_event_fingerprint,
    }
}

pub(crate) fn read_behavior_artifact(
    path: impl AsRef<Path>,
) -> Result<ProducerUsabilityBehaviorArtifact, ProducerUsabilityDiagnosticError> {
    let artifact: ProducerUsabilityBehaviorArtifact = read_json(path)?;
    if artifact.schema_version != PRODUCER_DIAGNOSTIC_SCHEMA_VERSION {
        return Err(ProducerUsabilityDiagnosticError::Configuration(format!(
            "behavior artifact uses schema {}, expected {}",
            artifact.schema_version, PRODUCER_DIAGNOSTIC_SCHEMA_VERSION
        )));
    }
    if artifact.enabled_command_fingerprint.is_empty()
        || artifact.disabled_command_fingerprint.is_empty()
        || artifact.enabled_event_fingerprint.is_empty()
        || artifact.disabled_event_fingerprint.is_empty()
    {
        return Err(ProducerUsabilityDiagnosticError::Configuration(
            "behavior artifact has an empty fingerprint".into(),
        ));
    }
    Ok(artifact)
}

pub(crate) fn write_behavior_artifact(
    path: impl AsRef<Path>,
    artifact: &ProducerUsabilityBehaviorArtifact,
) -> Result<Vec<u8>, ProducerUsabilityDiagnosticError> {
    write_json(path.as_ref().to_owned(), artifact)
}

fn scenario_artifact(
    plan: &ProducerUsabilityPlan,
) -> Result<ProducerUsabilityScenarioArtifact, ProducerUsabilityDiagnosticError> {
    let base = awbrn_ai::board::arena(false, 1);
    let (owner, hostile) = fixture_seats(&base);
    let position = producer_position(&base, owner);
    let hostile_position = producer_position(&base, hostile);
    let mut scenarios = Vec::new();

    scenarios.push(scenario(
        "empty-producer",
        &base,
        position,
        ProducerUsability::Open,
    ));

    let mut releasable = base.clone();
    add_fixture_unit(&mut releasable, owner, position, UnitAction::Ready);
    scenarios.push(scenario(
        "unblock-and-produce",
        &releasable,
        position,
        ProducerUsability::Releasable,
    ));

    let mut blocked = base.clone();
    add_fixture_unit(&mut blocked, owner, position, UnitAction::Immobilized);
    scenarios.push(scenario(
        "matched-friendly-blocked",
        &blocked,
        position,
        ProducerUsability::FriendlyBlocked,
    ));

    let mut hostile_state = base.clone();
    add_fixture_unit(&mut hostile_state, hostile, position, UnitAction::Ready);
    scenarios.push(scenario(
        "hostile-blocked",
        &hostile_state,
        position,
        ProducerUsability::HostileBlocked,
    ));
    scenarios.push(fog_hidden_occupation_scenario(
        &base,
        owner,
        hostile,
        hostile_position,
    )?);

    let available = scenarios
        .iter()
        .map(|scenario| scenario.fixture.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if plan
        .scenario_fixtures
        .iter()
        .any(|fixture| !available.contains(fixture.as_str()))
    {
        return Err(ProducerUsabilityDiagnosticError::Configuration(
            "producer usability plan names an unknown scenario fixture".into(),
        ));
    }
    let scenarios = scenarios
        .into_iter()
        .filter(|scenario| plan.scenario_fixtures.contains(&scenario.fixture))
        .collect();
    Ok(ProducerUsabilityScenarioArtifact {
        schema_version: PRODUCER_DIAGNOSTIC_SCHEMA_VERSION,
        scenarios,
    })
}

fn scenario(
    fixture: &str,
    state: &State,
    position: Pos,
    expected: ProducerUsability,
) -> ProducerUsabilityScenarioResult {
    scenario_from_report(
        fixture,
        "authoritative",
        classify_producers(state),
        position,
        expected,
    )
}

fn fog_hidden_occupation_scenario(
    base: &State,
    recipient: PlayerIdx,
    owner: PlayerIdx,
    position: Pos,
) -> Result<ProducerUsabilityScenarioResult, ProducerUsabilityDiagnosticError> {
    let mut state = base.clone();
    add_fixture_unit(&mut state, owner, position, UnitAction::Ready);
    let mut observation = observe_for_seat(&state, recipient)
        .map_err(ProducerUsabilityDiagnosticError::Observation)?;
    observation.settings.fog = true;
    observation
        .units
        .retain(|unit| unit.location != Location::Board { position });
    let report = classify_producers_in_observation(&observation)
        .map_err(|error| ProducerUsabilityDiagnosticError::Observation(error.to_string()))?;
    Ok(scenario_from_report(
        "fog-hidden-occupation",
        "fog-visible",
        report,
        position,
        ProducerUsability::Unknown,
    ))
}

fn scenario_from_report(
    fixture: &str,
    mode: &str,
    report: ProducerUsabilityReport,
    position: Pos,
    expected: ProducerUsability,
) -> ProducerUsabilityScenarioResult {
    let actual = report
        .records
        .iter()
        .find(|record| record.position == position)
        .map(|record| record.class)
        .unwrap_or(ProducerUsability::Disabled);
    let tile = report
        .records
        .iter()
        .find(|record| record.position == position)
        .map(|record| record.producer_tile.to_string())
        .unwrap_or_else(|| "missing".into());
    ProducerUsabilityScenarioResult {
        fixture: fixture.into(),
        mode: mode.into(),
        producer_records: report.records,
        expected_classes: [(tile.clone(), expected)].into_iter().collect(),
        actual_classes: [(tile, actual)].into_iter().collect(),
        passed: actual == expected,
    }
}

fn performance_artifact(
    plan: &ProducerUsabilityPlan,
) -> Result<ProducerUsabilityPerformanceArtifact, ProducerUsabilityDiagnosticError> {
    let available = ["arena", "amber-valley", "late-game"];
    if plan
        .performance_fixtures
        .iter()
        .any(|fixture| !available.contains(&fixture.as_str()))
    {
        return Err(ProducerUsabilityDiagnosticError::Configuration(
            "producer usability plan names an unknown performance fixture".into(),
        ));
    }
    let mut fixtures = Vec::new();
    for (name, state) in [
        ("arena", awbrn_ai::board::arena(false, 7)),
        ("amber-valley", awbrn_ai::board::amber_valley(false, 7)),
        ("late-game", late_game_fixture()),
    ]
    .into_iter()
    .filter(|(name, _)| {
        plan.performance_fixtures
            .iter()
            .any(|fixture| fixture == name)
    }) {
        let rows = feature_benchmark_rows(&state);
        fixtures.push(measure_fixture(name, &state, &rows, plan.sample_count));
    }
    Ok(ProducerUsabilityPerformanceArtifact {
        schema_version: PRODUCER_DIAGNOSTIC_SCHEMA_VERSION,
        fixtures,
    })
}

fn late_game_fixture() -> State {
    let replay = ReplayParser::new()
        .parse(LATE_GAME_REPLAY)
        .expect("the checked-in late-game replay parses");
    let map: AwbwMapData =
        serde_json::from_slice(LATE_GAME_MAP).expect("the checked-in late-game map parses");
    let mut adapter = RecordedAdapter::new(&replay, &map)
        .expect("the checked-in late-game fixture has a valid initial state");
    let target = PlayerId::from("3588610");
    for (index, action) in replay.turns.iter().enumerate() {
        if is_join_setup(action, replay.turns.get(index + 1)) {
            continue;
        }
        let transition = adapter
            .advance(action)
            .expect("the checked-in late-game action applies");
        let state = transition.post_state();
        if action.kind_name() == "End" && state.turn.day == 15 && state.turn.active_player == target
        {
            assert!(state.units.len() > 20, "late-game fixture is populated");
            return state.clone();
        }
    }
    panic!("the checked-in replay has no populated day-15 position");
}

fn is_join_setup(action: &Action, next: Option<&Action>) -> bool {
    let Action::Move(movement) = action else {
        return false;
    };
    let Some(Action::Join { join_action, .. }) = next else {
        return false;
    };
    let moving_id = movement
        .unit
        .values()
        .find_map(|unit| unit.get_value())
        .map(|unit| unit.units_id.as_u32());
    let joining_id = join_action
        .join_id
        .values()
        .find_map(|unit| unit.get_value())
        .copied();
    moving_id == joining_id
}

fn measure_fixture(
    name: &str,
    state: &State,
    rows: &[EventRow],
    sample_count: u32,
) -> ProducerUsabilityPerformance {
    let (baseline_producer_count, baseline_occupied_producer_count) = baseline_producer_data(state);
    let mut isolated_baseline_times = Vec::with_capacity(sample_count as usize);
    let mut isolated_candidate_times = Vec::with_capacity(sample_count as usize);
    let mut complete_baseline_times = Vec::with_capacity(sample_count as usize);
    let mut complete_candidate_times = Vec::with_capacity(sample_count as usize);
    let (perspective, _) = fixture_seats(state);
    let observation = observe_for_seat(state, perspective).expect("the fixture observes");
    let visible_session =
        awvm::session::Session::from_observation(&observation).expect("the observation reifies");
    let baseline_cost = baseline_extraction_cost(state);
    let candidate_cost = feature_extraction_cost(state);
    for iteration in 0..PRODUCER_BENCHMARK_WARMUP_ITERATIONS {
        measure_isolated(state, &observation, &visible_session, iteration % 2 == 0);
        measure_complete(rows, iteration % 2 == 0);
    }
    for sample in 0..sample_count {
        let baseline_first = sample % 2 == 0;
        let (isolated_baseline, isolated_candidate) =
            measure_isolated(state, &observation, &visible_session, baseline_first);
        let (complete_baseline, complete_candidate) = measure_complete(rows, baseline_first);
        isolated_baseline_times.push(isolated_baseline);
        isolated_candidate_times.push(isolated_candidate);
        complete_baseline_times.push(complete_baseline);
        complete_candidate_times.push(complete_candidate);
    }
    isolated_baseline_times.sort_unstable();
    isolated_candidate_times.sort_unstable();
    complete_baseline_times.sort_unstable();
    complete_candidate_times.sort_unstable();
    let isolated_baseline_median = percentile(&isolated_baseline_times, 50);
    let isolated_baseline_p95 = percentile(&isolated_baseline_times, 95);
    let isolated_candidate_median = percentile(&isolated_candidate_times, 50);
    let isolated_candidate_p95 = percentile(&isolated_candidate_times, 95);
    let complete_baseline_median = percentile(&complete_baseline_times, 50);
    let complete_baseline_p95 = percentile(&complete_baseline_times, 95);
    let complete_candidate_median = percentile(&complete_candidate_times, 50);
    let complete_candidate_p95 = percentile(&complete_candidate_times, 95);
    ProducerUsabilityPerformance {
        schema_version: PRODUCER_DIAGNOSTIC_SCHEMA_VERSION,
        fixture: name.into(),
        sample_count,
        isolated_baseline_median_nanos: isolated_baseline_median,
        isolated_baseline_p95_nanos: isolated_baseline_p95,
        isolated_candidate_median_nanos: isolated_candidate_median,
        isolated_candidate_p95_nanos: isolated_candidate_p95,
        isolated_median_relative_change: relative_change(
            isolated_candidate_median,
            isolated_baseline_median,
        ),
        isolated_p95_relative_change: relative_change(
            isolated_candidate_p95,
            isolated_baseline_p95,
        ),
        complete_baseline_median_nanos: complete_baseline_median,
        complete_baseline_p95_nanos: complete_baseline_p95,
        complete_candidate_median_nanos: complete_candidate_median,
        complete_candidate_p95_nanos: complete_candidate_p95,
        complete_median_relative_change: relative_change(
            complete_candidate_median,
            complete_baseline_median,
        ),
        complete_p95_relative_change: relative_change(
            complete_candidate_p95,
            complete_baseline_p95,
        ),
        baseline_producer_count: u32::try_from(baseline_producer_count).unwrap_or(u32::MAX),
        baseline_occupied_producer_count: u32::try_from(baseline_occupied_producer_count)
            .unwrap_or(u32::MAX),
        baseline_cost,
        candidate_cost,
    }
}

fn measure_isolated(
    state: &State,
    observation: &Observation,
    visible_session: &awvm::session::Session,
    baseline_first: bool,
) -> (u64, u64) {
    let mut baseline = None;
    let mut candidate = None;
    for baseline_side in [baseline_first, !baseline_first] {
        if baseline_side {
            let start = Instant::now();
            baseline_producer_extraction(state, observation);
            baseline = Some(start.elapsed().as_nanos() as u64);
        } else {
            let start = Instant::now();
            candidate_producer_extraction(state, observation, visible_session);
            candidate = Some(start.elapsed().as_nanos() as u64);
        }
    }
    (baseline.unwrap_or_default(), candidate.unwrap_or_default())
}

fn measure_complete(rows: &[EventRow], baseline_first: bool) -> (u64, u64) {
    let mut baseline = None;
    let mut candidate = None;
    for baseline_side in [baseline_first, !baseline_first] {
        if baseline_side {
            let start = Instant::now();
            std::hint::black_box(
                extract_feature_rows_without_producer(rows).expect("benchmark rows are valid"),
            );
            baseline = Some(start.elapsed().as_nanos() as u64);
        } else {
            let start = Instant::now();
            std::hint::black_box(extract_feature_rows(rows).expect("benchmark rows are valid"));
            candidate = Some(start.elapsed().as_nanos() as u64);
        }
    }
    (baseline.unwrap_or_default(), candidate.unwrap_or_default())
}

fn baseline_producer_extraction(state: &State, observation: &Observation) {
    let authoritative = classify_producers(state);
    let fog = classify_producers_in_observation(observation).expect("the observation classifies");
    std::hint::black_box((authoritative, fog));
}

fn baseline_extraction_cost(state: &State) -> ProducerUsabilityCost {
    let (perspective, _) = fixture_seats(state);
    let observation = observe_for_seat(state, perspective).expect("the fixture observes");
    let authoritative = classify_producers(state);
    let fog = classify_producers_in_observation(&observation).expect("the observation classifies");
    ProducerUsabilityCost {
        movement_or_legality_queries: authoritative.movement_queries + fog.movement_queries,
        threat_map_builds: 0,
        scratch_allocations: authoritative.scratch_allocations + fog.scratch_allocations,
        full_state_clones: authoritative.full_state_clones + fog.full_state_clones,
    }
}

fn candidate_producer_extraction(
    state: &State,
    observation: &Observation,
    visible_session: &awvm::session::Session,
) -> ProducerUsabilityCountsReport {
    let mut extractor = ProducerUsabilityExtractor::new();
    let authoritative = extractor.state_counts(state);
    let fog = extractor
        .observation_counts_with_session(observation, visible_session)
        .expect("the observation classifies");
    std::hint::black_box((authoritative, fog)).0
}

fn feature_extraction_cost(state: &State) -> ProducerUsabilityCost {
    let (perspective, _) = fixture_seats(state);
    let observation = observe_for_seat(state, perspective).expect("the fixture observes");
    let visible_session =
        awvm::session::Session::from_observation(&observation).expect("the observation reifies");
    let mut extractor = ProducerUsabilityExtractor::new();
    let authoritative = extractor.state_counts(state);
    let fog = extractor
        .observation_counts_with_session(&observation, &visible_session)
        .expect("the observation classifies");
    ProducerUsabilityCost {
        movement_or_legality_queries: authoritative.movement_queries + fog.movement_queries,
        scratch_allocations: authoritative.scratch_allocations + fog.scratch_allocations,
        full_state_clones: authoritative.full_state_clones + fog.full_state_clones,
        threat_map_builds: 0,
    }
}

fn feature_benchmark_rows(state: &State) -> Vec<EventRow> {
    let (first, _) = fixture_seats(state);
    let mut terminal = state.clone();
    terminal.match_state = awvm::semantic::Match::Finished {
        outcome: awvm::semantic::Outcome::Victory {
            winners: vec![state.player(first).team.clone()],
            reason: awvm::semantic::VictoryReason::DayLimit,
        },
    };
    let metadata = ("producer-feature-benchmark", PairKey::new(61748, 7, 0));
    vec![
        EventRow {
            schema_version: EVENT_LOG_SCHEMA_VERSION,
            sequence: 0,
            match_id: metadata.0.into(),
            attempt: 0,
            pair: metadata.1.clone(),
            match_seed: 7,
            seat_order: SeatOrderVariant::AgentFirst,
            map_fingerprint: "fixture".into(),
            configuration_fingerprint: "fixture".into(),
            event_kind: EventKind::TurnEnd,
            day: state.turn.day,
            active_player: state.turn.active_player.clone(),
            turn_index: 0,
            command_index: 0,
            command: Some(awvm::transition::Command::EndTurn {
                player: state.player_id(first).clone(),
            }),
            command_fingerprint: 0,
            invalidation: None,
            state: state.clone(),
        },
        EventRow {
            schema_version: EVENT_LOG_SCHEMA_VERSION,
            sequence: 1,
            match_id: metadata.0.into(),
            attempt: 0,
            pair: metadata.1,
            match_seed: 7,
            seat_order: SeatOrderVariant::AgentFirst,
            map_fingerprint: "fixture".into(),
            configuration_fingerprint: "fixture".into(),
            event_kind: EventKind::Terminal,
            day: terminal.turn.day,
            active_player: terminal.turn.active_player.clone(),
            turn_index: 1,
            command_index: 1,
            command: None,
            command_fingerprint: 0,
            invalidation: None,
            state: terminal,
        },
    ]
}

fn baseline_producer_data(state: &State) -> (usize, usize) {
    let producers = state
        .board
        .tiles()
        .filter(|tile| {
            ruleset::terrain_has(tile.terrain, TerrainTrait::ProducesGround)
                || ruleset::terrain_has(tile.terrain, TerrainTrait::ProducesAir)
                || ruleset::terrain_has(tile.terrain, TerrainTrait::ProducesSea)
        })
        .count();
    let occupied = state
        .units
        .iter()
        .filter_map(|unit| match unit.location {
            Location::Board { position } => Some(position),
            Location::Cargo { .. } => None,
        })
        .filter(|position| {
            let terrain = state.board.tile(*position).terrain;
            ruleset::terrain_has(terrain, TerrainTrait::ProducesGround)
                || ruleset::terrain_has(terrain, TerrainTrait::ProducesAir)
                || ruleset::terrain_has(terrain, TerrainTrait::ProducesSea)
        })
        .count();
    (producers, occupied)
}

fn percentile(values: &[u64], percent: usize) -> u64 {
    let index =
        ((values.len().saturating_sub(1)) * percent / 100).min(values.len().saturating_sub(1));
    values.get(index).copied().unwrap_or_default()
}

fn relative_change(candidate: u64, baseline: u64) -> f64 {
    if baseline == 0 {
        if candidate == 0 { 0.0 } else { f64::INFINITY }
    } else {
        candidate as f64 / baseline as f64 - 1.0
    }
}

struct DecisionProvenance<'a> {
    experiment_id: &'a str,
    corpus_fingerprint: &'a str,
    scenario_fingerprint: &'a str,
    performance_fingerprint: &'a str,
    feature_rows_fingerprint: &'a str,
    behavior_fingerprint: &'a str,
    disabled_corpus_fingerprint: &'a str,
    behavior: &'a ProducerUsabilityBehaviorArtifact,
    thresholds: &'a ProducerUsabilityThresholds,
}

fn build_decision(
    manifest: &RunManifest,
    feature_rows: &crate::feature_analysis::FeatureExtraction,
    scenarios: &ProducerUsabilityScenarioArtifact,
    performance: &ProducerUsabilityPerformanceArtifact,
    provenance: DecisionProvenance<'_>,
) -> ProducerUsabilityDecision {
    let DecisionProvenance {
        experiment_id,
        corpus_fingerprint,
        scenario_fingerprint,
        performance_fingerprint,
        feature_rows_fingerprint,
        behavior_fingerprint,
        disabled_corpus_fingerprint,
        behavior,
        thresholds,
    } = provenance;
    let (threshold_results, correctness, performance_ok, search_ok) =
        threshold_results_for(scenarios, performance, behavior, thresholds);

    let base = awbrn_ai::board::arena(false, 1);
    let fog = fog_unknown_fixture(&base).ok();
    let authoritative = classify_producers(&base);
    let authoritative_counts = authoritative.counts_by_seat.clone();
    let fog_visible_counts = fog
        .as_ref()
        .map(|report| report.counts_by_seat.clone())
        .unwrap_or_default();
    let fog_unknown_counts = fog
        .as_ref()
        .map(|report| {
            report
                .counts_by_seat
                .iter()
                .map(|counts| counts.unknown)
                .collect()
        })
        .unwrap_or_default();

    let unblock = scenario_summary(
        scenarios,
        "unblock-and-produce",
        ProducerUsability::Releasable,
    );
    let blocked = scenario_summary(
        scenarios,
        "matched-friendly-blocked",
        ProducerUsability::FriendlyBlocked,
    );
    let decision = if correctness && performance_ok && search_ok {
        "accept"
    } else if correctness {
        "revise"
    } else {
        "reject"
    };
    let mut reasons = Vec::new();
    if !correctness {
        reasons.push("one or more producer class scenarios failed".into());
    }
    if !performance_ok {
        reasons.push("median extraction cost exceeded the five percent limit".into());
    }
    if !search_ok {
        reasons.push("enabled and disabled gameplay fingerprints differ".into());
    }
    if reasons.is_empty() {
        reasons.push("all producer-usability correctness and cost thresholds passed".into());
    }

    let mut artifact_fingerprints = BTreeMap::new();
    artifact_fingerprints.insert("input-corpus".into(), corpus_fingerprint.into());
    artifact_fingerprints.insert(
        "feature-analysis/features.jsonl".into(),
        feature_rows_fingerprint.into(),
    );
    artifact_fingerprints.insert(
        "producer-usability-scenarios.json".into(),
        scenario_fingerprint.into(),
    );
    artifact_fingerprints.insert(
        "producer-usability-performance.json".into(),
        performance_fingerprint.into(),
    );
    artifact_fingerprints.insert(
        "producer-usability-behavior.json".into(),
        behavior_fingerprint.into(),
    );
    artifact_fingerprints.insert(
        "producer-usability-disabled/events.jsonl".into(),
        disabled_corpus_fingerprint.into(),
    );
    let mut artifact_locations = BTreeMap::new();
    artifact_locations.insert(
        "input-corpus".into(),
        manifest
            .event_log
            .as_deref()
            .unwrap_or("events.jsonl")
            .into(),
    );
    artifact_locations.insert(
        "feature-analysis/features.jsonl".into(),
        "feature-analysis/features.jsonl".into(),
    );
    artifact_locations.insert(
        "producer-usability-scenarios.json".into(),
        "producer-usability-scenarios.json".into(),
    );
    artifact_locations.insert(
        "producer-usability-performance.json".into(),
        "producer-usability-performance.json".into(),
    );
    artifact_locations.insert(
        "producer-usability-behavior.json".into(),
        "producer-usability-behavior.json".into(),
    );
    artifact_locations.insert(
        "producer-usability-disabled/events.jsonl".into(),
        "producer-usability-disabled/events.jsonl".into(),
    );

    let mut decision_record = ProducerUsabilityDecision {
        schema_version: PRODUCER_DIAGNOSTIC_SCHEMA_VERSION,
        experiment_id: experiment_id.into(),
        decision_record_fingerprint: String::new(),
        source_revision: manifest.source_revision.clone(),
        dirty_worktree: manifest.dirty_worktree,
        executable_fingerprint: manifest.executable_fingerprint.clone(),
        experiment_plan_fingerprint: manifest.experiment_plan_fingerprint.clone(),
        input_corpus_fingerprint: corpus_fingerprint.into(),
        baseline_identifier: PRODUCER_BASELINE_IDENTIFIER.into(),
        candidate_identifier: PRODUCER_CANDIDATE_IDENTIFIER.into(),
        fixture_coverage: scenarios
            .scenarios
            .iter()
            .map(|scenario| scenario.fixture.clone())
            .collect(),
        performance_fixture_coverage: performance
            .fixtures
            .iter()
            .map(|fixture| fixture.fixture.clone())
            .collect(),
        map_coverage: manifest.maps.iter().map(|map| map.map_id).collect(),
        pair_coverage: manifest
            .pairs
            .iter()
            .map(|pair| {
                format!(
                    "map-{}-seed-{}-pair-{}",
                    pair.map_id, pair.run_seed, pair.pair_index
                )
            })
            .collect(),
        seed_coverage: manifest.pairs.iter().map(|pair| pair.run_seed).collect(),
        feature_row_coverage: ProducerFeatureRowCoverage {
            rows: feature_rows.rows.len(),
            matches: feature_rows.matches,
            matches_with_rows: feature_rows.matches_with_rows,
            authoritative_rows: feature_rows
                .rows
                .iter()
                .filter(|row| row.mode == crate::feature_analysis::FeatureMode::Authoritative)
                .count(),
            fog_visible_rows: feature_rows
                .rows
                .iter()
                .filter(|row| row.mode == crate::feature_analysis::FeatureMode::FogVisible)
                .count(),
        },
        thresholds: *thresholds,
        threshold_results,
        unblock_and_produce: unblock,
        matched_blocked_fixture: blocked,
        authoritative_counts,
        fog_visible_counts,
        fog_unknown_counts,
        search_decision_changes: behavior.search_decision_changes,
        behavior: behavior.clone(),
        observed_error_concern: "producer-availability; candidate-space, repair-policy, and leaf-evaluation causes remain separate experiments".into(),
        recommendations: ProducerUsabilityRecommendations {
            diagnostic: "retain producer usability in diagnostics".into(),
            production_opportunity:
                "use as an input to the independent production-opportunity experiment".into(),
            greedy: "do not change greedy behavior in this experiment".into(),
            search: "do not change search behavior in this experiment".into(),
        },
        decision: decision.into(),
        reasons,
        artifact_fingerprints,
        artifact_locations,
    };
    decision_record.decision_record_fingerprint = decision_fingerprint(&decision_record);
    decision_record
}

fn threshold_results_for(
    scenarios: &ProducerUsabilityScenarioArtifact,
    performance: &ProducerUsabilityPerformanceArtifact,
    behavior: &ProducerUsabilityBehaviorArtifact,
    thresholds: &ProducerUsabilityThresholds,
) -> (BTreeMap<String, bool>, bool, bool, bool) {
    let correctness = scenarios.scenarios.iter().all(|scenario| scenario.passed);
    let isolated_median_ok = performance.fixtures.iter().all(|fixture| {
        fixture.isolated_median_relative_change.is_finite()
            && fixture.isolated_median_relative_change <= thresholds.maximum_median_relative_change
    });
    let isolated_p95_ok = performance.fixtures.iter().all(|fixture| {
        fixture.isolated_p95_relative_change.is_finite()
            && fixture.isolated_p95_relative_change <= thresholds.maximum_p95_relative_change
    });
    let complete_median_ok = performance.fixtures.iter().all(|fixture| {
        fixture.complete_median_relative_change.is_finite()
            && fixture.complete_median_relative_change <= thresholds.maximum_median_relative_change
    });
    let complete_p95_ok = performance.fixtures.iter().all(|fixture| {
        fixture.complete_p95_relative_change.is_finite()
            && fixture.complete_p95_relative_change <= thresholds.maximum_p95_relative_change
    });
    let performance_ok =
        isolated_median_ok && isolated_p95_ok && complete_median_ok && complete_p95_ok;
    let search_ok = behavior.passed
        && behavior.search_decision_changes == thresholds.required_search_decision_changes;
    let threshold_results = BTreeMap::from([
        ("classification-correctness".into(), correctness),
        ("isolated-median-extraction-cost".into(), isolated_median_ok),
        ("isolated-p95-extraction-cost".into(), isolated_p95_ok),
        ("complete-median-extraction-cost".into(), complete_median_ok),
        ("complete-p95-extraction-cost".into(), complete_p95_ok),
        ("search-decision-changes".into(), search_ok),
    ]);
    (threshold_results, correctness, performance_ok, search_ok)
}

fn decision_fingerprint(decision: &ProducerUsabilityDecision) -> String {
    let mut fingerprint_record = decision.clone();
    fingerprint_record.decision_record_fingerprint.clear();
    fingerprint_record.threshold_results.clear();
    fingerprint_record.decision.clear();
    fingerprint_record.reasons.clear();
    fingerprint_record.artifact_fingerprints.insert(
        "producer-usability-performance.json".into(),
        "wall-clock-excluded".into(),
    );
    let fingerprint_input = serde_json::to_vec(&fingerprint_record).expect("decision serializes");
    fingerprint_bytes(&fingerprint_input)
}

/// Read and verify all producer artifacts for one completed run.
pub(crate) fn verify_producer_usability_artifacts(
    manifest: &RunManifest,
    output: impl AsRef<Path>,
) -> Result<(), ProducerUsabilityDiagnosticError> {
    let output = output.as_ref();
    let plan_value = manifest.producer_usability_plan.as_ref().ok_or_else(|| {
        ProducerUsabilityDiagnosticError::Configuration(
            "manifest has no materialized producer usability plan".into(),
        )
    })?;
    let plan: ProducerUsabilityPlan = serde_json::from_value(plan_value.clone())?;
    plan.validate()
        .map_err(ProducerUsabilityDiagnosticError::Configuration)?;
    if manifest.experiment_plan_fingerprint.is_empty() {
        return Err(ProducerUsabilityDiagnosticError::Configuration(
            "manifest has no experiment plan fingerprint".into(),
        ));
    }

    let event_path = crate::manifest::resolve_event_log_path(output, manifest)?;
    let event_bytes = fs::read(&event_path)?;
    let events = read_event_log(&event_path)?;
    let feature_path = output.join("feature-analysis/features.jsonl");
    let feature_bytes = fs::read(&feature_path)?;
    let feature_rows = read_feature_rows(&feature_path)?;
    let scenario_path = output.join("producer-usability-scenarios.json");
    let performance_path = output.join("producer-usability-performance.json");
    let behavior_path = output.join("producer-usability-behavior.json");
    let scenario_bytes = fs::read(&scenario_path)?;
    let performance_bytes = fs::read(&performance_path)?;
    let behavior_bytes = fs::read(&behavior_path)?;
    let disabled_path = output.join("producer-usability-disabled/events.jsonl");
    let disabled_bytes = fs::read(&disabled_path)?;
    let disabled_events = read_event_log(&disabled_path)?;
    let scenarios: ProducerUsabilityScenarioArtifact = read_json(&scenario_path)?;
    let performance: ProducerUsabilityPerformanceArtifact = read_json(&performance_path)?;
    let behavior: ProducerUsabilityBehaviorArtifact = read_behavior_artifact(&behavior_path)?;
    let decision_path = output.join("producer-usability-decision.json");
    let decision: ProducerUsabilityDecision = read_json(&decision_path)?;

    if scenarios.schema_version != PRODUCER_DIAGNOSTIC_SCHEMA_VERSION
        || performance.schema_version != PRODUCER_DIAGNOSTIC_SCHEMA_VERSION
    {
        return Err(ProducerUsabilityDiagnosticError::Configuration(
            "producer artifact schema does not match the executable".into(),
        ));
    }
    let expected_scenarios = plan
        .scenario_fixtures
        .iter()
        .collect::<std::collections::BTreeSet<_>>();
    let actual_scenarios = scenarios
        .scenarios
        .iter()
        .map(|scenario| &scenario.fixture)
        .collect::<std::collections::BTreeSet<_>>();
    if expected_scenarios != actual_scenarios {
        return Err(ProducerUsabilityDiagnosticError::Configuration(
            "scenario artifact coverage differs from the materialized plan".into(),
        ));
    }
    let expected_performance = plan
        .performance_fixtures
        .iter()
        .collect::<std::collections::BTreeSet<_>>();
    let actual_performance = performance
        .fixtures
        .iter()
        .map(|fixture| &fixture.fixture)
        .collect::<std::collections::BTreeSet<_>>();
    if expected_performance != actual_performance
        || performance
            .fixtures
            .iter()
            .any(|fixture| fixture.sample_count != plan.sample_count)
    {
        return Err(ProducerUsabilityDiagnosticError::Configuration(
            "performance artifact coverage differs from the materialized plan".into(),
        ));
    }

    let expected_behavior = build_behavior_artifact(&events, &disabled_events);
    if behavior != expected_behavior {
        return Err(ProducerUsabilityDiagnosticError::Configuration(
            "behavior artifact does not match its enabled and disabled event logs".into(),
        ));
    }
    if decision.schema_version != PRODUCER_DIAGNOSTIC_SCHEMA_VERSION
        || decision.experiment_id != plan.experiment_id
        || decision.experiment_plan_fingerprint != manifest.experiment_plan_fingerprint
        || decision.input_corpus_fingerprint != fingerprint_bytes(&event_bytes)
        || decision.baseline_identifier != PRODUCER_BASELINE_IDENTIFIER
        || decision.candidate_identifier != PRODUCER_CANDIDATE_IDENTIFIER
        || decision.thresholds != plan.thresholds
        || decision.behavior != behavior
        || decision.search_decision_changes != behavior.search_decision_changes
    {
        return Err(ProducerUsabilityDiagnosticError::Configuration(
            "producer decision does not match the manifest or producer artifacts".into(),
        ));
    }
    let (expected_threshold_results, expected_correctness, expected_performance, expected_search) =
        threshold_results_for(&scenarios, &performance, &behavior, &plan.thresholds);
    let expected_decision = if expected_correctness && expected_performance && expected_search {
        "accept"
    } else if expected_correctness {
        "revise"
    } else {
        "reject"
    };
    if decision.threshold_results != expected_threshold_results
        || decision.decision != expected_decision
    {
        return Err(ProducerUsabilityDiagnosticError::Configuration(
            "producer decision results do not match the producer artifacts".into(),
        ));
    }
    let feature_matches = feature_rows
        .iter()
        .map(|row| row.match_id.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let event_matches = latest_attempt_rows(&events)
        .iter()
        .map(|row| row.match_id.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let expected_feature_coverage = ProducerFeatureRowCoverage {
        rows: feature_rows.len(),
        matches: event_matches,
        matches_with_rows: feature_matches,
        authoritative_rows: feature_rows
            .iter()
            .filter(|row| row.mode == crate::feature_analysis::FeatureMode::Authoritative)
            .count(),
        fog_visible_rows: feature_rows
            .iter()
            .filter(|row| row.mode == crate::feature_analysis::FeatureMode::FogVisible)
            .count(),
    };
    if decision.feature_row_coverage != expected_feature_coverage {
        return Err(ProducerUsabilityDiagnosticError::Configuration(
            "producer decision feature coverage does not match features.jsonl".into(),
        ));
    }

    let expected_fingerprints = BTreeMap::from([
        ("input-corpus".into(), fingerprint_bytes(&event_bytes)),
        (
            "feature-analysis/features.jsonl".into(),
            fingerprint_bytes(&feature_bytes),
        ),
        (
            "producer-usability-scenarios.json".into(),
            fingerprint_bytes(&scenario_bytes),
        ),
        (
            "producer-usability-performance.json".into(),
            fingerprint_bytes(&performance_bytes),
        ),
        (
            "producer-usability-behavior.json".into(),
            fingerprint_bytes(&behavior_bytes),
        ),
        (
            "producer-usability-disabled/events.jsonl".into(),
            fingerprint_bytes(&disabled_bytes),
        ),
    ]);
    let event_location = manifest.event_log.as_deref().unwrap_or("events.jsonl");
    let expected_locations = BTreeMap::from([
        ("input-corpus".into(), event_location.into()),
        (
            "feature-analysis/features.jsonl".into(),
            "feature-analysis/features.jsonl".into(),
        ),
        (
            "producer-usability-scenarios.json".into(),
            "producer-usability-scenarios.json".into(),
        ),
        (
            "producer-usability-performance.json".into(),
            "producer-usability-performance.json".into(),
        ),
        (
            "producer-usability-behavior.json".into(),
            "producer-usability-behavior.json".into(),
        ),
        (
            "producer-usability-disabled/events.jsonl".into(),
            "producer-usability-disabled/events.jsonl".into(),
        ),
    ]);
    if decision.artifact_fingerprints != expected_fingerprints
        || decision.artifact_locations != expected_locations
        || decision.decision_record_fingerprint != decision_fingerprint(&decision)
    {
        return Err(ProducerUsabilityDiagnosticError::Configuration(
            "producer artifact provenance is incomplete or stale".into(),
        ));
    }
    Ok(())
}

fn scenario_summary(
    artifact: &ProducerUsabilityScenarioArtifact,
    fixture: &str,
    expected: ProducerUsability,
) -> ProducerUsabilityScenarioSummary {
    let actual = artifact
        .scenarios
        .iter()
        .find(|scenario| scenario.fixture == fixture)
        .and_then(|scenario| scenario.actual_classes.values().next().copied())
        .unwrap_or(ProducerUsability::Disabled);
    ProducerUsabilityScenarioSummary {
        fixture: fixture.into(),
        expected,
        actual,
        passed: actual == expected,
    }
}

fn render_decision_markdown(decision: &ProducerUsabilityDecision) -> String {
    let mut markdown = String::new();
    markdown.push_str("# Producer usability decision\n\n");
    markdown.push_str(&format!("Decision: `{}`\n\n", decision.decision));
    markdown.push_str(&format!(
        "Source revision: `{}`\n\n",
        decision.source_revision
    ));
    markdown.push_str(&format!(
        "Observed error concern: {}\n\n",
        decision.observed_error_concern
    ));
    markdown.push_str("## Thresholds\n\n");
    for (name, passed) in &decision.threshold_results {
        markdown.push_str(&format!(
            "- `{name}`: {}\n",
            if *passed { "pass" } else { "fail" }
        ));
    }
    markdown.push_str("\n## Required scenarios\n\n");
    markdown.push_str(&format!(
        "- `{}`: {:?} (expected {:?})\n",
        decision.unblock_and_produce.fixture,
        decision.unblock_and_produce.actual,
        decision.unblock_and_produce.expected
    ));
    markdown.push_str(&format!(
        "- `{}`: {:?} (expected {:?})\n",
        decision.matched_blocked_fixture.fixture,
        decision.matched_blocked_fixture.actual,
        decision.matched_blocked_fixture.expected
    ));
    markdown.push_str("\n## Recommendations\n\n");
    markdown.push_str(&format!(
        "- Diagnostics: {}\n- Production opportunity: {}\n- Greedy: {}\n- Search: {}\n",
        decision.recommendations.diagnostic,
        decision.recommendations.production_opportunity,
        decision.recommendations.greedy,
        decision.recommendations.search
    ));
    markdown
}

fn observe_for_seat(state: &State, seat: PlayerIdx) -> Result<Observation, String> {
    observe(&AwbwVisibility, state, state.player_id(seat)).map_err(|error| error.to_string())
}

fn fog_unknown_fixture(state: &State) -> Result<ProducerUsabilityReport, String> {
    let (recipient, owner) = fixture_seats(state);
    let position = producer_position(state, owner);
    let mut state = state.clone();
    add_fixture_unit(&mut state, owner, position, UnitAction::Ready);
    let mut observation = observe_for_seat(&state, recipient)?;
    observation.settings.fog = true;
    observation
        .units
        .retain(|unit| unit.location != Location::Board { position });
    classify_producers_in_observation(&observation).map_err(|error| error.to_string())
}

fn fixture_seats(state: &State) -> (PlayerIdx, PlayerIdx) {
    let mut seats = state.players.seats().map(|(seat, _)| seat);
    (
        seats.next().expect("the fixture has a first seat"),
        seats.next().expect("the fixture has a second seat"),
    )
}

fn producer_position(state: &State, owner: PlayerIdx) -> Pos {
    state
        .board
        .iter()
        .find(|(_, tile)| {
            tile.owner.is_owned_by(owner)
                && (ruleset::terrain_has(tile.terrain, TerrainTrait::ProducesGround)
                    || ruleset::terrain_has(tile.terrain, TerrainTrait::ProducesAir)
                    || ruleset::terrain_has(tile.terrain, TerrainTrait::ProducesSea))
        })
        .map(|(position, _)| position)
        .expect("the fixture has an owned producer")
}

fn add_fixture_unit(state: &mut State, owner: PlayerIdx, position: Pos, action: UnitAction) {
    state.units.push(Unit {
        id: UnitId::new(60_000 + u32::try_from(state.units.len()).unwrap_or_default()),
        kind: UnitKind::Infantry,
        owner,
        hp: 100,
        fuel: ruleset::profile(UnitKind::Infantry).max_fuel,
        ammo: ruleset::profile(UnitKind::Infantry).max_ammo,
        action,
        concealment: Concealment::Exposed,
        location: Location::Board { position },
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_plan_declares_the_required_audit() {
        let plan = ProducerUsabilityPlan::default();
        plan.validate().expect("the default plan is valid");
        assert_eq!(plan.sample_count, 100);
        assert_eq!(plan.thresholds.maximum_median_relative_change, 0.05);
        assert_eq!(plan.thresholds.maximum_p95_relative_change, 0.05);
        assert_eq!(plan.thresholds.required_search_decision_changes, 0);
    }

    #[test]
    fn required_scenarios_are_deterministic_and_distinguish_unblocking() {
        let plan = ProducerUsabilityPlan::default();
        let first = scenario_artifact(&plan).expect("scenarios build");
        let second = scenario_artifact(&plan).expect("scenarios build again");
        assert_eq!(first, second);
        let releasable = first
            .scenarios
            .iter()
            .find(|scenario| scenario.fixture == "unblock-and-produce")
            .expect("unblock fixture");
        let blocked = first
            .scenarios
            .iter()
            .find(|scenario| scenario.fixture == "matched-friendly-blocked")
            .expect("blocked fixture");
        assert_eq!(releasable.actual_classes, releasable.expected_classes);
        assert_eq!(blocked.actual_classes, blocked.expected_classes);
        assert_ne!(releasable.actual_classes, blocked.actual_classes);
    }

    #[test]
    fn late_game_benchmark_uses_the_populated_checked_in_fixture() {
        let state = late_game_fixture();
        assert_eq!(state.turn.day, 15);
        assert!(state.units.len() > 20);
    }
}
