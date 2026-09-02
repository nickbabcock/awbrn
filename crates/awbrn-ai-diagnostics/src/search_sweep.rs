//! The Search sweep search coverage experiment.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use awbrn_ai::EvalBreakdown;
use awbrn_ai::agent::{NodeBudget, SearchStats};
use awbrn_ai::agents::{
    SearchAllocator, SearchCandidateEvaluation, Weights, audit_with_allocator, order_candidate_id,
};
use awbrn_ai::baseline::BaselineConfig;
use awbrn_ai::board::arena;
use awbrn_ai::eval::EvalWeights;
use awbrn_ai_diagnostic_types::{
    ExecutionMode, RunLimits, RunManifest, TelemetryMode, fingerprint_bytes,
};
use awvm::ruleset::UnitKind;
use awvm::semantic::{AwbwVisibility, CellIdx, Location, Observation, UnitId, observe};
use awvm::session::{Order, OrderKind, Session};
use awvm::transition::{Command, ExecuteOutcome, execute};
use serde::{Deserialize, Serialize};

use crate::map_registry::MapRegistry;
use crate::plan::{AgentSpec, EXPERIMENT_PLAN_SCHEMA_VERSION, ExperimentPlan};
use crate::tournament::{
    AgentFactory, SearchCoverageArtifact, SearchFactory, StrategicFactory, TournamentError,
    TournamentPerformance, TournamentSummary, latest_match_records, run_paired_tournament,
};

/// The search sweep plan schema.
pub const SEARCH_SWEEP_PLAN_SCHEMA_VERSION: u16 = 1;
/// The search sweep artifact schema.
pub const SEARCH_SWEEP_ARTIFACT_SCHEMA_VERSION: u16 = 1;
/// The budgets required by the search sweep matrix.
pub const SEARCH_SWEEP_BUDGETS: [u32; 4] = [4, 16, 64, 256];

/// Thresholds declared before a Search sweep run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchSweepThresholds {
    /// Minimum complete pairs on every map and cell.
    pub minimum_complete_pairs_per_map: usize,
    /// Minimum mean pair-point improvement over the reference cell.
    pub minimum_material_pair_point_improvement: f64,
    /// Maximum allowed 95 percent pair-level half-width.
    pub required_pair_level_uncertainty_bound: f64,
    /// Maximum median candidate decision time in nanoseconds.
    pub maximum_median_decision_nanos: u64,
    /// Maximum p95 candidate decision time in nanoseconds.
    pub maximum_p95_decision_nanos: u64,
    /// Maximum allowed allocation regression as a fraction.
    #[serde(default)]
    pub maximum_allocation_regression: Option<f64>,
}

/// User-authored Search sweep experiment plan.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchSweepPlan {
    /// Plan schema version.
    pub schema_version: u16,
    /// Stable run name.
    pub run_id: String,
    /// Registered maps in the experiment.
    pub maps: Vec<u32>,
    /// Seeds used to choose a budget and allocator.
    pub tuning_run_seeds: Vec<u64>,
    /// Seeds used only for confirmation.
    pub evaluation_run_seeds: Vec<u64>,
    /// Number of pairs for each map and run seed.
    pub pairs_per_map: u64,
    /// Limits shared by every matrix cell.
    pub limits: RunLimits,
    /// Decisions made from the evidence.
    pub thresholds: SearchSweepThresholds,
    /// Optional human note.
    #[serde(default)]
    pub annotations: Option<String>,
}

/// Read and validate a Search sweep plan.
pub fn read_search_sweep_plan(path: impl AsRef<Path>) -> Result<SearchSweepPlan, SearchSweepError> {
    let plan: SearchSweepPlan = serde_json::from_slice(&fs::read(path)?)?;
    plan.validate()?;
    Ok(plan)
}

impl SearchSweepPlan {
    /// Validate fields that do not depend on the map registry.
    pub fn validate(&self) -> Result<(), SearchSweepError> {
        if self.schema_version != SEARCH_SWEEP_PLAN_SCHEMA_VERSION {
            return Err(SearchSweepError::Configuration(format!(
                "unsupported Search sweep plan schema {}",
                self.schema_version
            )));
        }
        if self.run_id.is_empty() || self.maps.is_empty() {
            return Err(SearchSweepError::Configuration(
                "Search sweep needs a run id and at least one map".into(),
            ));
        }
        if self.tuning_run_seeds.is_empty() || self.evaluation_run_seeds.is_empty() {
            return Err(SearchSweepError::Configuration(
                "Search sweep needs tuning and evaluation seeds".into(),
            ));
        }
        let mut maps = BTreeSet::new();
        if self.maps.iter().any(|map| !maps.insert(*map)) {
            return Err(SearchSweepError::Configuration(
                "Search sweep repeats a map".into(),
            ));
        }
        let tuning = self
            .tuning_run_seeds
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let evaluation = self
            .evaluation_run_seeds
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        if tuning.len() != self.tuning_run_seeds.len()
            || evaluation.len() != self.evaluation_run_seeds.len()
            || !tuning.is_disjoint(&evaluation)
        {
            return Err(SearchSweepError::Configuration(
                "Search sweep seed sets must be unique and disjoint".into(),
            ));
        }
        if self.pairs_per_map == 0
            || self.limits.day_limit == 0
            || self.limits.node_budget == 0
            || self.limits.refusal_limit == 0
        {
            return Err(SearchSweepError::Configuration(
                "Search sweep limits and pair count must be positive".into(),
            ));
        }
        if !self
            .thresholds
            .minimum_material_pair_point_improvement
            .is_finite()
            || self.thresholds.minimum_material_pair_point_improvement < 0.0
            || !self
                .thresholds
                .required_pair_level_uncertainty_bound
                .is_finite()
            || self.thresholds.required_pair_level_uncertainty_bound < 0.0
            || self
                .thresholds
                .maximum_allocation_regression
                .is_some_and(|value| !value.is_finite() || value < 0.0)
        {
            return Err(SearchSweepError::Configuration(
                "Search sweep thresholds must be finite and nonnegative".into(),
            ));
        }
        if self.thresholds.minimum_complete_pairs_per_map == 0
            || self.thresholds.maximum_median_decision_nanos == 0
            || self.thresholds.maximum_p95_decision_nanos == 0
        {
            return Err(SearchSweepError::Configuration(
                "Search sweep thresholds must be positive".into(),
            ));
        }
        Ok(())
    }
}

/// A coverage ratio and its counters.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CoverageRates {
    /// Searchable coordinates visited divided by searchable coordinates.
    pub coordinate_coverage: f64,
    /// Final-quartile coordinates visited divided by final-quartile coordinates.
    pub late_coordinate_coverage: f64,
    /// Decisions exhausted before their final coordinate.
    pub budget_exhaustion_rate: f64,
    /// Changed seed plans divided by seed plans.
    pub seed_change_rate: f64,
    /// Raw search counters.
    pub counters: SearchStats,
}

impl CoverageRates {
    fn from_stats(counters: SearchStats) -> Self {
        Self {
            coordinate_coverage: ratio(
                counters.coverage.visited_searchable_coordinates,
                counters.coverage.searchable_coordinates,
            ),
            late_coordinate_coverage: ratio(
                counters.coverage.visited_final_quartile_coordinates,
                counters.coverage.final_quartile_searchable_coordinates,
            ),
            budget_exhaustion_rate: ratio(
                counters
                    .coverage
                    .decisions_exhausted_before_final_coordinate,
                counters.coverage.decisions,
            ),
            seed_change_rate: ratio(
                counters.coverage.changed_seed_plans,
                counters.coverage.seed_plans,
            ),
            counters,
        }
    }
}

/// A paired result with performance data.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PairedResult {
    /// Candidate wins.
    pub wins: usize,
    /// Paired draws.
    pub draws: usize,
    /// Candidate losses.
    pub losses: usize,
    /// Complete pairs.
    pub complete_pairs: usize,
    /// Pairs that could not enter the reducer.
    pub incomplete_pairs: usize,
    /// Mean candidate pair-point differential.
    pub pair_points: f64,
    /// Mean differential against the four-node sequential-quota reference.
    pub pair_point_delta_from_reference: f64,
    /// Lower normal 95 percent interval.
    pub uncertainty_low: f64,
    /// Upper normal 95 percent interval.
    pub uncertainty_high: f64,
    /// 95 percent interval half-width.
    pub uncertainty_half_width: f64,
    /// Invalid commands.
    pub invalid_commands: u64,
    /// Unresolvable plays.
    pub unrealizable_plays: u64,
    /// Evaluated nodes per search decision.
    pub nodes_per_decision: f64,
    /// Evaluated nodes per second.
    pub nodes_per_second: f64,
    /// Median decision time in nanoseconds.
    pub median_decision_nanos: u64,
    /// p95 decision time in nanoseconds.
    pub p95_decision_nanos: u64,
}

/// Coverage and paired result for one map in one cell.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SearchSweepMapReport {
    /// Map identity.
    pub map_id: u32,
    /// Coverage for the candidate.
    pub coverage: CoverageRates,
    /// Paired outcome and cost.
    pub paired: PairedResult,
}

/// Runtime and node measurements for one matrix cell.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SearchSweepPerformance {
    /// Evaluated nodes per decision.
    pub nodes_per_decision: f64,
    /// Evaluated nodes per second.
    pub nodes_per_second: f64,
    /// Median decision time in nanoseconds.
    pub median_decision_nanos: u64,
    /// p95 decision time in nanoseconds.
    pub p95_decision_nanos: u64,
}

/// Compact coverage rates for one matrix cell.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SearchSweepCoverageSummary {
    /// Searchable coordinates that received a visit.
    pub coordinate_coverage: f64,
    /// Final-quartile coordinates that received a visit.
    pub late_coordinate_coverage: f64,
    /// Decisions that exhausted the budget before their final coordinate.
    pub budget_exhaustion_rate: f64,
    /// Seed plans that changed.
    pub seed_change_rate: f64,
    /// Search decisions in the cell.
    pub decisions: u64,
    /// Nodes requested by the cell.
    pub nodes_requested: u64,
    /// Nodes used by the cell.
    pub nodes_used: u64,
}

/// Compact summary for one matrix cell in the decision record.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchSweepCellSummary {
    /// Tuning or evaluation seed set.
    pub seed_set: String,
    /// Allocator name.
    pub allocator: SearchAllocator,
    /// Node budget.
    pub node_budget: u32,
    /// Equal-map-weighted coverage.
    pub coverage: SearchSweepCoverageSummary,
    /// Equal-map-weighted paired result.
    pub paired: PairedResult,
    /// Separate performance measurements.
    pub performance: SearchSweepPerformance,
}

/// One matrix cell.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchSweepCellReport {
    /// Tuning or evaluation seed set.
    pub seed_set: String,
    /// Allocator name.
    pub allocator: SearchAllocator,
    /// Node budget.
    pub node_budget: u32,
    /// Output directory with the materialized manifest.
    pub output: String,
    /// Manifest fingerprint for provenance.
    pub manifest_fingerprint: String,
    /// Configuration fingerprint for the candidate and baseline.
    pub configuration_fingerprint: String,
    /// Executable fingerprint for provenance.
    pub executable_fingerprint: String,
    /// Source revision for provenance.
    pub source_revision: String,
    /// Source dirty state for provenance.
    pub dirty_worktree: bool,
    /// Equal-map-weighted coverage.
    pub corpus_coverage: CoverageRates,
    /// Equal-map-weighted paired result.
    pub corpus_paired: PairedResult,
    /// Separate performance measurements.
    pub performance: SearchSweepPerformance,
    /// Per-map reports.
    pub maps: Vec<SearchSweepMapReport>,
}

/// Search coverage artifact for the Search sweep matrix.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchSweepCoverageArtifact {
    /// Artifact schema version.
    pub schema_version: u16,
    /// Fingerprint of the Search sweep plan.
    pub plan_fingerprint: String,
    /// Cells in deterministic order.
    pub cells: Vec<SearchSweepCellReport>,
}

/// Reachability details for one deterministic scenario.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UnblockAndProduceAudit {
    /// Scenario name.
    pub scenario: String,
    /// Allocator under test.
    pub allocator: SearchAllocator,
    /// Node budget under test.
    pub node_budget: u32,
    /// Unit that blocks the producer.
    pub blocker_unit: String,
    /// Producer tile.
    pub producer_tile: u32,
    /// Tile released by the blocker move.
    pub released_tile: u32,
    /// Destination used to release the producer.
    pub blocker_destination: u32,
    /// Production order required by the fixture.
    pub required_production: String,
    /// Exact move coordinate in the required candidate.
    pub blocker_move_coordinate: Option<usize>,
    /// Exact production coordinate in the required candidate.
    pub production_coordinate: Option<usize>,
    /// Coordinate required by the scenario.
    pub required_coordinate: Option<usize>,
    /// Whether the coordinate was visited.
    pub coordinate_visited: bool,
    /// Whether a legal alternative was generated.
    pub alternative_generated: bool,
    /// Whether the alternative was legal and applied.
    pub alternative_legal_and_applied: bool,
    /// Number of alternatives enumerated at the coordinate.
    pub alternatives_generated: u64,
    /// Number of alternatives rejected at the coordinate.
    pub alternatives_rejected: u64,
    /// Number of complete alternatives evaluated at the coordinate.
    pub alternatives_evaluated: u64,
    /// Whether repair produced a dependent action.
    pub dependent_action_generated: bool,
    /// Whether the exact fixture candidate was generated.
    pub required_candidate_generated: bool,
    /// Whether the exact fixture candidate reached the evaluator.
    pub required_candidate_evaluated: bool,
    /// Whether the exact fixture candidate was selected.
    pub required_candidate_selected: bool,
    /// The exact fixture candidate.
    pub required_candidate_plan: Vec<String>,
    /// Score of the exact fixture candidate.
    pub required_candidate_score: Option<f64>,
    /// Breakdown of the exact fixture candidate.
    pub required_candidate_breakdown: Option<EvalBreakdown>,
    /// Exact fixture candidate score minus selected-plan score.
    pub required_candidate_score_relative_to_selected: Option<f64>,
    /// Candidate score minus the selected plan score.
    pub leaf_score_relative_to_selected: Option<f64>,
    /// The greedy seed choices.
    pub seed_plan: Vec<String>,
    /// The selected choices.
    pub selected_plan: Vec<String>,
    /// The greedy seed score.
    pub seed_score: f64,
    /// The selected score.
    pub selected_score: f64,
    /// The greedy seed score breakdown.
    pub seed_breakdown: EvalBreakdown,
    /// The selected score breakdown.
    pub selected_breakdown: EvalBreakdown,
    /// One primary failure owner.
    pub primary_cause: String,
}

/// The Search sweep decision record.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchSweepDecision {
    /// Artifact schema version.
    pub schema_version: u16,
    /// Plan fingerprint.
    pub plan_fingerprint: String,
    /// Source revision from the materialized run.
    pub source_revision: String,
    /// Source dirty state.
    pub dirty_worktree: bool,
    /// Materialized cell manifests.
    pub configuration_fingerprints: Vec<String>,
    /// Materialized executable identities.
    pub executable_fingerprints: Vec<String>,
    /// Predeclared thresholds.
    pub thresholds: SearchSweepThresholds,
    /// Threshold outcomes by selected candidate.
    pub threshold_results: BTreeMap<String, bool>,
    /// Selected allocator, when one passed held-out evaluation.
    pub selected_allocator: Option<SearchAllocator>,
    /// Selected node budget, when one passed held-out evaluation.
    pub selected_node_budget: Option<u32>,
    /// Allocator retained until a candidate passes held-out evaluation.
    pub retained_allocator: SearchAllocator,
    /// Node budget retained until a candidate passes held-out evaluation.
    pub retained_node_budget: u32,
    /// Compact coverage, outcome, and performance summaries.
    pub summaries: Vec<SearchSweepCellSummary>,
    /// Search coverage decision.
    pub search_coverage_decision: String,
    /// Audit result for every scenario.
    pub scenario_audits: Vec<UnblockAndProduceAudit>,
    /// Tuning seed set.
    pub tuning_run_seeds: Vec<u64>,
    /// Held-out seed set.
    pub evaluation_run_seeds: Vec<u64>,
    /// Reason for the next experiment.
    pub next_experiment: String,
}

/// Outputs from a Search sweep run.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchSweepSummary {
    /// Output directory.
    pub output: PathBuf,
    /// Coverage artifact.
    pub coverage: SearchSweepCoverageArtifact,
    /// Budget sweep artifact.
    pub budget_sweep: SearchSweepCoverageArtifact,
    /// Scenario audit.
    pub scenarios: Vec<UnblockAndProduceAudit>,
    /// Decision record.
    pub decision: SearchSweepDecision,
}

/// Errors from Search sweep.
#[derive(Debug, thiserror::Error)]
pub enum SearchSweepError {
    #[error("Search sweep I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("Search sweep JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Search sweep configuration error: {0}")]
    Configuration(String),
    #[error("Search sweep plan error: {0}")]
    Plan(#[from] crate::plan::PlanError),
    #[error("Search sweep tournament error: {0}")]
    Tournament(#[from] TournamentError),
}

/// Run the complete Search sweep matrix and write its four artifacts.
pub fn run_search_sweep(
    plan_path: impl AsRef<Path>,
    output: impl AsRef<Path>,
) -> Result<SearchSweepSummary, SearchSweepError> {
    let plan_path = plan_path.as_ref();
    let plan = read_search_sweep_plan(plan_path)?;
    let registry = MapRegistry::load_checked_in()
        .map_err(TournamentError::from)
        .map_err(SearchSweepError::from)?;
    let output = output.as_ref().to_owned();
    fs::create_dir_all(&output)?;
    let plan_bytes = fs::read(plan_path)?;
    let plan_fingerprint = fingerprint_bytes(&plan_bytes);
    let mut cells = Vec::new();
    for (seed_set, seeds) in [
        ("tuning", plan.tuning_run_seeds.as_slice()),
        ("evaluation", plan.evaluation_run_seeds.as_slice()),
    ] {
        for allocator in [
            SearchAllocator::SequentialQuota,
            SearchAllocator::RoundRobin,
        ] {
            for node_budget in SEARCH_SWEEP_BUDGETS {
                cells.push(run_cell(
                    plan_path,
                    &plan,
                    &registry,
                    &output,
                    &plan_fingerprint,
                    seed_set,
                    seeds,
                    allocator,
                    node_budget,
                )?);
            }
        }
    }
    apply_reference_deltas(&mut cells);
    let coverage = SearchSweepCoverageArtifact {
        schema_version: SEARCH_SWEEP_ARTIFACT_SCHEMA_VERSION,
        plan_fingerprint: plan_fingerprint.clone(),
        cells: cells.clone(),
    };
    // Keep both names for existing search-sweep consumers. Both files use the same data.
    write_json(output.join("search-coverage-matrix.json"), &coverage)?;
    write_json(output.join("budget-sweep.json"), &coverage)?;
    let scenario_audits = unblock_and_produce_audits();
    write_json(output.join("scenario-reachability.json"), &scenario_audits)?;
    let decision = make_decision(&plan, &coverage, &scenario_audits, &cells)?;
    write_json(output.join("search-sweep-decision.json"), &decision)?;
    fs::write(
        output.join("search-sweep-decision.md"),
        render_decision(&decision, &coverage),
    )?;
    Ok(SearchSweepSummary {
        output,
        coverage: coverage.clone(),
        budget_sweep: coverage,
        scenarios: scenario_audits,
        decision,
    })
}

#[allow(clippy::too_many_arguments)]
fn run_cell(
    plan_path: &Path,
    plan: &SearchSweepPlan,
    registry: &MapRegistry,
    output: &Path,
    plan_fingerprint: &str,
    seed_set: &str,
    seeds: &[u64],
    allocator: SearchAllocator,
    node_budget: u32,
) -> Result<SearchSweepCellReport, SearchSweepError> {
    let allocator_name = allocator_name(allocator);
    let cell_output = output
        .join(seed_set)
        .join(format!("{allocator_name}-{node_budget}"));
    let candidate_identifier = format!("search-sweep-{seed_set}-{allocator_name}-{node_budget}");
    let mut reports = Vec::new();
    for run_seed in seeds {
        let experiment = experiment_plan(plan, &candidate_identifier, node_budget, *run_seed);
        let materialized = experiment.materialize(plan_path, registry)?;
        let candidate = SearchFactory::new_with_allocator(
            &candidate_identifier,
            Weights::CAPTURER_SHORTFALL_50,
            EvalWeights::STANDARD,
            NodeBudget::new(node_budget).ok_or_else(|| {
                SearchSweepError::Configuration("Search sweep node budget is zero".into())
            })?,
            allocator,
        );
        let baseline = StrategicFactory::new(BaselineConfig::LOCKED);
        let manifest =
            search_sweep_manifest(materialized.manifest, &experiment, &candidate, &baseline)?;
        let manifest_fingerprint = manifest
            .fingerprint()
            .map_err(SearchSweepError::Configuration)?;
        let executable_fingerprint = manifest.executable_fingerprint.clone();
        let configuration_fingerprint = manifest.configuration_fingerprint.clone();
        let run_output = cell_output.join(format!("seed-{run_seed}"));
        let tournament =
            run_paired_tournament(&manifest, registry, &candidate, &baseline, &run_output)?;
        let coverage_path = run_output.join("search-coverage.json");
        let artifact: SearchCoverageArtifact = serde_json::from_slice(&fs::read(coverage_path)?)?;
        reports.push(cell_report(
            seed_set,
            allocator,
            node_budget,
            &run_output,
            &manifest_fingerprint,
            &configuration_fingerprint,
            &executable_fingerprint,
            &manifest.source_revision,
            manifest.dirty_worktree,
            &tournament,
            &artifact,
            plan.maps.as_slice(),
            plan.pairs_per_map as usize,
            plan_fingerprint,
        ));
    }
    merge_cell_reports(reports)
}

fn experiment_plan(
    plan: &SearchSweepPlan,
    identifier: &str,
    node_budget: u32,
    run_seed: u64,
) -> ExperimentPlan {
    ExperimentPlan {
        schema_version: EXPERIMENT_PLAN_SCHEMA_VERSION,
        run_id: format!("{}-{identifier}-{run_seed}", plan.run_id),
        candidate: AgentSpec::Search {
            identifier: identifier.into(),
            preset: "production".into(),
            node_budget,
        },
        baseline: AgentSpec::Strategic {
            configuration: "locked".into(),
        },
        maps: plan.maps.clone(),
        run_seed,
        pairs_per_map: plan.pairs_per_map,
        limits: plan.limits.clone(),
        telemetry: TelemetryMode::Enabled,
        capture_policy: Default::default(),
        analyses: Vec::new(),
        annotations: plan.annotations.clone(),
    }
}

fn search_sweep_manifest(
    mut manifest: RunManifest,
    experiment: &ExperimentPlan,
    candidate: &SearchFactory,
    baseline: &StrategicFactory,
) -> Result<RunManifest, SearchSweepError> {
    manifest.mode = ExecutionMode::Diagnostic;
    manifest.telemetry = TelemetryMode::Enabled;
    manifest.agents = vec![candidate.identity().clone(), baseline.identity().clone()];
    manifest.executable_fingerprint = format!(
        "{}+{}",
        candidate.identity().executable_fingerprint,
        baseline.identity().executable_fingerprint
    );
    let bytes = serde_json::to_vec(&(
        candidate.identity(),
        baseline.identity(),
        &manifest.maps,
        &experiment.maps,
        experiment.run_seed,
        experiment.pairs_per_map,
        &experiment.limits,
        experiment.telemetry,
        &experiment.capture_policy,
        &experiment.analyses,
        &manifest.source_fingerprint,
        Vec::<String>::new(),
    ))?;
    manifest.configuration_fingerprint = fingerprint_bytes(&bytes);
    manifest
        .validate()
        .map_err(SearchSweepError::Configuration)?;
    Ok(manifest)
}

#[allow(clippy::too_many_arguments)]
fn cell_report(
    seed_set: &str,
    allocator: SearchAllocator,
    node_budget: u32,
    output: &Path,
    manifest_fingerprint: &str,
    configuration_fingerprint: &str,
    executable_fingerprint: &str,
    source_revision: &str,
    dirty_worktree: bool,
    tournament: &TournamentSummary,
    artifact: &SearchCoverageArtifact,
    maps: &[u32],
    expected_pairs_per_map: usize,
    _plan_fingerprint: &str,
) -> SearchSweepCellReport {
    let mut map_stats = BTreeMap::<u32, SearchStats>::new();
    for row in &artifact.matches {
        if let Some(stats) = &row.candidate {
            map_stats.entry(row.map_id).or_default().add(stats.clone());
        }
    }
    let mut map_reports = maps
        .iter()
        .map(|map_id| {
            let coverage = CoverageRates::from_stats(map_stats.remove(map_id).unwrap_or_default());
            let paired = paired_result_for_map(tournament, Some(*map_id), expected_pairs_per_map);
            SearchSweepMapReport {
                map_id: *map_id,
                coverage,
                paired,
            }
        })
        .collect::<Vec<_>>();
    map_reports.sort_by_key(|report| report.map_id);
    let corpus = CoverageRates::from_stats(artifact.candidate.clone());
    let paired = paired_result(tournament, &artifact.candidate);
    let performance = performance_from_paired(&paired);
    SearchSweepCellReport {
        seed_set: seed_set.into(),
        allocator,
        node_budget,
        output: output.to_string_lossy().replace('\\', "/"),
        manifest_fingerprint: manifest_fingerprint.into(),
        configuration_fingerprint: configuration_fingerprint.into(),
        executable_fingerprint: executable_fingerprint.into(),
        source_revision: source_revision.into(),
        dirty_worktree,
        corpus_coverage: corpus,
        corpus_paired: paired,
        performance,
        maps: map_reports,
    }
}

fn merge_cell_reports(
    reports: Vec<SearchSweepCellReport>,
) -> Result<SearchSweepCellReport, SearchSweepError> {
    let Some(first) = reports.first().cloned() else {
        return Err(SearchSweepError::Configuration(
            "Search sweep cell has no runs".into(),
        ));
    };
    if reports.len() == 1 {
        return Ok(first);
    }
    let mut merged = first;
    for report in reports.iter().skip(1) {
        merged
            .corpus_coverage
            .counters
            .add(report.corpus_coverage.counters.clone());
        merged.corpus_coverage = CoverageRates::from_stats(merged.corpus_coverage.counters.clone());
        merged.corpus_paired = merge_paired(&merged.corpus_paired, &report.corpus_paired);
        merged.performance = performance_from_paired(&merged.corpus_paired);
        for map in &mut merged.maps {
            if let Some(other) = report.maps.iter().find(|other| other.map_id == map.map_id) {
                map.coverage.counters.add(other.coverage.counters.clone());
                map.coverage = CoverageRates::from_stats(map.coverage.counters.clone());
                map.paired = merge_paired(&map.paired, &other.paired);
            }
        }
    }
    merged.output = reports
        .iter()
        .map(|report| report.output.as_str())
        .collect::<Vec<_>>()
        .join(",");
    Ok(merged)
}

fn paired_result(tournament: &TournamentSummary, counters: &SearchStats) -> PairedResult {
    let mut result = paired_result_for_map(tournament, None, 0);
    result.nodes_per_decision =
        ratio_f64(counters.coverage.nodes_used, counters.coverage.decisions);
    let times = latest_decision_times(&tournament.performance, None);
    result.median_decision_nanos = percentile(&times, 50);
    result.p95_decision_nanos = percentile(&times, 95);
    result.nodes_per_second = ratio_f64(
        counters.coverage.nodes_used.saturating_mul(1_000_000_000),
        times.iter().sum::<u64>(),
    );
    result
}

fn paired_result_for_map(
    tournament: &TournamentSummary,
    map_id: Option<u32>,
    expected_pairs_per_map: usize,
) -> PairedResult {
    let observations = tournament
        .reduction
        .observations
        .iter()
        .filter(|observation| map_id.is_none_or(|map| observation.key.map_id == map))
        .collect::<Vec<_>>();
    let values = observations
        .iter()
        .map(|observation| observation.differential)
        .collect::<Vec<_>>();
    let mean = mean(&values);
    let half_width = normal_half_width(&values);
    let mut result = PairedResult {
        wins: values.iter().filter(|value| **value > 1.0e-9).count(),
        draws: values.iter().filter(|value| value.abs() <= 1.0e-9).count(),
        losses: values.iter().filter(|value| **value < -1.0e-9).count(),
        complete_pairs: observations.len(),
        incomplete_pairs: if map_id.is_none() {
            tournament
                .reduction
                .coverage
                .expected
                .saturating_sub(tournament.reduction.coverage.valid)
        } else {
            expected_pairs_per_map.saturating_sub(observations.len())
        },
        pair_points: mean,
        pair_point_delta_from_reference: 0.0,
        uncertainty_low: mean - half_width,
        uncertainty_high: mean + half_width,
        uncertainty_half_width: half_width,
        invalid_commands: performance_value(tournament, map_id, |record| record.invalid_commands),
        unrealizable_plays: performance_value(tournament, map_id, |record| {
            record.unrealizable_plays
        }),
        ..PairedResult::default()
    };
    result.complete_pairs = values.len();
    result
}

fn performance_from_paired(paired: &PairedResult) -> SearchSweepPerformance {
    SearchSweepPerformance {
        nodes_per_decision: paired.nodes_per_decision,
        nodes_per_second: paired.nodes_per_second,
        median_decision_nanos: paired.median_decision_nanos,
        p95_decision_nanos: paired.p95_decision_nanos,
    }
}

fn merge_paired(left: &PairedResult, right: &PairedResult) -> PairedResult {
    let complete = left.complete_pairs + right.complete_pairs;
    let mean = if complete == 0 {
        0.0
    } else {
        (left.pair_points * left.complete_pairs as f64
            + right.pair_points * right.complete_pairs as f64)
            / complete as f64
    };
    PairedResult {
        wins: left.wins + right.wins,
        draws: left.draws + right.draws,
        losses: left.losses + right.losses,
        complete_pairs: complete,
        incomplete_pairs: left.incomplete_pairs + right.incomplete_pairs,
        pair_points: mean,
        pair_point_delta_from_reference: 0.0,
        uncertainty_half_width: left
            .uncertainty_half_width
            .max(right.uncertainty_half_width),
        uncertainty_low: mean
            - left
                .uncertainty_half_width
                .max(right.uncertainty_half_width),
        uncertainty_high: mean
            + left
                .uncertainty_half_width
                .max(right.uncertainty_half_width),
        invalid_commands: left.invalid_commands + right.invalid_commands,
        unrealizable_plays: left.unrealizable_plays + right.unrealizable_plays,
        nodes_per_decision: weighted_mean(
            left.nodes_per_decision,
            left.complete_pairs,
            right.nodes_per_decision,
            right.complete_pairs,
        ),
        nodes_per_second: weighted_mean(
            left.nodes_per_second,
            left.complete_pairs,
            right.nodes_per_second,
            right.complete_pairs,
        ),
        median_decision_nanos: left.median_decision_nanos.max(right.median_decision_nanos),
        p95_decision_nanos: left.p95_decision_nanos.max(right.p95_decision_nanos),
    }
}

fn weighted_mean(left: f64, left_count: usize, right: f64, right_count: usize) -> f64 {
    let count = left_count + right_count;
    if count == 0 {
        0.0
    } else {
        (left * left_count as f64 + right * right_count as f64) / count as f64
    }
}

fn performance_value(
    tournament: &TournamentSummary,
    map_id: Option<u32>,
    value: impl Fn(&crate::tournament::MatchPerformance) -> u64,
) -> u64 {
    latest_records(&tournament.performance)
        .filter(|record| map_id.is_none_or(|map| record.map_id == map))
        .map(value)
        .sum()
}

fn latest_records(
    performance: &TournamentPerformance,
) -> impl Iterator<Item = &crate::tournament::MatchPerformance> {
    latest_match_records(&performance.match_records).into_iter()
}

fn latest_decision_times(performance: &TournamentPerformance, map_id: Option<u32>) -> Vec<u64> {
    latest_records(performance)
        .filter(|record| map_id.is_none_or(|map| record.map_id == map))
        .flat_map(|record| record.candidate_decision_times_nanos.iter().copied())
        .collect()
}

struct UnblockAndProduceFixture {
    view: Observation,
    blocker_unit: UnitId,
    producer_tile: CellIdx,
    released_tile: CellIdx,
    blocker_destination: CellIdx,
    required_move: Order,
    required_production: Order,
}

fn unblock_and_produce_audits() -> Vec<UnblockAndProduceAudit> {
    let fixture = unblock_and_produce_fixture();
    let budgets = [
        NodeBudget::FOUR,
        NodeBudget::SIXTEEN,
        NodeBudget::new(64).unwrap(),
        NodeBudget::new(256).unwrap(),
    ];
    let mut reports = Vec::new();
    for allocator in [
        SearchAllocator::SequentialQuota,
        SearchAllocator::RoundRobin,
    ] {
        for budget in budgets {
            reports.push(audit_unblock_and_produce(&fixture, allocator, budget));
        }
    }
    reports
}

fn unblock_and_produce_fixture() -> UnblockAndProduceFixture {
    const PRODUCER_TILE: CellIdx = CellIdx::from_raw(82);
    const BLOCKER_DESTINATION: CellIdx = CellIdx::from_raw(41);
    let initial = arena(false, 1);
    let player = initial.turn.active_player.clone();
    let state = match execute(&initial, Command::EndTurn { player }, &[]) {
        Ok(ExecuteOutcome::Accepted(execution)) => execution.state,
        other => panic!("the Search sweep scenario setup did not execute: {other:?}"),
    };
    let active_seat = state
        .players
        .seat(&state.turn.active_player)
        .expect("the scenario active player has a seat");
    let producer_position = state
        .board
        .dimensions()
        .position_of(PRODUCER_TILE)
        .expect("the scenario producer is on the board");
    let blocker_unit = state
        .units
        .iter()
        .find(|unit| {
            unit.owner == active_seat
                && matches!(
                    unit.location,
                    Location::Board { position } if position == producer_position
                )
        })
        .map(|unit| unit.id)
        .expect("the scenario has a friendly blocker on its producer");
    let view = observe(&AwbwVisibility, &state, &state.turn.active_player)
        .expect("the Search sweep scenario is observable");
    let session = Session::from_observation(&view).expect("the scenario session opens");
    let blocker_index = session
        .index_of(blocker_unit)
        .expect("the blocker is visible in the scenario");
    let required_move = Order::new(blocker_index, BLOCKER_DESTINATION, OrderKind::Capture);
    let required_production =
        Order::unitless(PRODUCER_TILE, OrderKind::Produce(UnitKind::Infantry));
    UnblockAndProduceFixture {
        view,
        blocker_unit,
        producer_tile: PRODUCER_TILE,
        released_tile: PRODUCER_TILE,
        blocker_destination: BLOCKER_DESTINATION,
        required_move,
        required_production,
    }
}

fn audit_unblock_and_produce(
    fixture: &UnblockAndProduceFixture,
    allocator: SearchAllocator,
    budget: NodeBudget,
) -> UnblockAndProduceAudit {
    let audit = audit_with_allocator(
        &fixture.view,
        19,
        Weights::CAPTURER_SHORTFALL_50,
        EvalWeights::STANDARD,
        budget,
        allocator,
    );
    let Some(audit) = audit else {
        return UnblockAndProduceAudit {
            scenario: "unblock-and-produce".into(),
            allocator,
            node_budget: budget.get(),
            blocker_unit: fixture.blocker_unit.to_string(),
            producer_tile: u32::from(fixture.producer_tile.get()),
            released_tile: u32::from(fixture.released_tile.get()),
            blocker_destination: u32::from(fixture.blocker_destination.get()),
            required_production: format!("{:?}", fixture.required_production),
            blocker_move_coordinate: None,
            production_coordinate: None,
            required_coordinate: None,
            coordinate_visited: false,
            alternative_generated: false,
            alternative_legal_and_applied: false,
            alternatives_generated: 0,
            alternatives_rejected: 0,
            alternatives_evaluated: 0,
            dependent_action_generated: false,
            required_candidate_generated: false,
            required_candidate_evaluated: false,
            required_candidate_selected: false,
            required_candidate_plan: Vec::new(),
            required_candidate_score: None,
            required_candidate_breakdown: None,
            required_candidate_score_relative_to_selected: None,
            leaf_score_relative_to_selected: None,
            seed_plan: Vec::new(),
            selected_plan: Vec::new(),
            seed_score: 0.0,
            selected_score: 0.0,
            seed_breakdown: EvalBreakdown::default(),
            selected_breakdown: EvalBreakdown::default(),
            primary_cause: "candidate-space".into(),
        };
    };
    let candidate = required_candidate(fixture, &audit.evaluated_candidates);
    let required_candidate_evaluated = candidate.is_some();
    let required_candidate_selected =
        candidate.is_some_and(|candidate| candidate.plan == audit.selected_plan);
    let required_candidate_plan =
        candidate.map_or_else(Vec::new, |candidate| candidate.plan.clone());
    let required_candidate_score = candidate.map(|candidate| candidate.score);
    let required_candidate_breakdown = candidate.map(|candidate| candidate.breakdown);
    let blocker_move_coordinate = candidate.and_then(|candidate| {
        candidate
            .plan
            .iter()
            .position(|order| *order == fixture.required_move)
    });
    let production_coordinate = candidate.and_then(|candidate| {
        candidate
            .plan
            .iter()
            .position(|order| *order == fixture.required_production)
    });
    let required_coordinate = audit
        .seed_plan
        .iter()
        .position(|order| order.unit() == fixture.required_move.unit())
        .or(blocker_move_coordinate);
    let required_candidate_generated = candidate.is_some()
        || required_coordinate.is_some_and(|coordinate| {
            let required_id = order_candidate_id(fixture.required_move);
            audit
                .coverage
                .alternative_visits_by_pass
                .iter()
                .flatten()
                .any(|visit| visit.coordinate == coordinate && visit.candidate_id == required_id)
        });
    let coordinate_visited = required_coordinate
        .is_some_and(|coordinate| audit.coverage.visited_coordinates.contains(&coordinate));
    let coordinate_results = required_coordinate.and_then(|coordinate| {
        audit
            .coverage
            .coordinates
            .iter()
            .find(|entry| entry.coordinate == coordinate)
    });
    let alternative_generated =
        coordinate_results.is_some_and(|entry| entry.alternatives_generated > 0);
    let alternative_legal_and_applied =
        coordinate_results.is_some_and(|entry| entry.alternatives_evaluated > 0);
    let alternatives_generated = coordinate_results.map_or(0, |entry| entry.alternatives_generated);
    let alternatives_rejected = coordinate_results.map_or(0, |entry| entry.alternatives_rejected);
    let alternatives_evaluated = coordinate_results.map_or(0, |entry| entry.alternatives_evaluated);
    let dependent_action_generated = candidate.is_some_and(|_| {
        blocker_move_coordinate
            .zip(production_coordinate)
            .is_some_and(|(move_coordinate, production_coordinate)| {
                move_coordinate < production_coordinate
            })
    });
    let primary_cause = if required_candidate_evaluated && !required_candidate_selected {
        "leaf-evaluation"
    } else if !coordinate_visited && audit.coverage.exhausted_before_final_coordinate {
        "node-starved"
    } else if !coordinate_visited {
        "coordinate-order"
    } else if !alternative_generated {
        "candidate-space"
    } else if !dependent_action_generated {
        "repair-policy"
    } else if !required_candidate_evaluated {
        "coordinate-order"
    } else {
        "none"
    };
    let required_candidate_score_relative_to_selected =
        required_candidate_score.map(|score| score - audit.selected_score);
    UnblockAndProduceAudit {
        scenario: "unblock-and-produce".into(),
        allocator,
        node_budget: budget.get(),
        blocker_unit: fixture.blocker_unit.to_string(),
        producer_tile: u32::from(fixture.producer_tile.get()),
        released_tile: u32::from(fixture.released_tile.get()),
        blocker_destination: u32::from(fixture.blocker_destination.get()),
        required_production: format!("{:?}", fixture.required_production),
        blocker_move_coordinate,
        production_coordinate,
        required_coordinate,
        coordinate_visited,
        alternative_generated,
        alternative_legal_and_applied,
        alternatives_generated,
        alternatives_rejected,
        alternatives_evaluated,
        dependent_action_generated,
        required_candidate_generated,
        required_candidate_evaluated,
        required_candidate_selected,
        required_candidate_plan: required_candidate_plan
            .iter()
            .map(|order| format!("{order:?}"))
            .collect(),
        required_candidate_score,
        required_candidate_breakdown,
        required_candidate_score_relative_to_selected,
        leaf_score_relative_to_selected: required_candidate_score_relative_to_selected,
        seed_plan: audit
            .seed_plan
            .iter()
            .map(|order| format!("{order:?}"))
            .collect(),
        selected_plan: audit
            .selected_plan
            .iter()
            .map(|order| format!("{order:?}"))
            .collect(),
        seed_score: audit.seed_score,
        selected_score: audit.selected_score,
        seed_breakdown: audit.seed_breakdown,
        selected_breakdown: audit.selected_breakdown,
        primary_cause: primary_cause.into(),
    }
}

fn required_candidate<'a>(
    fixture: &UnblockAndProduceFixture,
    candidates: &'a [SearchCandidateEvaluation],
) -> Option<&'a SearchCandidateEvaluation> {
    candidates.iter().find(|candidate| {
        let blocker_move = candidate
            .plan
            .iter()
            .position(|order| *order == fixture.required_move);
        let production = candidate
            .plan
            .iter()
            .position(|order| *order == fixture.required_production);
        blocker_move.is_some_and(|blocker_move| {
            production.is_some_and(|production| blocker_move < production)
                && candidate
                    .plan
                    .last()
                    .is_some_and(|order| order.kind() == OrderKind::EndTurn)
        })
    })
}

fn make_decision(
    plan: &SearchSweepPlan,
    coverage: &SearchSweepCoverageArtifact,
    scenarios: &[UnblockAndProduceAudit],
    cells: &[SearchSweepCellReport],
) -> Result<SearchSweepDecision, SearchSweepError> {
    let reference = cells
        .iter()
        .find(|cell| {
            cell.seed_set == "evaluation"
                && cell.allocator == SearchAllocator::SequentialQuota
                && cell.node_budget == 4
        })
        .ok_or_else(|| {
            SearchSweepError::Configuration("Search sweep has no sequential-quota reference".into())
        })?;
    let mut threshold_results = BTreeMap::new();
    let mut selected = None;
    for cell in cells.iter().filter(|cell| cell.seed_set == "evaluation") {
        let key = format!("{}-{}", allocator_name(cell.allocator), cell.node_budget);
        let complete = cell
            .maps
            .iter()
            .all(|map| map.paired.complete_pairs >= plan.thresholds.minimum_complete_pairs_per_map);
        let improvement = cell.corpus_paired.pair_points - reference.corpus_paired.pair_points;
        let outcome = improvement >= plan.thresholds.minimum_material_pair_point_improvement;
        let uncertainty = cell.corpus_paired.uncertainty_half_width
            <= plan.thresholds.required_pair_level_uncertainty_bound;
        let performance = cell.corpus_paired.median_decision_nanos
            <= plan.thresholds.maximum_median_decision_nanos
            && cell.corpus_paired.p95_decision_nanos <= plan.thresholds.maximum_p95_decision_nanos;
        let allocation = plan
            .thresholds
            .maximum_allocation_regression
            .is_none_or(|limit| {
                let allowed =
                    (reference.corpus_paired.invalid_commands as f64 * (1.0 + limit)).ceil() as u64;
                cell.corpus_paired.invalid_commands <= allowed
            });
        let passed = complete && outcome && uncertainty && performance && allocation;
        threshold_results.insert(format!("{key}.complete_pairs"), complete);
        threshold_results.insert(format!("{key}.pair_improvement"), outcome);
        threshold_results.insert(format!("{key}.uncertainty"), uncertainty);
        threshold_results.insert(format!("{key}.performance"), performance);
        threshold_results.insert(format!("{key}.allocation"), allocation);
        threshold_results.insert(key, passed);
        if passed && selected.is_none_or(|(_, budget)| cell.node_budget > budget) {
            selected = Some((cell.allocator, cell.node_budget));
        }
    }
    let (selected_allocator, selected_node_budget, decision, next) = match selected {
        Some((allocator, budget)) => (
            Some(allocator),
            Some(budget),
            "accept".into(),
            "Measure the accepted search baseline against each strategic feature.".into(),
        ),
        None => (
            None,
            None,
            "revise".into(),
            "Inspect the scenario owner before adding strategic features.".into(),
        ),
    };
    let first = cells
        .first()
        .ok_or_else(|| SearchSweepError::Configuration("Search sweep has no cells".into()))?;
    Ok(SearchSweepDecision {
        schema_version: SEARCH_SWEEP_ARTIFACT_SCHEMA_VERSION,
        plan_fingerprint: coverage.plan_fingerprint.clone(),
        source_revision: first.source_revision.clone(),
        dirty_worktree: first.dirty_worktree,
        configuration_fingerprints: cells
            .iter()
            .map(|cell| cell.configuration_fingerprint.clone())
            .collect(),
        executable_fingerprints: cells
            .iter()
            .map(|cell| cell.executable_fingerprint.clone())
            .collect(),
        thresholds: plan.thresholds.clone(),
        threshold_results,
        selected_allocator,
        selected_node_budget,
        retained_allocator: SearchAllocator::SequentialQuota,
        retained_node_budget: NodeBudget::FOUR.get(),
        summaries: cells.iter().map(cell_summary).collect(),
        search_coverage_decision: decision,
        scenario_audits: scenarios.to_vec(),
        tuning_run_seeds: plan.tuning_run_seeds.clone(),
        evaluation_run_seeds: plan.evaluation_run_seeds.clone(),
        next_experiment: next,
    })
}

fn apply_reference_deltas(cells: &mut [SearchSweepCellReport]) {
    let Some(reference) = cells.iter().find(|cell| {
        cell.seed_set == "evaluation"
            && cell.allocator == SearchAllocator::SequentialQuota
            && cell.node_budget == 4
    }) else {
        return;
    };
    let reference_corpus = reference.corpus_paired.pair_points;
    let reference_maps = reference
        .maps
        .iter()
        .map(|map| (map.map_id, map.paired.pair_points))
        .collect::<BTreeMap<_, _>>();
    for cell in cells {
        cell.corpus_paired.pair_point_delta_from_reference =
            cell.corpus_paired.pair_points - reference_corpus;
        for map in &mut cell.maps {
            map.paired.pair_point_delta_from_reference = map.paired.pair_points
                - reference_maps.get(&map.map_id).copied().unwrap_or_default();
        }
    }
}

fn render_decision(
    decision: &SearchSweepDecision,
    coverage: &SearchSweepCoverageArtifact,
) -> String {
    let mut text = String::new();
    text.push_str("# Search sweep decision\n\n");
    text.push_str(&format!(
        "Decision: `{}`\n\n",
        decision.search_coverage_decision
    ));
    text.push_str("## Selected configuration\n\n");
    text.push_str(&format!(
        "Allocator: `{}`\n\nNode budget: `{}`\n\n",
        decision
            .selected_allocator
            .map(allocator_name)
            .unwrap_or("none"),
        decision
            .selected_node_budget
            .map_or_else(|| "none".into(), |budget| budget.to_string())
    ));
    text.push_str("## Matrix\n\n");
    text.push_str("| Seed set | Allocator | Budget | Coverage | Late coverage | W/D/L | Pair points | Uncertainty | Median ns | p95 ns |\n|---|---|---:|---:|---:|---:|---:|---:|---:|---:|\n");
    for cell in &coverage.cells {
        text.push_str(&format!(
            "| {} | {} | {} | {:.4} | {:.4} | {}/{}/{} | {:.4} | {:.4} | {} | {} |\n",
            cell.seed_set,
            allocator_name(cell.allocator),
            cell.node_budget,
            cell.corpus_coverage.coordinate_coverage,
            cell.corpus_coverage.late_coordinate_coverage,
            cell.corpus_paired.wins,
            cell.corpus_paired.draws,
            cell.corpus_paired.losses,
            cell.corpus_paired.pair_points,
            cell.corpus_paired.uncertainty_half_width,
            cell.corpus_paired.median_decision_nanos,
            cell.corpus_paired.p95_decision_nanos
        ));
    }
    text.push_str("\n## Next experiment\n\n");
    text.push_str(&decision.next_experiment);
    text.push('\n');
    text
}

fn cell_summary(cell: &SearchSweepCellReport) -> SearchSweepCellSummary {
    let counters = &cell.corpus_coverage.counters.coverage;
    SearchSweepCellSummary {
        seed_set: cell.seed_set.clone(),
        allocator: cell.allocator,
        node_budget: cell.node_budget,
        coverage: SearchSweepCoverageSummary {
            coordinate_coverage: cell.corpus_coverage.coordinate_coverage,
            late_coordinate_coverage: cell.corpus_coverage.late_coordinate_coverage,
            budget_exhaustion_rate: cell.corpus_coverage.budget_exhaustion_rate,
            seed_change_rate: cell.corpus_coverage.seed_change_rate,
            decisions: counters.decisions,
            nodes_requested: counters.nodes_requested,
            nodes_used: counters.nodes_used,
        },
        paired: cell.corpus_paired.clone(),
        performance: cell.performance.clone(),
    }
}

fn allocator_name(allocator: SearchAllocator) -> &'static str {
    match allocator {
        SearchAllocator::SequentialQuota => "sequential-quota",
        SearchAllocator::RoundRobin => "round-robin",
    }
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    ratio_f64(numerator, denominator)
}

fn ratio_f64(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn normal_half_width(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let average = mean(values);
    let variance = values
        .iter()
        .map(|value| (value - average).powi(2))
        .sum::<f64>()
        / (values.len() - 1) as f64;
    1.96 * (variance / values.len() as f64).sqrt()
}

fn percentile(values: &[u64], percentile: usize) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let mut values = values.to_vec();
    values.sort_unstable();
    let index = ((values.len() * percentile).div_ceil(100)).saturating_sub(1);
    values[index.min(values.len() - 1)]
}

fn write_json(path: impl AsRef<Path>, value: &impl Serialize) -> Result<(), SearchSweepError> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_sweep_plan_requires_disjoint_seed_sets() {
        let mut plan = SearchSweepPlan {
            schema_version: SEARCH_SWEEP_PLAN_SCHEMA_VERSION,
            run_id: "search-sweep".into(),
            maps: vec![1],
            tuning_run_seeds: vec![1],
            evaluation_run_seeds: vec![1],
            pairs_per_map: 1,
            limits: RunLimits {
                day_limit: 1,
                node_budget: 1,
                refusal_limit: 1,
            },
            thresholds: SearchSweepThresholds {
                minimum_complete_pairs_per_map: 1,
                minimum_material_pair_point_improvement: 0.1,
                required_pair_level_uncertainty_bound: 0.5,
                maximum_median_decision_nanos: 1,
                maximum_p95_decision_nanos: 1,
                maximum_allocation_regression: None,
            },
            annotations: None,
        };
        assert!(plan.validate().is_err());
        plan.evaluation_run_seeds = vec![2];
        plan.validate().expect("disjoint seed sets validate");
    }

    #[test]
    fn percentile_is_deterministic() {
        assert_eq!(percentile(&[9, 1, 5], 50), 5);
        assert_eq!(percentile(&[9, 1, 5], 95), 9);
    }

    #[test]
    fn coverage_rates_use_equal_units() {
        let mut stats = SearchStats::default();
        stats.coverage.searchable_coordinates = 4;
        stats.coverage.visited_searchable_coordinates = 2;
        stats.coverage.final_quartile_searchable_coordinates = 2;
        stats.coverage.visited_final_quartile_coordinates = 1;
        stats.coverage.decisions = 2;
        stats.coverage.decisions_exhausted_before_final_coordinate = 1;
        stats.coverage.seed_plans = 2;
        stats.coverage.changed_seed_plans = 1;
        let rates = CoverageRates::from_stats(stats);
        assert_eq!(rates.coordinate_coverage, 0.5);
        assert_eq!(rates.late_coordinate_coverage, 0.5);
        assert_eq!(rates.budget_exhaustion_rate, 0.5);
        assert_eq!(rates.seed_change_rate, 0.5);
    }

    #[test]
    fn audit_unblock_and_produce_is_replayable_and_classifies_one_primary_cause() {
        let first = unblock_and_produce_audits();
        let second = unblock_and_produce_audits();
        assert_eq!(first, second);
        assert_eq!(first.len(), 8);
        assert!(first.iter().all(|report| {
            matches!(
                report.primary_cause.as_str(),
                "node-starved"
                    | "coordinate-order"
                    | "candidate-space"
                    | "repair-policy"
                    | "leaf-evaluation"
                    | "none"
            )
        }));
        assert!(first.iter().all(|report| {
            report.blocker_unit == "1"
                && report.producer_tile == 82
                && report.released_tile == 82
                && report.blocker_destination == 41
                && report.required_production.contains("Produce(Infantry)")
                && report.required_candidate_generated
                && report.required_candidate_evaluated
                && report.dependent_action_generated
                && report.blocker_move_coordinate == Some(1)
                && report.production_coordinate == Some(2)
                && report.required_coordinate == Some(1)
                && report.required_candidate_plan.len() == 4
                && report.required_candidate_breakdown.is_some()
                && !report.required_candidate_selected
                && report.primary_cause == "leaf-evaluation"
                && report
                    .required_candidate_score_relative_to_selected
                    .is_some_and(|delta| delta < 0.0)
        }));
    }
}
