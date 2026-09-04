use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use awbrn_ai::{EvalWeights, NodeBudget, baseline::BaselineConfig};
use awbrn_ai_diagnostic_types::{FramePolicy, Invalidation, RunManifest, fingerprint_bytes};
use awbrn_ai_diagnostics::{
    AgentFactory, AgentSpec, AnalysisStage, ExperimentPlan, MapRegistry, PlanError,
    ProducerUsabilityPlan, SearchFactory, StrategicFactory, TacticalMode, TournamentError,
    command_stream_fingerprint, event_stream_fingerprint, extract_feature_rows, read_event_log,
    read_manifest, read_plan, reanalyse_event_log_with_manifest, run_diagnostic,
    run_paired_tournament, run_plan, run_producer_usability_diagnostics_from_manifest,
    run_review_with_tilesets, verify_artifact, verify_expected_fingerprints, write_manifest,
    write_or_validate_manifest,
};
use serde_json::json;

fn temporary_directory() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "awbrn-ai-diagnostics-pipeline-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&path).expect("the test directory is unused");
    path
}

fn search_candidate() -> SearchFactory {
    SearchFactory::new(
        "search-production-v1",
        BaselineConfig::PRODUCTION.weights,
        EvalWeights::STANDARD,
        NodeBudget::SIXTEEN,
    )
}

fn experiment_plan(run_id: &str, candidate: AgentSpec) -> ExperimentPlan {
    ExperimentPlan {
        schema_version: awbrn_ai_diagnostics::EXPERIMENT_PLAN_SCHEMA_VERSION,
        run_id: run_id.into(),
        candidate,
        baseline: AgentSpec::Strategic {
            configuration: "locked".into(),
        },
        maps: vec![61748],
        run_seed: 1,
        pairs_per_map: 1,
        limits: awbrn_ai_diagnostic_types::RunLimits {
            day_limit: 1,
            node_budget: 1,
            refusal_limit: 1,
        },
        telemetry: awbrn_ai_diagnostic_types::TelemetryMode::Enabled,
        capture_policy: Default::default(),
        analyses: Vec::new(),
        producer_usability: None,
        annotations: None,
    }
}

#[test]
fn search_candidate_identity_is_stable() {
    assert_eq!(
        search_candidate().identity().configuration_fingerprint,
        "1fa0910ff8a578c3"
    );
}

#[test]
fn the_raw_event_log_rebuilds_match_and_reduction_outputs() {
    let root = temporary_directory();
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/ai-diagnostics/smoke-manifest.json");
    let manifest: RunManifest = read_manifest(&manifest_path).expect("the fixture manifest loads");
    let registry = MapRegistry::load_checked_in().expect("the fixed maps load");
    let candidate = StrategicFactory::new(BaselineConfig::PRODUCTION);
    let baseline = StrategicFactory::new(BaselineConfig::LOCKED);
    let run = root.join("run");
    run_paired_tournament(&manifest, &registry, &candidate, &baseline, &run)
        .expect("the paired run completes");
    let first_performance =
        fs::read(run.join("performance.json")).expect("the first performance output exists");
    let first_performance: awbrn_ai_diagnostics::TournamentPerformance =
        serde_json::from_slice(&first_performance).expect("the first performance output reads");
    let first_events = fs::read(run.join("events.jsonl")).expect("the event log exists");
    let first_matches = fs::read(run.join("matches.jsonl")).expect("the match rows exist");
    fs::remove_file(run.join("matches.jsonl")).expect("the disposable match rows remove");
    let resumed = run_paired_tournament(&manifest, &registry, &candidate, &baseline, &run)
        .expect("the paired run resumes without replaying complete matches");
    assert_eq!(resumed.performance.matches, first_performance.matches);
    assert_eq!(
        resumed.performance.total_commands,
        first_performance.total_commands
    );
    assert_eq!(
        resumed.performance.match_records,
        first_performance.match_records
    );
    assert!(resumed.performance.wall_clock_nanos >= first_performance.wall_clock_nanos);
    assert_eq!(
        first_events,
        fs::read(run.join("events.jsonl")).expect("the resumed event log exists")
    );
    assert_eq!(
        first_matches,
        fs::read(run.join("matches.jsonl")).expect("the match rows are rebuilt")
    );
    let events = read_event_log(run.join("events.jsonl")).expect("the event log reads");
    let features = extract_feature_rows(&events).expect("turn features extract");
    assert_eq!(features.matches, 2);
    assert_eq!(features.matches_with_rows, 2);
    assert!(!features.rows.is_empty());
    assert!(features.rows.iter().all(|row| row.turn_index > 0));
    assert!(
        features
            .rows
            .iter()
            .all(|row| row.active_seat != row.just_acted_seat)
    );
    let mut expected = manifest.clone();
    expected.expected.event_log = Some(fingerprint_bytes(&first_events));
    expected.expected.command = events
        .iter()
        .filter(|row| row.event_kind == awbrn_ai_diagnostics::EventKind::Terminal)
        .map(|row| {
            (
                row.match_id.clone(),
                format!("{:016x}", row.command_fingerprint),
            )
        })
        .collect();
    expected.expected.derived_tables.insert(format!(
        "matches.jsonl={}",
        fingerprint_bytes(&first_matches)
    ));
    verify_expected_fingerprints(&expected, run.join("events.jsonl"), &run)
        .expect("declared fingerprints are enforced");
    let recorded_commands = expected.expected.command.clone();
    expected.expected.command = [("not-a-recorded-match".into(), "0000000000000000".into())]
        .into_iter()
        .collect();
    assert!(verify_expected_fingerprints(&expected, run.join("events.jsonl"), &run).is_err());
    expected.expected.command = recorded_commands;
    expected.expected.event_log = Some("wrong-event-log".into());
    assert!(verify_expected_fingerprints(&expected, run.join("events.jsonl"), &run).is_err());
    let reanalysis = root.join("reanalysis");
    reanalyse_event_log_with_manifest(run.join("events.jsonl"), &reanalysis, &manifest)
        .expect("the event log reanalyses");
    for name in [
        "matches.jsonl",
        "reduction.json",
        "reduction.csv",
        "commands.csv",
        "states.jsonl",
        "summary.json",
    ] {
        assert_eq!(
            fs::read(run.join(name)).expect("the run output exists"),
            fs::read(reanalysis.join(name)).expect("the reanalysis output exists"),
            "{name} is reproducible from the raw event log"
        );
    }
    let review = root.join("review");
    // Synthetic atlases: this checks that the review renders and links the
    // frames it selected, not how they look, so it does not need the
    // generated `assets/textures` files.
    let review_summary = run_review_with_tilesets(
        run.join("manifest.json"),
        &review,
        awbrn_image::fixtures::tilesets(),
    )
    .expect("the event log produces an offline review");
    assert!(review_summary.frames > 0);
    let index = fs::read_to_string(review.join("index.html")).expect("the review index exists");
    assert!(index.contains("Captured frames"));
    assert!(index.contains("frame-0000-start.png"));
    fs::remove_dir_all(root).expect("the test directory removes");
}

#[test]
fn an_experiment_plan_resolves_a_candidate_and_materializes_coverage() {
    let registry = MapRegistry::load_checked_in().expect("the fixed maps load");
    let mut plan = experiment_plan(
        "plan-test",
        AgentSpec::Search {
            identifier: "search-test".into(),
            preset: "production".into(),
            node_budget: 1,
        },
    );
    plan.run_seed = 7;
    plan.pairs_per_map = 2;
    let materialized = plan
        .materialize("assets/ai-diagnostics/plan.json", &registry)
        .expect("the experiment plan materializes");
    assert_eq!(materialized.manifest.maps.len(), 1);
    assert_eq!(materialized.manifest.pairs.len(), 2);
    assert!(!materialized.manifest.source_revision.is_empty());
    assert!(!materialized.manifest.source_fingerprint.is_empty());
    assert_eq!(materialized.candidate.identity().identifier, "search-test");
    assert_eq!(
        materialized.baseline.identity().identifier,
        "greedy-baseline-v1"
    );
}

#[test]
fn plan_round_trip_and_diagnostics_only_tactical_identity_are_stable() {
    let mut plan = experiment_plan(
        "tactical-plan-test",
        AgentSpec::TacticalRerank {
            identifier: "tactical-test".into(),
            configuration: "locked".into(),
            top_k: 3,
            mode: TacticalMode::Collateral,
            penalty_percent: 100,
        },
    );
    plan.run_seed = 8;
    let bytes = serde_json::to_vec(&plan).expect("the plan serializes");
    assert_eq!(
        serde_json::from_slice::<ExperimentPlan>(&bytes).unwrap(),
        plan
    );
    let registry = MapRegistry::load_checked_in().expect("the fixed maps load");
    let materialized = plan
        .materialize("assets/ai-diagnostics/plan.json", &registry)
        .expect("the tactical plan materializes");
    assert_eq!(
        materialized.candidate.identity().executable_fingerprint,
        awbrn_ai_diagnostics::TACTICAL_EXECUTABLE_FINGERPRINT
    );
    assert_ne!(
        materialized.candidate.identity().configuration_fingerprint,
        materialized.baseline.identity().configuration_fingerprint
    );
}

#[test]
fn plan_rejects_unsafe_model_paths_before_file_access() {
    let plan = experiment_plan(
        "unsafe-model-plan",
        AgentSpec::LearnedRerank {
            model: PathBuf::from("../model.json"),
            baseline_configuration: "locked".into(),
            top_k: 1,
        },
    );
    let registry = MapRegistry::load_checked_in().expect("the fixed maps load");
    let error = plan
        .materialize("assets/ai-diagnostics/plan.json", &registry)
        .expect_err("unsafe model paths are refused");
    assert!(matches!(error, PlanError::Configuration(message) if message.contains("safe")));
}

#[test]
fn plan_rejects_removed_provenance_overrides() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/ai-diagnostics/smoke-plan.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(path).expect("the smoke plan reads"))
            .expect("the smoke plan JSON reads");
    value["source_revision"] = json!("override");
    value["dirty_worktree"] = json!(false);
    let error = serde_json::from_value::<ExperimentPlan>(value)
        .expect_err("removed provenance overrides are refused");
    assert!(error.to_string().contains("unknown field"));
}

#[test]
fn plan_rejects_a_learned_model_with_an_insufficient_corpus() {
    let root = temporary_directory();
    let model_path = root.join("model.json");
    let metric = json!({
        "rows": 1,
        "groups": 1,
        "log_loss": 0.5,
        "brier_score": 0.25,
        "accuracy": 1.0,
        "baseline_probability": 0.5,
        "baseline_log_loss": 0.693,
        "baseline_brier_score": 0.25,
        "baseline_accuracy": 0.5
    });
    let summary = json!({
        "samples": 1,
        "mean": 0.5,
        "stddev": 0.0,
        "ci95_low": 0.5,
        "ci95_high": 0.5
    });
    let model = json!({
        "feature_names": awbrn_ai_diagnostics::FEATURE_NAMES,
        "intercept": 0.0,
        "weights": [],
        "reduced_intercept": 0.0,
        "reduced_weights": [{
            "name": "turn_index",
            "coefficient": 0.0,
            "odds_ratio": 1.0,
            "selected": true
        }],
        "l2_penalty": 0.1,
        "iterations": 1,
        "converged": true,
        "reduced_converged": true,
        "fit_metrics": metric,
        "cross_validation": {
            "repeats": 1,
            "requested_folds": 2,
            "folds": 2,
            "evaluations": 2,
            "rows": 1,
            "groups": 1,
            "log_loss": summary,
            "brier_score": summary,
            "accuracy": summary,
            "baseline_log_loss": summary,
            "baseline_brier_score": summary,
            "baseline_accuracy": summary
        },
        "full_cross_validation": null,
        "selection_rule": "test"
    });
    let report = json!({
        "schema_version": awbrn_ai_diagnostics::feature_analysis::FEATURE_ANALYSIS_SCHEMA_VERSION,
        "event_rows": 1,
        "matches": 1,
        "matches_with_rows": 1,
        "skipped_draws": 0,
        "skipped_incomplete": 0,
        "rows": 1,
        "minimum_matches": 100,
        "sufficient_corpus": false,
        "corpus_fingerprint": "corpus",
        "modes": [{
            "mode": "fog-visible",
            "rows": 1,
            "matches": 1,
            "validation_groups": 1,
            "model": model,
            "ablations": [],
            "turn_ranges": [],
            "map_turn_ranges": [],
            "collinearity": []
        }]
    });
    fs::write(
        &model_path,
        serde_json::to_vec(&report).expect("the learned report serializes"),
    )
    .expect("the learned report writes");
    let mut plan = experiment_plan(
        "insufficient-learned-plan",
        AgentSpec::LearnedRerank {
            model: PathBuf::from("model.json"),
            baseline_configuration: "locked".into(),
            top_k: 1,
        },
    );
    plan.run_seed = 10;
    let registry = MapRegistry::load_checked_in().expect("the fixed maps load");
    let error = plan
        .materialize(root.join("plan.json"), &registry)
        .expect_err("an insufficient learned corpus is refused");
    assert!(error.to_string().contains("corpus is insufficient"));
    fs::remove_dir_all(root).expect("the test directory removes");
}

#[test]
fn the_checked_in_search_plan_is_runnable() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/ai-diagnostics/search-production-multimap-plan.json");
    let plan = read_plan(&path).expect("the checked-in experiment plan reads");
    let registry = MapRegistry::load_checked_in().expect("the fixed maps load");
    let materialized = plan
        .materialize(&path, &registry)
        .expect("the checked-in experiment plan materializes");
    assert_eq!(materialized.manifest.maps.len(), 4);
    assert_eq!(materialized.manifest.pairs.len(), 52);
    assert_eq!(materialized.analyses.len(), 3);
}

#[test]
fn the_checked_in_smoke_plan_runs_the_full_pipeline() {
    let plan_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/ai-diagnostics/smoke-plan.json");
    let output = temporary_directory();
    let summary = run_plan(&plan_path, &output).expect("the checked-in smoke plan runs");
    assert_eq!(summary.tournament.performance.matches, 4);
    assert!(summary.feature_analysis.is_some());
    assert!(summary.review.is_some());
    assert!(summary.verification.is_some());
    fs::remove_dir_all(output).expect("the smoke output removes");
}

#[test]
fn a_changed_plan_cannot_resume_an_existing_manifest() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/ai-diagnostics/smoke-plan.json");
    let mut plan = read_plan(&source).expect("the smoke plan reads");
    let root = temporary_directory();
    let plan_path = root.join("plan.json");
    fs::write(
        &plan_path,
        serde_json::to_vec_pretty(&plan).expect("the plan serializes"),
    )
    .expect("the plan writes");
    let output = root.join("run");
    run_plan(&plan_path, &output).expect("the initial plan runs");
    let manifest = fs::read(output.join("manifest.json")).expect("the manifest reads");

    plan.run_seed += 1;
    fs::write(
        &plan_path,
        serde_json::to_vec_pretty(&plan).expect("the changed plan serializes"),
    )
    .expect("the changed plan writes");
    let error = run_plan(&plan_path, &output).expect_err("a changed plan cannot resume");
    assert!(error.to_string().contains("existing manifest"));
    assert_eq!(manifest, fs::read(output.join("manifest.json")).unwrap());
    fs::remove_dir_all(root).expect("the changed-plan output removes");
}

#[test]
fn a_changed_source_fingerprint_cannot_resume_an_existing_manifest() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/ai-diagnostics/smoke-manifest.json");
    let manifest: RunManifest = read_manifest(&source).expect("the smoke manifest reads");
    let root = temporary_directory();
    let path = root.join("manifest.json");
    write_manifest(&manifest, &path).expect("the manifest writes");

    let mut changed = manifest;
    changed.source_fingerprint = "changed-source".into();
    let error = write_or_validate_manifest(&changed, &path)
        .expect_err("a changed source fingerprint cannot resume");
    assert!(error.to_string().contains("source fingerprint"));
    fs::remove_dir_all(root).expect("the test directory removes");
}

#[test]
fn the_generic_plan_runs_requested_analysis_stages() {
    let root = temporary_directory();
    let mut plan = experiment_plan(
        "generic-pipeline-test",
        AgentSpec::Strategic {
            configuration: "production".into(),
        },
    );
    plan.run_seed = 402;
    plan.pairs_per_map = 2;
    plan.limits = awbrn_ai_diagnostic_types::RunLimits {
        day_limit: 35,
        node_budget: 4,
        refusal_limit: 64,
    };
    plan.analyses = vec![
        AnalysisStage::OutcomeFeatures,
        AnalysisStage::Review,
        AnalysisStage::Verification,
    ];
    let plan_path = root.join("plan.json");
    fs::write(
        &plan_path,
        serde_json::to_vec_pretty(&plan).expect("the test plan serializes"),
    )
    .expect("the test plan writes");
    let output = root.join("run");
    let summary = run_plan(&plan_path, &output).expect("the generic plan runs");
    assert!(summary.feature_analysis.is_some());
    assert!(summary.review.is_some());
    assert!(summary.verification.is_some());
    assert!(
        output
            .join("feature-analysis/feature-analysis.json")
            .exists()
    );
    fs::remove_dir_all(root).expect("the test directory removes");
}

#[test]
fn producer_stage_keeps_gameplay_neutral_against_a_disabled_control() {
    let root = temporary_directory();
    let mut enabled = experiment_plan(
        "producer-enabled",
        AgentSpec::Strategic {
            configuration: "production".into(),
        },
    );
    enabled.baseline = enabled.candidate.clone();
    enabled.analyses = vec![AnalysisStage::ProducerUsability];
    enabled.producer_usability = Some(ProducerUsabilityPlan::default());
    if let Some(plan) = enabled.producer_usability.as_mut() {
        plan.performance_fixtures = vec!["late-game".into(), "arena".into(), "amber-valley".into()];
        plan.thresholds.maximum_median_relative_change = 0.07;
    }
    let expected_producer_plan = enabled.producer_usability.clone().unwrap();
    assert_eq!(
        expected_producer_plan.experiment_id,
        awbrn_ai_diagnostics::PRODUCER_EXPERIMENT_ID
    );
    let enabled_plan = root.join("enabled-plan.json");
    fs::write(
        &enabled_plan,
        serde_json::to_vec_pretty(&enabled).expect("the enabled plan serializes"),
    )
    .expect("the enabled plan writes");
    let enabled_output = root.join("enabled");
    let enabled_summary = run_plan(&enabled_plan, &enabled_output).expect("the enabled plan runs");
    assert!(enabled_summary.producer_usability.is_some());
    let enabled_manifest =
        read_manifest(enabled_output.join("manifest.json")).expect("the enabled manifest reads");
    assert_eq!(
        enabled_manifest.producer_usability_plan,
        Some(serde_json::to_value(&expected_producer_plan).expect("the producer plan serializes"))
    );
    assert_eq!(
        enabled_manifest.experiment_plan_fingerprint,
        fingerprint_bytes(&serde_json::to_vec(&enabled).expect("the enabled plan serializes"))
    );
    let reanalysed = run_producer_usability_diagnostics_from_manifest(
        &enabled_manifest,
        enabled_output.join("events.jsonl"),
        &enabled_output,
    )
    .expect("the materialized producer plan reanalyses");
    assert_eq!(
        reanalysed.decision.thresholds,
        expected_producer_plan.thresholds
    );
    assert_eq!(
        reanalysed.decision.experiment_id,
        awbrn_ai_diagnostics::PRODUCER_EXPERIMENT_ID
    );

    let mut disabled = enabled;
    disabled.run_id = "producer-disabled".into();
    disabled.analyses.clear();
    disabled.producer_usability = None;
    let disabled_plan = root.join("disabled-plan.json");
    fs::write(
        &disabled_plan,
        serde_json::to_vec_pretty(&disabled).expect("the disabled plan serializes"),
    )
    .expect("the disabled plan writes");
    let disabled_output = root.join("disabled");
    run_plan(&disabled_plan, &disabled_output).expect("the disabled plan runs");

    let enabled_events =
        read_event_log(enabled_output.join("events.jsonl")).expect("the enabled event log reads");
    let disabled_events =
        read_event_log(disabled_output.join("events.jsonl")).expect("the disabled event log reads");
    assert_eq!(
        command_stream_fingerprint(&enabled_events),
        command_stream_fingerprint(&disabled_events)
    );
    assert_eq!(
        event_stream_fingerprint(&enabled_events),
        event_stream_fingerprint(&disabled_events)
    );
    let behavior: awbrn_ai_diagnostics::ProducerUsabilityBehaviorArtifact = serde_json::from_slice(
        &fs::read(enabled_output.join("producer-usability-behavior.json"))
            .expect("the behavior artifact exists"),
    )
    .expect("the behavior artifact reads");
    assert!(behavior.passed);
    fs::remove_dir_all(root).expect("the producer outputs remove");
}

#[test]
fn agent_identity_comes_from_the_factory_configuration() {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/ai-diagnostics/smoke-manifest.json");
    let manifest: RunManifest = read_manifest(&manifest_path).expect("the fixture manifest loads");
    let registry = MapRegistry::load_checked_in().expect("the fixed maps load");
    let candidate = StrategicFactory::new(BaselineConfig::LOCKED);
    let baseline = StrategicFactory::new(BaselineConfig::LOCKED);
    let output = temporary_directory().join("wrong-agent");
    let error = run_paired_tournament(&manifest, &registry, &candidate, &baseline, &output)
        .expect_err("a factory with the wrong configuration is refused");
    assert!(matches!(error, TournamentError::Configuration(_)));
    fs::remove_dir_all(output.parent().unwrap()).expect("the test directory removes");
}

#[test]
fn the_diagnostic_command_runs_and_verifies_the_complete_pipeline() {
    let root = temporary_directory();
    let source_manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/ai-diagnostics/smoke-manifest.json");
    let mut manifest: RunManifest =
        read_manifest(&source_manifest).expect("the fixture manifest loads");
    manifest.capture_policy.frame_policy = FramePolicy::Disabled;
    let manifest_path = root.join("manifest.json");
    write_manifest(&manifest, &manifest_path).expect("the test manifest writes");
    let output = root.join("experiment");
    let summary =
        run_diagnostic(&manifest_path, &output).expect("the diagnostic pipeline completes");
    assert_eq!(
        summary.tournament.reduction.status,
        awbrn_ai_diagnostic_types::ReductionStatus::Complete
    );
    assert_eq!(summary.review.frames, 0);
    let verification = verify_artifact(output.join("manifest.json"), &output)
        .expect("the complete artifact verifies");
    assert_eq!(verification.matches, 2);
    fs::remove_dir_all(root).expect("the test directory removes");
}

#[test]
#[ignore = "requires generated rendering assets"]
fn the_diagnostic_command_renders_selected_frames() {
    let root = temporary_directory();
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/ai-diagnostics/smoke-manifest.json");
    let output = root.join("experiment");
    let summary = run_diagnostic(&manifest, &output).expect("the diagnostic pipeline completes");
    assert!(summary.review.frames > 0);
    let index = summary.review.output.join("index.html");
    let index = fs::read_to_string(index).expect("the review index exists");
    assert!(index.contains("frame-0000-start.png"));
    fs::remove_dir_all(root).expect("the test directory removes");
}

/// A process stopped mid-append leaves half a row, which the next append must
/// not write behind.
#[test]
fn a_half_written_event_row_is_dropped_before_resume() {
    let root = temporary_directory();
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/ai-diagnostics/smoke-manifest.json");
    let manifest: RunManifest = read_manifest(&manifest_path).expect("the fixture manifest loads");
    let registry = MapRegistry::load_checked_in().expect("the fixed maps load");
    let candidate = StrategicFactory::new(BaselineConfig::PRODUCTION);
    let baseline = StrategicFactory::new(BaselineConfig::LOCKED);
    let run = root.join("run");
    run_paired_tournament(&manifest, &registry, &candidate, &baseline, &run)
        .expect("the source run completes");
    let events = fs::read_to_string(run.join("events.jsonl")).expect("the source event log exists");
    let (first, rest) = events.split_once('\n').expect("the log holds several rows");
    let partial = format!("{first}\n{}", &rest[..rest.len().min(64)]);
    assert!(!partial.ends_with('\n'), "the log ends mid-row");
    fs::write(run.join("events.jsonl"), partial).expect("the partial event log writes");

    let resumed = run_paired_tournament(&manifest, &registry, &candidate, &baseline, &run)
        .expect("the interrupted run resumes");
    assert_eq!(
        resumed.reduction.status,
        awbrn_ai_diagnostic_types::ReductionStatus::Complete
    );
    // Reading validates every row and its sequence, so a row appended behind
    // the half-written one would stop this.
    let rows = read_event_log(run.join("events.jsonl")).expect("the resumed log reads");
    assert!(rows.len() > 1);
    fs::remove_dir_all(root).expect("the test directory removes");
}

#[test]
fn an_interrupted_attempt_is_invalidated_before_resume() {
    let root = temporary_directory();
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/ai-diagnostics/smoke-manifest.json");
    let manifest: RunManifest = read_manifest(&manifest_path).expect("the fixture manifest loads");
    let registry = MapRegistry::load_checked_in().expect("the fixed maps load");
    let candidate = StrategicFactory::new(BaselineConfig::PRODUCTION);
    let baseline = StrategicFactory::new(BaselineConfig::LOCKED);
    let run = root.join("run");
    run_paired_tournament(&manifest, &registry, &candidate, &baseline, &run)
        .expect("the source run completes");
    let first_event = fs::read_to_string(run.join("events.jsonl"))
        .expect("the source event log exists")
        .split_once('\n')
        .map_or_else(String::new, |(line, _)| format!("{line}\n"));
    fs::write(run.join("events.jsonl"), first_event).expect("the partial event log writes");

    let resumed = run_paired_tournament(&manifest, &registry, &candidate, &baseline, &run)
        .expect("the interrupted run resumes");
    assert_eq!(
        resumed.reduction.status,
        awbrn_ai_diagnostic_types::ReductionStatus::Complete
    );
    let events = read_event_log(run.join("events.jsonl")).expect("the resumed log reads");
    assert!(events.iter().any(|row| {
        row.event_kind == awbrn_ai_diagnostics::EventKind::AttemptInvalidated
            && row.invalidation == Some(Invalidation::Abandoned)
    }));
    fs::remove_dir_all(root).expect("the test directory removes");
}
