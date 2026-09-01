//! Orchestration for the complete deterministic diagnostic workflow.

use std::path::Path;

use awbrn_ai_diagnostic_types::{ExecutionMode, RunManifest, TelemetryMode};

use crate::feature_analysis::{FeatureAnalysisSummary, analyze_event_log};
use crate::manifest::read_manifest;
use crate::map_registry::MapRegistry;
use crate::plan::{AnalysisStage, PlanError, read_plan};
use crate::review::{ReviewError, ReviewSummary, run_review};
use crate::tournament::{TournamentError, TournamentSummary, run_paired_tournament};
use crate::verify::{VerificationSummary, VerifyError, verify_artifact};

/// The outputs of one complete diagnostic workflow.
#[derive(Clone, Debug)]
pub struct DiagnosticSummary {
    pub tournament: TournamentSummary,
    pub review: ReviewSummary,
    pub verification: VerificationSummary,
}

/// Outputs selected by one generic experiment plan.
#[derive(Debug)]
pub struct PlanRunSummary {
    pub tournament: TournamentSummary,
    pub feature_analysis: Option<FeatureAnalysisSummary>,
    pub review: Option<ReviewSummary>,
    pub verification: Option<VerificationSummary>,
}

type StageOutputs = (
    Option<FeatureAnalysisSummary>,
    Option<ReviewSummary>,
    Option<VerificationSummary>,
);

/// Errors from the complete diagnostic workflow.
#[derive(Debug, thiserror::Error)]
pub enum DiagnosticError {
    #[error("diagnostic manifest error: {0}")]
    Manifest(#[from] crate::manifest::ManifestError),
    #[error("diagnostic tournament error: {0}")]
    Tournament(#[from] TournamentError),
    #[error("diagnostic review error: {0}")]
    Review(#[from] ReviewError),
    #[error("diagnostic verification error: {0}")]
    Verification(#[from] VerifyError),
    #[error("diagnostic configuration error: {0}")]
    Configuration(String),
    #[error("diagnostic plan error: {0}")]
    Plan(#[from] PlanError),
    #[error("diagnostic feature-analysis error: {0}")]
    Feature(#[from] crate::feature_analysis::FeatureAnalysisError),
}

/// Execute or resume matches, derive all outputs, render the requested review,
/// and fail when the paired reduction is not complete.
pub fn run_diagnostic(
    manifest_path: impl AsRef<Path>,
    output: impl AsRef<Path>,
) -> Result<DiagnosticSummary, DiagnosticError> {
    let manifest = read_manifest(&manifest_path)?;
    validate_execution_mode(&manifest)?;
    let registry = MapRegistry::load_checked_in().map_err(TournamentError::from)?;
    let candidate =
        crate::tournament::StrategicFactory::new(awbrn_ai::baseline::BaselineConfig::PRODUCTION);
    let baseline =
        crate::tournament::StrategicFactory::new(awbrn_ai::baseline::BaselineConfig::LOCKED);
    let output = output.as_ref().to_owned();
    let tournament = run_paired_tournament(&manifest, &registry, &candidate, &baseline, &output)?;
    let verification = verify_artifact(output.join("manifest.json"), &output)?;
    let review = run_review(output.join("manifest.json"), output.join("review"))?;
    Ok(DiagnosticSummary {
        tournament,
        review,
        verification,
    })
}

/// Materialize an experiment plan, run it, and execute its selected stages.
pub fn run_plan(
    plan_path: impl AsRef<Path>,
    output: impl AsRef<Path>,
) -> Result<PlanRunSummary, DiagnosticError> {
    let plan_path = plan_path.as_ref();
    let plan = read_plan(plan_path)?;
    let registry = MapRegistry::load_checked_in().map_err(TournamentError::from)?;
    let materialized = plan.materialize(plan_path, &registry)?;
    validate_execution_mode(&materialized.manifest)?;
    let output = output.as_ref().to_owned();
    let tournament = run_paired_tournament(
        &materialized.manifest,
        &registry,
        materialized.candidate.as_ref(),
        materialized.baseline.as_ref(),
        &output,
    )?;

    let (feature_analysis, review, verification) =
        dispatch_stages(&materialized.analyses, &output, &materialized.manifest)?;
    Ok(PlanRunSummary {
        tournament,
        feature_analysis,
        review,
        verification,
    })
}

/// Run optional stages in their declared dependency order.
fn dispatch_stages(
    stages: &[AnalysisStage],
    output: &Path,
    manifest: &RunManifest,
) -> Result<StageOutputs, DiagnosticError> {
    let mut feature_analysis = None;
    let mut review = None;
    let mut verification = None;
    let event_path = crate::manifest::resolve_event_log_path(output, manifest)?;
    for stage in [
        AnalysisStage::OutcomeFeatures,
        AnalysisStage::Review,
        AnalysisStage::Verification,
    ] {
        if !stages.contains(&stage) {
            continue;
        }
        match stage {
            AnalysisStage::OutcomeFeatures => {
                feature_analysis = Some(analyze_event_log(
                    &event_path,
                    output.join("feature-analysis"),
                )?);
            }
            AnalysisStage::Review => {
                review = Some(run_review(
                    output.join("manifest.json"),
                    output.join("review"),
                )?);
            }
            AnalysisStage::Verification => {
                verification = Some(verify_artifact(output.join("manifest.json"), output)?);
            }
        }
    }
    Ok((feature_analysis, review, verification))
}

fn validate_execution_mode(manifest: &RunManifest) -> Result<(), DiagnosticError> {
    if manifest.mode != ExecutionMode::Diagnostic {
        return Err(DiagnosticError::Configuration(
            "ai-diagnostics run requires manifest mode diagnostic; use a separate performance run"
                .into(),
        ));
    }
    if manifest.telemetry != TelemetryMode::Enabled {
        return Err(DiagnosticError::Configuration(
            "ai-diagnostics run requires telemetry enabled so events.jsonl remains authoritative"
                .into(),
        ));
    }
    Ok(())
}
