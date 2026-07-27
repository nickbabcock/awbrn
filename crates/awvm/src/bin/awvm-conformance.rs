//! Conformance runner for an external implementation.
//!
//! The case playback lives in [`awvm::conformance`]; this binary spawns the
//! adapter under test and reports the run. To exercise this crate's own
//! implementation, use the in-process runner in `tests/conformance.rs` instead.

use std::env;
use std::path::PathBuf;

use awvm::conformance::{self, Progress, Subprocess};

fn main() {
    match run() {
        Ok(summary) if summary.failed == 0 => {
            println!(
                "PASS: {} assertions; {} cases skipped",
                summary.passed, summary.skipped
            );
        }
        Ok(summary) => {
            eprintln!(
                "FAIL: {} assertions passed, {} failed, {} cases skipped",
                summary.passed, summary.failed, summary.skipped
            );
            std::process::exit(1);
        }
        Err(message) => {
            eprintln!("ERROR: {message}");
            std::process::exit(2);
        }
    }
}

fn run() -> Result<conformance::Summary, conformance::ConformanceError> {
    let mut args = env::args().skip(1);
    let implementation = args
        .next()
        .ok_or("usage: awvm-conformance <implementation-executable> [fixture-root]")?;
    let root = PathBuf::from(args.next().unwrap_or_else(|| "spec/fixtures".into()));
    if args.next().is_some() {
        return Err("too many arguments".into());
    }
    let mut peer = Subprocess::spawn(&implementation)?;
    let summary = conformance::run(&mut peer, &root, Progress::Verbose);
    peer.shutdown()?;
    summary
}
