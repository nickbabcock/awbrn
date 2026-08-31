//! Read-only verification of a completed diagnostic artifact.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use awbrn_ai_diagnostic_types::{ReductionPlan, ReductionStatus, RunManifest};

use crate::events::{
    EventLogError, observations_from_event_log, read_event_log, render_derived_outputs,
    render_event_tables, verify_expected_fingerprints,
};
use crate::manifest::{ManifestError, read_manifest, resolve_event_log_path};
use crate::map_registry::{MapRegistry, MapRegistryError};

/// A concise result from artifact verification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerificationSummary {
    pub output: PathBuf,
    pub event_rows: usize,
    pub matches: usize,
    pub reduction: ReductionStatus,
}

/// Errors from read-only artifact verification.
#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    #[error("verification manifest error: {0}")]
    Manifest(#[from] ManifestError),
    #[error("verification event log error: {0}")]
    Event(#[from] EventLogError),
    #[error("verification map registry error: {0}")]
    Map(#[from] MapRegistryError),
    #[error("verification JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("verification I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("verification failed: {0}")]
    Invalid(String),
}

/// Verify a complete artifact directory without changing it.
pub fn verify_artifact(
    manifest_path: impl AsRef<Path>,
    output: impl AsRef<Path>,
) -> Result<VerificationSummary, VerifyError> {
    let manifest = read_manifest(manifest_path)?;
    let output = output.as_ref().to_owned();
    let event_path = resolve_event_log_path(&output, &manifest)?;
    let events = read_event_log(&event_path)?;
    let registry = MapRegistry::load_checked_in()?;
    validate_manifest_maps(&manifest, &registry)?;
    validate_event_identities(&manifest, &events)?;

    let observations = observations_from_event_log(&events, &manifest);
    let expected_reduction =
        ReductionPlan::from_pairs(manifest.expected_pairs()).reduce(observations.iter().cloned());
    if expected_reduction.status != ReductionStatus::Complete {
        return Err(VerifyError::Invalid(format!(
            "reduction is {:?}: {}",
            expected_reduction.status,
            expected_reduction.errors.join("; ")
        )));
    }

    let rendered = render_derived_outputs(&observations, &manifest)?;
    let (expected_commands, expected_states, expected_summary) = render_event_tables(&events)?;
    for (name, expected) in [
        ("matches.jsonl", rendered.matches),
        ("reduction.json", rendered.reduction_json),
        ("reduction.csv", rendered.reduction_csv),
        ("commands.csv", expected_commands),
        ("states.jsonl", expected_states),
        (
            "summary.json",
            serde_json::to_vec_pretty(&expected_summary)?,
        ),
    ] {
        let actual = fs::read(output.join(name))
            .map_err(|error| VerifyError::Invalid(format!("{name} cannot be read: {error}")))?;
        if actual != expected {
            return Err(VerifyError::Invalid(format!(
                "{name} does not match event-log regeneration"
            )));
        }
    }
    let manifest_fingerprint = manifest.fingerprint().map_err(VerifyError::Invalid)?;
    let stored_fingerprint = fs::read_to_string(output.join("manifest-fingerprint.txt"))?;
    if stored_fingerprint.trim() != manifest_fingerprint {
        return Err(VerifyError::Invalid(
            "manifest-fingerprint.txt does not match the manifest".into(),
        ));
    }
    verify_expected_fingerprints(&manifest, &event_path, &output)?;

    Ok(VerificationSummary {
        output,
        event_rows: events.len(),
        matches: observations.len(),
        reduction: expected_reduction.status,
    })
}

fn validate_manifest_maps(
    manifest: &RunManifest,
    registry: &MapRegistry,
) -> Result<(), VerifyError> {
    for expected in &manifest.maps {
        let Some(map) = registry.get(expected.map_id) else {
            return Err(VerifyError::Invalid(format!(
                "manifest map {} is not in the fixed registry",
                expected.map_id
            )));
        };
        if expected.source_fingerprint != map.source_fingerprint
            || expected.normalized_fingerprint != map.normalized_fingerprint
        {
            return Err(VerifyError::Invalid(format!(
                "manifest fingerprints differ for map {}",
                expected.map_id
            )));
        }
    }
    Ok(())
}

fn validate_event_identities(
    manifest: &RunManifest,
    events: &[crate::events::EventRow],
) -> Result<(), VerifyError> {
    let maps = manifest
        .maps
        .iter()
        .map(|map| (map.map_id, map.normalized_fingerprint.as_str()))
        .collect::<BTreeMap<_, _>>();
    let pairs = manifest
        .expected_pairs()
        .into_iter()
        .collect::<BTreeSet<_>>();
    for row in events {
        let Some(map_fingerprint) = maps.get(&row.pair.map_id) else {
            return Err(VerifyError::Invalid(format!(
                "event {} names an unknown map {}",
                row.sequence, row.pair.map_id
            )));
        };
        if row.map_fingerprint != *map_fingerprint {
            return Err(VerifyError::Invalid(format!(
                "event {} has a map fingerprint mismatch",
                row.sequence
            )));
        }
        if row.configuration_fingerprint != manifest.configuration_fingerprint {
            return Err(VerifyError::Invalid(format!(
                "event {} has a configuration fingerprint mismatch",
                row.sequence
            )));
        }
        if !pairs.contains(&row.pair) {
            return Err(VerifyError::Invalid(format!(
                "event {} names a pair outside the manifest",
                row.sequence
            )));
        }
    }
    Ok(())
}
