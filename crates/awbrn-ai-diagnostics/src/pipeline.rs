//! Orchestration for the complete deterministic diagnostic workflow.

use std::path::Path;

use awbrn_ai::baseline::BaselineConfig;
use awbrn_ai_diagnostic_types::{ExecutionMode, RunManifest, TelemetryMode};

use crate::manifest::read_manifest;
use crate::map_registry::MapRegistry;
use crate::review::{ReviewError, ReviewSummary, run_review};
use crate::tournament::{
    StrategicFactory, TournamentError, TournamentSummary, run_paired_tournament,
};
use crate::verify::{VerificationSummary, VerifyError, verify_artifact};

/// The outputs of one complete diagnostic workflow.
#[derive(Clone, Debug)]
pub struct DiagnosticSummary {
    pub tournament: TournamentSummary,
    pub review: ReviewSummary,
    pub verification: VerificationSummary,
}

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
    let candidate = StrategicFactory::new(BaselineConfig::PRODUCTION);
    let baseline = StrategicFactory::new(BaselineConfig::LOCKED);
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
