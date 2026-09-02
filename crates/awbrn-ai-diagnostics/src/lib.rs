//! Opt-in rendering and offline review tools for headless AI runs.

pub mod capture;
pub mod events;
pub mod feature_analysis;
pub mod learned;
pub mod manifest;
pub mod map_registry;
pub mod pipeline;
pub mod plan;
pub mod review;
pub mod search_sweep;
pub mod tactical;
pub mod tournament;
pub mod verify;

pub use capture::{VisualCapture, VisualCaptureIdentity};
pub use events::{
    EventKind, EventLogError, EventLogWriter, EventMetadata, EventRow, MatchEventRow,
    ReanalysisSummary, observations_from_event_log, read_event_log, reanalyse_event_log,
    reanalyse_event_log_with_manifest, row_for_state, verify_expected_fingerprints,
    write_derived_outputs,
};
pub use feature_analysis::{
    AblationReport, CollinearityReport, CrossValidationReport, DatasetMetrics, FEATURE_NAMES,
    FeatureAnalysisError, FeatureAnalysisReport, FeatureAnalysisSummary, FeatureExtraction,
    FeatureMode, FeatureRow, FeatureVector, FeatureWeight, MapTurnRangeReport, MetricSummary,
    ModeAnalysisReport, ModelReport, PairMetric, ReducedEvaluator, TurnRange, TurnRangeReport,
    analyze_event_log, extract_feature_rows, fit_feature_analysis, observable_features,
};
pub use learned::{LEARNED_EXECUTABLE_FINGERPRINT, LearnedFactory};
pub use manifest::{
    ManifestError, read_manifest, resolve_event_log_path, write_manifest,
    write_or_validate_manifest,
};
pub use map_registry::{
    CANONICAL_SEATS, MapManifest, MapManifestEntry, MapRegistry, MapRegistryError, RegisteredMap,
};
pub use pipeline::{DiagnosticError, DiagnosticSummary, PlanRunSummary, run_diagnostic, run_plan};
pub use plan::{
    AgentSpec, AnalysisStage, EXPERIMENT_PLAN_SCHEMA_VERSION, ExperimentPlan, MaterializedPlan,
    PlanError, TacticalMode, read_plan,
};
pub use review::{ReviewError, ReviewSummary, run_review, run_review_with_tilesets};
pub use search_sweep::{
    CoverageRates, PairedResult, SEARCH_SWEEP_ARTIFACT_SCHEMA_VERSION, SEARCH_SWEEP_BUDGETS,
    SEARCH_SWEEP_PLAN_SCHEMA_VERSION, SearchSweepCellReport, SearchSweepCellSummary,
    SearchSweepCoverageArtifact, SearchSweepCoverageSummary, SearchSweepDecision, SearchSweepError,
    SearchSweepMapReport, SearchSweepPerformance, SearchSweepPlan, SearchSweepSummary,
    SearchSweepThresholds, UnblockAndProduceAudit, read_search_sweep_plan, run_search_sweep,
};
pub use tactical::{
    TACTICAL_EXECUTABLE_FINGERPRINT, TacticalFactory, TacticalRerank, TacticalRerankMode,
};
pub use tournament::{
    AgentFactory, MatchPerformance, SEARCH_COVERAGE_SCHEMA_VERSION, SEARCH_EXECUTABLE_FINGERPRINT,
    STRATEGIC_EXECUTABLE_FINGERPRINT, SearchCoverageArtifact, SearchCoverageMatch, SearchFactory,
    StrategicFactory, TournamentError, TournamentPerformance, TournamentSummary, run_manifest,
    run_paired_tournament,
};
pub use verify::{VerificationSummary, VerifyError, verify_artifact};
