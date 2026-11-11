//! Git source records for diagnostic runs.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

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

/// The largest untracked file, in bytes, whose contents go into the source
/// fingerprint. A larger file is recorded by its path and size only, so a
/// build output that `.gitignore` does not exclude does not stop the run.
const UNTRACKED_FILE_LIMIT: u64 = 32 * 1024 * 1024;

/// The size of the read buffer for an untracked file.
const READ_CHUNK: usize = 64 * 1024;

/// The fingerprint of a sequence of parts, as
/// [`awbrn_ai_diagnostic_types::fingerprint_bytes`] gives it for the
/// concatenated parts.
///
/// The hash takes the bytes one at a time, so the parts do not go into one
/// buffer. Each part is its length as a little-endian `u64`, then its bytes.
struct Fingerprint {
    hash: u64,
}

impl Fingerprint {
    fn new() -> Self {
        Self {
            hash: 0xcbf2_9ce4_8422_2325,
        }
    }

    fn bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.hash ^= u64::from(*byte);
            self.hash = self.hash.wrapping_mul(0x1000_0000_01b3);
        }
    }

    fn part(&mut self, part: &[u8]) {
        self.bytes(&(part.len() as u64).to_le_bytes());
        self.bytes(part);
    }

    fn finish(&self) -> String {
        format!("{:016x}", self.hash)
    }
}

/// Capture the Git source state that can affect a diagnostic run.
pub(crate) fn source_provenance(plan_path: &Path) -> Result<SourceProvenance, PlanError> {
    source_provenance_with_limit(plan_path, UNTRACKED_FILE_LIMIT)
}

/// [`source_provenance`] with `limit` as the largest untracked file whose
/// contents go into the fingerprint.
fn source_provenance_with_limit(
    plan_path: &Path,
    limit: u64,
) -> Result<SourceProvenance, PlanError> {
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

    let mut fingerprint = Fingerprint::new();
    fingerprint.bytes(b"awbrn-source-fingerprint-v1\0");
    fingerprint.part(revision.as_bytes());
    fingerprint.part(&diff);
    for path in &paths {
        append_source_entry(&mut fingerprint, &root, path, limit)?;
    }
    Ok(SourceProvenance {
        revision,
        dirty: !diff.is_empty() || !paths.is_empty(),
        fingerprint: fingerprint.finish(),
    })
}

fn append_source_entry(
    fingerprint: &mut Fingerprint,
    root: &Path,
    path: &str,
    limit: u64,
) -> Result<(), PlanError> {
    fingerprint.part(path.as_bytes());
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
        fingerprint.part(b"symlink");
        fingerprint.part(target.as_os_str().as_encoded_bytes());
    } else if metadata.is_file() && metadata.len() > limit {
        // Do not read a large file. Record its size.
        fingerprint.part(b"large");
        fingerprint.part(&metadata.len().to_le_bytes());
    } else if metadata.is_file() {
        append_file_contents(fingerprint, &entry, metadata.len()).map_err(|error| {
            PlanError::Configuration(format!(
                "cannot read untracked source file {path:?} for source fingerprint: {error}"
            ))
        })?;
    } else {
        fingerprint.part(b"non-regular");
        fingerprint.part(b"");
    }
    Ok(())
}

/// Add the contents of the file at `entry` as one part, in chunks. `len` is
/// the size from the metadata. When the file changes size during the read,
/// this returns an error.
fn append_file_contents(
    fingerprint: &mut Fingerprint,
    entry: &Path,
    len: u64,
) -> std::io::Result<()> {
    let mut file = fs::File::open(entry)?;
    fingerprint.bytes(&len.to_le_bytes());
    let mut buffer = vec![0; READ_CHUNK];
    let mut read = 0_u64;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        read += count as u64;
        if read > len {
            break;
        }
        fingerprint.bytes(&buffer[..count]);
    }
    if read != len {
        return Err(std::io::Error::other(format!(
            "the file changed size during the read: {len} bytes before, {read} bytes or more read"
        )));
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
            let mut input = Fingerprint::new();
            append_source_entry(&mut input, directory.path(), "pipe", UNTRACKED_FILE_LIMIT)
                .unwrap();
            input.finish()
        };
        let special = capture();
        assert_ne!(special, Fingerprint::new().finish());
        assert_eq!(special, capture());
        fs::remove_file(&pipe).unwrap();
        fs::write(pipe, "non-regular").unwrap();
        assert_ne!(special, capture());
    }

    /// The fingerprint that the earlier code made: all parts in one buffer.
    fn buffered(parts: &[&[u8]]) -> String {
        let mut input = b"awbrn-source-fingerprint-v1\0".to_vec();
        for part in parts {
            input.extend_from_slice(&(part.len() as u64).to_le_bytes());
            input.extend_from_slice(part);
        }
        awbrn_ai_diagnostic_types::fingerprint_bytes(&input)
    }

    /// The streamed fingerprint is the same as the fingerprint of one
    /// buffer, so the provenance of a normal tree does not change.
    #[test]
    fn the_streamed_fingerprint_matches_the_buffered_one() {
        let directory = repository();
        let plan = directory.path().join("plan.json");
        let large = vec![7_u8; READ_CHUNK * 2 + 5];
        fs::write(directory.path().join("a.rs"), "first").unwrap();
        fs::write(directory.path().join("b.bin"), &large).unwrap();
        let provenance = source_provenance(&plan).unwrap();
        let revision = git_text(directory.path(), &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(
            provenance.fingerprint,
            buffered(&[
                revision.as_bytes(),
                b"",
                b"a.rs",
                b"first",
                b"b.bin",
                &large
            ])
        );
    }

    /// An untracked file above the limit is recorded by its size, and its
    /// contents are not read. A file at the limit is read as before.
    #[test]
    fn a_large_untracked_file_is_recorded_by_its_size() {
        let directory = repository();
        let plan = directory.path().join("plan.json");
        let file = directory.path().join("large.bin");
        fs::write(&file, "0123456789").unwrap();
        let first = source_provenance_with_limit(&plan, 9).unwrap();
        assert!(first.dirty);
        fs::write(&file, "abcdefghij").unwrap();
        assert_eq!(first, source_provenance_with_limit(&plan, 9).unwrap());
        fs::write(&file, "abcdefghijk").unwrap();
        assert_ne!(first, source_provenance_with_limit(&plan, 9).unwrap());

        fs::write(&file, "0123456789").unwrap();
        let read = source_provenance_with_limit(&plan, 10).unwrap();
        assert_ne!(first.fingerprint, read.fingerprint);
        fs::write(&file, "abcdefghij").unwrap();
        assert_ne!(read, source_provenance_with_limit(&plan, 10).unwrap());
    }
}
