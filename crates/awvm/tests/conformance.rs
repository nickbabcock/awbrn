//! The normative fixture corpus, run in-process.
//!
//! This is the whole of `spec/fixtures/**`, discovered from disk. Adding a
//! fixture adds it to this test; there is no list to maintain and no way for
//! the suite to drift from the corpus.
//!
//! `awvm-conformance` runs the same cases against an external adapter over the
//! JSON Lines protocol. That binary exists for other implementations; this test
//! is how the reference implementation checks itself.

use std::path::PathBuf;

use awvm::conformance::{self, InProcess, Progress};

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spec/fixtures")
}

#[test]
fn every_fixture_conforms() {
    let root = fixture_root();
    let summary = conformance::run(&mut InProcess, &root, Progress::Silent)
        .unwrap_or_else(|error| panic!("conformance run failed: {error}"));

    assert_eq!(
        summary.failed,
        0,
        "{} of {} assertions failed:\n{}",
        summary.failed,
        summary.passed + summary.failed,
        summary.failures.join("\n")
    );

    // The implementation advertises every feature the corpus exercises. A skip
    // means a fixture's feature path is no longer claimed in `protocol::FEATURES`.
    assert_eq!(summary.skipped, 0, "cases were skipped");

    // Guards against the runner silently asserting nothing — a passing run with
    // zero assertions would otherwise look identical to a passing run.
    assert!(
        summary.passed >= 394,
        "expected at least 394 assertions, ran {}",
        summary.passed
    );
}

#[test]
fn every_fixture_on_disk_is_reachable() {
    let root = fixture_root();
    let mut files = Vec::new();
    conformance::collect_json(&root, &mut files).expect("walk fixture root");
    assert!(
        files.len() >= 313,
        "expected at least 313 fixtures on disk, found {}",
        files.len()
    );
}
