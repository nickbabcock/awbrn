//! Native manifest persistence and compatibility checks.

use std::fs;
use std::path::{Component, Path, PathBuf};

pub use awbrn_ai_diagnostic_types::*;

/// Errors from manifest persistence.
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("manifest I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("manifest schema error: {0}")]
    Schema(#[from] RunManifestError),
    #[error("existing manifest does not match the requested run")]
    Mismatch,
    #[error("existing manifest source fingerprint does not match the requested run")]
    SourceMismatch,
    #[error("manifest path is not safe: {0}")]
    UnsafePath(String),
}

/// Read and validate a manifest from disk.
pub fn read_manifest(path: impl AsRef<Path>) -> Result<RunManifest, ManifestError> {
    Ok(RunManifest::from_json(&fs::read(path)?)?)
}

/// Write a validated manifest with stable pretty-printing.
pub fn write_manifest(manifest: &RunManifest, path: impl AsRef<Path>) -> Result<(), ManifestError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, manifest.to_json()?)?;
    Ok(())
}

/// Write a manifest, or verify the existing file before a resume.
pub fn write_or_validate_manifest(
    manifest: &RunManifest,
    path: impl AsRef<Path>,
) -> Result<(), ManifestError> {
    let path = path.as_ref();
    if path.exists() {
        let existing = read_manifest(path)?;
        if existing.source_fingerprint != manifest.source_fingerprint {
            return Err(ManifestError::SourceMismatch);
        }
        if existing != *manifest {
            return Err(ManifestError::Mismatch);
        }
        return Ok(());
    }
    write_manifest(manifest, path)
}

/// Resolve the event log path relative to an artifact directory.
pub fn resolve_event_log_path(
    base: impl AsRef<Path>,
    manifest: &RunManifest,
) -> Result<PathBuf, ManifestError> {
    let relative = manifest.event_log.as_deref().unwrap_or("events.jsonl");
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ManifestError::UnsafePath(relative.to_owned()));
    }
    Ok(base.as_ref().join(path))
}
