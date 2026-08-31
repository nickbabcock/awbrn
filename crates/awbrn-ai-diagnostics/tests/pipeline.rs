use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use awbrn_ai::baseline::BaselineConfig;
use awbrn_ai_diagnostic_types::{FramePolicy, Invalidation, RunManifest, fingerprint_bytes};
use awbrn_ai_diagnostics::{
    MapRegistry, StrategicFactory, TournamentError, read_event_log, read_manifest,
    reanalyse_event_log_with_manifest, run_diagnostic, run_paired_tournament,
    run_review_with_tilesets, verify_artifact, verify_expected_fingerprints, write_manifest,
};

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
    let first_events = fs::read(run.join("events.jsonl")).expect("the event log exists");
    let first_matches = fs::read(run.join("matches.jsonl")).expect("the match rows exist");
    fs::remove_file(run.join("matches.jsonl")).expect("the disposable match rows remove");
    run_paired_tournament(&manifest, &registry, &candidate, &baseline, &run)
        .expect("the paired run resumes without replaying complete matches");
    assert_eq!(
        first_events,
        fs::read(run.join("events.jsonl")).expect("the resumed event log exists")
    );
    assert_eq!(
        first_matches,
        fs::read(run.join("matches.jsonl")).expect("the match rows are rebuilt")
    );
    let events = read_event_log(run.join("events.jsonl")).expect("the event log reads");
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
