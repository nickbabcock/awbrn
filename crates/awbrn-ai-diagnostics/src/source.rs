//! Git source records for diagnostic runs.

use std::fs;
use std::path::{Path, PathBuf};

use awbrn_ai_diagnostic_types::fingerprint_bytes;
use serde::{Deserialize, Serialize};

use crate::plan::PlanError;

/// The Git source state used by a diagnostic run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceProvenance {
    /// The Git commit identifier.
    pub revision: String,
    /// Whether the source has local changes.
    pub dirty: bool,
    /// A fingerprint of the commit and local source changes.
    pub fingerprint: String,
}

/// Capture the Git source state that can affect a diagnostic run.
pub(crate) fn source_provenance(plan_path: &Path) -> Result<SourceProvenance, PlanError> {
    let root = git_root(plan_path)?;
    let revision = git_text(&root, &["rev-parse", "HEAD"])?;
    let diff = git_bytes(
        &root,
        &[
            "diff",
            "--binary",
            "--no-ext-diff",
            "--no-textconv",
            "HEAD",
            "--",
        ],
    )?;
    let untracked = git_bytes(&root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    let mut paths = untracked
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8_lossy(path).into_owned())
        .collect::<Vec<_>>();
    paths.sort();

    let mut fingerprint_input = b"awbrn-source-fingerprint-v1\0".to_vec();
    append_fingerprint_part(&mut fingerprint_input, revision.as_bytes());
    append_fingerprint_part(&mut fingerprint_input, &diff);
    for path in &paths {
        append_source_entry(&mut fingerprint_input, &root, path)?;
    }
    Ok(SourceProvenance {
        revision,
        dirty: !diff.is_empty() || !paths.is_empty(),
        fingerprint: fingerprint_bytes(&fingerprint_input),
    })
}

fn append_source_entry(
    fingerprint_input: &mut Vec<u8>,
    root: &Path,
    path: &str,
) -> Result<(), PlanError> {
    append_fingerprint_part(fingerprint_input, path.as_bytes());
    let entry = root.join(path);
    let metadata = fs::symlink_metadata(&entry).map_err(|error| {
        PlanError::Configuration(format!(
            "cannot inspect untracked source entry {path:?} for source fingerprint: {error}"
        ))
    })?;
    if metadata.is_symlink() {
        let target = fs::read_link(&entry).map_err(|error| {
            PlanError::Configuration(format!(
                "cannot read untracked source link {path:?} for source fingerprint: {error}"
            ))
        })?;
        append_fingerprint_part(fingerprint_input, b"symlink");
        append_fingerprint_part(fingerprint_input, target.as_os_str().as_encoded_bytes());
    } else if metadata.is_file() {
        let bytes = fs::read(&entry).map_err(|error| {
            PlanError::Configuration(format!(
                "cannot read untracked source file {path:?} for source fingerprint: {error}"
            ))
        })?;
        append_fingerprint_part(fingerprint_input, &bytes);
    } else {
        append_fingerprint_part(fingerprint_input, b"non-regular");
        append_fingerprint_part(fingerprint_input, b"");
    }
    Ok(())
}

fn git_root(plan_path: &Path) -> Result<PathBuf, PlanError> {
    for directory in [plan_directory(plan_path), Path::new(".")] {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(directory)
            .output();
        if let Ok(output) = output
            && output.status.success()
        {
            let root = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if !root.is_empty() {
                return Ok(PathBuf::from(root));
            }
        }
    }
    Err(PlanError::Configuration(
        "cannot determine the Git repository for source provenance".into(),
    ))
}

fn git_bytes(root: &Path, arguments: &[&str]) -> Result<Vec<u8>, PlanError> {
    let output = std::process::Command::new("git")
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| PlanError::Configuration(format!("cannot run Git: {error}")))?;
    if !output.status.success() {
        return Err(PlanError::Configuration(format!(
            "Git command {arguments:?} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(output.stdout)
}

fn git_text(root: &Path, arguments: &[&str]) -> Result<String, PlanError> {
    let bytes = git_bytes(root, arguments)?;
    let value = String::from_utf8_lossy(&bytes).trim().to_owned();
    if value.is_empty() {
        return Err(PlanError::Configuration(format!(
            "Git command {arguments:?} returned an empty value"
        )));
    }
    Ok(value)
}

fn append_fingerprint_part(input: &mut Vec<u8>, part: &[u8]) {
    input.extend_from_slice(&(part.len() as u64).to_le_bytes());
    input.extend_from_slice(part);
}

fn plan_directory(plan_path: &Path) -> &Path {
    plan_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repository() -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        for arguments in [
            vec!["init", "--quiet"],
            vec![
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "--quiet",
                "--allow-empty",
                "-m",
                "Initial commit",
            ],
        ] {
            let output = std::process::Command::new("git")
                .args(arguments)
                .current_dir(directory.path())
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
        }
        directory
    }

    #[test]
    fn regular_file_contents_change_source_fingerprint() {
        let directory = repository();
        let plan = directory.path().join("plan.json");
        let clean = source_provenance(&plan).unwrap();
        assert!(!clean.dirty);
        let file = directory.path().join("source.rs");
        fs::write(&file, "first").unwrap();
        let first = source_provenance(&plan).unwrap();
        assert!(first.dirty);
        assert_ne!(clean.fingerprint, first.fingerprint);
        fs::write(file, "second").unwrap();
        assert_ne!(
            first.fingerprint,
            source_provenance(&plan).unwrap().fingerprint
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_fingerprint_uses_target_path_without_reading_target() {
        use std::os::unix::fs::symlink;

        let directory = repository();
        let external = tempfile::tempdir().unwrap();
        let target = external.path().join("target");
        fs::write(&target, "first").unwrap();
        let link = directory.path().join("link");
        symlink(&target, &link).unwrap();
        let plan = directory.path().join("plan.json");
        let first = source_provenance(&plan).unwrap();
        assert!(first.dirty);
        fs::write(&target, "second").unwrap();
        assert_eq!(first, source_provenance(&plan).unwrap());
        fs::remove_file(&target).unwrap();
        assert_eq!(first, source_provenance(&plan).unwrap());
        fs::remove_file(&link).unwrap();
        symlink("another-missing-target", &link).unwrap();
        assert_ne!(
            first.fingerprint,
            source_provenance(&plan).unwrap().fingerprint
        );
    }

    #[cfg(unix)]
    #[test]
    fn special_file_is_recorded_without_reading_it() {
        let directory = tempfile::tempdir().unwrap();
        let pipe = directory.path().join("pipe");
        let output = std::process::Command::new("mkfifo")
            .arg(&pipe)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let capture = || {
            let mut input = Vec::new();
            append_source_entry(&mut input, directory.path(), "pipe").unwrap();
            input
        };
        let special = capture();
        assert!(!special.is_empty());
        assert_eq!(special, capture());
        fs::remove_file(&pipe).unwrap();
        fs::write(pipe, "non-regular").unwrap();
        assert_ne!(special, capture());
    }
}
